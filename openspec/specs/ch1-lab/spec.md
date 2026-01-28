# Capability: ch1-lab

## Purpose

`ch1-lab` is a minimal RISC-V Supervisor-mode (S-mode) bare-metal binary that demonstrates integrating the workspace crate `rcore-console` for `println!` and leveled logging. It initializes a console backend that forwards output to SBI legacy console, emits a small log/print test payload, and then requests shutdown via SBI `system_reset`.

## Requirements

### Requirement: Boot entry and control transfer
The binary MUST provide a supervisor entry symbol named `_start` that is placed in section `.text.entry` and is suitable for execution without an initialized stack.

#### Scenario: SEE jumps to the fixed entry address
- **WHEN** a Supervisor Execution Environment (SEE) transfers control to the binary entry address (as arranged by the linker script)
- **THEN** `_start` executes without requiring a pre-existing stack and sets the stack pointer to a valid stack region owned by the binary
- **AND THEN** `_start` transfers control to `rust_main` and never returns

### Requirement: Stack provisioning
The binary MUST reserve a stack region of 4096 bytes in `.bss.uninit` and MUST set the stack pointer to the top of that region before entering `rust_main`.

#### Scenario: Stack is established before Rust code runs
- **WHEN** `_start` begins execution
- **THEN** it points `sp` at `STACK + 4096`
- **AND THEN** `rust_main` is entered with a usable stack

### Requirement: Console initialization for printing/logging
The binary MUST initialize `rcore-console` exactly once by calling `rcore_console::init_console` with a console backend that forwards output bytes to SBI legacy console output.

#### Scenario: Console is configured before any output
- **WHEN** `rust_main` begins execution
- **THEN** it registers a console backend via `rcore_console::init_console`
- **AND THEN** subsequent uses of `print!` / `println!` / `log` emit bytes via the backend

### Requirement: Log level configuration from build-time environment
The binary MUST set the runtime maximum log level by calling `rcore_console::set_log_level(option_env!("LOG"))`.

#### Scenario: LOG is unset
- **WHEN** the binary is compiled without a `LOG` environment variable
- **THEN** `option_env!("LOG")` evaluates to `None`
- **AND THEN** `rcore_console::set_log_level(None)` selects the `Trace` maximum level

#### Scenario: LOG is set to a valid `log::LevelFilter`
- **WHEN** the binary is compiled with `LOG` set to a string accepted by `log::LevelFilter::from_str`
- **THEN** `rcore_console::set_log_level(Some(LOG))` selects that maximum level

### Requirement: Test output and logging
After initializing `rcore-console` and setting the log level, the binary MUST call `rcore_console::test_log()` exactly once.

#### Scenario: Test payload is emitted
- **WHEN** `rust_main` runs under an SBI implementation that supports `legacy::console_putchar`
- **THEN** the binary emits the `test_log()` payload (ASCII art, a blank line, and leveled log lines) via `rcore-console` output

### Requirement: Normal shutdown request
After `rcore_console::test_log()` completes, the binary MUST request system shutdown via `sbi_rt::system_reset(Shutdown, NoReason)` and MUST NOT continue normal execution afterward.

#### Scenario: Clean exit
- **WHEN** `rust_main` finishes executing `rcore_console::test_log()`
- **THEN** it calls `system_reset` with `Shutdown` and `NoReason`
- **AND THEN** normal control flow does not proceed past the shutdown request

### Requirement: Panic handling
On panic, the binary MUST attempt to print the panic info via `println!` and MUST request system shutdown via `sbi_rt::system_reset(Shutdown, SystemFailure)`, and MUST not return from the panic handler.

#### Scenario: Panic path shutdown
- **WHEN** a panic occurs anywhere in the binary after console initialization
- **THEN** the `#[panic_handler]` prints `PanicInfo` via `println!`
- **AND THEN** it requests shutdown with reason `SystemFailure`
- **AND THEN** execution does not return to the caller

## Public API

This crate is a binary (`#![no_std]`, `#![no_main]`). Its externally visible interface is via exported symbols expected by the linker/SEE.

### Symbols
- `_start() -> !`: Supervisor entry symbol; sets up stack and jumps to Rust code.

## Build Configuration

### build.rs
- The build script MUST write a linker script named `linker.ld` into Cargo `OUT_DIR`.
- The build script MUST pass `-T<OUT_DIR>/linker.ld` to the linker via `cargo:rustc-link-arg`.
- The build script MUST request rebuild when `build.rs` changes via `cargo:rerun-if-changed=build.rs`.

#### Scenario: Cargo build emits and uses the linker script
- **WHEN** Cargo builds `ch1-lab`
- **THEN** `build.rs` writes `<OUT_DIR>/linker.ld`
- **AND THEN** the final link uses that linker script via `-T.../linker.ld`

### Environment variables
- `OUT_DIR`: MUST be provided by Cargo during build-script execution; used as the output location for the generated `linker.ld`.
- `LOG`: MAY be provided at compile time; used via `option_env!("LOG")` to select `rcore-console` log level.

### Generated files
- `<OUT_DIR>/linker.ld`: Linker script defining `OUTPUT_ARCH(riscv)` and section layout with `.text` starting at `0x80200000`.

## Dependencies

### Workspace crates (Preconditions)
- `rcore-console`:
  - MUST provide trait `rcore_console::Console` with method `put_char(&self, u8)`.
  - MUST provide `rcore_console::init_console(&'static dyn Console)` which registers the console backend and installs a `log::Log` logger.
  - MUST provide `rcore_console::set_log_level(Option<&str>)` which sets the runtime max log level (defaulting to `Trace` when input is `None` or unparseable).
  - MUST provide `rcore_console::test_log()` which emits a deterministic test payload via `println!` and `log` macros.
  - `rcore_console::println!` / `rcore_console::print!` MUST write through the installed console backend once initialized.

### External crates
- `sbi-rt` (feature: `legacy`): MUST provide `legacy::console_putchar` and `system_reset` with the `Shutdown`, `NoReason`, and `SystemFailure` values used by this binary.

