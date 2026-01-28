# Capability: ch2

## Purpose

`ch2` is a minimal RISC-V Supervisor-mode (S-mode) batch-processing kernel binary. It embeds one or more user applications as assembly (selected via `APP_ASM`), iterates them sequentially, handles traps for `ecall` to dispatch system calls, and requests shutdown via SBI when all applications finish.

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
- **WHEN** `rust_main` starts executing
- **THEN** the kernel initializes `rcore_console` with a console backend that writes bytes via SBI legacy console output
- **AND THEN** subsequent `print!/println!` and `log::*` output is routed to that backend

### Requirement: Syscall subsystem initialization
The kernel MUST initialize the syscall subsystem with an implementation of `syscall::IO` and `syscall::Process` before executing any user application.

#### Scenario: Syscall handlers are ready before the first app executes
- **WHEN** `rust_main` reaches syscall initialization
- **THEN** it invokes both `syscall::init_io(...)` and `syscall::init_process(...)`
- **AND THEN** user `ecall` traps can be dispatched via `syscall::handle(...)`

### Requirement: Embedded user application image selection
The kernel MUST embed user application images via inline assembly included from the compile-time environment variable `APP_ASM`.

#### Scenario: Build selects a concrete user application assembly bundle
- **WHEN** the kernel is compiled with `APP_ASM` set
- **THEN** the assembly text at that path is included into the final kernel binary via `global_asm!(include_str!(env!("APP_ASM")))`

### Requirement: Application enumeration and batch execution order
The kernel MUST enumerate embedded applications via `linker::AppMeta::locate().iter()` and MUST execute applications sequentially in that iteration order.

#### Scenario: Multiple apps are executed one by one
- **WHEN** the embedded application bundle contains N applications
- **THEN** the kernel iterates the N `AppMeta` records in order
- **AND THEN** each application is executed to completion (exit or killed) before the next one begins

### Requirement: Per-application context creation and stack provisioning
For each application, the kernel MUST create a user context with the application base address as the initial user entry.
The kernel MUST provision a per-application user stack and MUST set the user context stack pointer to the top of that stack before first execution of the application.

#### Scenario: App begins executing with a valid user stack
- **WHEN** the kernel begins executing application i
- **THEN** it creates `LocalContext::user(app_base)`
- **AND THEN** it sets the user stack pointer to the top of a stack region reserved for that application before `ctx.execute()`

### Requirement: Trap handling and application termination policy
After starting an application, the kernel MUST repeatedly enter user execution and handle traps until the application either exits via `SYS_exit` or is terminated due to an unexpected trap.
For any trap other than `UserEnvCall`, the kernel MUST treat the application as killed.

#### Scenario: Non-syscall trap kills the app
- **WHEN** an application traps with a cause other than `Exception::UserEnvCall`
- **THEN** the kernel logs an error describing the trap
- **AND THEN** the kernel terminates that application and continues with the next application (if any)

### Requirement: Syscall dispatch ABI (register convention)
On a `UserEnvCall` trap, the kernel MUST interpret the syscall ID from register `a7` and syscall arguments from registers `a0..a5`.
The kernel MUST dispatch the syscall by invoking `syscall::handle(Caller { entity: 0, flow: 0 }, id, args)`.

#### Scenario: Syscall ID and arguments are read from the user context
- **WHEN** an application executes `ecall` from U-mode
- **THEN** the kernel reads the syscall ID from `ctx.a(7)` and arguments from `ctx.a(0)..ctx.a(5)`
- **AND THEN** it calls into the syscall subsystem with those values

### Requirement: Syscall return-value and exit semantics
The kernel MUST implement syscall completion, return-value propagation, and termination semantics as specified below.
If `syscall::handle(...)` returns `SyscallResult::Done(ret)`:
- For `SYS_exit`, the kernel MUST treat the application as exited and MUST report the exit code taken from user register `a0`.
- For any other syscall, the kernel MUST write `ret` to user register `a0`, MUST advance the user instruction pointer to the next instruction, and MUST continue executing the application.
If `syscall::handle(...)` returns `SyscallResult::Unsupported(id)`, the kernel MUST log an error and MUST terminate the application.

#### Scenario: Non-exit syscall returns to user and continues
- **WHEN** `ecall` traps into the kernel for a supported, non-exit syscall
- **THEN** the kernel writes the returned value to `a0`
- **AND THEN** it advances the user context to the next instruction and resumes user execution

#### Scenario: Exit syscall terminates the app
- **WHEN** `ecall` traps into the kernel for `SYS_exit`
- **THEN** the kernel treats the app as finished and records the exit code from `a0`
- **AND THEN** the kernel does not resume that app again

#### Scenario: Unsupported syscall terminates the app
- **WHEN** an application calls a syscall ID not supported by `syscall::handle`
- **THEN** the kernel logs an "unsupported syscall" error including the syscall ID
- **AND THEN** the kernel terminates that application

### Requirement: Instruction-cache synchronization after traps
After terminating an application execution loop (exit, unsupported syscall, or killed), the kernel MUST execute an instruction-cache synchronization fence before proceeding.

