## Context

`signal-impl` implements the workspace `signal::Signal` trait for a single process. It is designed for a `#![no_std]` kernel environment with `alloc` available, and integrates with `kernel_context::LocalContext` to redirect execution into user-installed signal handlers.

## Goals / Non-Goals

- Goals:
  - Provide a minimal per-process signal state machine (pending/mask/handling/actions).
  - Support kernel-managed stop/continue semantics (`SIGSTOP`/`SIGCONT`).
  - Support user handler delivery by rewriting `LocalContext` and later restoring it.
- Non-Goals:
  - Multi-signal delivery in a single `handle_signals()` call.
  - POSIX-complete semantics (e.g., siginfo, alternate stacks, SA_* flags, nested handlers).
  - Fine-grained default actions beyond terminate/ignore.

## Decisions

- Decision: Represent pending and mask sets as a single-word bitset (`usize`).
  - Why: Fast O(1) set operations and compact storage per process.
  - Constraint: The selection logic treats “no pending bits” as `trailing_zeros == 64`, which implies a 64-bit `usize` environment (e.g., RV64). Using this crate on a 32-bit `usize` target is out of scope unless the implementation is revised.

- Decision: Model in-progress handling as `None | Frozen | UserSignal(saved_context)`.
  - Why: A single `Option` captures the control-flow gate for nested delivery and enables `sig_return()` to restore the pre-handler user context.

- Decision: For user-handler delivery, rewrite PC to handler and set `a0` to the signal number.
  - Why: Minimal ABI surface; the kernel can deliver the signal by a single context rewrite without additional trampolines in this crate.

## Risks / Trade-offs

- The “first deliverable signal” selection is based on the lowest set bit after masking; this favors lower-numbered signals and does not guarantee fairness.
- The action table is indexed by `SignalNo as usize`; correctness relies on `signal::MAX_SIG` and the `SignalNo` numeric mapping remaining consistent.
- The default action policy is intentionally simplified (terminate vs ignore) and may diverge from full POSIX behavior.

