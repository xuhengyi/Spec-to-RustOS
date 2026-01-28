# Capability: ch7

## Purpose

`ch7` is a RISC-V Supervisor-mode (S-mode) `#![no_std]` / `#![no_main]` kernel binary that:
- boots via `linker::boot0!`,
- initializes console/logging and heap,
- builds an Sv39 kernel address space (including a portal transit slot and MMIO),
- loads an `initproc` ELF from the embedded filesystem,
- runs user processes through a portal-based context execution loop,
- handles `ecall`-based syscalls and performs **temporary** post-syscall signal handling,
- shuts down via SBI `system_reset` on completion or panic.

## Requirements

### Requirement: Boot entry and basic initialization
The binary MUST provide a boot entry (via `linker::boot0!`) that transfers control to `rust_main`, and `rust_main` MUST:
- zero `.bss`,
- initialize `rcore_console` and set its log level from `LOG` (if provided),
- initialize `kernel_alloc` using the kernel layout and transfer remaining memory as heap.

#### Scenario: Normal boot
- **WHEN** the SEE transfers control to the kernel entry point
- **THEN** `rust_main` runs after `.bss` is zeroed
- **AND THEN** console logging is usable
- **AND THEN** heap allocation is available for subsequent initialization

### Requirement: Kernel address space construction (Sv39)
During boot, the kernel MUST construct an Sv39 address space that:
- identity maps all regions described by `linker::KernelLayout` with flags derived from region titles,
- maps a contiguous heap region from `layout.end()` up to `layout.start() + MEMORY` as writable,
- maps a single portal-transit page at `VPN::MAX` with the flags `__G_XWRV`,
- maps each range in `MMIO` as writable.
After mapping, the kernel MUST write `satp` to enable Sv39 using the new root page table.

#### Scenario: Mapping kernel regions and enabling paging
- **WHEN** `rust_main` calls the kernel-space setup routine
- **THEN** text/rodata/data/boot regions are mapped with their expected permissions
- **AND THEN** the heap region and MMIO ranges are mapped writable
- **AND THEN** Sv39 is enabled by setting `satp` to the new root PPN

### Requirement: Portal transit setup
During boot, the kernel MUST allocate enough memory for a `kernel_context::foreign::MultislotPortal` with one slot, and MUST initialize it as a transit portal at the virtual page `VPN::MAX`.

#### Scenario: Portal is available for foreign execution
- **WHEN** the kernel completes boot
- **THEN** a portal transit is initialized at `PROTAL_TRANSIT.base().val()`
- **AND THEN** the scheduler loop can execute a user process via `ForeignContext::execute`

### Requirement: Loading the initial user process
During boot, the kernel MUST open and read the file named `initproc` from the filesystem (via `FS.open(..., RDONLY)` and `read_all`) and MUST attempt to load it as an ELF executable.
If the ELF is valid, the kernel MUST add it to the process manager as the initial runnable process.

#### Scenario: `initproc` exists and is loadable
- **WHEN** `initproc` is present in the filesystem and is a valid RISC-V executable ELF
- **THEN** the kernel creates a `Process` from the ELF
- **AND THEN** the process is inserted into the `PROCESSOR` manager and becomes runnable

### Requirement: User execution loop and trap classification
The kernel MUST run a scheduling loop that selects the next process via the process manager and executes it via the portal.
After returning from `execute`, the kernel MUST classify the trap cause using `scause`.
The kernel MUST treat `UserEnvCall` as a syscall trap and MUST treat any other trap as unsupported and terminate the current process with a negative exit code.

#### Scenario: User makes a syscall
- **WHEN** a user process executes `ecall` in U-mode
- **THEN** the kernel observes `scause == UserEnvCall`
- **AND THEN** the kernel dispatches a syscall and eventually resumes or terminates the current process

#### Scenario: Unsupported trap
- **WHEN** the kernel returns from user execution with a trap cause other than `UserEnvCall`
- **THEN** the kernel logs an error
- **AND THEN** the kernel marks the current process as exited with a negative code

### Requirement: Syscall dispatch and return semantics
On `UserEnvCall`, the kernel MUST:
- advance the user context past the trapping instruction,
- decode the syscall ID from register `a7`,
- collect arguments from `a0..a5`,
- call `syscall::handle(Caller { entity: 0, flow: 0 }, id, args)`.
If the syscall result is `Done(ret)` and the syscall is not `EXIT`, the kernel MUST write `ret` back to `a0` and mark the current process as suspended.
If the syscall is `EXIT`, the kernel MUST mark the current process as exited with the exit code.
If the syscall is unsupported, the kernel MUST terminate the current process with an error code.

