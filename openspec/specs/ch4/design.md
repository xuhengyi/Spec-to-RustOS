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

## Algorithm Descriptions (实现指导)

以下描述为汇编逻辑、地址空间切换和上下文切换提供实现指导，使用描述性语言而非具体代码。实现者可根据目标平台（RISC-V Sv39）自行编写汇编。

### 1. Boot 汇编序列

**目的**：在进入高级语言入口前建立有效栈。

**算法**：
1. 将 `sp` 设置为内核镜像末尾地址（由链接脚本符号 `__end` 提供，或等价符号）。栈向下增长，故 `sp` 应指向栈区域的高端。
2. 无条件跳转到 `rust_main`。无需保存返回地址，因为 `rust_main` 永不返回。
3. 链接脚本必须将 `.boot.stack` 段放在 BSS 之后，大小由 `boot0!` 宏参数指定。

**约束**：Boot 入口必须位于 `.text.entry` 段，以便链接脚本将其放在内核起始处。

### 2. 线程上下文切换（调度线程 ↔ 用户上下文）

**目的**：从调度线程切换到用户进程执行，并在 trap 时恢复调度线程。

**数据结构**：上下文结构体需保存 x1..x31（ra, sp, gp, tp, t0..t2, s0..s1, a0..a7, s2..s11, t3..t6）共 31 个通用寄存器。`sepc` 单独保存。`sstatus` 在切换时通过 CSR 操作设置，不必全部存入结构体。

**出向切换（进入用户）算法**：
1. 在 Rust 包装层：将 `sscratch` 与上下文结构体指针交换，以便 trap 时能通过 `sscratch` 找到该结构体；设置 `sepc`、`sstatus`；保存 `ra` 到栈；调用裸函数入口。
2. 裸函数入口：在栈上分配 32×8 字节；将 x1..x31 依次存入该区域（x2 即 sp 也存入）；将 `stvec` 设为 trap 处理标签；从 `sscratch` 取出目标上下文指针，将当前 `sp` 存入该指针（作为调度上下文的 sp 备份），然后将 `sp` 设为该指针；从目标上下文恢复 x1..x31（包括 sp）；执行 `sret` 进入目标上下文。

**入向切换（trap 返回调度）算法**：
1. Trap 发生时，`stvec` 指向的标签：用 `csrrw sp, sscratch, sp` 交换 `sp` 与 `sscratch`，此时 `sp` 指向调度上下文的保存区（或等价结构）。
2. 将 x1..x31 保存到当前 `sp` 指向的区域（x2 位置存 `sscratch` 的旧值，即用户 sp）；从 `sp` 加载“调度上下文的 sp”（即之前存入的调度栈指针）；恢复 x1..x31；栈指针加回 32×8；`ret` 返回调度循环。

**关键点**：`sscratch` 在出向切换前被设为上下文指针，在 trap 时通过 `csrrw` 与 `sp` 交换，使得 trap 入口能立即使用正确的栈和上下文位置。

### 3. 异界传送门（地址空间切换）

**目的**：在切换 `satp` 时，保证执行流不中断。因为切换后当前 PC 的虚拟地址在新地址空间中可能无效，所以需要一段在**两个地址空间中同一虚拟地址**都映射且可执行的代码——即传送门代码。

**传送门页面布局**（从低地址到高地址）：
- 元数据（slot 数量、代码长度等）
- 传送门代码（位置无关，可复制到任意页）
- 每个 slot 一个 `PortalCache` 结构体

**PortalCache 布局**（按 8 字节对齐，具体由 `kernel_context::foreign::PortalCache` 定义，汇编必须与之一致）：
- 偏移 0：目标 a0（返回时写回）
- 偏移 8：目标 a1（或用于暂存）
- 偏移 16：目标 satp（切换时与当前 satp 交换）
- 偏移 24：目标 sstatus
- 偏移 32：目标 sepc
- 偏移 40：当前 stvec（切换时保存，返回时恢复）
- 偏移 48：当前 sscratch（切换时保存，返回时恢复）

**出向（进入用户地址空间）算法**：
1. 在 Rust 层：将目标 satp、sepc、sstatus、a0 写入 cache；将当前上下文的 PC 设为传送门代码入口，a0 设为 cache 地址；以“特权态、关中断”方式调用线程切换（因为传送门代码需特权执行）。
2. 线程切换的 `sret` 会跳转到传送门代码，此时 a0 = cache 地址。
3. 传送门代码：保存 a1 到 cache；将 cache 中的 satp 与当前 satp 交换，执行 `sfence.vma`；从 cache 加载 sstatus、sepc 到 CSR；将 `stvec`、`sscratch` 与 cache 中对应槽交换（保存当前值，以便返回时恢复）；从 cache 加载 a0、a1；`sret` 进入用户空间。

