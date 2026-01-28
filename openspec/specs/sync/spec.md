# Capability: sync

## Purpose
`sync` crate SHALL provide basic synchronization primitives for a uniprocessor, kernel-style environment.
This crate is `#![no_std]` and uses `alloc`.

The primitives in this crate MUST NOT perform scheduling by themselves. Instead, APIs return:
- `bool` to indicate whether the current thread may continue (`true`) or MUST be blocked by the caller (`false`).
- `Option<ThreadId>` to indicate a thread that the caller SHOULD wake / enqueue into the scheduler.

## Preconditions
- The runtime MUST be uniprocessor (or otherwise guarantee mutual exclusion equivalent to “disable interrupts”) for `UPIntrFreeCell`-based critical sections.
- A global allocator MUST be available (because this crate uses `alloc`, including `VecDeque` and `Arc`).
- The workspace crate `rcore-task-manage` MUST provide a `ThreadId` type whose values uniquely identify threads for queueing/wakeup decisions.
- The caller (scheduler/task manager) MUST implement:
  - “block” semantics for a given `ThreadId` when an API returns `false`.
  - “wake/enqueue” semantics for a returned `ThreadId` when an API returns `Some(ThreadId)`.

## Requirements

### Requirement: Interrupt-masked exclusive access cell
The crate SHALL provide `UPIntrFreeCell<T>` to offer interior mutability guarded by interrupt masking and dynamic borrow checking.

`UPIntrFreeCell::exclusive_access()` MUST:
- disable interrupts for the duration of the returned guard’s lifetime, supporting nested calls,
- return a mutable access guard (`UPIntrRefMut<'_, T>`) that releases the borrow and restores interrupt enable state on drop,
- panic if the underlying `RefCell<T>` is already mutably borrowed.

`UPIntrFreeCell::exclusive_session(f)` MUST:
- acquire exclusive access,
- run `f(&mut T)` while interrupts are masked,
- release exclusive access before returning.

#### Scenario: Exclusive access masks and restores interrupts
- **WHEN** a caller enters `exclusive_access()` and then drops the returned `UPIntrRefMut`
- **THEN** interrupts SHALL be disabled during the guard lifetime and SHALL be restored to the pre-entry state after drop

#### Scenario: Nested exclusive access
- **WHEN** a caller calls `exclusive_access()` in a nested fashion (re-entering while already masked)
- **THEN** interrupt masking SHALL remain in effect until the last guard is dropped, and the original interrupt-enable state SHALL be restored exactly once

#### Scenario: Borrow conflict
- **WHEN** a caller calls `exclusive_access()` while the inner value is already mutably borrowed
- **THEN** the call MUST panic

### Requirement: Blocking mutex trait and implementation
The crate SHALL provide a `Mutex` trait and a blocking implementation `MutexBlocking` suitable for scheduler-driven blocking.

For `MutexBlocking::lock(tid)`:
- **IF** the mutex is unlocked, it MUST mark itself locked and return `true`.
- **IF** the mutex is locked, it MUST enqueue `tid` into its wait queue and return `false`.

For `MutexBlocking::unlock()`:
- It MUST panic if called while the mutex is not locked.
- **IF** there is a waiting thread, it MUST dequeue one `ThreadId` and return `Some(tid)` without marking the mutex unlocked (ownership transfer semantics).
- **IF** there is no waiting thread, it MUST mark the mutex unlocked and return `None`.

#### Scenario: Lock acquired immediately
- **WHEN** `lock(tid)` is called on an unlocked `MutexBlocking`
- **THEN** it SHALL return `true`

#### Scenario: Lock contention causes enqueue
- **WHEN** `lock(tid2)` is called while the mutex is locked
- **THEN** it SHALL return `false` and `tid2` SHALL be enqueued for later wakeup/ownership transfer

#### Scenario: Unlock transfers ownership to a waiter
- **WHEN** `unlock()` is called and there exists at least one waiting thread
- **THEN** it SHALL return `Some(waiter_tid)` and the caller SHOULD wake/enqueue `waiter_tid` as the new owner

#### Scenario: Unlock releases mutex when no waiters
- **WHEN** `unlock()` is called and the wait queue is empty
- **THEN** it SHALL return `None` and the mutex SHALL become unlocked

### Requirement: Condition variable wait queue
The crate SHALL provide `Condvar` as a wait queue that can hand off a waiting thread ID to the caller.

`Condvar::wait_no_sched(tid)` MUST enqueue `tid` into the condition variable wait queue and return `false`.

`Condvar::signal()` MUST dequeue and return one waiting `ThreadId` if present; otherwise it MUST return `None`.

#### Scenario: Wait enqueues a thread ID
- **WHEN** a caller calls `wait_no_sched(tid)`
- **THEN** it SHALL return `false` and `tid` SHALL be recorded in the condition variable wait queue

