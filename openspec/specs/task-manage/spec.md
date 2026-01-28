# Capability: task-manage

## Purpose
`task-manage` crate SHALL provide:
- task identifier types (`ProcId`, `ThreadId`, `CoroId`) suitable for ordering, hashing, and storage in ordered maps/queues,
- generic task-store and ready-queue traits (`Manage`, `Schedule`) that abstract over concrete container implementations,
- optional relationship/management helpers for process trees and process-thread associations behind feature flags (`proc`, `thread`).

This crate is `#![no_std]` and uses `alloc`.

## Preconditions
- A global allocator MUST be available (because this crate uses `alloc`, including `BTreeMap` and `Vec`).
- Callers MUST respect feature gating:
  - `PManager` and `ProcRel` are only available when feature `proc` is enabled.
  - `PThreadManager` and `ProcThreadRel` are only available when feature `thread` is enabled.

## Requirements

### Requirement: Monotonic task identifiers
The crate MUST provide task identifier types `ProcId`, `ThreadId`, and `CoroId` with the following properties:
- They MUST be `Copy` and `Ord` (to support ordered collections and priority/runnable queues).
- `new()` MUST return an identifier value that is monotonically increasing within the current execution and MUST NOT reuse previously returned values from the same type.
- `from_usize(v)` MUST construct an identifier from the provided raw value `v`.
- `get_usize()` MUST return the raw value used to represent the identifier.

#### Scenario: Creating and inspecting an ID
- **WHEN** a caller constructs an ID via `ProcId::new()` (or `ThreadId::new()`, `CoroId::new()`)
- **THEN** the caller can retrieve the raw value via `get_usize()`
- **AND THEN** a second call to `new()` returns an ID that compares greater than the first one (per `Ord`)

### Requirement: Generic task storage interface
The crate MUST provide a `Manage<T, I>` trait (where `I: Copy + Ord`) that abstracts CRUD-style access to a collection of tasks keyed by an ID:
- `insert(id, item)` MUST store `item` under `id`, making it retrievable via `get_mut(id)`.
- `delete(id)` MUST remove the item under `id` (if present), such that `get_mut(id)` no longer returns it.
- `get_mut(id)` MUST return `Some(&mut T)` if an item with `id` exists, and `None` otherwise.

#### Scenario: Insert-get-delete lifecycle
- **WHEN** a caller calls `insert(id, item)` and then `get_mut(id)`
- **THEN** `get_mut(id)` returns `Some(&mut T)`
- **AND THEN** after `delete(id)`, `get_mut(id)` returns `None`

### Requirement: Generic ready-queue scheduling interface
The crate MUST provide a `Schedule<I>` trait (where `I: Copy + Ord`) representing a runnable-queue:
- `add(id)` MUST enqueue `id` for future selection.
- `fetch()` MUST return `Some(id)` for a previously enqueued runnable item when one exists, otherwise it MUST return `None`.

#### Scenario: Enqueue and fetch
- **WHEN** a caller calls `add(id)` and then calls `fetch()`
- **THEN** `fetch()` returns `Some(id)` (or a queued ID according to the scheduler policy)

### Requirement: Process parent/child relationship tracking (feature `proc`)
When feature `proc` is enabled, the crate MUST provide `ProcRel` to track process-tree relationships:
- `ProcRel::new(parent_pid)` MUST initialize an empty child list and an empty dead-child list with `parent = parent_pid`.
- `add_child(child_pid)` MUST add `child_pid` to the active child list.
- `del_child(child_pid, exit_code)` MUST remove `child_pid` from the active child list (if present) and MUST record `(child_pid, exit_code)` in the dead-child list.
- `wait_any_child()` MUST:
  - return `None` if there are no children at all,
  - return `Some((ProcId::from_usize(usize::MAX - 1), -1))` (sentinel “still running”) if there exists at least one active child and no dead child is available,
  - otherwise return `Some((dead_child_pid, exit_code))` for a dead child.
- `wait_child(child_pid)` MUST:
  - return `Some((child_pid, exit_code))` if that child is recorded as dead,
  - return `Some((ProcId::from_usize(usize::MAX - 1), -1))` (sentinel “still running”) if that child exists but is still active,
  - return `None` if that child does not exist in either active or dead lists.

#### Scenario: Waiting for any child with none present
- **WHEN** `wait_any_child()` is called and both active and dead child lists are empty
- **THEN** it returns `None`

