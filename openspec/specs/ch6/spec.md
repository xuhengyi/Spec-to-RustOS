# Capability: ch6

## Purpose

`ch6` is a RISC-V Supervisor-mode (S-mode) `#![no_std]` kernel binary that boots with an Sv39 kernel address space, initializes heap and syscall subsystems, mounts an `easy-fs` filesystem backed by a VirtIO block device, loads an initial user process from an ELF file described by the on-disk image, and then repeatedly schedules/runs user processes while handling traps and dispatching syscalls.

## Requirements

### Requirement: Boot entry, BSS zeroing, and non-returning main
The binary MUST provide a supervisor entry symbol suitable for execution in a bare-metal environment and MUST transfer control into `rust_main() -> !`.
On entry, the binary MUST zero the kernel `.bss` segment before performing any logic that relies on zero-initialized static storage.
`rust_main` MUST NOT return.

#### Scenario: Kernel boots and clears BSS before initialization
- **WHEN** a Supervisor Execution Environment (SEE) transfers control to the kernel entry point (as arranged by the linker script)
- **THEN** the kernel clears its `.bss` region before initializing subsystems
- **AND THEN** the kernel enters `rust_main` and never returns to the SEE

### Requirement: Console and logging initialization
During normal boot, the kernel MUST initialize the console backend and MUST configure the logging level from the optional `LOG` compile-time environment variable.

#### Scenario: Console is available for printing and logging
- **WHEN** `rust_main` begins executing
- **THEN** the kernel initializes `rcore_console` with a console backend that writes bytes via SBI legacy console output
- **AND THEN** `print!/println!` and `rcore_console::log::*` output is routed to that backend

### Requirement: Heap initialization and memory transfer
The kernel MUST initialize the kernel heap allocator and MUST transfer the remaining RAM region (after the kernel image) into the allocator as free memory.

#### Scenario: Heap becomes usable for allocations
- **WHEN** `rust_main` completes heap initialization
- **THEN** subsequent heap allocations used by this binary (including page tables, DMA buffers, and filesystem buffers) can succeed

### Requirement: Kernel address space construction (Sv39) and activation
The kernel MUST construct a kernel Sv39 address space that:
- maps kernel image regions per the linker-provided layout with appropriate execute/read/write permissions,
- maps the heap region (RAM after the kernel image up to the configured memory size) as writable,
- maps a single “portal transit” virtual page,
- maps all MMIO regions declared by `MMIO`,
and MUST activate the resulting page table by writing `satp`.

#### Scenario: Kernel activates its page table and can access mapped regions
- **WHEN** `rust_main` builds the kernel address space
- **THEN** the kernel maps kernel regions, heap, portal transit page, and `MMIO` ranges
- **AND THEN** it writes `satp` to activate Sv39 translation for subsequent execution

### Requirement: Portal creation and transit mapping for user execution
The kernel MUST allocate and initialize a `kernel_context::foreign::MultislotPortal` transit region and MUST ensure that user address spaces include a mapping for the portal transit page that is consistent with the kernel mapping.

#### Scenario: Kernel can enter and return from user execution
- **WHEN** the kernel schedules a user process and invokes `ForeignContext::execute(...)`
- **THEN** control transfers into user code and returns to the kernel on trap using the initialized portal transit mapping

### Requirement: VirtIO block device provisioning for `easy-fs`
The kernel MUST expose a global `easy_fs::BlockDevice` implementation backed by a VirtIO Block MMIO device at `0x1000_1000`, and MUST support block reads and writes as required by `easy-fs`.

#### Scenario: Filesystem can read/write blocks through the VirtIO device
- **WHEN** the filesystem requests block I/O via `BlockDevice::{read_block, write_block}`
- **THEN** the request is forwarded to the VirtIO block driver and completes or fails with a panic message indicating VirtIO I/O error

### Requirement: Filesystem root mount and path operations
The kernel MUST initialize a filesystem manager `FS` rooted at the `easy-fs` root inode on the global block device.
`FS` MUST implement:
- `open(path, flags)` for basic file open/create/truncate behavior,
- `find(path)` and `readdir(path)` for inode lookup and directory listing.
If `OpenFlags::CREATE` is present and the file exists, `open` MUST clear the file contents before returning a handle.
If `OpenFlags::TRUNC` is present, `open` MUST clear the file contents before returning a handle.

#### Scenario: Open with CREATE clears existing file
- **WHEN** a caller opens an existing file with `OpenFlags::CREATE`
- **THEN** the file contents are cleared before the returned `FileHandle` is used

### Requirement: Loading an initial user process from the filesystem
During boot, the kernel MUST open `initproc` from the filesystem as read-only, read its entire contents, parse it as an ELF file, and attempt to construct a user process from it.
If user process construction succeeds, the kernel MUST add the process into the scheduler as a runnable process.

