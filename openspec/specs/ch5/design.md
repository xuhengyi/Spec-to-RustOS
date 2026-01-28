## Context

`ch5` is a small teaching kernel for RISC-V S-mode that demonstrates “process management + syscalls” without relying on a Rust runtime (`#![no_std]`, `#![no_main]`). It constructs a Sv39 kernel address space, maps a special “portal transit” page, loads user processes from embedded ELF payloads, and runs them via foreign-context execution.

## Goals / Non-Goals

- Goals:
  - Provide a minimal end-to-end flow: boot → init memory/page table → load `initproc` → run a scheduler loop → handle syscalls/traps → shutdown.
  - Demonstrate process primitives (`fork/exec/wait/getpid/exit`) and basic IO/clock syscalls.
  - Keep the control flow auditable: a single scheduling loop with explicit trap decoding and syscall dispatch.
- Non-Goals:
  - Provide a stable Rust library API (this is a binary crate; the “API” is the boot/symbol contract and runtime behavior).
  - Provide a complete memory manager: Sv39 page-table deallocation and root drop are not implemented in `Sv39Manager` (left as `todo!()`).
  - Provide a full POSIX process model: child exit/wait semantics are constrained by the underlying task-manager behavior and this crate’s minimal integration.

## Key Design Decisions

- **Single global processor manager**: `PROCESSOR` is a global `PManager<Process, ProcManager>`; task selection is delegated to the task-manager crate, while `ProcManager` supplies storage and a FIFO-ready queue.
- **Sv39 address spaces via `kernel-vm`**: both kernel and user address spaces are built using `AddressSpace<Sv39, Sv39Manager>`, with permissions expressed through `VmFlags`.
- **Portal mapping model**: a single portal-transit virtual page is mapped in the kernel page table and then copied into each user address space by copying the relevant root PTE. This keeps the portal VA stable across address spaces.
- **Syscall integration**: `syscall::handle` is treated as the syscall routing point; this crate provides trait implementations (IO/Process/Scheduling/Clock) in `SyscallContext`.

## Unsafe / ABI Invariants

- **Bootstrap/entry**:
  - Entry symbols and stack setup are provided by `linker::boot0!`; correctness depends on the generated linker script and SEE entry behavior.
- **`satp` activation**:
  - The kernel writes `satp` directly to enable Sv39 translation. Correctness requires that the root page table PPN is valid and that Sv39 is supported by the platform.
- **Portal transit mapping**:
  - `map_portal` writes directly into a page-table entry selected by a fixed VPN index. Correctness depends on `KERNEL_SPACE` being initialized and remaining valid for the lifetime of the kernel.
- **User memory translation for syscalls**:
  - Syscall implementations translate user pointers through `AddressSpace::translate` and then dereference raw pointers. Correctness requires the translation to enforce appropriate permissions and to return pointers valid for the requested `count`.
- **Page-table allocation**:
  - `Sv39Manager` allocates page-table pages using `alloc_zeroed` with page alignment assumptions. Deallocation/root-drop paths are unimplemented; changes that start freeing page tables must implement these safely.

## Risks / Trade-offs

- **Incomplete reclaiming**: leaving `deallocate` / `drop_root` as `todo!()` can leak memory or prevent teardown paths from being correct if they become reachable.
- **Simplified scheduling**: FIFO-ready queueing is simple and predictable but not fair under all workloads and lacks priorities/time-slicing.
- **Tight coupling to embedded apps**: `exec` only supports names that exist in the embedded registry derived from the app metadata; there is no filesystem layer.

## Open Questions

- Should `Sv39Manager::{deallocate,drop_root}` be implemented to make address-space teardown safe and to support `exec`/process exit without leaking page-table pages?
- Should process exit/wait semantics be extended to preserve exit codes and parent-child relationships in a more POSIX-like way (subject to task-manager semantics)?
