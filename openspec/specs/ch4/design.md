## Context

`ch4` is a minimal RISC-V Sv39 kernel binary that boots via the workspace `linker` crate, sets up an Sv39 address space, loads embedded user ELF applications into per-process address spaces, and runs them sequentially with a small syscall/trap handler.

This chapter intentionally prioritizes a “single core, single run-queue, minimal services” design over completeness (e.g., no general process lifecycle management, no page reclamation).

## Goals / Non-Goals

- Goals:
  - Provide a deterministic boot sequence (BSS/console/log/heap).
  - Establish a working Sv39 kernel address space and install it via `satp`.
  - Load ELF64 RISC-V executables into user address spaces with basic segment permissions.
  - Execute user processes via `kernel_context` foreign context switching.
  - Support a minimal syscall set sufficient for basic user output and time.

- Non-Goals:
  - Preemptive multitasking or time-sliced scheduling.
  - Robust process isolation hardening (this is educational code with `unsafe` shortcuts).
  - Full POSIX-like syscall coverage.
  - Memory reclamation, page freeing, or long-running stability.

## Architecture Overview

- **Boot path**: `linker::boot0!` transfers control to `rust_main`.
- **Runtime init**: `rust_main` zeroes BSS, initializes console/logging, initializes heap, then constructs the kernel address space.
- **Kernel address space**: `kernel_space(...)` maps kernel regions + heap + one portal transit page, installs `satp`, and returns an `AddressSpace<Sv39, Sv39Manager>`.
- **App loading**: iterates embedded apps from `linker::AppMeta::locate()`, parses as ELF, creates a `Process` with its own `AddressSpace`, and maps an “portal PTE” into the process root.
- **Execution**: a scheduling thread (`LocalContext::thread`) runs `schedule()`, which initializes the portal transit and syscall subsystem, then loops over the first process until it exits or is terminated.

## Key Decisions

- **Sequential scheduling**: processes are executed one-at-a-time (always index 0). There is no round-robin queue rotation; termination removes the current process and exposes the next.
- **Portal-based context separation**: user execution is performed via `kernel_context::foreign::{ForeignContext, MultislotPortal}`. One dedicated virtual page (`PROTAL_TRANSIT`) is used to host the portal transit mapping.
- **String-based page permission encoding**: memory permissions are expressed via `VmFlags::build_from_str(...)` / `FromStr`, making permission intent readable in this learning-oriented code.

## Unsafe / Memory Model Constraints (MUST-hold invariants)

The implementation relies on the following invariants; violating them can cause immediate crashes or undefined behavior:

- **Identity mapping assumption**:
  - `kernel_space` maps regions with `PPN::new(s.floor().val())` (using virtual address-derived values as the physical page number).
  - `Sv39Manager::p_to_v` and `Sv39Manager::v_to_p` also assume a direct, deterministic mapping relationship that is consistent with the address space layout.
  - Therefore, the platform and `linker::KernelLayout` MUST be compatible with the mapping scheme used here (effectively requiring a consistent mapping relationship for the addresses involved).

- **Heap transfer correctness**:
  - `kernel_alloc::transfer` receives a raw slice derived from the kernel layout end to `layout.start() + MEMORY`.
  - `MEMORY` MUST be chosen such that this range is valid RAM and does not overlap reserved/IO regions.

- **Portal page allocation/alignment**:
  - The portal allocation uses a page-aligned `Layout` and asserts its size is less than one page.
  - The portal physical buffer MUST remain valid for the lifetime of portal usage (this design never frees it).

- **User pointer translation must be range-correct**:
  - Syscall implementations translate user pointers via `AddressSpace::translate(VAddr, flags)`.
  - The underlying translation MUST ensure the returned pointer is safe to use for the intended *byte range* (`count` bytes for `write`, `sizeof(TimeSpec)` bytes for `clock_gettime`) and for the required access mode (read/write).

- **UTF-8 assumption in console write**:
  - `write` prints via `core::str::from_utf8_unchecked(...)`.
  - Therefore, user buffers passed to `write` for `STDOUT`/`STDDEBUG` MUST contain valid UTF-8 for the printed range, otherwise undefined behavior may occur.

- **Alignment assumption for `TimeSpec` writes**:
  - `clock_gettime` writes a `TimeSpec` by treating the translated user pointer as a `TimeSpec` location.
  - The user pointer `tp` MUST be aligned sufficiently for `TimeSpec`, and the translated mapping MUST respect that alignment.

- **Unimplemented page freeing**:
  - `Sv39Manager::{deallocate, drop_root}` are `todo!()` in this chapter.
  - Any path that attempts to free pages via the `PageManager` interface MUST NOT be invoked in `ch4`’s intended execution model.

## Operational Limitations

- **No recovery on unexpected traps**: unsupported traps terminate the current process.
- **Minimal process identity**: the syscall caller identity is hard-coded (e.g., `Caller { entity: 0, flow: 0 }` in the scheduling loop) and is not a general mechanism.
- **No exit status propagation**: `exit(status)` is accepted but status handling is intentionally minimal.

## Feature Matrix

`ch4` defines no crate-level feature flags. Dependency features used:

- `sbi-rt`: `legacy`
- `kernel-context`: `foreign`
- `syscall`: `kernel`

