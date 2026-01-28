## Context

`ch1-lab` is a minimal RISC-V Supervisor-mode (S-mode) bare-metal binary intended to be launched by a Supervisor Execution Environment (SEE) implementing the RISC-V SBI. It demonstrates wiring a platform-specific character output primitive (SBI legacy console) into the workspace crate `rcore-console` to obtain `print!`/`println!` and leveled logging.

## Goals / Non-Goals

- Goals:
  - Provide a single-entry boot path (`_start`) that establishes a stack and transfers control to Rust code.
  - Implement an `rcore_console::Console` backend that forwards bytes to SBI legacy console output.
  - Initialize `rcore-console`, configure log level from build-time `LOG`, emit a test payload, then request shutdown.
- Non-Goals:
  - No memory management, `.bss` zeroing, traps/interrupts, drivers, or user process support.
  - No portability beyond the RISC-V + SBI environment assumptions described below.

## Decisions

### Decision: Entry is a Rust naked function
`_start` is implemented as a Rust `#[unsafe(naked)]` function with inline `naked_asm!` to avoid compiler-generated prologue/epilogue and to allow stack pointer setup before entering Rust code.

### Decision: Stack is reserved in `.bss.uninit`
A fixed-size stack (`4096` bytes) is reserved as a `static mut` in section `.bss.uninit`. `_start` sets `sp` to the top of this array prior to transferring to `rust_main`.

### Decision: `rcore-console` is initialized with a zero-sized backend
The console backend is a unit struct (`struct Console;`). The code passes `&Console` to `rcore_console::init_console`, relying on Rust's ability to provide a `'static` reference for a promotable constant rvalue of a zero-sized type (otherwise, callers would need a `static` instance).

### Decision: Linker script is generated at build time
`build.rs` emits a linker script into Cargo `OUT_DIR` and passes it to the linker via `-T.../linker.ld`. This keeps the crate self-contained.

## Safety / Unsafe Invariants

### `_start` naked entry
- `_start` MUST NOT rely on a pre-initialized stack.
- The inline assembly MUST set `sp` to a valid aligned address within the reserved stack region before transferring to `rust_main`.
- `_start` MUST transfer control to a non-returning function and MUST NOT return.

### Static mutable stack
- The stack object is `static mut` and MUST only be used as a raw memory region for `sp` initialization.
- No concurrent access assumptions are made; this binary is single-hart by design.

## Environment Assumptions (Preconditions)

- The binary is linked for RISC-V (`OUTPUT_ARCH(riscv)`).
- The SEE (or loader) transfers control to address `0x80200000`, which corresponds to the start of `.text` per the generated linker script.
- The SBI implementation supports:
  - Legacy console output (`sbi_rt::legacy::console_putchar`)
  - System reset (`sbi_rt::system_reset`) with shutdown types/reasons used by the crate.
- The `rcore-console` logger is initialized before any `println!` / `log` output is required, including in the panic path described by the capability spec.

## Trade-offs / Risks

- Relying on a fixed link address and legacy SBI console reduces portability.
- If a panic occurs before `rcore_console::init_console` is called, `println!` may fail (e.g., by panicking due to an uninitialized console singleton), potentially obscuring the original panic cause.

