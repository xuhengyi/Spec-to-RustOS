# Capability: ch5

## Purpose

`ch5` is a RISC-V Supervisor-mode (S-mode) `#![no_std]`, `#![no_main]` kernel binary that initializes a Sv39 kernel address space, embeds a set of user applications, loads an initial `initproc` process from ELF, and runs a minimal process scheduler with trap handling and syscall dispatch.

## Requirements

### Requirement: Build-time linker script generation
The build script MUST write a linker script named `linker.ld` into Cargo `OUT_DIR` using the exact bytes of `linker::SCRIPT`, and MUST pass it to the linker via `cargo:rustc-link-arg=-T<OUT_DIR>/linker.ld`.

#### Scenario: Cargo build emits and uses the linker script
- **WHEN** Cargo builds `ch5`
- **THEN** `build.rs` writes `<OUT_DIR>/linker.ld` with content equal to `linker::SCRIPT`
- **AND THEN** the final link uses that linker script via `-T.../linker.ld`

### Requirement: App embedding via assembly include
The kernel binary MUST embed user applications by including an assembly source referenced by the `APP_ASM` environment variable at compile time.

#### Scenario: Build provides an app assembly bundle
- **WHEN** `APP_ASM` resolves to a valid assembly source path at compile time
- **THEN** the kernel image contains the app metadata and app payload referenced by that assembly

### Requirement: Boot and early initialization
On entering the kernel main routine, the kernel MUST:
- Zero the `.bss` region described by `linker::KernelLayout`
- Initialize the console backend and configure log level from `LOG` (if present)
- Initialize the kernel heap and transfer remaining RAM (up to a fixed `MEMORY` capacity) into the allocator

#### Scenario: Kernel initializes memory and console
- **WHEN** the kernel enters `rust_main`
- **THEN** `.bss` is zeroed before heap allocations are used
- **AND THEN** console output is available via the configured console backend

### Requirement: Kernel address space setup (Sv39)
The kernel MUST construct a Sv39 address space that maps:
- All kernel regions returned by `linker::KernelLayout::iter()` with permissions derived from region title
- A heap region covering the remaining RAM within the fixed `MEMORY` capacity
- A single “portal transit” virtual page used for foreign-context execution

The kernel MUST activate the constructed address space by writing `satp` with Sv39 mode and the root page-table PPN.

#### Scenario: Kernel enables paging for the main loop
- **WHEN** early initialization completes
- **THEN** the kernel constructs a page table that covers kernel text/rodata/data/boot and heap memory
- **AND THEN** `satp` is set to Sv39 with the new root page-table PPN before scheduling begins

### Requirement: Portal mapping for user address spaces
For any user process address space created or cloned by this crate, the kernel MUST map the portal transit page into that address space by copying the portal PTE from the kernel root page table.

#### Scenario: User execution has a portal mapping
- **WHEN** a user process is created from ELF or cloned via `fork`
- **THEN** the process address space contains a valid mapping at the portal transit VPN
- **AND THEN** foreign-context execution may use that portal to enter/exit user mode

### Requirement: Embedded application registry
The kernel MUST expose an in-kernel application registry derived from `linker::AppMeta::locate()` and an `app_names` string table, mapping application names to their embedded bytes.

#### Scenario: App registry enumerates embedded apps by name
- **WHEN** the kernel initializes the application registry
- **THEN** each `AppMeta` payload is associated with exactly one NUL-terminated name from the `app_names` table

### Requirement: Initial process loading
At boot, the kernel MUST attempt to load an application named `initproc` from the embedded application registry. If the `initproc` ELF is valid, the kernel MUST create a `Process` from it and insert it into the process manager.

#### Scenario: `initproc` is present and valid
- **WHEN** the embedded registry contains an `initproc` entry containing a valid RISC-V executable ELF
- **THEN** the kernel constructs a `Process` from that ELF and schedules it for execution

### Requirement: Scheduler loop and context execution
The kernel MUST repeatedly:
- Select a runnable process (if any) from the process manager
- Execute the process via its foreign context
- Handle the resulting trap cause

If no runnable process exists, the kernel MUST stop the loop and request shutdown via SBI `system_reset`.

#### Scenario: No runnable tasks remain
- **WHEN** the process manager reports no next runnable process
- **THEN** the kernel prints an informational message and exits the scheduling loop
- **AND THEN** the kernel requests shutdown via SBI `system_reset(Shutdown, NoReason)`

### Requirement: Trap handling and syscall dispatch
If a trap occurs due to a user environment call, the kernel MUST:
- Advance the saved user PC to the next instruction
- Decode syscall ID from `a7` and arguments from `a0..a5`
- Dispatch via `syscall::handle`

For non-EXIT syscalls that complete successfully, the kernel MUST place the return value into user register `a0` and suspend the current task. For the EXIT syscall, the kernel MUST mark the current task as exited with the returned code. Unsupported syscalls MUST terminate the current task with exit code `-2`. Unsupported non-syscall traps MUST terminate the current task with exit code `-3`.

#### Scenario: Syscall returns to user code
- **WHEN** a user process executes `ecall` and `syscall::handle` returns `Done(ret)`
- **THEN** the kernel writes `ret` into user register `a0` (except for `EXIT`)
- **AND THEN** the current process is suspended (or marked exited for `EXIT`)

### Requirement: Syscall surface provided by this crate
This crate MUST provide syscall backends for:
- IO: `read` and `write`
- Process: `exit`, `fork`, `exec`, `wait`, `getpid`
- Scheduling: `sched_yield`
- Clock: `clock_gettime(CLOCK_MONOTONIC)`

