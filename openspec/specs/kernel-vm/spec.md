# Capability: kernel-vm

本规格描述 crate `kernel-vm`（目录 `kernel-vm/`）的对外契约与边界：提供内核虚拟内存/页表管理的最小抽象（`PageManager`）以及地址空间容器（`AddressSpace`），并基于 `page-table` crate 完成映射建立、地址翻译与地址空间克隆。

## Purpose

为 `kernel-vm` 定义可验证的对外契约：明确调用方必须满足的前置条件（尤其是页表内存可访问性、物理页所有权、以及 `unsafe` 使用所依赖的不变量），并明确 panic/断言等可观察行为。

## Requirements

### Requirement: 物理页管理抽象 `PageManager`
crate `kernel-vm` MUST 提供 trait `PageManager<Meta: VmMeta>`，用于抽象“为页表与用户页分配/释放物理页，以及在当前地址空间中访问这些物理页”的能力。

调用方实现 `PageManager` 时 MUST 满足以下前置条件（否则后续 `AddressSpace` 的行为 MAY panic 或导致未定义行为）：
- `new_root()` MUST 创建一个新的根页表页，并使其可通过 `root_ptr()` 在当前地址空间中被安全访问。
- `root_ptr()` MUST 返回指向根页表页首个 `Pte<Meta>` 的非空指针，且其生命周期 MUST 覆盖所有使用该地址空间页表的操作。
- `p_to_v(ppn)` 与 `v_to_p(ptr)` MUST 在其覆盖范围内保持一致性：对由 `allocate`/`new_root` 产生并仍然存活的页，`v_to_p(p_to_v(ppn)) MUST == ppn`。
- `allocate(len, flags)` MUST 分配 `len` 个**连续的**物理页，并返回这些页在当前地址空间中的一段**连续可访问**的虚拟内存起始指针；实现者 MAY 根据需要修改 `flags`（例如补充 `VALID`）。
- `deallocate(pte, len)` MUST 回收 `pte` 指示起始物理页起、长度为 `len` 的页序列，并返回一个 `usize`（该返回值的具体语义由实现者定义；`kernel-vm` 仅透传/不解释）。
- `check_owned(pte)` MUST 指示 `pte` 指向的物理页是否由该 `PageManager` “拥有”，以便 `kernel-vm` 在遍历页表时决定是否可以进入下级页表页（见 `AddressSpace::map_extern`）。
- `drop_root()` MUST 释放根页表页（与 `new_root` 对应）。

#### Scenario: 自定义 `PageManager` 驱动 `AddressSpace` 构造
- **WHEN** 调用方提供一个满足上述前置条件的 `PageManager` 实现 `M`
- **AND WHEN** 调用 `AddressSpace::<Meta, M>::new()`
- **THEN** `M::new_root()` MUST 被用于构造该地址空间的根页表页，且后续 `root_ppn()/root()` 操作 MUST 可工作

### Requirement: 地址空间容器 `AddressSpace` 的创建与根页表访问
crate `kernel-vm` MUST 提供 `pub struct AddressSpace<Meta: VmMeta, M: PageManager<Meta>>` 作为地址空间容器，并提供以下能力：
- `AddressSpace::new()` MUST 创建一个新的地址空间，其 `areas` MUST 为空，且其根页表 MUST 由 `M::new_root()` 创建。
- `AddressSpace::root_ppn()` MUST 返回根页表的物理页号（经由 `PageManager::root_ppn()`）。
- `AddressSpace::root()` MUST 以 `unsafe { PageTable::from_root(root_ptr) }` 的方式构造一个 `page_table::PageTable<Meta>` 视图，用于遍历/修改页表。

`AddressSpace` 在内部使用 `alloc::vec::Vec`，因此调用方 MUST 在链接/运行环境中提供可用的全局分配器（`#[global_allocator]`），否则使用 `AddressSpace::new()` 等 API MAY 在分配时 panic/abort（由分配器策略决定）。

#### Scenario: 创建空地址空间并读取根页表物理页号
- **WHEN** 调用方已提供可用的全局分配器
- **AND WHEN** 调用 `let s = AddressSpace::<Meta, M>::new(); let r = s.root_ppn();`
- **THEN** `r` MUST 等于 `M::root_ppn()` 的返回值

