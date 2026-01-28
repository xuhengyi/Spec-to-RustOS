## Context

`ch6` is a teaching kernel binary that combines:
- an Sv39 virtual-memory setup for the kernel and user processes,
- a “foreign context” execution model (enter U-mode, return on trap) mediated by a portal transit page,
- a VirtIO block device used by `easy-fs` to load ELF programs from an on-disk image, and
- a minimal process manager providing runnable-queue scheduling plus basic process syscalls.

This capability is intentionally “single-binary, single-address-space-manager” and uses `spin` locks / `static mut` globals rather than a fully safe/structured kernel object model.

## Goals / Non-Goals

- Goals:
  - Provide an end-to-end path: boot → VM/heap → filesystem → load `initproc` ELF → schedule/run processes → trap/syscall handling → shutdown.
  - Keep core OS concepts visible in a small codebase (page-table flags, ELF segment mapping, fd table, and scheduling queue).
- Non-Goals:
  - Memory reclamation completeness: page-table deallocation (`PageManager::deallocate` / `drop_root`) is not implemented.
  - Preemptive scheduling or timer interrupts as a scheduling driver (this kernel is effectively cooperative from the `sched_yield` perspective).
  - Full VFS semantics (e.g., `link`/`unlink` are unimplemented in the `FSManager` implementation).

## Key Architecture Notes

### Kernel vs User address spaces

- The kernel constructs a single kernel Sv39 address space and enables translation by writing `satp`.
- Each user `Process` owns a distinct user Sv39 address space created from ELF `PT_LOAD` segments plus a user stack mapping.
- `fork` clones a process address space via the `kernel-vm` cloning facility.

### Portal transit mapping

The kernel and all user address spaces must share a consistent mapping for the “portal transit” page:
- The kernel maps a dedicated VPN (the maximum VPN) to the portal’s physical page.
- User address spaces copy the corresponding root page-table entry from the kernel address space (`map_portal`).

This provides a stable rendezvous point for switching into/out of foreign user execution.

### Filesystem bootstrapping

The global `FS` is constructed from:
- `BLOCK_DEVICE`: a VirtIO-backed `easy_fs::BlockDevice`
- `EasyFileSystem::open(...)` and `EasyFileSystem::root_inode(...)`

Boot loads `initproc` via `FS.open("initproc", RDONLY)` and `read_all(...)` into memory before parsing with `xmas-elf`.

## Unsafe / ABI Invariants

### `satp` activation and page-table correctness

- The kernel assumes it can safely write `satp` to enable Sv39 translation.
- The kernel assumes all mapped regions are identity-mapped (physical equals virtual for mapped extern regions) in the ways expected by `kernel-vm`.
- Any mismatch between mapping permissions and actual usage (e.g., writing to a non-writable mapped region) will fault and is treated as fatal for the current process or kernel.

### Global `static mut` state

`ch6` uses `static mut` globals such as `KERNEL_SPACE` and `PROCESSOR`.
Correctness depends on a single-core or otherwise externally serialized execution model where:
- initialization occurs exactly once before use,
- accesses to `PROCESSOR` are effectively serialized by the kernel’s control flow (and any internal synchronization provided by `PManager`),
- `KERNEL_SPACE` is fully initialized before any call paths that rely on address translation in the VirtIO HAL.

### Portal memory alignment/size

- The portal transit allocation assumes a page-aligned allocation of at least one Sv39 page.
- The code asserts the portal size is less than a page and uses a page-aligned `Layout`.

### VirtIO DMA memory lifetime

The VirtIO HAL:
- allocates DMA buffers with page-aligned, zeroed memory,
- deallocates them with the same size/alignment assumptions,
- translates virtual-to-physical using the kernel address space’s translation routine.

This assumes the allocated DMA buffers remain mapped and accessible for the duration required by the VirtIO driver and that address translation succeeds for the requested buffers.

## Risks / Trade-offs

- Memory leaks and missing deallocation: unimplemented page-table deallocation means long-running workloads may exhaust memory.
- Tight coupling between kernel VM mappings and device/driver assumptions (e.g., identity mapping and translation availability).
- Minimal error handling in boot-critical paths (many `unwrap`/`expect`/`assert`), appropriate for a tutorial kernel but not production use.