#### Scenario: Syscall returns to user
- **WHEN** a user process performs a supported syscall that completes successfully
- **THEN** the kernel stores the return value in `a0`
- **AND THEN** the process is suspended and becomes eligible to run again

### Requirement: Post-syscall signal handling (temporary placement)
After each syscall trap is handled, the kernel MUST invoke the current process’s signal handler logic (via `task.signal.handle_signals(ctx)`).
If signal handling indicates the process is killed, the kernel MUST mark the current process as exited with the corresponding exit code.

#### Scenario: Pending signal terminates a process
- **WHEN** a user process has a pending fatal signal at the end of a syscall
- **THEN** signal handling returns `ProcessKilled(exit_code)`
- **AND THEN** the kernel marks the process as exited with that code

### Requirement: Process creation from ELF
The kernel MUST be able to construct a `Process` from a RISC-V executable ELF such that:
- each `PT_LOAD` segment is mapped into the user address space with flags derived from ELF segment permissions and the `U`/`V` bits,
- a 2-page user stack is mapped at `VPN[(1<<26)-2 .. (1<<26))` with `U_WRV`,
- the portal transit mapping is present in the process’s page table,
- the user entry point is set from the ELF header,
- the initial user stack pointer is set to `1 << 38`,
- the process begins with at least `stdin` and `stdout` file descriptors.

#### Scenario: ELF process boots to user entry
- **WHEN** the kernel creates a process from a valid RISC-V executable ELF
- **THEN** the process address space contains mapped load segments and a user stack
- **AND THEN** the process context is configured to begin at the ELF entry with a valid user stack pointer

### Requirement: Process fork semantics
The kernel MUST support `fork` such that:
- the child receives a new PID,
- the parent address space is cloned into a fresh address space and includes the portal mapping,
- the child inherits a copy of the parent user context and file descriptor table,
- the child signal state is derived from the parent via `Signal::from_fork`,
- the syscall return value for the child is set to `0` in `a0`,
- the parent receives the child PID as the return value.

#### Scenario: Fork creates a runnable child
- **WHEN** a process invokes `fork`
- **THEN** a child process is created with a distinct PID
- **AND THEN** the child’s syscall return value is `0`
- **AND THEN** the parent receives the child PID

### Requirement: Filesystem and file-backed I/O
The kernel MUST provide a filesystem manager (`FS`) backed by `easy_fs::EasyFileSystem` over a block device.
The filesystem manager MUST support:
- open with `CREATE` (create if missing, otherwise clear),
- open with `TRUNC` (clear existing file),
- readdir on the root inode.
The kernel MUST implement basic read/write syscalls such that:
- `STDIN` reads bytes via SBI legacy `console_getchar`,
- `STDOUT` and `STDDEBUG` writes print bytes to console,
- file-backed descriptors read/write through `easy_fs` using translated user buffers.

#### Scenario: Read `initproc` from the filesystem
- **WHEN** the kernel opens `initproc` for read
- **THEN** `read_all` reads the inode contents to EOF
- **AND THEN** the resulting bytes can be passed to the ELF loader

### Requirement: Clock source for `clock_gettime`
The kernel MUST implement `clock_gettime` for `CLOCK_MONOTONIC` by writing a `TimeSpec` into user memory, derived from the `time` CSR with the crate’s conversion logic.

#### Scenario: User requests monotonic time
- **WHEN** a process calls `clock_gettime(CLOCK_MONOTONIC, tp)`
- **THEN** the kernel writes a `TimeSpec` to `tp` in user memory
- **AND THEN** the syscall returns `0`

### Requirement: VirtIO block device integration
The kernel MUST provide a block device at the MMIO base `0x1000_1000` and MUST expose it as an `easy_fs::BlockDevice`.
The VirtIO HAL MUST support DMA allocation/deallocation and MUST translate virtual to physical addresses using the kernel address space translation.

#### Scenario: Filesystem performs block I/O
- **WHEN** `easy_fs` issues a block read/write via the block device
- **THEN** the request is routed through `virtio-drivers` to the MMIO device
- **AND THEN** DMA buffers are allocated and translated as required by the VirtIO HAL

### Requirement: Shutdown and panic behavior
After the scheduler loop ends, the kernel MUST request shutdown via `sbi_rt::system_reset(Shutdown, NoReason)`.
On panic, the kernel MUST print panic info and MUST request shutdown via `system_reset(Shutdown, SystemFailure)`.

#### Scenario: Kernel finishes execution
- **WHEN** the scheduler loop ends with no runnable tasks
- **THEN** the kernel requests shutdown with `Shutdown` / `NoReason`

#### Scenario: Kernel panics
- **WHEN** the kernel panics
- **THEN** panic information is printed to the console
- **AND THEN** the kernel requests shutdown with `Shutdown` / `SystemFailure`