### Requirement: 建立外部映射 `AddressSpace::map_extern`
crate `kernel-vm` MUST 提供 `AddressSpace::map_extern(range: Range<VPN<Meta>>, pbase: PPN<Meta>, flags: VmFlags<Meta>)`，用于将一段虚拟页号区间映射到从 `pbase` 开始的连续物理页序列，并将该虚拟区间记录到 `areas`。

该函数的调用方 MUST 满足以下前置条件：
- `range` MUST 为非空区间（`range.end.val() > range.start.val()`）。
- `range` 目标页表项 MUST 处于未映射状态；若目标页表项已有效，当前实现内部断言 MAY panic。
- 对遍历过程中遇到的页表页，`PageManager::check_owned(pte)` 与 `PageManager::p_to_v(pte.ppn())` MUST 能保证：当 `check_owned(pte) == true` 时，`p_to_v` 返回的指针可被安全解引用为页表页。

当映射建立成功时，该函数 MUST：
- 将 `range.start..range.end` 追加到 `self.areas`（不做去重/合并）
- 将 `range` 中每个 VPN 依次映射到 `pbase..pbase+count` 中对应的 PPN，并使用同一份 `flags` 建立页表项

当映射建立未完成时，当前实现包含 `todo!()`；调用方 MUST 将其视为可观察行为：该函数 MAY panic 且不会提供回滚保证。

#### Scenario: 映射一段 VPN 区间到连续物理页
- **WHEN** 调用方选择一个未映射的 `range = vpn_a..vpn_b`
- **AND WHEN** 调用 `map_extern(range.clone(), ppn0, flags)`
- **THEN** `areas` MUST 追加记录 `vpn_a..vpn_b`
- **AND THEN** 对任意 `vpn` 满足 `vpn_a <= vpn < vpn_b`，页表中该 `vpn` 的 PPN MUST 等于 `ppn0 + (vpn.val() - vpn_a.val())`

#### Scenario: 传入空 `range` 导致 panic
- **WHEN** 调用方以 `range.start == range.end` 调用 `map_extern`
- **THEN** 当前实现 MAY panic（由于内部映射“未完成”分支触发 `todo!()`）

### Requirement: 分配、拷贝并建立映射 `AddressSpace::map`
crate `kernel-vm` MUST 提供 `AddressSpace::map(range, data, offset, flags)`，用于：
1) 分配 `range` 覆盖的页数 `count`
2) 将 `data` 拷贝到新分配的页内存中（起始偏移为 `offset`），并对前后空洞进行零填充
3) 建立从 `range` 到该新分配物理页序列的映射（等价于随后调用 `map_extern`）

调用方 MUST 满足以下前置条件：
- `count << Meta::PAGE_BITS MUST >= data.len() + offset`；否则当前实现会在 `assert!` 处 panic。

#### Scenario: 将一段初始数据映射到指定虚拟区间
- **WHEN** 调用方以满足前置条件的 `(range, data, offset, flags)` 调用 `map`
- **THEN** `PageManager::allocate(count, &mut flags)` MUST 被调用以分配物理页
- **AND THEN** `[0..offset)` 与 `[(offset+data.len())..size)` MUST 被写为 0，且 `data` MUST 被逐字节拷贝到 `[offset..offset+data.len())`
- **AND THEN** `range` MUST 被映射到新分配的物理页序列

### Requirement: 地址翻译 `AddressSpace::translate`
crate `kernel-vm` MUST 提供 `AddressSpace::translate<T>(addr: VAddr<Meta>, flags: VmFlags<Meta>) -> Option<NonNull<T>>`，用于在页表中查询 `addr` 所在页的映射并检查权限。

该函数 MUST：
- 遍历页表定位 `addr.floor()` 对应页表项；若不存在有效页表项，则返回 `None`
- **ONLY IF** 该页表项 `pte.flags().contains(flags)` 为真时，返回 `Some(ptr)`；其中 `ptr` MUST 指向 `pte.ppn()` 在当前地址空间中的虚拟地址（由 `PageManager::p_to_v` 提供）再加上 `addr.offset()` 的地址
- 否则返回 `None`

#### Scenario: 映射存在且权限满足时返回指针
- **WHEN** `addr` 所在页已映射且其 `pte.flags()` 包含调用方传入的 `flags`
- **THEN** `translate` MUST 返回 `Some(NonNull<T>)`

