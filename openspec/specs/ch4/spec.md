# Capability: ch4

`ch4` is a `#![no_std]` RISC-V (Sv39) kernel *binary* that boots via the workspace `linker` crate, initializes basic runtime services (console/log/heap), constructs a kernel address space, loads embedded user applications from ELF images, and runs them with a minimal syscall/trap handling loop.

## Purpose

This capability specifies the externally observable contracts of the `ch4` kernel binary: how it boots, constructs memory mappings, loads embedded applications, handles traps/syscalls, and shuts down.

## Requirements

### Requirement: Boot and runtime initialization
The kernel MUST, during boot, initialize its BSS, console/logging, and heap allocator before attempting to create address spaces or load applications.

#### Scenario: Normal boot sequence
- **WHEN** the kernel enters `rust_main` as the boot entry
- **THEN** it MUST zero the BSS region described by `linker::KernelLayout`
- **AND THEN** it MUST initialize `rcore_console` using a console backend that emits bytes via SBI legacy console output
- **AND THEN** it MUST configure log level from the `LOG` environment variable (if present) and emit the console test log
- **AND THEN** it MUST initialize the kernel heap and transfer the remaining memory range (up to the configured `MEMORY` capacity) into the allocator

### Requirement: Kernel address space construction (Sv39)
The kernel MUST construct an Sv39 page-table-based kernel address space that maps:
- the kernel’s own regions described by `linker::KernelLayout`,
- a heap region backed by the remaining physical memory capacity, and
- a “portal transit” virtual page used by `kernel_context::foreign::MultislotPortal`.

#### Scenario: Kernel mapping and SATP installation
- **WHEN** `kernel_space(layout, memory, portal)` is invoked during boot
- **THEN** it MUST map each `layout` region with permissions derived from its title (`Text`, `Rodata`, `Data`, `Boot`)
- **AND THEN** it MUST map the remaining heap physical range as writable kernel memory
- **AND THEN** it MUST map the portal transit virtual page with global, user-accessible permissions (as configured by the implementation)
- **AND THEN** it MUST install the page table via `satp::set` in Sv39 mode before returning

### Requirement: Embedded application discovery and ELF loading
The kernel MUST discover embedded application images and attempt to load each as a user process by parsing it as a RISC-V executable ELF and mapping its loadable segments into a fresh user address space.

#### Scenario: Load a valid RISC-V executable ELF into a process
- **WHEN** `linker::AppMeta::locate().iter()` yields an application byte slice that is a valid ELF file
- **THEN** the kernel MUST accept it only if its header indicates `Type::Executable` and `Machine::RISC_V`
- **AND THEN** it MUST map each `PT_LOAD` segment into the process address space with user (`U`) and valid (`V`) permissions, plus `X/W/R` as indicated by ELF segment flags
- **AND THEN** it MUST allocate and map a 2-page user stack at the configured top-of-user-stack VPN range with user writable permissions
- **AND THEN** it MUST create a user-mode execution context whose entry point is the ELF entry address and whose stack pointer is the configured user stack top

### Requirement: Portal mapping into processes
For every successfully created process, the kernel MUST map the portal transit page into the process by copying the kernel address space portal PTE entry into the process root page table at the portal VPN index.

#### Scenario: Process portal mapping
- **WHEN** a process has been created and the kernel address space has been constructed
- **THEN** the kernel MUST copy the portal PTE entry (at the portal VPN index) from the kernel address space root into the process address space root
- **AND THEN** the process context MUST be executable through `kernel_context`’s foreign context mechanism using that portal transit mapping

### Requirement: Scheduling loop and trap/syscall handling
The kernel MUST run processes sequentially and handle user-mode traps by dispatching syscalls via the `syscall` crate. Unsupported syscalls or unsupported traps MUST terminate (remove) the current process.

#### Scenario: Handle supported syscall and continue
- **WHEN** a process traps with `UserEnvCall`
- **AND** the syscall ID is supported and the syscall handler returns `SyscallResult::Done(ret)`
- **AND** the syscall ID is not `EXIT`
- **THEN** the kernel MUST write `ret` to the process return register, advance the user PC to the next instruction, and resume execution of the same process later

#### Scenario: Handle `EXIT` syscall and remove process
- **WHEN** a process traps with `UserEnvCall`
- **AND** the syscall ID is `EXIT`
- **AND** the syscall handler returns `SyscallResult::Done(_)`
- **THEN** the kernel MUST remove the current process from the process list

#### Scenario: Unsupported syscall terminates process
- **WHEN** a process traps with `UserEnvCall`
- **AND** the syscall handler returns `SyscallResult::Unsupported(_)`
- **THEN** the kernel MUST remove the current process from the process list

#### Scenario: Unsupported trap terminates process
- **WHEN** a process traps with any cause other than `UserEnvCall`
- **THEN** the kernel MUST log the trap cause, `stval`, and the process PC
- **AND THEN** it MUST remove the current process from the process list

#### Scenario: Shutdown after all processes exit
- **WHEN** the process list becomes empty
- **THEN** the kernel MUST request system shutdown via SBI system reset

### Requirement: Minimal syscall host implementation (I/O, scheduling, clock)
The kernel MUST provide a syscall host context that supports at least:
- `write` to `STDOUT` and `STDDEBUG` by translating user pointers into readable kernel pointers and printing UTF-8 bytes, and
- `clock_gettime(CLOCK_MONOTONIC)` by translating a user pointer and writing a `TimeSpec`.