#### Scenario: Waiting for any child while children are still running
- **WHEN** `wait_any_child()` is called, the active child list is non-empty, and the dead child list is empty
- **THEN** it returns the sentinel value `Some((ProcId::from_usize(usize::MAX - 1), -1))`

#### Scenario: Waiting for a specific child that has exited
- **WHEN** `wait_child(child_pid)` is called and `(child_pid, exit_code)` exists in the dead child list
- **THEN** it returns `Some((child_pid, exit_code))`

### Requirement: Process management helper (feature `proc`)
When feature `proc` is enabled, the crate MUST provide `PManager<P, MP>` as a helper that combines:
- an underlying process object store implementing `Manage<P, ProcId>`,
- an underlying runnable queue implementing `Schedule<ProcId>`,
- an internal `ProcId -> ProcRel` map for parent/child tracking,
- a tracked “current” running process ID.

Callers MUST initialize `PManager` by calling `set_manager(manager)` before calling any method that accesses the underlying manager/scheduler; otherwise those methods MUST panic.

Callers MUST ensure the relationship map contains required entries (including the init process ID `ProcId::from_usize(0)` when using reparenting semantics); otherwise reparenting-related methods MAY panic.

`find_next()` MUST:
- call `fetch()` on the underlying scheduler,
- set the internal current ID to the fetched ID if the corresponding task exists,
- return `Some(&mut P)` for the fetched task if present, otherwise return `None`.

`make_current_suspend()` MUST:
- enqueue the current process ID into the scheduler via `add(id)`,
- clear the current process ID.

`add(id, task, parent)` MUST:
- insert the process into the underlying store,
- enqueue the process ID into the scheduler,
- record the parent/child relationship in the relationship map.

`make_current_exited(exit_code)` MUST:
- delete the current process from the underlying store,
- update the parent’s relationship state to record the exited child and `exit_code` (if the parent relationship exists),
- reparent all of the exiting process’s children to init process `ProcId::from_usize(0)` and add them to init’s child list,
- clear the current process ID.

`wait(child_pid)` MUST delegate to the current process’s `ProcRel` as:
- “wait any child” if `child_pid.get_usize() == usize::MAX`,
- otherwise “wait a specific child”.

#### Scenario: Selecting the next runnable process
- **WHEN** `find_next()` is called and the underlying scheduler returns a runnable ID whose task exists
- **THEN** `find_next()` returns `Some(&mut P)` and the internal current ID becomes that ID

#### Scenario: Suspending the current process re-enqueues it
- **WHEN** a current process is set and `make_current_suspend()` is called
- **THEN** the current process ID is enqueued into the scheduler and current becomes `None`

#### Scenario: Exiting a process records an exit code for the parent
- **WHEN** `make_current_exited(exit_code)` is called and the parent relationship exists
- **THEN** the parent’s dead-child list records `(exiting_pid, exit_code)`

### Requirement: Process-thread relationship tracking (feature `thread`)
When feature `thread` is enabled, the crate MUST provide `ProcThreadRel` to track:
- a process parent/child relationship (same shape as `ProcRel`),
- a process’ thread set and exited-thread list.

`ProcThreadRel::wait_thread(thread_tid)` MUST:
- return `Some(exit_code)` if the thread is recorded as dead,
- return `Some(-2)` (sentinel “still running”) if the thread exists but is still active,
- return `None` if the thread does not exist in either active or dead lists.

#### Scenario: Waiting for a thread that is still running
- **WHEN** `wait_thread(thread_tid)` is called and `thread_tid` exists in the active thread list but not in the dead thread list
- **THEN** it returns `Some(-2)`

### Requirement: Combined process + thread management helper (feature `thread`)
When feature `thread` is enabled, the crate MUST provide `PThreadManager<P, T, MT, MP>` as a helper that combines:
- an underlying thread store implementing `Manage<T, ThreadId>`,
- an underlying runnable queue implementing `Schedule<ThreadId>`,
- an underlying process store implementing `Manage<P, ProcId>`,
- internal relationship tracking `ProcId -> ProcThreadRel`,
- an internal map from `ThreadId -> ProcId`,
- a tracked “current” running thread ID.

Callers MUST initialize `PThreadManager` by calling `set_manager(thread_manager)` and `set_proc_manager(proc_manager)` before calling methods that access them; otherwise those methods MUST panic.

Callers MUST call `add_proc(pid, proc, parent)` before adding threads belonging to `pid` via `add(tid, task, pid)`; otherwise subsequent operations that rely on `ThreadId -> ProcId` mapping MAY panic.

