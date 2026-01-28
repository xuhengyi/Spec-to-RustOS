## Context

`kernel-context` provides low-level RISC-V Supervisor-mode (S-mode) context switching and (optionally) cross-address-space execution via a “portal” trampoline. The implementation uses inline assembly and `#[unsafe(naked)]` functions to precisely control register save/restore and privileged CSR manipulation.

## Goals / Non-Goals

- Goals:
  - Provide a compact, `#![no_std]` representation of a thread context with predictable layout.
  - Provide an execution primitive that transfers control to the represented context and returns to the caller after a trap/return.
  - When `foreign` is enabled, provide a mechanism to temporarily switch address spaces (`satp`) to execute a context in a different page table, while restoring the original address space afterward.
- Non-Goals:
  - A full scheduler, trap handler, or syscall subsystem.
  - A portable (non-RISC-V) context abstraction.

## Key Design Decisions

- Decision: `LocalContext` is `#[repr(C)]` and stores integer registers as an array plus an explicit `sepc` field.
  - Rationale: The assembly switcher treats the context memory as a fixed layout for save/restore and expects stable field ordering.

- Decision: `LocalContext::execute` is `unsafe` and owns privileged CSR side-effects.
  - Rationale: Correctness depends on running in S-mode, on the availability and meaning of CSRs, and on invariants about trap routing and `sscratch` usage that the type system cannot enforce.

- Decision: The “foreign” portal uses a `PortalCache` in a shared (“public”) address space as the sole data exchange mechanism.
  - Rationale: Portal code must run while `satp` changes, so both sides need a stable mapping for the exchange record.

## Unsafe / Invariants

- `LocalContext::execute`:
  - Modifies `sscratch`, `sepc`, `sstatus`, and `stvec`.
  - Assumes the caller environment can tolerate temporary changes to `stvec` and that returning via the installed vector will resume the caller context correctly.
  - Updates the saved `sepc` on return, which is part of the “post-trap” state capture contract.

- `foreign` portal execution:
  - Requires that portal object memory and `PortalCache` slots are correctly mapped into a shared address space, typically with read/write/execute permissions for the portal object if code and caches are contiguous.
  - Temporarily disables interrupts and forces S-mode while running portal code to avoid re-entrancy and privilege mismatches during address-space switching.

## Feature Matrix

- Default (no features):
  - Provides `LocalContext` and its execution primitive for the current address space.
- `foreign`:
  - Adds `kernel_context::foreign` with `ForeignContext`, portal traits, `PortalCache`, `SlotKey` helpers, and a reference implementation `MultislotPortal`.
  - Introduces the optional `spin` dependency for `spin::Lazy` to locate the portal code slice at runtime.

## Risks / Trade-offs

- Assembly and `#[unsafe(naked)]` code is fragile across ABI/toolchain changes and is inherently harder to test.
- The portal code slice location (`PORTAL_TEXT`) is discovered at runtime by scanning for an expected tail sequence; this couples correctness to codegen details and may be sensitive to compiler evolution.

## Migration Plan

No migration plan: this document describes the current capability boundary and invariants.

## Open Questions

- Should the portal text discovery mechanism be replaced by a link-time symbol range to reduce coupling to codegen details?
