## Context

`ch1` is intentionally minimal and runs as a RISC-V Supervisor-mode bare-metal binary. It provides its own entry (`_start`) and stack, relies on SBI for console output and shutdown, and does not depend on a Rust runtime (`#![no_std]`, `#![no_main]`).

## Goals / Non-Goals

- Goals:
  - Keep the entry/stack/bootstrap path small and auditable.
  - Demonstrate the smallest end-to-end “boot → print → shutdown” flow in S-mode using SBI.
- Non-Goals:
  - Provide a general-purpose runtime (no global init, no `.bss` clearing contract, no trap handling, no scheduling).
  - Provide a stable Rust library API (this is a binary crate; the external interface is the `_start` symbol).

## Unsafe / ABI Invariants

- `_start` is declared `#[unsafe(naked)]` and MUST NOT rely on a pre-existing stack. Any change that introduces stack usage before setting `sp` would violate the bootstrap contract.
- `_start` sets `sp` to the end of a statically reserved `STACK` region. Correctness depends on that address being acceptable as a stack pointer for the target ABI (alignment requirements are a platform/toolchain concern and are treated as a precondition for this crate).
- Control transfer from `_start` to `rust_main` is performed via a direct jump and assumes `rust_main` does not return.

## Linker Script Assumptions

- The generated linker script places `.text` at `0x80200000` and ensures `.text.entry` is first. The SEE is expected to transfer control to the resulting entry point address.
- The stack is placed in `.bss.uninit`; the crate assumes only that the memory exists and is writable (its initial contents are not relied upon).