## Public API

This crate is a binary (`#![no_std]`, `#![no_main]`). Its externally visible interface is primarily via exported symbols and runtime side effects.

### Symbols
- `_start() -> !`: Boot entry symbol generated by `linker::boot0!` that eventually calls `rust_main`.

### Public items (Rust visibility)
- `PROCESSOR`: global process manager (declared `pub static mut`).
- `MMIO`: MMIO ranges that are mapped into the kernel address space.

## Build Configuration

### build.rs
- The build script MUST write a linker script named `linker.ld` into Cargo `OUT_DIR`, using `linker::SCRIPT`.
- The build script MUST pass `-T<OUT_DIR>/linker.ld` to the linker via `cargo:rustc-link-arg`.
- The build script MUST request rebuild when `build.rs` changes via `cargo:rerun-if-changed=build.rs`.
- The build script MUST request rebuild when `LOG` or `APP_ASM` changes via `cargo:rerun-if-env-changed`.

#### Scenario: Cargo build emits and uses the linker script
- **WHEN** Cargo builds `ch7`
- **THEN** `build.rs` writes `<OUT_DIR>/linker.ld`
- **AND THEN** the final link uses that linker script via `-T.../linker.ld`

### Environment variables
- `OUT_DIR`: MUST be provided by Cargo for `build.rs`; used as the output location for `linker.ld`.
- `LOG`: MAY be provided; used at runtime to set log level and triggers rebuild when changed.
- `APP_ASM`: MAY be provided; triggers rebuild when changed.

### Generated files
- `<OUT_DIR>/linker.ld`: Linker script produced from `linker::SCRIPT`.

## Dependencies

### Workspace crates (Preconditions)
- `linker`:
  - MUST provide `boot0!` to define the boot entry and stack reservation.
  - MUST provide `KernelLayout::locate()` and `KernelLayout::iter()` describing mapped kernel regions.
  - MUST provide `KernelLayout::zero_bss()` to clear `.bss`.
  - MUST provide `SCRIPT` used by `build.rs` to generate `linker.ld`.
- `rcore-console` (`console`):
  - MUST provide `init_console`, `set_log_level`, and logging/printing macros used by the kernel.
- `kernel-alloc`:
  - MUST provide `init(start)` and `transfer(&mut [u8])` to establish heap allocation.
- `kernel-vm`:
  - MUST provide `AddressSpace` mapping and translation for Sv39 and `VmFlags` parsing/building.
  - MUST provide `PageManager` traits used by the local `Sv39Manager` implementation.
- `kernel-context`:
  - MUST provide `foreign::MultislotPortal` and `foreign::ForeignContext::execute`.
  - MUST provide `LocalContext::user(entry)` to create a user-mode context.
- `rcore-task-manage` (`task-manage`):
  - MUST provide `PManager` and `ProcId` plus scheduling/management traits used by the kernel loop.
- `syscall`:
  - MUST provide `init_io/init_process/init_scheduling/init_clock/init_signal`.
  - MUST provide `handle(caller, id, args)` and syscall ID/result types used by dispatch.
  - MUST define the syscall trait set (`IO`, `Process`, `Scheduling`, `Clock`, `Signal`) implemented by this crate.
- `easy-fs`:
  - MUST provide `EasyFileSystem`, inode/file-handle abstractions, `BlockDevice`, and buffer types.
- `signal`:
  - MUST define signal numbers (`SignalNo`), constants (`MAX_SIG`), result types (`SignalResult`), and the `Signal` trait.
- `signal-impl`:
  - MUST provide `SignalImpl` implementing the `signal::Signal` trait and methods required by this kernel (`add_signal`, `handle_signals`, `from_fork`, `set_action`, `update_mask`, `sig_return`, etc.).

### External crates
- `sbi-rt` (feature: `legacy`): MUST provide `system_reset` and legacy console I/O used for stdin/stdout.
- `virtio-drivers`: MUST provide `VirtIOBlk`, `VirtIOHeader`, and `Hal` required for block device I/O.
- `xmas-elf`: MUST provide ELF parsing used for loading `initproc` and exec targets.
- `riscv`: MUST provide CSR access (`scause`, `satp`, `time`) used by trap handling, paging enablement, and time.
- `spin`: MUST provide `Lazy` and `Mutex` used for globals and file handles.

### Platform/SEE (Preconditions)
- The SEE MUST boot the kernel at the entry point defined by the generated linker script.
- The runtime environment MUST permit SBI calls from S-mode for legacy console I/O and `system_reset`.
- A VirtIO block device MUST be present at MMIO base `0x1000_1000` for filesystem access.

