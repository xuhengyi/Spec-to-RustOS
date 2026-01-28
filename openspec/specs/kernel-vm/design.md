## Context

`kernel-vm` 是一个小而聚焦的 `#![no_std]` crate，用于在内核中管理页表与地址空间映射。它以 `page-table` crate 提供的页表遍历框架为基础，并通过 `PageManager` 将“物理页的分配/释放 + 在当前地址空间中访问页表页/物理页”的细节交给上层实现。

该 crate 同时依赖 `alloc`（用于 `Vec`），因此需要调用方提供全局分配器。

## Goals / Non-Goals

- Goals:
  - 提供最小的页表/地址空间操作 API：建立映射、地址翻译、按记录的虚拟区间克隆地址空间
  - 将“物理页如何分配、如何在当前地址空间中访问物理页对应内存”的策略下沉到 `PageManager`
- Non-Goals:
  - 不提供 unmap/munmap、权限变更、映射回滚等完整 VM 子系统能力
  - 不负责提供全局分配器或内核直映窗口；仅声明前置条件

## Key Invariants

为保证 `AddressSpace` 的 `translate` 与 `cloneself` 行为正确，调用方与 `PageManager` 实现者需要共同维护以下不变量：

- **Root page table validity**: `PageManager::root_ptr()` 返回的指针必须指向一个有效的根页表页（`Pte<Meta>` 数组），并在地址空间存活期间保持可访问。
- **P2V mapping for page-table pages**: `page_table::walk/walk_mut` 需要在遍历过程中通过 `Visitor::meet`/`Decorator::meet` 进入下级页表页；因此当一个页表项 `pte` 被视为“可进入”时，`PageManager::p_to_v(pte.ppn())` 必须返回可解引用的页表页指针。
- **Contiguous physical pages for an area**: `AddressSpace::map_extern` 总是将 `range` 映射到从 `pbase` 开始的一段连续物理页序列；`cloneself` 假设这一点成立，并以“从首页开始按字节连续拷贝 `count << PAGE_BITS`”的方式复制内容。
- **Uniform flags within an area**: `map_extern` 为 `range` 内每个页使用相同 `flags`；`cloneself` 只读取该 area 的首个页表项的 flags，并将其用于整个 range。
- **Areas bookkeeping**: `AddressSpace::areas` 是 `pub` 字段，但 `cloneself` 把它当作可信的“由 `map`/`map_extern` 追加的虚拟区间列表”。若调用方手动修改/构造该列表，可能导致克隆拷贝错误甚至 panic。

## Safety Notes (`unsafe` dependencies)

当前实现包含多处 `unsafe`，其安全性依赖 `PageManager` 与不变量：

- `AddressSpace::root()` 使用 `unsafe { PageTable::from_root(root_ptr) }`：
  - 要求 `root_ptr` 指向有效页表页内存，且对齐/大小满足 `page-table` 的期望。
- `AddressSpace::map()` 对 `allocate()` 返回指针做原始内存写入与 slice 构造：
  - 要求 `allocate(count, ...)` 返回的内存区间覆盖 `count << PAGE_BITS` 字节且可写。
- `AddressSpace::translate()` 对 `p_to_v(ppn)` 返回值做 `add(addr.offset())` 并强转为 `T`：
  - 要求被翻译地址对应物理页在当前地址空间中可访问，且调用方选择的 `T` 与目标内存布局一致。
- `AddressSpace::cloneself()` 将源地址空间中首页的 `ppn` 视为连续 `size` 字节的起点并进行拷贝：
  - 依赖“area 映射到连续物理页”的不变量；否则会越界/拷贝错误数据。

## Error Handling / Observable Panics

- `map_extern` 在映射未完成时进入 `todo!()`；目前没有回滚逻辑。调用方应将其视为“可能 panic 且无回滚保证”的可观察行为。
- 多处 `assert!(!pte.is_valid())` 用于假设目标 PTE 未被占用；若调用方对已映射区间重复映射，可能触发 panic。
- `map` 使用 `assert!(size >= data.len() + offset)` 保证拷贝不越界；违反时 panic。

## Open Questions

- `map_extern` 在部分映射已写入但后续失败时的回滚/一致性策略：是“全部成功才提交”还是“尽力而为 + 记录已映射范围”？

