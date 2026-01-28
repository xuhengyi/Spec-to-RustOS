# Capability: kernel-context

## Purpose

`kernel-context` is a `#![no_std]` crate that provides:
- a RISC-V Supervisor-mode (S-mode) local thread context representation (`LocalContext`) with helpers to read/write architectural register slots and program counter, and
- an (optional) “foreign address space” execution facility (`foreign` feature) to execute a `LocalContext` in a different address space via a portal mechanism.

## Requirements

### Requirement: Local context construction
The crate MUST provide constructors for `LocalContext` such that:
- `LocalContext::empty()` produces a zeroed context with `supervisor == false`, `interrupt == false`, and `pc == 0`.
- `LocalContext::user(pc)` produces a context configured to return to user mode (i.e. `supervisor == false`) at `pc`, and MUST set `interrupt == true`.
- `LocalContext::thread(pc, interrupt)` produces a context configured to return to supervisor mode (i.e. `supervisor == true`) at `pc`, and MUST set `interrupt` to the provided value.

#### Scenario: Create an empty context
- **WHEN** `LocalContext::empty()` is called
- **THEN** the returned `LocalContext` has `pc() == 0`
- **AND THEN** `supervisor == false` and `interrupt == false`

#### Scenario: Create a user context
- **WHEN** `LocalContext::user(ENTRY)` is called
- **THEN** the returned `LocalContext` has `pc() == ENTRY`
- **AND THEN** `supervisor == false`
- **AND THEN** `interrupt == true`

#### Scenario: Create a supervisor thread context
- **WHEN** `LocalContext::thread(ENTRY, INTR)` is called
- **THEN** the returned `LocalContext` has `pc() == ENTRY`
- **AND THEN** `supervisor == true`
- **AND THEN** `interrupt == INTR`

### Requirement: Local context register slot accessors
`LocalContext` MUST provide accessor methods for its general register slots such that:
- `x(n)` and `x_mut(n)` access the \(n\)-th integer register slot using 1-based indexing (i.e. `n = 1` refers to `x1`, …, `n = 31` refers to `x31`).
- `a(n)` and `a_mut(n)` access argument registers by mapping `a0..` onto integer slots `x10..` (i.e. `a(0)` is `x10`).
- `ra()` MUST return the value of `x1`.
- `sp()`, `sp_mut()` MUST read/write the value of `x2`.

#### Scenario: Read and write an integer register slot
- **WHEN** a caller writes through `*ctx.x_mut(3) = V`
- **THEN** `ctx.x(3) == V`

#### Scenario: Read and write an argument register
- **WHEN** a caller writes through `*ctx.a_mut(0) = V`
- **THEN** `ctx.a(0) == V`

#### Scenario: Read `ra` and `sp`
- **WHEN** a caller sets `x1` and `x2` via `x_mut(1)` and `x_mut(2)`
- **THEN** `ra()` returns the value stored in `x1`
- **AND THEN** `sp()` returns the value stored in `x2`

### Requirement: Local context program counter helpers
`LocalContext` MUST provide program-counter helpers such that:
- `pc()` returns the saved return PC for the context.
- `pc_mut()` returns a mutable reference to that saved PC.
- `move_next()` MUST advance the saved PC by 4 bytes using wrapping arithmetic.

#### Scenario: Advance PC to the next instruction
- **WHEN** a caller invokes `move_next()` on a context with `pc() == P`
- **THEN** `pc()` becomes `P + 4` (wrapping on overflow)

### Requirement: Local context execution
`LocalContext` MUST provide an `unsafe` execution operation `execute(&mut self) -> usize` such that:
- It switches into the represented context using a RISC-V `sret`-based control transfer.
- It MUST write `sepc` and `sstatus` based on the context’s saved PC and `(supervisor, interrupt)` flags.
- It MUST temporarily install a trap vector (`stvec`) suitable for resuming the caller (“scheduler”) context after a trap/return.
- It MUST update the context’s saved PC to the post-execution `sepc` observed when control returns to the caller.
- It MUST return the post-execution `sstatus` value.