#### Scenario: `write` prints user buffer to console
- **WHEN** a process invokes `write(fd, buf, count)` with `fd` equal to `STDOUT` or `STDDEBUG`
- **THEN** the kernel MUST translate `(buf, count)` as a readable user memory region
- **AND THEN** it MUST print the bytes to the console
- **AND THEN** it MUST return `count` on success

#### Scenario: `write` rejects unsupported file descriptors
- **WHEN** a process invokes `write(fd, buf, count)` with an unsupported `fd`
- **THEN** the kernel MUST return a negative value and log an error

#### Scenario: `clock_gettime` writes monotonic time
- **WHEN** a process invokes `clock_gettime(CLOCK_MONOTONIC, tp)`
- **THEN** the kernel MUST translate `tp` as a writable user memory location
- **AND THEN** it MUST write a monotonic time value into `*tp` as a `TimeSpec`
- **AND THEN** it MUST return 0 on success

### Requirement: Panic behavior
On panic, the kernel MUST log the panic info and request system shutdown (system failure) via SBI system reset.

#### Scenario: Kernel panic
- **WHEN** the panic handler is invoked
- **THEN** it MUST log the panic info
- **AND THEN** it MUST request system shutdown with a failure reason

## Public API

This crate is a `bin` crate (`#![no_main]`) and does **not** provide a stable Rust library API for external consumers. Its externally relevant interfaces are primarily:

### Linker / symbol-level entry points
- `rust_main`: kernel boot entry used by `linker::boot0!`.

### Environment / build-time interfaces
- `APP_ASM` (env var): MUST point to an assembly source file path embedded via `include_str!(env!("APP_ASM"))`.
- `LOG` (env var): MAY set the runtime log level via `rcore_console::set_log_level(option_env!("LOG"))`.

## Build Configuration

- **build.rs**:
  - MUST write `linker::SCRIPT` to `$OUT_DIR/linker.ld`.
  - MUST pass `-T$OUT_DIR/linker.ld` to the Rust compiler linker args.
  - MUST request rebuild when `build.rs` changes.
  - MUST request rebuild when environment variables `LOG` or `APP_ASM` change.
- **Environment variables**:
  - `OUT_DIR` (provided by Cargo): used as the output directory for generated `linker.ld`.
  - `LOG`: see Public API.
  - `APP_ASM`: see Public API.
- **Generated files**:
  - `$OUT_DIR/linker.ld`

## Dependencies

### Workspace crates (preconditions)

The `ch4` crate depends on the following workspace crates; they MUST provide the described symbols/semantics:

- **`linker`**:
  - MUST provide `SCRIPT: &str` (linker script content) used by `build.rs`.
  - MUST provide `boot0!(...)` macro that wires the boot entry to `rust_main` and sets up a boot stack.
  - MUST provide `KernelLayout::locate()` and `KernelLayout::{start,end,len,iter,zero_bss}` to locate kernel memory regions and zero BSS.
  - MUST provide `KernelRegionTitle` variants (`Text`, `Rodata`, `Data`, `Boot`) as used for permission selection.
  - MUST provide `AppMeta::locate()` to enumerate embedded application images as byte slices.

- **`rcore-console`** (path `../console`):
  - MUST provide `init_console`, `set_log_level`, `test_log`, and logging/printing macros (`print!`, `println!`, `log::*`) compatible with `#![no_std]`.
  - MUST define a `Console` trait with `put_char(u8)` used for backend output.

- **`kernel-alloc`**:
  - MUST provide `init(heap_start)` and `transfer(&mut [u8])` to initialize and extend the heap allocator.

- **`kernel-vm`**:
  - MUST provide `AddressSpace<Sv39, Mgr>` with `new`, `map`, `map_extern`, `translate`, `root`, `root_ppn`.
  - MUST provide `page_table` types `Sv39`, `VPN`, `PPN`, `VAddr`, `VmFlags`, and `MmuMeta` (page size constants and helpers).
  - `VmFlags` MUST support construction from strings via `build_from_str` / `FromStr`.
  - MUST provide `PageManager<Sv39>` trait used by `Sv39Manager`.

- **`kernel-context`**:
  - MUST provide `LocalContext::{thread,user}` to build kernel and user contexts.
  - MUST provide `foreign::{ForeignContext, MultislotPortal}` and allow `ForeignContext::execute(portal, ())` style execution switching.
  - `MultislotPortal` MUST provide `calculate_size(n)` and `init_transit(base, n)`.

- **`syscall`**:
  - MUST provide initialization functions: `init_io`, `init_process`, `init_scheduling`, `init_clock`.
  - MUST provide `handle(Caller, SyscallId, [usize; 6]) -> SyscallResult`.
  - MUST provide `Caller { entity, flow }`, `SyscallId`, `SyscallResult::{Done, Unsupported}`.
  - MUST provide syscall-domain traits and constants used by the host context, including at least:
    - `IO::write`, `Process::exit`, `Scheduling::sched_yield`, `Clock::clock_gettime`
    - `STDOUT`, `STDDEBUG`, `ClockId`, `TimeSpec`

### External crates

- **`sbi-rt`**:
  - MUST provide SBI system reset (`system_reset`) and legacy console output (`legacy::console_putchar`).
- **`riscv`**:
  - MUST provide access to RISC-V CSRs/registers used by the kernel (`satp`, `stval`, `scause`, `time`).
- **`xmas-elf`**:
  - MUST parse ELF64 and provide access to headers and program segments used for loading.

