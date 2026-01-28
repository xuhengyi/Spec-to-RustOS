## Context
`task-manage` is a small `#![no_std]` + `alloc` library that provides:
- monotonically allocated ID newtypes (`ProcId`, `ThreadId`, `CoroId`),
- two minimal abstraction traits (`Manage`, `Schedule`) for “task store” + “runnable queue”,
- optional helper structs for tracking and maintaining task relationships (`proc` / `thread` features).

The helpers are intended to be embedded inside a kernel/task subsystem (e.g., PCB/TCB management) without prescribing concrete container implementations.

## Goals / Non-Goals
- Goals:
  - Provide simple, allocation-backed relationship tracking between tasks using IDs.
  - Allow callers to plug in their own storage and scheduling containers via traits.
  - Keep the API `no_std` friendly while still allowing dynamic collections via `alloc`.
- Non-Goals:
  - Provide a full scheduler implementation or policy (priority, fairness, time slicing).
  - Provide synchronization/locking (caller must ensure safe concurrent use if needed).
  - Define OS-level semantics for blocking/wakeup; this crate only enqueues/dequeues IDs.

## Feature Matrix
- Feature `proc`:
  - Exposes `ProcRel` and `PManager`.
  - Manages a process tree (parent/children), and a runnable queue of `ProcId`.
- Feature `thread`:
  - Exposes `ProcThreadRel` and `PThreadManager`.
  - Manages both processes and threads: runnable queue is `ThreadId`, processes are stored separately.
  - Provides process wait (`wait`) and thread wait (`waittid`) semantics via relationship maps.

`proc` and `thread` are orthogonal cargo features; the `thread` path does not require enabling `proc` (it has its own relationship type for process trees).

## Core Invariants (Caller Responsibilities)
The managers use internal `Option<...>` fields and `unwrap()` heavily; as a result, several invariants are *required* to avoid panics:

- **Manager initialization**:
  - `PManager::set_manager()` MUST be called before any method that touches the underlying task store/scheduler.
  - `PThreadManager::set_manager()` and `set_proc_manager()` MUST be called before any method that touches the underlying thread/proc stores or scheduler.

- **Relationship map completeness**:
  - When using process reparenting on exit (`make_current_exited` / `del_proc`), the relationship map MUST contain an entry for init PID `ProcId::from_usize(0)` so children can be transferred to it.
  - Parent PID entries SHOULD exist before creating children, otherwise child creation will not be recorded on the parent side.

- **Thread ownership mapping** (`PThreadManager`):
  - A process MUST be created via `add_proc(pid, ...)` before adding any threads to it via `add(tid, ..., pid)`.
  - `tid2pid` MUST contain a mapping for any thread that may call `wait`, `waittid`, or exit handling; otherwise those operations may panic.

## Sentinel Values
The relationship types encode “still running” states using sentinel return values:
- Process wait:
  - Returns `Some((ProcId::from_usize(-2 as _), -1))` when the target child exists but has not exited.
  - Returns `None` when the target does not exist (or when waiting for any child and there are no children at all).
- Thread wait:
  - Returns `Some(-2)` when the target thread exists but has not exited.
  - Returns `None` when the thread does not exist.

Callers typically interpret “-2” as a non-terminal wait result (e.g., `EAGAIN`-like) and “None” as “no such child/thread”.

## Recommended Initialization Sequences

### `PManager` (feature `proc`)
- Create `PManager::new()`.
- Call `set_manager(impl Manage<Proc, ProcId> + Schedule<ProcId>)`.
- Create/init `ProcId::from_usize(0)` process entry AND relationship entry if reparenting is used.
- For each new process:
  - Decide its `parent: ProcId`
  - Call `add(pid, proc, parent)`

### `PThreadManager` (feature `thread`)
- Create `PThreadManager::new()`.
- Call `set_manager(impl Manage<Thread, ThreadId> + Schedule<ThreadId>)`.
- Call `set_proc_manager(impl Manage<Proc, ProcId>)`.
- Create/init `ProcId::from_usize(0)` process entry AND relationship entry if reparenting is used.
- For each new process:
  - Call `add_proc(pid, proc, parent)`
- For each new thread belonging to a process:
  - Call `add(tid, thread, pid)`

## Risks / Trade-offs
- The APIs are intentionally minimal, but the internal `unwrap()` usage means mis-ordered initialization is surfaced as panics rather than recoverable errors.
- Relationship updates are “best effort” in some paths (e.g., missing parent rel is ignored), but later paths may still assume the presence of required entries for reparenting or ownership lookup.

## Open Questions
- Should sentinel values be replaced with an explicit enum (e.g., `WaitStatus::{Exited, StillRunning, NoSuchChild}`) to make misuse harder?
- Should manager APIs return `Result` instead of panicking for missing initialization or missing mappings?

