# Capability: signal-impl

## Purpose

`signal-impl` crate SHALL provide a `no_std` implementation of the workspace `signal::Signal` trait for per-process signal management, including signal delivery, masking, default actions, and user-context handoff/return.

## Preconditions

- The `kernel-context` crate MUST provide `kernel_context::LocalContext` with:
  - `Clone` semantics that capture the full user context needed to resume execution.
  - Mutable accessors `pc_mut()` and `a_mut(index)` suitable for updating the user PC and argument registers.
- The `signal` crate MUST provide:
  - `SignalNo` identifiers including (at least) `SIGKILL`, `SIGSTOP`, `SIGCONT`, `SIGCHLD`, `SIGURG`.
  - `MAX_SIG` such that all used `SignalNo as usize` values are within `0..=MAX_SIG`.
  - `SignalAction` that is `Copy` and contains a handler address field (`handler`) compatible with `LocalContext`’s PC representation.
  - `SignalResult` variants: `NoSignal`, `Handled`, `Ignored`, `IsHandlingSignal`, `ProcessSuspended`, `ProcessKilled(i32)`.

## Requirements

### Requirement: Signal state model
The crate SHALL model a process signal subsystem as:
- A set of **received** (pending) signals.
- A set of **masked** signals that SHALL NOT be delivered while masked.
- A **handling** state that indicates whether the process is currently handling a kernel suspend signal or a user signal.
- A per-signal action table of size `MAX_SIG + 1`.

#### Scenario: Initial state
- **WHEN** a new `SignalImpl` instance is created
- **THEN** `received` MUST be empty, `mask` MUST be empty, `handling` MUST be `None`, and the action table MUST contain no user-installed action entries

### Requirement: Pending signal selection and masking
The implementation SHALL select at most one deliverable signal per `handle_signals()` call:
- A signal SHALL be deliverable only if it is pending and not masked.
- The selected signal MUST be removed from the pending set as part of selection.

#### Scenario: Masked signal is not delivered
- **WHEN** a signal is pending but the corresponding bit is set in the mask
- **THEN** `handle_signals()` MUST NOT deliver that signal and MUST NOT remove it from the pending set

#### Scenario: Deliverable signal is consumed
- **WHEN** a signal becomes pending and is not masked
- **THEN** the first subsequent successful selection MUST remove it from the pending set

### Requirement: Action installation constraints
The implementation SHALL provide per-signal action installation with special-case restrictions:
- `SIGKILL` and `SIGSTOP` MUST NOT be installable as user actions.
- Action installation for other signals MUST overwrite the prior entry.

#### Scenario: Installing action for SIGKILL is rejected
- **WHEN** `set_action(SIGKILL, action)` is called
- **THEN** it MUST report failure and MUST NOT change the stored action table

#### Scenario: Installing action for a normal signal succeeds
- **WHEN** `set_action(signum, action)` is called for a signal other than `SIGKILL` and `SIGSTOP`
- **THEN** it MUST report success and the stored action for `signum` MUST equal the provided action

### Requirement: Action query semantics
The implementation SHALL support querying the effective action for a signal:
- Querying `SIGKILL` or `SIGSTOP` MUST return `None`.
- Querying other signals MUST return a value:
  - If an action was installed, it MUST return that installed action.
  - Otherwise, it MUST return the `SignalAction::default()` value.

#### Scenario: Querying SIGSTOP returns None
- **WHEN** `get_action_ref(SIGSTOP)` is called
- **THEN** it MUST return `None`

#### Scenario: Querying unset action returns default
- **WHEN** `get_action_ref(signum)` is called for a signal other than `SIGKILL` and `SIGSTOP`, and no action is installed
- **THEN** it MUST return `Some(SignalAction::default())`

### Requirement: Mask update behavior
The implementation SHALL update the process signal mask and return the previous mask value.

#### Scenario: Mask update returns old value
- **WHEN** `update_mask(new_mask)` is called
- **THEN** it MUST set the internal mask to `new_mask` and MUST return the previous mask value

### Requirement: Delivery behavior while already handling a signal
While a process is already handling a signal, the implementation SHALL behave as follows:
- If the process is in a **frozen** (kernel-suspended) state:
  - It MUST remain suspended until an unmasked `SIGCONT` is pending.
  - When `SIGCONT` is consumed, it MUST clear the frozen state and report that handling progressed.
- If the process is handling a **user signal**:
  - It MUST report that the process is already handling a signal and MUST NOT deliver another signal.

#### Scenario: Frozen process continues suspending until SIGCONT
- **WHEN** `handling` indicates a frozen state and no unmasked `SIGCONT` is pending
- **THEN** `handle_signals()` MUST return `ProcessSuspended`

#### Scenario: Frozen process resumes on SIGCONT
- **WHEN** `handling` indicates a frozen state and an unmasked `SIGCONT` is pending
- **THEN** `handle_signals()` MUST consume `SIGCONT`, clear `handling`, and return `Handled`

#### Scenario: User-signal handler blocks nested delivery
- **WHEN** `handling` indicates a user-signal handling state
- **THEN** `handle_signals()` MUST return `IsHandlingSignal`