**入向（trap 返回内核地址空间）算法**：
1. 用户 trap 时，`stvec` 指向传送门内的 trap 标签。
2. Trap 时 `sscratch` 为 cache 地址（由出向时设置）。用 `csrrw a0, sscratch, a0` 交换，使得 a0 指向 cache，sscratch 指向用户 a0。
3. 将 a1 保存到 cache；将 sscratch 与 cache 中对应槽交换，恢复 sscratch，并将当前 a0 存入 cache；从 cache 取回内核 satp，与当前 satp 交换，执行 `sfence.vma`；从 cache 恢复 a1；从 cache 取回原 `stvec` 并写回 CSR；跳转到原 `stvec`（即调度线程的 trap 入口），从而完成“返回内核地址空间 + 回到调度上下文”的连锁恢复。

**关键点**：传送门代码和 cache 必须在同一物理页，且该页在 kernel 和用户地址空间中以**相同虚拟地址**映射，否则切换 satp 后无法访问。

**位置无关**：传送门代码必须位置无关（如用 `la`/`auipc` 等 PC 相对寻址获取 trap 标签地址），因为代码会被复制到 portal 页，在任意加载地址执行。

**实现注意**：调用 `ForeignContext::execute` 时，Rust 层会临时将 `supervisor` 置为 true、`interrupt` 置为 false，因为传送门代码必须在内核态、关中断下执行；切换完成后会恢复原值。

### 4. 调度栈与用户栈布局

**调度栈**：分配在内核地址空间的高端虚拟地址（例如 VPN 接近 `1<<26` 的区间），用于调度线程的栈帧。栈顶（sp 初值）应设为该区间的最高地址。

**用户栈**：每个进程的地址空间在用户空间高端（例如 VPN `(1<<26)-2` 到 `(1<<26)`）映射 2 页栈。栈顶（sp 初值）设为该区间最高地址（如 `1<<38` 对应 Sv39 下 2 页栈的顶端）。

### 5. 用户指针翻译与 syscall 安全

**目的**：syscall 处理程序需要访问用户传入的指针（如 `write` 的 buf、`clock_gettime` 的 tp），必须通过地址空间翻译验证可访问性。

**算法**：对用户提供的 `(vaddr, size)`，调用 `AddressSpace::translate(vaddr, required_flags)`。若返回 `Some(ptr)`，则 `ptr` 在 `[vaddr, vaddr+size)` 范围内可安全访问；若返回 `None`，则拒绝 syscall 并返回错误。`required_flags` 根据访问类型选择（可读、可写等）。

### 6. sscratch 在各阶段的值（实现检查清单）

| 阶段 | sscratch 值 | 用途 |
|------|-------------|------|
| 进入 execute_naked 前 | 目标上下文指针（用户 LocalContext） | 线程切换时保存/恢复用 |
| 传送门出向 sret 前 | cache 地址 | trap 时 portal 用 a0=sscratch 找到 cache |
| 用户态运行中 | cache 地址 | 同上 |
| 用户 trap 进入 portal | cache 地址（出向 sret 前已设置） | portal 首条 `csrrw a0, sscratch, a0` 使 a0=cache |
| portal 恢复后、jr 前 | 用户上下文指针（从 cache 6*8 恢复） | execute_naked trap 期望 sscratch=用户上下文，用于 `csrrw sp, sscratch, sp` 后保存用户寄存器 |
| execute_naked trap 返回后 | 由 LocalContext::execute 包装恢复 | 恢复调用前值 |

**常见错误**：若 portal 出向未将 stvec 设为 portal trap、未将 sscratch 设为 cache，用户 trap 会跳转到错误地址或无法找到 cache，导致挂起或二次 fault。

### 7. execute() 挂起排查清单

当 `execute()` 不返回时，按以下顺序排查：

1. **stvec 是否指向 portal trap entry**
   - 在用户态首次 ecall 前，`stvec` 必须为 portal 页内 trap 标签的虚拟地址（即 portal 基址 + 代码段内 trap 标签偏移）。
   - 该地址在 kernel 与用户地址空间中必须相同（portal 页同一映射）。
   - 若 stvec 仍指向内核 trap 入口，用户 trap 会跳转到内核空间地址，在用户页表下可能 fault 或挂起。
   - 调试时可打印 `stvec` 与 portal 页基址，确认 stvec 落在 portal 页范围内。

2. **Trap 返回路径是否正确**
   - portal trap handler 恢复 satp 后，必须 `jr` 到「原 stvec」（即 execute_naked 的 trap 标签）。
   - 不能跳转到独立的 `__trap_handler`，除非其行为与 execute_naked trap 完全一致（期望 sscratch=用户上下文，保存用户寄存器后加载调度上下文并 ret）。

3. **用户态入口与 heap::init()**
   - 检查用户 `_start` 是否调用 `heap::init()` 且无死循环。
   - 检查 ELF entry 是否正确（应为 `_start` 或等价入口）。
   - 检查用户栈 sp 是否指向有效可写区域顶端。

4. **init_transit 调用时机**
   - `MultislotPortal::init_transit` 必须在 portal 页已映射且当前处于该地址空间时调用。
   - 通常在 `schedule()` 内、首次 `execute()` 前调用，此时处于 kernel 地址空间且 portal 已映射。

## Feature Matrix

`ch4` defines no crate-level feature flags. Dependency features used:

- `sbi-rt`: `legacy`
- `kernel-context`: `foreign`
- `syscall`: `kernel`

