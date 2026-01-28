## Context

`ch7` is a teaching kernel chapter that extends `ch6` with a **basic signal implementation** and related syscall wiring. The crate is intentionally a single binary that ties together:
- boot/linker layout,
- page table and address-space management (Sv39),
- a portal-based context mechanism for entering/exiting user code,
- a process manager/scheduler,
- a minimal filesystem stack on a VirtIO block device,
- and a signal subsystem (`signal` + `signal-impl`) integrated into the syscall/trap flow.

## Goals / Non-Goals

- Goals:
  - Provide an end-to-end runnable kernel that can load and execute an `initproc` from the filesystem.
  - Provide syscall plumbing for basic process lifecycle and signals (`kill`, `sigaction`, `sigprocmask`, `sigreturn`).
  - Keep the control flow small enough to be readable and specifiable.
- Non-Goals:
  - Full trap handling (page faults, timer interrupts, external interrupts).
  - Fully correct signal delivery timing; current placement is explicitly temporary.
  - Complete `easy_fs` feature coverage (e.g., link/unlink are unimplemented).

## Key Design Decisions

- **Portal-based user execution**
  - The kernel runs user code via `kernel_context::foreign::ForeignContext::execute` and a `MultislotPortal`.
  - This design avoids implementing a full trap return path in this chapter’s kernel loop, at the cost of limiting where post-trap logic can be injected.

- **Temporary signal handling placement**
  - Signal handling is invoked **after** `syscall::handle` returns.
  - This is called out as a stopgap: correct signal delivery would conceptually occur after all trap handling and immediately before returning to user mode.

- **Minimal IO model**
  - `STDIN` is implemented by blocking reads from SBI legacy `console_getchar`.
  - `STDOUT`/`STDDEBUG` write to the console.
  - Other file descriptors are file-backed via `easy_fs`.

## Unsafe / ABI / Memory Invariants

- `KERNEL_SPACE` and `PROCESSOR` are `static mut` globals.
  - Correctness requires single-core or otherwise synchronized access patterns.
  - Code assumes these are initialized before any use and remain valid for the kernel lifetime.

- Sv39 page-table memory ownership is tracked via a custom `OWNED` bit in `VmFlags`.
  - `Sv39Manager::deallocate` and `drop_root` are `todo!()`; the chapter assumes page tables are not reclaimed.

- Address translation is used to validate user pointers for syscalls.
  - Safety requires that `AddressSpace::translate` enforces permissions (`READABLE`/`WRITEABLE`) consistent with the intended syscall.

- The VirtIO HAL uses kernel address-space translation for `virt_to_phys`.
  - This assumes the kernel address space maps the relevant virtual addresses and the returned physical address is suitable for DMA.

## Known Limitations / Follow-ups

- Missing timer interrupts limits support for signal-based sleep/suspend and more realistic scheduling.
- Lack of a generalized `File` trait and “blocking in kernel” primitives makes pipes and some IO patterns out of scope.
- Process exit code reporting and `wait` semantics are simplified compared to upstream tutorials and may not satisfy all signal test cases.