### Requirement: Signal-specific default delivery outcomes
When a deliverable signal is selected and the process is not already handling a signal, `handle_signals()` MUST:
- For `SIGKILL`: return `ProcessKilled` with an exit code derived from the signal number.
- For `SIGSTOP`: enter the frozen state and return `ProcessSuspended`.
- For other signals:
  - If a user action is installed, deliver to the user handler (see next requirement).
  - Otherwise, apply the default action policy:
    - `SIGCHLD` and `SIGURG` MUST be ignored.
    - All other signals MUST terminate the process with an exit code derived from the signal number.

#### Scenario: SIGCHLD is ignored by default
- **WHEN** a deliverable `SIGCHLD` is selected and no user action is installed for it
- **THEN** `handle_signals()` MUST return `Ignored`

#### Scenario: SIGSTOP suspends the process
- **WHEN** a deliverable `SIGSTOP` is selected
- **THEN** `handle_signals()` MUST set `handling` to the frozen state and MUST return `ProcessSuspended`

### Requirement: User-handler delivery via context rewrite
When delivering a signal to a user-installed handler, the implementation MUST:
- Save a snapshot of the current `LocalContext` into the handling state.
- Rewrite the current `LocalContext` such that:
  - The program counter MUST be set to the installed handler address.
  - Register `a0` (argument 0) MUST be set to the signal number as `usize`.
- Report successful delivery via `SignalResult`.

#### Scenario: Delivering to user handler rewrites PC and a0
- **WHEN** a deliverable signal other than `SIGKILL`/`SIGSTOP` is selected and a user action is installed
- **THEN** `handle_signals()` MUST store the prior `LocalContext` in `handling`, set the current PC to the handler address, set `a0` to the signal number, and return `Handled`

### Requirement: Returning from user handler restores saved context
The implementation SHALL support returning from a user signal handler via `sig_return()`:
- If the process is currently handling a user signal, it MUST restore the saved `LocalContext` into the current context and report success.
- Otherwise, it MUST report failure and MUST NOT discard the current handling state.

#### Scenario: sig_return restores context after user handler
- **WHEN** `sig_return(current_context)` is called while handling a user signal
- **THEN** it MUST restore the saved context into `current_context` and return `true`

#### Scenario: sig_return is rejected while frozen
- **WHEN** `sig_return(current_context)` is called while in the frozen state
- **THEN** it MUST return `false` and MUST keep the frozen handling state unchanged

### Requirement: Fork semantics for signal state
When cloning signal state across process fork, the implementation MUST:
- Clear pending received signals.
- Clear handling state.
- Preserve the signal mask.
- Preserve the signal action table.

#### Scenario: from_fork copies mask/actions but clears pending/handling
- **WHEN** `from_fork()` is called on a `SignalImpl`
- **THEN** the returned signal object MUST have an empty pending set and no handling state, and MUST have the same mask and action table as the source object

### Requirement: SignalSet bitset operations
The crate SHALL provide an internal bitset type used for representing signal sets:
- Bits MUST be addable/removable and queryable by index.
- It MUST support computing a set-union (bitwise OR) and set-difference (remove bits present in the other set).
- It MUST support selecting the lowest-index bit that is set in the pending set and not masked.

#### Scenario: find-first-one returns lowest unmasked pending bit
- **WHEN** multiple bits are pending, and some are masked
- **THEN** the bitset selection MUST return the lowest index bit that is pending and not masked, or `None` if none exist

## Public API

### Types
- `HandlingSignal`: describes whether the process is suspended by a kernel signal or is running a user signal handler with a saved `LocalContext`.
- `SignalImpl`: per-process signal implementation that fulfills the `signal::Signal` contract.

### Functions / Methods
- `SignalImpl::new() -> SignalImpl`: constructs a new signal implementation instance.
- `Signal::from_fork(&mut self) -> Box<dyn Signal>`: clones signal state for fork (see requirement above).
- `Signal::clear(&mut self)`: clears installed actions.
- `Signal::add_signal(&mut self, signal: SignalNo)`: marks a signal as pending.
- `Signal::is_handling_signal(&self) -> bool`: reports whether any signal is currently being handled.
- `Signal::set_action(&mut self, signum: SignalNo, action: &SignalAction) -> bool`: installs an action with restrictions.
- `Signal::get_action_ref(&self, signum: SignalNo) -> Option<SignalAction>`: queries effective action.
- `Signal::update_mask(&mut self, mask: usize) -> usize`: replaces the mask and returns the old mask.
- `Signal::handle_signals(&mut self, current_context: &mut LocalContext) -> SignalResult`: delivers at most one signal and may rewrite the context.
- `Signal::sig_return(&mut self, current_context: &mut LocalContext) -> bool`: restores saved context after user handler returns.

## Build Configuration

- build.rs: none
- 环境变量: none
- Feature flags: none
- Target constraints:
  - This crate SHALL be usable in `#![no_std]` environments that provide `alloc`.

## Dependencies

- Workspace crates:
  - `kernel-context` (preconditioned `LocalContext` semantics and mutation APIs)
  - `signal` (preconditioned signal identifiers, actions, results, and trait contract)
- External crates:
  - `alloc`

