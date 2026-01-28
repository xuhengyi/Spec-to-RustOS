## Context
`sync` is a small kernel-oriented synchronization crate built for a `#![no_std]` environment with `alloc`.
It targets a uniprocessor-style execution model where **interrupt masking** can be used as the core mutual-exclusion primitive.

## Goals / Non-Goals
- Goals:
  - Provide simple primitives (mutex, semaphore, condvar wait queue) that integrate with an external scheduler.
  - Keep critical sections small and deterministic by using interrupt masking plus dynamic borrow checks.
- Non-Goals:
  - Multiprocessor-safe synchronization (no atomic/lock-based SMP support).
  - Performing scheduling internally (no “block current and run next” inside this crate).

## Key Design: `UPIntrFreeCell<T>`
`UPIntrFreeCell<T>` combines:
- `RefCell<T>` for dynamic borrow checking (panic on misuse).
- Interrupt masking via RISC-V `sstatus.SIE` to prevent preemption/interrupt re-entry into the same critical section.

### Nested masking
Interrupt masking is tracked via a global nesting counter:
- Entering the first critical section records the previous `SIE` state and disables interrupts.
- Nested entries keep interrupts disabled.
- Exiting the last critical section restores interrupts iff they were enabled at the first entry.

### Safety boundary
`UPIntrFreeCell::new` is `unsafe` because callers must guarantee the execution model assumptions (uniprocessor-equivalent) and correct usage patterns. Safe constructors in this crate wrap it where appropriate.

## Scheduler-Driven Blocking Model
All blocking primitives return **signals** to the caller rather than performing scheduling:
- A `false` return value means “the caller should block the provided `ThreadId`”.
- A returned `Some(ThreadId)` means “the caller should wake/enqueue this thread”.

This design keeps `sync` independent from the scheduler implementation while still enabling deterministic tests and integration with `rcore-task-manage`.

## Ownership Transfer Semantics
Some primitives intentionally avoid “unlock then re-lock” patterns by transferring ownership/resources on wakeup:
- `MutexBlocking::unlock()` returns `Some(waiter_tid)` without marking the mutex unlocked. This implies the woken thread becomes the owner without needing to call `lock()` again.
- `Semaphore::down()` decrements first; if it returns `false`, the thread SHOULD block and later continue after being woken by `up()` without repeating `down()`.

These semantics are critical to avoid deadlocks (e.g., re-enqueueing a woken thread back into the same wait queue).

## Condvar Scope
`Condvar` is primarily a wait queue + signal mechanism (`wait_no_sched`, `signal`).
`wait_with_mutex` is explicitly a simplified helper (test-oriented) and is not a full condition-variable implementation (e.g., it unwraps `mutex.unlock()` and may panic if misused).

## Risks / Trade-offs
- Dynamic borrow panics: Misuse of `UPIntrFreeCell` can panic at runtime, which is acceptable for kernel debug builds/tests but must be treated carefully in production kernels.
- Uniprocessor assumption: Interrupt masking does not provide SMP safety; using this crate in multi-core contexts requires additional outer synchronization not provided here.

## Open Questions
- Should a future revision provide an SMP-safe cell/lock abstraction (e.g., using atomics) behind feature flags?
