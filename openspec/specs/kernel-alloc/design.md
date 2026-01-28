## Context

`kernel-alloc` 旨在为内核提供一个最小、可移植的堆分配能力：通过 `#[global_allocator]` 挂载全局分配器，并暴露 `init/transfer` 以让上层（内核内存管理器/启动代码）决定“哪些内存可被当作堆”以及“何时把它交给分配器”。

该 crate 不处理页表/映射，也不尝试区分“物理页分配 vs 虚拟堆分配”；其前置条件是内核已能直接访问将被托管的内存区域。

## Goals / Non-Goals

- Goals:
  - 提供可用于 `alloc` crate 的全局堆分配器实现
  - 提供最小初始化接口（`init`）与内存托管接口（`transfer`）
  - 在 `no_std` 环境下可工作
- Non-Goals:
  - 不负责建立地址映射或修改页表以“让内存可访问”
  - 不提供并发安全保证（不内置锁/原子同步）
  - 不提供内存统计/碎片度量/回收策略等高级能力

## Decisions

- Decision: 采用 buddy allocator 作为底层算法  
  - Why: buddy allocator 实现简单、可预测，适合内核早期与受限环境；对齐/分割/合并符合通用堆分配需求。

- Decision: 使用 `init(base_address)` + `unsafe transfer(region)` 的两阶段模型  
  - Why: 将“分配器元数据初始化”和“可用堆内存供给”解耦；允许内核在不同阶段逐步把内存交给堆（例如引导阶段先给一小段，之后再扩容）。

- Decision: 不内置锁（无并发保护）  
  - Why: 避免在内核早期引入锁实现依赖与额外开销；由上层根据调度/中断模型决定是否需要外部同步。

## Capacity / Parameters (当前实现约束)

当前实现使用固定参数的 `BuddyAllocator<21, UsizeBuddy, LinkedListBuddy>`。源码注释给出的最大容量估算为：\(6 + 21 + 3 = 30\)，即约 \(2^{30}\) 字节（约 1 GiB）级别的可管理堆容量。

该容量与参数属于实现细节；调用方 MUST NOT 假设堆容量可以超过当前实现所能管理的上限，并应准备好处理分配失败（见 `spec.md` 中 `handle_alloc_error` 行为）。

## Safety Constraints

- `unsafe transfer` 的安全性由调用方保证：
  - 被托管区域不得重叠、不得再被引用/访问
  - 地址必须可被内核安全读写
  - 区域语义上应位于 `base_address` 之后（当前实现不做运行时检查）
- `dealloc` 的正确性依赖 `GlobalAlloc` 前置条件（指针与布局匹配、非空等）

## Risks / Trade-offs

- 无并发保护：
  - Risk: 多核/中断重入会造成数据竞争、堆结构损坏，进而导致未定义行为
  - Mitigation: 上层在进入分配器前禁用中断/抢占，或以锁包裹 `alloc/dealloc/transfer`

- 分配失败处理委托给 `handle_alloc_error`：
  - Risk: 可能 panic/abort，影响内核健壮性
  - Mitigation: 上层通过容量规划、提前 `transfer` 足够堆内存、或配置 panic 策略来控制故障模式

## Migration Plan

无（本文件为对当前实现的设计约束说明，不涉及迁移）。

## Open Questions

- 是否需要在未来引入可选的并发支持（例如 feature flag 启用自旋锁包装）？
- 是否需要在 `transfer` 时加入调试断言/运行时检查（例如检测重叠、检测与 `base_address` 的相对关系）以提升可诊断性？