#### Scenario: Execute and return to the caller
- **WHEN** a caller invokes `unsafe { ctx.execute() }`
- **THEN** control transfers to `ctx.pc()` in the privilege mode determined by `ctx.supervisor`
- **AND THEN** the call eventually returns to the caller when the executed context traps/returns
- **AND THEN** `ctx.pc()` is updated to the PC at which the trap/return occurred
- **AND THEN** the returned `usize` equals the `sstatus` observed on return

### Requirement: Foreign address space execution (feature `foreign`)
When the crate is built with feature `foreign`, it MUST provide a `foreign` module that enables executing a `LocalContext` in a different address space via a portal mechanism, including:
- `ForeignContext` as a pairing of a `LocalContext` and a target address space identifier (`satp`).
- `PortalCache` as a `#[repr(C)]` cache record that is expected to be mapped in a shared (“public”) address space.
- `ForeignPortal` and `MonoForeignPortal` traits to describe portal entry and cache placement in the shared address space.
- `SlotKey` and standard `SlotKey` implementations for `()`, `usize`, and `TpReg`.

#### Scenario: Build without `foreign`
- **WHEN** the crate is built without feature `foreign`
- **THEN** the `kernel_context::foreign` module is not available

#### Scenario: Build with `foreign`
- **WHEN** the crate is built with feature `foreign`
- **THEN** the `kernel_context::foreign` module is available
- **AND THEN** portal-based foreign execution APIs are available under it

### Requirement: Portal cache initialization and address reporting (feature `foreign`)
With feature `foreign` enabled, `PortalCache` MUST provide methods such that:
- `init(satp, pc, a0, supervisor, interrupt)` initializes the cache with the target execution parameters and computed `sstatus`.
- `address()` returns the cache’s address as a `usize` suitable for passing to portal entry code.

#### Scenario: Initialize a portal cache
- **WHEN** a caller invokes `cache.init(SATP, PC, A0, SUP, INTR)`
- **THEN** the cache stores `satp == SATP` and `sepc == PC`
- **AND THEN** the cache stores `a0 == A0`

#### Scenario: Pass cache address to portal code
- **WHEN** a caller invokes `cache.address()`
- **THEN** the returned value equals the address of `cache` in memory

### Requirement: Foreign portal addressing (feature `foreign`)
With feature `foreign` enabled, `MonoForeignPortal` MUST define the portal object layout contract, and the blanket `impl<T: MonoForeignPortal> ForeignPortal for T` MUST:
- compute `transit_entry()` as `transit_address() + text_offset()`, and
- compute `transit_cache(key)` as a mutable `PortalCache` reference at `transit_address() + cache_offset(key.index())`.

#### Scenario: Compute portal entry address
- **WHEN** a `MonoForeignPortal` implementation provides `transit_address()` and `text_offset()`
- **THEN** `unsafe { portal.transit_entry() }` equals their sum

#### Scenario: Compute portal cache address by slot key
- **WHEN** a `MonoForeignPortal` implementation provides `cache_offset(key)`
- **THEN** `unsafe { portal.transit_cache(key) }` refers to the `PortalCache` located at `transit_address() + cache_offset(key.index())`

### Requirement: Foreign context execution via portal (feature `foreign`)
With feature `foreign` enabled, `ForeignContext::execute(&mut self, portal, key) -> usize` MUST:
- run the portal code in supervisor mode with interrupts disabled (for the duration of the portal execution),
- write the local context’s `pc` to the portal entry, and pass the portal cache address in `a0`,
- restore the original `(supervisor, interrupt)` flags of the `LocalContext` before returning, and
- update the local context’s `a0` from the `PortalCache` on return.

#### Scenario: Execute a foreign context and restore flags
- **WHEN** a caller invokes `unsafe { foreign_ctx.execute(&mut portal, key) }`
- **THEN** the portal is entered with `LocalContext.supervisor == true` and `LocalContext.interrupt == false` during portal execution
- **AND THEN** on return, the original `LocalContext.supervisor` and `LocalContext.interrupt` values are restored