#### Scenario: Kernel fences instruction cache before continuing
- **WHEN** the kernel is about to stop executing an application due to exit or kill
- **THEN** it executes `fence.i` before proceeding to the next application or shutdown

### Requirement: End-of-batch shutdown request
After all applications have been processed, the kernel MUST request shutdown via `sbi_rt::system_reset(Shutdown, NoReason)` and MUST NOT return.

#### Scenario: Clean shutdown after all apps
- **WHEN** the kernel finishes processing the last application
- **THEN** it calls `system_reset` with `Shutdown` and `NoReason`
- **AND THEN** execution does not return to any Rust caller

### Requirement: Panic handling
On panic, the kernel MUST print the panic information and MUST request system shutdown via `sbi_rt::system_reset(Shutdown, SystemFailure)`. The panic handler MUST NOT return.

#### Scenario: Panic path prints and shuts down
- **WHEN** a panic occurs anywhere in the kernel
- **THEN** the panic handler prints the `PanicInfo`
- **AND THEN** it calls `system_reset` with `Shutdown` and `SystemFailure` and never returns

## Public API

This crate is a binary (`#![no_std]`, `#![no_main]`). Its externally visible interface is via exported symbols expected by the linker/SEE and by the embedded application assembly bundle.

### Symbols
- `_start() -> !`: Supervisor entry symbol provided by `linker::boot0!`; sets up an initial stack and transfers control to `rust_main`.

## Build Configuration

### build.rs
- The build script MUST write a linker script named `linker.ld` into Cargo `OUT_DIR` using the `linker::SCRIPT` string.
- The build script MUST pass `-T<OUT_DIR>/linker.ld` to the linker via `cargo:rustc-link-arg`.
- The build script MUST request rebuild when `build.rs` changes via `cargo:rerun-if-changed=build.rs`.
- The build script MUST request rebuild when `LOG` or `APP_ASM` environment variables change via `cargo:rerun-if-env-changed`.

#### Scenario: Cargo build emits and uses the linker script
- **WHEN** Cargo builds `ch2`
- **THEN** `build.rs` writes `<OUT_DIR>/linker.ld`
- **AND THEN** the final link uses that linker script via `-T.../linker.ld`

### Environment variables
- `OUT_DIR`: MUST be provided by Cargo during build-script execution; used as the output location for the generated `linker.ld`.
- `APP_ASM`: MUST be provided at compile time; identifies the assembly file to embed as user applications.
- `LOG`: MAY be provided at compile time; used to configure runtime log level via `rcore_console::set_log_level(option_env!("LOG"))`.

### Generated files
- `<OUT_DIR>/linker.ld`: Linker script content equal to `linker::SCRIPT`.

## Dependencies

### Workspace crates (Preconditions)
- `linker`:
  - MUST provide `boot0!(entry; stack = N)` that defines the kernel entry symbol and sets up an initial stack before calling `entry`.
  - MUST provide `KernelLayout::locate()` and an unsafe `zero_bss()` that clears the kernel `.bss`.
  - MUST provide `AppMeta::locate().iter()` yielding application metadata whose elements can provide an application base pointer via `as_ptr()`.
  - MUST provide `SCRIPT` as a linker-script string consumable by `build.rs`.
- `rcore-console` (`rcore_console`):
  - MUST provide `init_console(&impl rcore_console::Console)`, `set_log_level(Option<&'static str>)`, and `test_log()`.
  - MUST provide printing macros (`print!`, `println!`) and logging macros via `rcore_console::log::*`.
  - MUST define the `rcore_console::Console` trait with `put_char(u8)`.
- `kernel-context`:
  - MUST provide `LocalContext` with `LocalContext::user(entry: usize)`.
  - MUST provide unsafe `execute()` that transfers control to user code until a trap occurs and then returns to the kernel.
  - MUST provide accessors for integer registers used by this kernel: `a(i)`, `a_mut(i)`, and `sp_mut()`.
  - MUST provide `move_next()` to advance the user instruction pointer past the trapping instruction for syscall return.
- `syscall` (feature: `kernel`):
  - MUST provide `init_io(&impl syscall::IO)` and `init_process(&impl syscall::Process)`.
  - MUST provide `handle(caller: Caller, id: SyscallId, args: [usize; 6]) -> SyscallResult`.
  - MUST define `Caller { entity, flow }`, `SyscallId` (including `EXIT`), `SyscallResult::{Done, Unsupported}`, and file-descriptor constants `STDOUT` and `STDDEBUG`.
  - MUST define traits `IO` and `Process` with methods used by this kernel (`write`, `exit`).

### External crates
- `sbi-rt` (feature: `legacy`): MUST provide `legacy::console_putchar` and `system_reset` with the `Shutdown`, `NoReason`, and `SystemFailure` values used by this binary.
- `riscv`: MUST provide `riscv::register::scause` accessors sufficient to obtain trap cause values used by this kernel.

### Platform/SEE (Preconditions)
- The SEE MUST boot this binary in RISC-V S-mode with an SBI implementation that permits calls to `legacy::console_putchar` and `system_reset`.
- The runtime environment MUST correctly report `scause` for user traps, including `Exception::UserEnvCall`.