#### Scenario: Boot loads and schedules initproc
- **WHEN** the filesystem contains an `initproc` file that is a valid RISC-V ELF executable
- **THEN** the kernel constructs a `Process` from it and enqueues it for execution

### Requirement: ELF-based process construction and memory mapping
The kernel MUST support creating a `Process` from a RISC-V executable ELF (`xmas-elf`) by:
- verifying the ELF is an executable for `Machine::RISC_V`,
- mapping each `PT_LOAD` segment into a fresh user address space at its requested virtual address,
- applying user page permissions derived from ELF segment flags (execute/write/read),
- provisioning a user stack region, and
- mapping the portal transit page into the user address space.
The initial user context MUST start at the ELF entry point and MUST set the user stack pointer to the top of the provisioned user stack.

#### Scenario: ELF load maps PT_LOAD segments and sets up user stack
- **WHEN** `Process::from_elf` is called with a valid RISC-V executable ELF
- **THEN** all `PT_LOAD` segments are mapped with user permissions matching ELF flags
- **AND THEN** a user stack is mapped and the initial user `sp` points to the top of the stack

### Requirement: Process fork semantics
The kernel MUST support `fork` by creating a child process that:
- has a new process ID,
- has an address space cloned from the parent,
- has a user context copied from the parent (with child `a0` set to 0),
- has a file-descriptor table copied from the parent by cloning each existing file handle entry.

#### Scenario: fork returns 0 to the child and preserves file descriptors
- **WHEN** a process invokes `fork`
- **THEN** the created child process observes return value 0 in `a0`
- **AND THEN** the child file-descriptor table mirrors the parent’s open entries at the time of fork

### Requirement: Process exec semantics
The kernel MUST support `exec` by loading the target ELF program from the filesystem and replacing the current process’s address space and context with the new program.
If the requested program name does not exist in the filesystem, the kernel MUST return `-1` and MUST print a list of available apps from the filesystem directory listing.

#### Scenario: exec replaces the current process image
- **WHEN** a process invokes `exec` with a valid program name present on the filesystem
- **THEN** the kernel loads the program ELF and replaces the current process’s address space and context

### Requirement: Cooperative scheduling and runnable-queue dispatch
The kernel MUST maintain a runnable-queue scheduler for processes and MUST repeatedly select the next runnable process and enter it in user mode.
If no runnable process exists, the kernel MUST print `no task` and MUST proceed to shutdown.

#### Scenario: Scheduler runs until queue exhaustion
- **WHEN** the runnable queue becomes empty
- **THEN** the kernel prints `no task`
- **AND THEN** it requests system shutdown

### Requirement: Trap handling policy and syscall dispatch ABI
After entering user mode, the kernel MUST inspect the trap cause.
For `UserEnvCall`:
- The kernel MUST advance the user instruction pointer to the next instruction.
- The kernel MUST read the syscall ID from register `a7` and syscall arguments from registers `a0..a5`.
- The kernel MUST dispatch the syscall by invoking `syscall::handle(Caller { entity: 0, flow: 0 }, id, args)`.
For any other trap, the kernel MUST treat it as unsupported and MUST terminate the current process with exit code `-3`.

#### Scenario: User ecall is dispatched through syscall::handle
- **WHEN** a process executes `ecall` from U-mode
- **THEN** the kernel reads ID from `a7` and args from `a0..a5`, advances past the trapping instruction, and calls `syscall::handle(...)`

### Requirement: Syscall return-value and exit semantics
The kernel MUST interpret the return value of `syscall::handle(...)` as follows.
If `syscall::handle(...)` returns `SyscallResult::Done(ret)`:
- For `SYS_exit`, the kernel MUST mark the current process as exited with the returned exit code.
- For any other syscall, the kernel MUST write `ret` to user register `a0` and MUST suspend the current process so that other runnable processes can run.
If `syscall::handle(...)` returns `SyscallResult::Unsupported(id)`, the kernel MUST terminate the current process with exit code `-2`.

#### Scenario: Non-exit syscall returns to user and yields
- **WHEN** a process invokes a supported, non-exit syscall
- **THEN** the kernel writes the returned value to `a0`
- **AND THEN** the current process is suspended and later may be rescheduled

### Requirement: IO syscalls over stdin/stdout and filesystem-backed file descriptors
The kernel MUST implement the syscall IO operations `read`, `write`, `open`, and `close` with the following behaviors:
- `read(STDIN, ...)` reads bytes from SBI legacy console input into the provided user buffer.
- `write(STDOUT/STDDEBUG, ...)` writes bytes from the provided user buffer to the console output.
- `open(path, flags)` interprets `path` as a NUL-terminated string in user memory and opens the corresponding filesystem path with `OpenFlags` derived from `flags`; on success it allocates a new file descriptor.
- `close(fd)` closes an existing file descriptor and returns 0; invalid or unopened descriptors return `-1`.
For filesystem-backed reads/writes, the kernel MUST validate that the user buffer pointer translates with the required access permissions.