#### Scenario: Observe a0 value returned from portal
- **WHEN** the portal writes a value into the cache’s `a0` field during execution
- **THEN** after `ForeignContext::execute` returns, `foreign_ctx.context.a(0)` equals that cached `a0` value

### Requirement: Multislot portal object (feature `foreign`)
With feature `foreign` enabled, the crate MUST provide `foreign::MultislotPortal` such that:
- `calculate_size(slots)` returns the total byte size required for a portal object with `slots` cache slots, including portal code and caches.
- `unsafe init_transit(transit, slots)` initializes a `MultislotPortal` located at `transit` by copying portal code into place and filling metadata, and returns a `&'static mut MultislotPortal`.

#### Scenario: Calculate multislot portal size
- **WHEN** a caller computes `MultislotPortal::calculate_size(N)`
- **THEN** the returned size is sufficient to hold the portal header, portal code, and `N` `PortalCache` slots

#### Scenario: Initialize a multislot portal in a public address space mapping
- **WHEN** a caller invokes `unsafe { MultislotPortal::init_transit(TRANSIT_ADDR, N) }`
- **THEN** the portal code is copied into memory at `TRANSIT_ADDR + sizeof(MultislotPortal)`
- **AND THEN** the returned reference points to `TRANSIT_ADDR`

## Public API

### Types
- `LocalContext`: RISC-V local thread context (general registers, `sepc`, plus `supervisor`/`interrupt` flags).

### Functions / Methods
- `LocalContext::empty() -> LocalContext`: Create a blank context.
- `LocalContext::user(pc: usize) -> LocalContext`: Create a user-mode context starting at `pc` with interrupts enabled.
- `LocalContext::thread(pc: usize, interrupt: bool) -> LocalContext`: Create a supervisor-mode thread context starting at `pc`.
- `LocalContext::{x,x_mut,a,a_mut,ra,sp,sp_mut,pc,pc_mut,move_next}`: Accessors and PC helper.
- `unsafe LocalContext::execute(&mut self) -> usize`: Execute the context; writes CSRs; returns `sstatus`.

### Module `foreign` (feature `foreign`)

#### Types
- `PortalCache`: `#[repr(C)]` portal cache record in the shared address space.
- `ForeignContext`: A foreign (different address space) thread context: `{ context: LocalContext, satp: usize }`.
- `TpReg`: A `SlotKey` that reads the current `tp` register to obtain a slot index.
- `MultislotPortal`: A portal object with contiguous code and multiple cache slots.

#### Traits
- `ForeignPortal`: Provides access to portal entry and a `PortalCache` slot in the shared address space.
- `MonoForeignPortal`: Layout contract for portal objects whose code and caches are contiguous.
- `SlotKey`: Converts a key to a cache slot index via `index(self) -> usize`.

#### Methods / Functions
- `PortalCache::{init,address}`
- `unsafe ForeignPortal::{transit_entry,transit_cache}`
- `ForeignContext::execute`
- `MultislotPortal::{calculate_size, init_transit}`

## Build Configuration

- **Crate attributes**: `#![no_std]`
- **Features**:
  - `foreign`: Enables `pub mod foreign` and adds dependency on `spin`.
- **build.rs**: none
- **Environment variables / generated files**: none

## Dependencies

- **Workspace crates**: none
- **External crates**:
  - `spin` (optional, enabled by feature `foreign`): used for `spin::Lazy`.

## Safety & Platform Preconditions (Non-Normative)

- The execution primitives are RISC-V S-mode specific and rely on S-mode CSRs (`sstatus`, `sscratch`, `sepc`, `stvec`, and for `foreign`, `satp`) and `sret`.
- `LocalContext::execute` and `foreign` portal operations are `unsafe` because they manipulate privileged machine state and assume correct memory mappings for context/portal objects.