#### Scenario: Signal wakes one waiter
- **WHEN** a caller calls `signal()` and there exists at least one waiting thread
- **THEN** it SHALL return `Some(waiter_tid)` and the caller SHOULD wake/enqueue `waiter_tid`

#### Scenario: Signal with empty queue
- **WHEN** a caller calls `signal()` and no threads are waiting
- **THEN** it SHALL return `None`

### Requirement: Simplified condvar wait-with-mutex helper
The crate SHALL provide `Condvar::wait_with_mutex(tid, mutex)` as a simplified helper used for test-oriented behavior.

`wait_with_mutex` MUST:
- call `mutex.unlock()` and unwrap its result, and
- attempt to reacquire the mutex by calling `mutex.lock(tid)`,
- return `(lock_result, Some(woken_tid))` where `woken_tid` is the unwrapped result of `mutex.unlock()`.

Because `mutex.unlock()` is unwrapped, callers MUST ensure `mutex.unlock()` returns `Some(ThreadId)` (i.e., it MUST NOT return `None`) to avoid a panic.

#### Scenario: Wait-with-mutex returns a woken thread id
- **WHEN** `wait_with_mutex(tid, mutex)` is called and `mutex.unlock()` returns `Some(woken_tid)`
- **THEN** it SHALL return `(mutex.lock(tid), Some(woken_tid))`

#### Scenario: Wait-with-mutex panics if unlock returns None
- **WHEN** `wait_with_mutex(tid, mutex)` is called and `mutex.unlock()` returns `None`
- **THEN** it MUST panic

### Requirement: Counting semaphore
The crate SHALL provide `Semaphore` implementing a counting semaphore with scheduler-driven blocking.

`Semaphore::new(res_count)` MUST initialize the semaphore with count = `res_count`.

`Semaphore::down(tid)` MUST:
- decrement the internal count by 1,
- **IF** the count becomes negative, enqueue `tid` into the wait queue and return `false`,
- **ELSE** return `true`.

`Semaphore::up()` MUST:
- increment the internal count by 1,
- dequeue and return one waiting `ThreadId` if present; otherwise return `None`.

#### Scenario: Down succeeds when resources available
- **WHEN** `down(tid)` is called and the internal count after decrement is non-negative
- **THEN** it SHALL return `true`

#### Scenario: Down blocks when resources exhausted
- **WHEN** `down(tid)` is called and the internal count after decrement is negative
- **THEN** it SHALL return `false` and `tid` SHALL be enqueued for later wakeup

#### Scenario: Up wakes one blocked thread
- **WHEN** `up()` is called and there exists at least one waiting thread
- **THEN** it SHALL return `Some(waiter_tid)` and the caller SHOULD wake/enqueue `waiter_tid`

## Public API

### Re-exports (crate root)
- `Condvar`
- `Mutex` (trait)
- `MutexBlocking`
- `Semaphore`
- `UPIntrFreeCell<T>`
- `UPIntrRefMut<'a, T>`

### `Mutex` (trait)
- `lock(&self, tid: ThreadId) -> bool`
- `unlock(&self) -> Option<ThreadId>`

### `MutexBlocking`
- `new() -> MutexBlocking`

### `Condvar`
- `new() -> Condvar`
- `signal(&self) -> Option<ThreadId>`
- `wait_no_sched(&self, tid: ThreadId) -> bool`
- `wait_with_mutex(&self, tid: ThreadId, mutex: Arc<dyn Mutex>) -> (bool, Option<ThreadId>)`

### `Semaphore`
- `new(res_count: usize) -> Semaphore`
- `down(&self, tid: ThreadId) -> bool`
- `up(&self) -> Option<ThreadId>`

### `UPIntrFreeCell<T>`
- `unsafe fn new(value: T) -> UPIntrFreeCell<T>`
- `exclusive_access(&self) -> UPIntrRefMut<'_, T>`
- `exclusive_session<F, V>(&self, f: F) -> V where F: FnOnce(&mut T) -> V`

### `UPIntrRefMut<'a, T>`
- Guard type returned by `UPIntrFreeCell::exclusive_access`
- Implements `Deref<Target = T>` and `DerefMut`
- On `Drop`, it MUST release the borrow and restore interrupt masking state

## Build Configuration
- `build.rs`: None
- Environment variables: None
- Generated files: None

## Dependencies
- Workspace crates:
  - `rcore-task-manage` (feature: `thread`): provides `ThreadId` used to represent threads in wait queues and wakeup decisions
- External crates:
  - `riscv`: used for `sstatus` interrupt enable/disable operations
  - `spin`: used for `Lazy` initialization of global interrupt masking state