#### Scenario: open reads a NUL-terminated user string and returns a new fd
- **WHEN** a process calls `open` with a user pointer to a valid NUL-terminated path string
- **THEN** the kernel opens that path via `FS.open(...)` and returns a new file descriptor index

### Requirement: Process syscalls: wait and getpid
The kernel MUST implement:
- `wait(pid, exit_code_ptr)` to wait for a child process and write its exit code to `exit_code_ptr` when present, returning the child pid; if no such child process exists, it MUST return `-1`.
- `getpid()` to return the current process ID.

#### Scenario: wait returns -1 when the requested child does not exist
- **WHEN** a process calls `wait` for a pid that is not a dead child
- **THEN** the syscall returns `-1`

### Requirement: Time syscall: clock_gettime
The kernel MUST implement `clock_gettime(CLOCK_MONOTONIC, tp)` by writing a `TimeSpec` derived from the RISC-V `time` CSR into user memory at `tp`.
For unsupported clock IDs, it MUST return `-1`.

#### Scenario: clock_gettime writes TimeSpec to user memory
- **WHEN** a process calls `clock_gettime` with `CLOCK_MONOTONIC` and a writable user pointer
- **THEN** the kernel writes a `TimeSpec { tv_sec, tv_nsec }` to that location and returns 0

### Requirement: Shutdown and panic behavior
After the scheduling loop ends (no runnable processes), the kernel MUST request shutdown via `sbi_rt::system_reset(Shutdown, NoReason)` and MUST NOT return.
On panic, the kernel MUST print the panic information and MUST request shutdown via `sbi_rt::system_reset(Shutdown, SystemFailure)`. The panic handler MUST NOT return.

#### Scenario: Clean shutdown after all tasks complete
- **WHEN** the kernel detects there are no runnable tasks remaining
- **THEN** it calls `system_reset(Shutdown, NoReason)` and never returns

## Public API

This crate is a binary (`#![no_std]`, `#![no_main]`). Its externally visible interface is via exported symbols expected by the linker/SEE. It also exposes several `pub` items that are intended for in-crate use but are technically public.

### Symbols
- `_start() -> !`: Supervisor entry symbol provided by `linker::boot0!`; sets up an initial stack and transfers control to `rust_main`.

### Constants
- `MMIO: &[(usize, usize)]`: MMIO regions to map into the kernel address space.

### Modules and items
- `fs`:
  - `FS: Lazy<FileSystem>`: Global filesystem manager.
  - `FileSystem`: `easy_fs::FSManager` implementation rooted at `easy-fs` root inode.
  - `read_all(Arc<FileHandle>) -> Vec<u8>`: Read the entire contents of a file handle into a byte vector.
- `virtio_block`:
  - `BLOCK_DEVICE: Lazy<Arc<dyn BlockDevice>>`: Global VirtIO-backed block device.
- `process`:
  - `Process`: User process representation (`pid`, `ForeignContext`, user `AddressSpace`, and fd table).
  - `Process::{from_elf, fork, exec}`: Process lifecycle operations used by syscall handlers.
- `processor`:
  - `PROCESSOR: PManager<Process, ProcManager>`: Global process manager and scheduler state.
  - `ProcManager`: Runnable-queue scheduler backing `PROCESSOR`.
- `impls` (internal):
  - `Sv39Manager`: `kernel_vm::PageManager` implementation used for Sv39 page-table allocation.
  - `Console`: `rcore_console::Console` backend using SBI legacy console output.
  - `SyscallContext`: Implements syscall traits `IO`, `Process`, `Scheduling`, and `Clock`.

## Build Configuration

### build.rs
- The build script MUST write a linker script named `linker.ld` into Cargo `OUT_DIR` using the `linker::SCRIPT` string.
- The build script MUST pass `-T<OUT_DIR>/linker.ld` to the linker via `cargo:rustc-link-arg`.
- The build script MUST request rebuild when `build.rs` changes via `cargo:rerun-if-changed=build.rs`.
- The build script MUST request rebuild when `LOG` or `APP_ASM` environment variables change via `cargo:rerun-if-env-changed`.

#### Scenario: Cargo build emits and uses the linker script
- **WHEN** Cargo builds `ch6`
- **THEN** `build.rs` writes `<OUT_DIR>/linker.ld`
- **AND THEN** the final link uses that linker script via `-T.../linker.ld`

### Environment variables
- `OUT_DIR`: MUST be provided by Cargo during build-script execution; used as the output location for the generated `linker.ld`.
- `LOG`: MAY be provided at compile time; used to configure runtime log level via `rcore_console::set_log_level(option_env!("LOG"))`.
- `APP_ASM`: MAY be set at compile time; `build.rs` tracks it for rebuilds (even though `ch6` does not directly embed app assembly).

