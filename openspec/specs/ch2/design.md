## Context

`ch2` is a small teaching kernel that demonstrates a single-core, sequential batch execution model: run an embedded user application until it traps, handle syscalls, and move on when the application exits or is killed.

Unlike later chapters, `ch2` intentionally keeps many mechanisms minimal and makes strong assumptions at safety boundaries.

## Goals / Non-Goals

### Goals
- Provide a clear control-flow skeleton for: boot → init → per-app run loop → trap decode → syscall dispatch → shutdown.
- Demonstrate integration points with workspace crates: `linker`, `rcore-console`, `kernel-context`, and `syscall`.

### Non-Goals
- Robust memory isolation between kernel and user applications.
- Complete syscall coverage, scheduling, or multi-process management.
- Safety-hardening for user pointers passed into syscalls.

## Key Flow (High-Level)

- **Boot & init**: clear `.bss`, set up console/log, initialize syscall subsystems.
- **Batch loop**: enumerate apps from `linker::AppMeta`, create a user context per app, set a user stack, then repeatedly `execute()` until a trap occurs.
- **Trap decode**:
  - `UserEnvCall`: dispatch syscall, update registers/PC, possibly terminate on exit/unsupported.
  - Anything else: treat as fatal and kill the app.
- **Shutdown**: after all apps are processed, call SBI `system_reset(Shutdown, NoReason)`.

## Safety Boundaries and Assumptions

### Decision: Trusting user pointers for `write`

The syscall IO implementation for `write` converts `(buf, count)` into `&[u8]` and then into `&str` using `from_utf8_unchecked`, and prints it directly.

**Assumptions:**
- The `buf..buf+count` range is readable in the current address space when handling the syscall.
- The bytes are valid UTF-8 (or at least safe to interpret as UTF-8 without validation).

**Trade-off:**
- This keeps the chapter focused, but it is not a safe interface for untrusted user memory.

### Decision: User stack is a kernel-allocated local buffer

`ch2` provisions a per-application stack as a stack-allocated local (`MaybeUninit<[usize; 256]>`) and points the user context stack pointer (`sp`) at it.

**Assumptions:**
- The stack object remains live for the duration of the app run loop.
- The compiler does not optimize the stack away (an explicit `black_box` is used to prevent this).

**Trade-off:**
- Simpler than managing a dedicated user stack region in memory, but not representative of hardened kernels.

### Unsafe operations that must be correct

`ch2` relies on the following unsafe operations behaving correctly according to their contracts:
- `linker::KernelLayout::locate().zero_bss()`: must only zero the intended `.bss` memory range.
- `LocalContext::execute()`: must correctly switch into user mode and return to the kernel on trap without corrupting kernel state.
- `asm!("fence.i")`: assumes the platform supports the RISC-V instruction and that it is sufficient for the intended instruction-cache synchronization point.

## Risks / Trade-offs

- **Memory safety risk**: syscall handlers may dereference invalid user pointers.
- **Correctness risk**: trap return path depends on correct `move_next()` semantics and register mapping.
- **Observability**: minimal error reporting (primarily logs) and a "kill on unexpected trap" policy.

## Open Questions

- Should the `write` path validate UTF-8 or treat output as raw bytes to avoid `from_utf8_unchecked`?
- Should the user stack be moved into a dedicated memory region managed by the kernel rather than a local variable?