`find_next()` MUST select the next runnable thread ID via the underlying scheduler and return a mutable reference to the corresponding thread task if present, setting the internal current thread ID.

`make_current_suspend()` MUST re-enqueue the current thread ID (if any) and clear current.

`make_current_blocked()` MUST clear current without re-enqueueing.

`make_current_exited(exit_code)` MUST:
- delete the current thread from the underlying thread store,
- update the owning process relationship to record the exited thread and `exit_code`,
- if the owning process now has zero remaining active threads, delete that process via `del_proc(pid, exit_code)`,
- clear current.

`wait(child_pid)` MUST perform process wait semantics on the current thread’s owning process.

`waittid(thread_tid)` MUST perform thread wait semantics on the current thread’s owning process.

#### Scenario: Adding a thread requires its owning process to exist
- **WHEN** `add(tid, task, pid)` is called for a `pid` that has not been created via `add_proc`
- **THEN** the caller MUST treat the result as invalid for later wait/exit operations (which may panic due to missing relationship/mapping)

#### Scenario: Exiting the last thread deletes the owning process
- **WHEN** `make_current_exited(exit_code)` is called and the owning process has no remaining active threads
- **THEN** `del_proc(owning_pid, exit_code)` is performed and the process is removed from the underlying process store

## Public API

### Re-exports (crate root)
- `ProcId`
- `ThreadId`
- `CoroId`
- `Manage` (trait)
- `Schedule` (trait)

### Feature: `proc`
- Types:
  - `PManager<P, MP>`
  - `ProcRel`
- Functions (methods):
  - `PManager::new() -> PManager<...>`
  - `PManager::set_manager(&mut self, manager: MP)`
  - `PManager::add(&mut self, id: ProcId, task: P, parent: ProcId)`
  - `PManager::find_next(&mut self) -> Option<&mut P>`
  - `PManager::current(&mut self) -> Option<&mut P>`
  - `PManager::get_task(&mut self, id: ProcId) -> Option<&mut P>`
  - `PManager::make_current_suspend(&mut self)`
  - `PManager::make_current_exited(&mut self, exit_code: isize)`
  - `PManager::wait(&mut self, child_pid: ProcId) -> Option<(ProcId, isize)>`
  - `ProcRel::new(parent: ProcId) -> ProcRel`
  - `ProcRel::{add_child, del_child, wait_any_child, wait_child}`

### Feature: `thread`
- Types:
  - `PThreadManager<P, T, MT, MP>`
  - `ProcThreadRel`
- Functions (methods):
  - `PThreadManager::new() -> PThreadManager<...>`
  - `PThreadManager::set_manager(&mut self, manager: MT)`
  - `PThreadManager::set_proc_manager(&mut self, proc_manager: MP)`
  - `PThreadManager::add_proc(&mut self, id: ProcId, proc: P, parent: ProcId)`
  - `PThreadManager::add(&mut self, id: ThreadId, task: T, pid: ProcId)`
  - `PThreadManager::find_next(&mut self) -> Option<&mut T>`
  - `PThreadManager::{make_current_suspend, make_current_blocked, make_current_exited}`
  - `PThreadManager::re_enque(&mut self, id: ThreadId)`
  - `PThreadManager::current(&mut self) -> Option<&mut T>`
  - `PThreadManager::get_task(&mut self, id: ThreadId) -> Option<&mut T>`
  - `PThreadManager::get_proc(&mut self, id: ProcId) -> Option<&mut P>`
  - `PThreadManager::del_proc(&mut self, id: ProcId, exit_code: isize)`
  - `PThreadManager::wait(&mut self, child_pid: ProcId) -> Option<(ProcId, isize)>`
  - `PThreadManager::waittid(&mut self, thread_tid: ThreadId) -> Option<isize>`
  - `PThreadManager::thread_count(&self, id: ProcId) -> usize`
  - `PThreadManager::get_thread(&mut self, id: ProcId) -> Option<&alloc::vec::Vec<ThreadId>>`
  - `PThreadManager::get_current_proc(&mut self) -> Option<&mut P>`
  - `ProcThreadRel::new(parent: ProcId) -> ProcThreadRel`
  - `ProcThreadRel::{add_child, del_child, wait_any_child, wait_child, add_thread, del_thread, wait_thread}`

## Build Configuration
- `build.rs`: None
- Environment variables: None
- Generated files: None

## Dependencies
- Workspace crates: None
- External crates: None
- Rust core libraries:
  - `core`: atomic counters for ID allocation
  - `alloc`: `BTreeMap` and `Vec` storage for relationships and manager state