#### Scenario: `exec` fails for unknown app names
- **WHEN** a process calls `exec` with a name not present in the embedded application registry
- **THEN** the kernel returns `-1` and prints a list of available app names

### Requirement: Panic handling
On panic, the kernel MUST print the panic info via the console and MUST request shutdown via `sbi_rt::system_reset(Shutdown, SystemFailure)`, and MUST NOT return.

#### Scenario: Panic path shutdown
- **WHEN** a panic occurs anywhere in the kernel
- **THEN** the panic handler prints the panic info
- **AND THEN** it calls `system_reset` with `Shutdown` and `SystemFailure`

## Public API

This crate is a binary (`#![no_std]`, `#![no_main]`). Its externally visible interface is via exported symbols expected by the linker/SEE and by embedded-application metadata conventions.

### Symbols
- `_start() -> !`: Supervisor entry symbol provided via `linker::boot0!(rust_main; ...)` and the generated linker script.

### Internal (Rust) items
These `pub` items exist within the crate sources but are not exposed as a stable library API (because this crate is not a library):
- `process::Process`: Process object holding `pid`, `ForeignContext`, and a Sv39 address space.
- `processor::PROCESSOR`: Global process manager (`PManager<Process, ProcManager>`).
- `processor::ProcManager`: Task store and ready-queue implementation.
- `impls::Sv39Manager`: Sv39 page-table allocator/manager used by `kernel-vm` address spaces.
- `impls::Console`: `rcore-console` backend using SBI legacy console putchar.
- `impls::SyscallContext`: Syscall backend implementing IO/process/scheduling/clock traits from `syscall`.

## Build Configuration

### build.rs
- The build script MUST write `<OUT_DIR>/linker.ld` and MUST pass `-T<OUT_DIR>/linker.ld` via `cargo:rustc-link-arg`.
- The build script MUST request rebuild when `build.rs` changes via `cargo:rerun-if-changed=build.rs`.
- The build script MUST request rebuild when `LOG` or `APP_ASM` changes via `cargo:rerun-if-env-changed`.

#### Scenario: Env var changes trigger rebuild
- **WHEN** `LOG` or `APP_ASM` changes between builds
- **THEN** Cargo rebuilds `ch5` to reflect the new configuration

### Environment variables
- `OUT_DIR`: MUST be provided by Cargo during build-script execution; used for generated `linker.ld`.
- `LOG`: MAY be provided; used to configure runtime log level, and also triggers rebuild when changed.
- `APP_ASM`: MUST be provided at compile time; points to the assembly source included via `include_str!`.

### Generated files
- `<OUT_DIR>/linker.ld`: Linker script written by `build.rs` (content equals `linker::SCRIPT`).

## Dependencies

### Workspace crates (Preconditions)
- `linker`:
  - MUST provide `SCRIPT: &[u8]` for `build.rs`.
  - MUST provide `boot0!` macro to define the supervisor entry sequence that transfers control to `rust_main`.
  - MUST provide `KernelLayout::locate()`, `KernelLayout::{start,end,len,iter}`, and `KernelLayout::zero_bss()`.
  - MUST provide `AppMeta::locate()` and an app-name string table contract compatible with `app_names`.
- `rcore-console`:
  - MUST provide `init_console`, `set_log_level`, `test_log`, and logging macros used by the kernel.
  - MUST accept a backend implementing `rcore_console::Console`.
- `kernel-alloc`:
  - MUST provide `init(heap_start)` and `transfer(&mut [u8])` as used to initialize the allocator with remaining RAM.
- `kernel-context` (feature: `foreign`):
  - MUST provide `foreign::MultislotPortal` for portal sizing and initialization.
  - MUST provide `foreign::ForeignContext` and `LocalContext::user(entry)` to create a user execution context.
- `kernel-vm`:
  - MUST provide `AddressSpace` with `map`, `map_extern`, `translate`, and `cloneself`.
  - MUST provide Sv39 page-table types and flag parsing used by this crate (`VmFlags::build_from_str`, etc.).
- `syscall` (feature: `kernel`):
  - MUST provide `init_io`, `init_process`, `init_scheduling`, `init_clock`.
  - MUST provide `handle(Caller, SyscallId, args)` returning `SyscallResult`.
  - MUST define the trait surfaces implemented by `SyscallContext` (IO/Process/Scheduling/Clock) and constants (e.g., `STDIN`, `STDOUT`, `STDDEBUG`).
- `rcore-task-manage` (path: `task-manage`, feature: `proc`):
  - MUST provide `ProcId` creation/conversion used by this crate.
  - MUST provide `PManager` methods used by this crate: `set_manager`, `add`, `find_next`, `current`, `make_current_suspend`, `make_current_exited`, and `wait`.

### External crates
- `sbi-rt` (feature: `legacy`): MUST provide legacy console IO (`legacy::console_putchar`, `legacy::console_getchar`) and `system_reset` with `Shutdown`, `NoReason`, and `SystemFailure`.
- `xmas-elf`: MUST parse ELF inputs and expose program headers used for `LOAD` segment mapping and entry-point discovery.
- `riscv`: MUST provide CSR access used for `scause` decoding and `time::read`.
- `spin`: MUST provide `Lazy` used for initializing the embedded app registry.

### Platform/SEE (Preconditions)
- The SEE MUST enter the binary at the entry symbol produced by `linker::boot0!` and the generated linker script.
- The platform MUST support Sv39 translation and permit writing `satp` in S-mode.
- SBI MUST be available for legacy console IO and `system_reset`.
