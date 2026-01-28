# Capability: ch1

## Purpose

`ch1` is a minimal RISC-V Supervisor-mode (S-mode) bare-metal binary that provides a fixed entry symbol, prints `Hello, world!` via SBI legacy console output, and then requests shutdown via SBI `system_reset`.

## Requirements

### Requirement: Boot entry and control transfer
The binary MUST provide a supervisor entry symbol named `_start` that is placed in section `.text.entry` and is suitable for execution without an initialized stack.

#### Scenario: SEE jumps to the fixed entry address
- **WHEN** a Supervisor Execution Environment (SEE) transfers control to the binary entry address (as arranged by the linker script)
- **THEN** `_start` executes without requiring a pre-existing stack
- **AND THEN** `_start` transfers control to `rust_main` and never returns

### Requirement: Stack provisioning
The binary MUST reserve a stack region of 4096 bytes in `.bss.uninit` and MUST set the stack pointer to the top of that region before entering `rust_main`.

#### Scenario: Stack is established before Rust code runs
- **WHEN** `_start` begins execution
- **THEN** it points `sp` at `STACK + 4096`
- **AND THEN** `rust_main` is entered with a usable stack

### Requirement: Console output
During normal execution, the binary MUST print the ASCII byte sequence `Hello, world!` by invoking SBI legacy console output once per byte.

#### Scenario: Hello world is emitted
- **WHEN** `rust_main` executes under an SBI implementation that supports `legacy::console_putchar`
- **THEN** the bytes of `Hello, world!` are passed to `legacy::console_putchar` in order
- **AND THEN** no additional bytes are required by this crate before shutdown is requested

### Requirement: Normal shutdown request
After printing `Hello, world!`, the binary MUST request system shutdown via `sbi_rt::system_reset(Shutdown, NoReason)` and MUST NOT return to its caller.

#### Scenario: Clean exit
- **WHEN** `rust_main` finishes printing `Hello, world!`
- **THEN** it calls `system_reset` with `Shutdown` and `NoReason`
- **AND THEN** execution does not return to any Rust caller

### Requirement: Panic handling
On panic, the binary MUST request system shutdown via `sbi_rt::system_reset(Shutdown, SystemFailure)` and MUST NOT return from the panic handler.

#### Scenario: Panic path shutdown
- **WHEN** a panic occurs anywhere in the binary
- **THEN** the `#[panic_handler]` calls `system_reset` with `Shutdown` and `SystemFailure`
- **AND THEN** execution does not return to the caller

## Public API

This crate is a binary (`#![no_std]`, `#![no_main]`). Its externally visible interface is via exported symbols expected by the linker/SEE.

### Symbols
- `_start() -> !`: Supervisor entry symbol; sets up stack and jumps to `rust_main`.

## Build Configuration

### build.rs
- The build script MUST write a linker script named `linker.ld` into Cargo `OUT_DIR`.
- The build script MUST pass `-T<OUT_DIR>/linker.ld` to the linker via `cargo:rustc-link-arg`.
- The build script MUST request rebuild when `build.rs` changes via `cargo:rerun-if-changed=build.rs`.

#### Scenario: Cargo build emits and uses the linker script
- **WHEN** Cargo builds `ch1`
- **THEN** `build.rs` writes `<OUT_DIR>/linker.ld`
- **AND THEN** the final link uses that linker script via `-T.../linker.ld`

### Environment variables
- `OUT_DIR`: MUST be provided by Cargo during build-script execution; used as the output location for the generated `linker.ld`.

### Generated files
- `<OUT_DIR>/linker.ld`: Linker script defining `OUTPUT_ARCH(riscv)` and section layout with `.text` starting at `0x80200000` and `.text.entry` placed first within `.text`.

## Dependencies

### External crates
- `sbi-rt` (feature: `legacy`): MUST provide `legacy::console_putchar` and `system_reset` with the `Shutdown`, `NoReason`, and `SystemFailure` values used by this binary.

### Platform/SEE (Preconditions)
- The SEE MUST transfer control to the binary entry point address consistent with the generated linker script (`.text` at `0x80200000`).
- The runtime environment MUST permit SBI calls from S-mode for legacy console output and `system_reset`.

