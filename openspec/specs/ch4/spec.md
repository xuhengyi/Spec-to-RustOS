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

### Requirement: Boot assembly sequence
The kernel MUST enter execution via a minimal assembly boot sequence that establishes a valid stack before transferring control to the high-level entry.

#### Scenario: Boot entry and stack setup
- **WHEN** the machine starts and control reaches the boot entry point
- **THEN** the boot sequence MUST load the stack pointer with the address of the end of the kernel image (or a dedicated boot stack region)
- **AND THEN** it MUST jump directly to the high-level entry (`rust_main`) without further setup
- **AND** the boot stack MUST be large enough to support all boot-time call frames

#### Scenario: Boot stack placement
- **WHEN** the linker script defines the boot layout
- **THEN** the boot stack MUST reside in a `.boot.stack` section placed after BSS
- **AND** the stack pointer MUST point to the high end of this region (stack grows down)

### Requirement: Scheduling thread and context switching
The kernel MUST establish a dedicated scheduling thread that runs in kernel mode and performs context switches to user processes. The switch MUST preserve the scheduler's register state and restore it when returning from user execution.

#### Scenario: Scheduling thread creation
- **WHEN** the kernel is ready to run user processes
- **THEN** it MUST create a kernel-mode thread context with entry point at the scheduling function
- **AND** the scheduling thread MUST use a dedicated kernel stack (separate from the boot stack)
- **AND** the stack pointer for this thread MUST be set to the top of the scheduling stack region

#### Scenario: Context switch to user process
- **WHEN** the scheduler invokes execution of a user process context
- **THEN** the switch mechanism MUST save all general-purpose registers of the current (scheduler) context to memory
- **AND** it MUST set `stvec` to a trap handler that will restore the scheduler context on trap
- **AND** it MUST use `sscratch` to hold a pointer to the context storage (for trap-time recovery)
- **AND** it MUST load the user context's registers and transfer control via `sret`

#### Scenario: Trap return to scheduler
- **WHEN** the user process traps (e.g., syscall, fault)
- **THEN** the trap handler MUST use `sscratch` to locate the scheduler's saved context
- **AND** it MUST restore the scheduler's general-purpose registers from that storage
- **AND** it MUST return to the scheduler loop (not to user space) so the scheduler can inspect trap cause and dispatch

### Requirement: Foreign context execution and address space switching
The kernel MUST execute user processes in their own address spaces. The switch from kernel address space to user address space MUST occur through a "portal" mechanism: a code page mapped identically in both kernel and user address spaces, so that execution can continue correctly across the `satp` change.

#### Scenario: Portal page identity mapping
- **WHEN** the kernel constructs address spaces
- **THEN** it MUST allocate a physical page for the portal (code + per-slot cache structures)
- **AND** it MUST map this physical page at the same virtual address in both the kernel address space and every user address space
- **AND** the portal virtual address MUST be chosen such that it occupies a distinct top-level VPN (e.g., `VPN::MAX` in Sv39) so a single PTE copy suffices per process

#### Scenario: Portal transit sequence (outbound)
- **WHEN** the scheduler invokes `ForeignContext::execute` for a user process
- **THEN** the implementation MUST prepare a cache structure (in the portal page) with: target `satp`, target `sepc`, target `sstatus`, and target `a0`/`a1`
- **AND** it MUST set the current context's PC to the portal code entry and `a0` to the cache address
- **AND** it MUST invoke the normal thread switch (which will `sret` into the portal code)
- **AND** the portal code MUST: save `a1`; swap `satp` with the value in the cache and issue `sfence.vma`; load `sstatus` and `sepc`; swap `stvec` and `sscratch` with cache; load `a0`/`a1` from cache; then `sret` into user space

#### Scenario: stvec MUST point to portal trap before sret to user (critical)
- **WHEN** the portal code is about to `sret` into user space
- **THEN** `stvec` MUST already be set to the portal's trap handler address (the label that will handle user traps)
- **AND** `sscratch` MUST be set to the cache address (so the trap handler can find the cache)
- **RATIONALE**: If `stvec` is not updated, user traps will jump to the kernel's trap handler address, which is invalid or unmapped in the user address space, causing a secondary fault or hang

#### Scenario: Portal transit sequence (inbound, on trap)
- **WHEN** the user process traps and `stvec` points to the portal's trap handler
- **THEN** the trap handler MUST use `sscratch` to find the cache (it was set to the cache address before `sret`)
- **AND** it MUST save the current `a0` into the cache
- **AND** it MUST swap `satp` back to the kernel value (from cache), issue `sfence.vma`, and restore `stvec`/`sscratch`
- **AND** it MUST jump to the kernel's trap vector (the original `stvec`) so the normal trap handling continues in kernel address space

#### Scenario: Trap return chain (no separate __trap_handler)
- **WHEN** the portal trap handler restores kernel space and jumps to the "original stvec"
- **THEN** the target MUST be the trap handler installed by the thread switch (e.g., `execute_naked`'s trap label), NOT a separate `__trap_handler` or other routine
- **AND** that trap handler MUST expect `sscratch` to hold the user context pointer (the structure used to save/restore the user's registers)
- **AND** that trap handler MUST restore the scheduler's registers and return to the caller of `execute()`
- **RATIONALE**: The portal jumps directly to the thread-switch trap handler; there is no intermediate handler. A separate `__trap_handler` that does not match the expected `sscratch`/context layout will cause corruption or hang

#### Scenario: Sv39 address space switch invariants
- **WHEN** changing `satp` (in either direction)
- **THEN** the implementation MUST execute `sfence.vma` (or equivalent) immediately after the `satp` write to ensure TLB consistency
- **AND** the portal code and cache MUST be accessible in both address spaces at the same virtual address for the switch to be safe

### Requirement: User program expectations and execute() return
The kernel assumes that user programs will eventually trap (e.g., via `ecall` for syscalls or `exit`). `execute()` returns only when the user process traps. If the user program never traps, `execute()` will not return.

#### Scenario: First user trap is the normal path
- **WHEN** a user program runs (e.g., `_start` → `heap::init()` → `main()` → `println!` → `ecall` for write)
- **THEN** the first trap is typically an `ecall` from the first syscall (write, exit, etc.)
- **AND** the kernel MUST have `stvec` pointing to the portal trap handler so the trap is correctly routed back to the scheduler

#### Scenario: execute() hangs when user never traps
- **WHEN** the user program enters an infinite loop before any `ecall`, or the entry point is wrong
- **THEN** `execute()` will not return
- **DEBUGGING**: Check user `_start`/`heap::init()` for infinite loops; verify ELF entry point and user stack are correct

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
  - MUST provide `LocalContext::execute` that saves scheduler registers, sets `stvec`/`sscratch`, loads target context, and `sret`s; on trap, the `stvec` handler MUST restore scheduler registers and return to the caller.
  - MUST provide `LocalContext` with `sp_mut`, `pc`, `pc_mut`, `a`, `a_mut`, `move_next` for register access.
  - MUST provide `foreign::{ForeignContext, MultislotPortal}` and allow `ForeignContext::execute(portal, ())` style execution switching.
  - `ForeignContext` MUST hold `context: LocalContext` and `satp: usize`; `execute` MUST prepare a portal cache with target satp/sepc/sstatus, set PC to portal entry and a0 to cache address, then invoke the thread switch.
  - `MultislotPortal` MUST provide `calculate_size(n)` and `init_transit(base, n)`; `init_transit` MUST copy position-independent portal code to the given base and return a reference to the initialized portal.

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