#### Scenario: 未映射或权限不足时返回 None
- **WHEN** `addr` 所在页未映射，或 `pte.flags()` 不包含所需 `flags`
- **THEN** `translate` MUST 返回 `None`

### Requirement: 地址空间克隆 `AddressSpace::cloneself`
crate `kernel-vm` MUST 提供 `AddressSpace::cloneself(&self, new_addrspace: &mut AddressSpace<Meta, M>)`，用于将 `self.areas` 中记录的每个虚拟区间在 `new_addrspace` 中重新分配物理页并拷贝数据，然后建立同等映射。

该函数的正确性依赖以下不变量；调用方 MUST 不破坏它们：
- `self.areas` 中的每个 `range` MUST 由 `map_extern`/`map` 产生（或与其等价），从而保证：该区间映射到一段连续物理页，且区间内所有页使用相同 `flags`。
- 调用方 MUST NOT 任意篡改 `AddressSpace::areas`（该字段为 `pub`）；若破坏上述不变量，`cloneself` 的行为 MAY panic 或拷贝错误数据。

#### Scenario: 克隆后两个地址空间包含相同的虚拟区间与内容
- **WHEN** `self` 仅通过 `map`/`map_extern` 建立映射并维护 `areas`
- **AND WHEN** 调用 `self.cloneself(&mut other)`
- **THEN** `other` MUST 为每个 `range in self.areas` 分配新的物理页并拷贝全部内容
- **AND THEN** `other` MUST 建立相同 VPN 区间的映射，且其 `flags` MUST 与源地址空间一致

### Requirement: 调试输出 `Debug for AddressSpace`
crate `kernel-vm` MUST 为 `AddressSpace` 提供 `fmt::Debug` 实现，用于输出根页表物理页号并借助 `page_table::PageTableFormatter` 打印页表结构；其中 `PageTableFormatter` 的物理页到指针转换 MUST 使用 `PageManager::p_to_v`。

#### Scenario: Debug 输出包含 root 物理页号
- **WHEN** 调用方对 `AddressSpace` 使用 `format!("{:?}", space)`
- **THEN** 输出 MUST 至少包含一行形如 `root: 0x...` 的根页表物理页号信息

## Public API

### Re-exports
- `pub extern crate page_table`: 重新导出 `page-table` crate，供调用方使用其类型（例如 `VmMeta`、`VPN/PPN/VAddr`、`VmFlags`、`Pte` 等）

### Types
- `pub trait PageManager<Meta: page_table::VmMeta>`: 物理页管理与页表页可访问性抽象
- `pub struct AddressSpace<Meta: page_table::VmMeta, M: PageManager<Meta>>`:
  - `pub areas: alloc::vec::Vec<core::ops::Range<page_table::VPN<Meta>>>`: 虚拟地址块记录（调用方可见；见 `cloneself` 前置条件）

### Functions / Methods
- `AddressSpace::new() -> Self`
- `AddressSpace::root_ppn(&self) -> page_table::PPN<Meta>`
- `AddressSpace::root(&self) -> page_table::PageTable<Meta>`
- `AddressSpace::map_extern(&mut self, range: Range<page_table::VPN<Meta>>, pbase: page_table::PPN<Meta>, flags: page_table::VmFlags<Meta>)`
- `AddressSpace::map(&mut self, range: Range<page_table::VPN<Meta>>, data: &[u8], offset: usize, flags: page_table::VmFlags<Meta>)`
- `AddressSpace::translate<T>(&self, addr: page_table::VAddr<Meta>, flags: page_table::VmFlags<Meta>) -> Option<core::ptr::NonNull<T>>`
- `AddressSpace::cloneself(&self, new_addrspace: &mut AddressSpace<Meta, M>)`

## Build Configuration

- build.rs: 无
- 环境变量: 无
- 生成文件: 无
- 运行时前置条件: 本 crate 为 `#![no_std]`，但依赖 `alloc`；因此调用方 MUST 提供全局分配器以支持 `Vec` 等类型

## Dependencies

- Workspace crates: 无
- External crates:
  - `page-table`: 页表结构、遍历（`walk/walk_mut`）与相关类型（`Pte/VmFlags/VPN/PPN/VAddr` 等）
  - `spin`: 依赖声明存在；当前源码未直接使用其 public 行为