### Generated files
- `<OUT_DIR>/linker.ld`: Linker script content equal to `linker::SCRIPT`.

## Dependencies

### Workspace crates (Preconditions)
- `linker`:
  - MUST provide `boot0!(entry; stack = N)` that defines the kernel entry symbol and sets up an initial stack before calling `entry`.
  - MUST provide `KernelLayout::locate()` and unsafe `zero_bss()` that clears the kernel `.bss`.
  - MUST provide `KernelLayout::iter()` with per-region metadata (range and title) so the kernel can map kernel image regions.
  - MUST provide `KernelRegionTitle` variants used to derive mapping permissions (`Text`, `Rodata`, `Data`, `Boot`).
  - MUST provide `SCRIPT` as a linker-script string consumable by `build.rs`.
- `rcore-console` (`rcore_console`):
  - MUST provide `init_console(&impl rcore_console::Console)`, `set_log_level(Option<&'static str>)`, and `test_log()`.
  - MUST provide printing macros (`print!`, `println!`) and logging macros via `rcore_console::log::*`.
  - MUST define the `rcore_console::Console` trait with `put_char(u8)`.
- `kernel-alloc`:
  - MUST provide `init(heap_start: usize)` and `transfer(&mut [u8])` to establish and populate the kernel heap.
- `kernel-vm`:
  - MUST provide `AddressSpace` with `new`, `map`, `map_extern`, `translate`, and `cloneself` used for kernel/user mappings and fork cloning.
  - MUST provide Sv39 types and helpers (`Sv39`, `VPN`, `PPN`, `VAddr`, `VmFlags`) and support constructing flags from a string (`build_from_str`).
  - MUST define the `PageManager` trait used by `Sv39Manager`.
- `kernel-context` (feature: `foreign`):
  - MUST provide `LocalContext::user(entry: usize)` to create initial user contexts.
  - MUST provide `foreign::{ForeignContext, MultislotPortal}` sufficient to enter user execution and return on trap.
  - MUST provide register accessors used by this kernel: `a(i)`, `a_mut(i)`, `move_next()`, and `sp_mut()`.
- `syscall` (feature: `kernel`):
  - MUST provide `init_io`, `init_process`, `init_scheduling`, and `init_clock`.
  - MUST provide `handle(caller: Caller, id: SyscallId, args: [usize; 6]) -> SyscallResult`.
  - MUST define `Caller { entity, flow }`, `SyscallId` (including `EXIT`), and `SyscallResult::{Done, Unsupported}`.
  - MUST provide syscall traits implemented here: `IO`, `Process`, `Scheduling`, and `Clock`, including required types (`TimeSpec`, `ClockId`) and fd constants (`STDIN`, `STDOUT`, `STDDEBUG`).
- `rcore-task-manage` (feature: `proc`):
  - MUST provide `PManager` operations used by this kernel (`set_manager`, `add`, `find_next`, `current`, `make_current_suspend`, `make_current_exited`, `wait`).
  - MUST provide `Manage` and `Schedule` traits used to implement `ProcManager`.
  - MUST provide `ProcId` with construction and conversion (`new`, `from_usize`, `get_usize`).
- `easy-fs`:
  - MUST provide `BlockDevice` and `EasyFileSystem::{open, root_inode}` to mount the filesystem on the block device.
  - MUST provide `FSManager`, `OpenFlags`, `Inode`, `FileHandle`, and `UserBuffer` as used by file and syscall code.

### External crates
- `virtio-drivers`: MUST provide `VirtIOBlk` and `Hal` used to implement a VirtIO-backed `BlockDevice`.
- `sbi-rt` (feature: `legacy`): MUST provide `legacy::{console_putchar, console_getchar}` and `system_reset` with the `Shutdown`, `NoReason`, and `SystemFailure` values used by this binary.
- `xmas-elf`: MUST provide ELF parsing sufficient to iterate program headers and read the RISC-V executable entry point.
- `riscv`: MUST provide `riscv::register::{satp, scause, time}` accessors used by this kernel.
- `spin`: MUST provide `Mutex` and `Lazy` used for globals and interior mutability in a `no_std` environment.

### Platform/SEE (Preconditions)
- The SEE MUST boot this binary in RISC-V S-mode with an SBI implementation that permits calls to `legacy::{console_putchar, console_getchar}` and `system_reset`.
- The runtime environment MUST provide a VirtIO Block MMIO device at address `0x1000_1000` with a capacity sufficient to host the `easy-fs` image containing `initproc` and any other programs used by tests.
- The hardware/SEE MUST allow enabling Sv39 translation via `satp` and MUST report user traps via `scause`, including `Exception::UserEnvCall`.

