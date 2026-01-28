# Capability: kernel-alloc

本规格描述 crate `kernel-alloc`（目录 `kernel-alloc/`）的对外契约与边界：为内核提供 `#[global_allocator]`（基于 buddy allocator），并暴露最小初始化/托管接口以将一段内存区域交给全局堆分配器管理。

## Purpose

为 `kernel-alloc` 定义可验证的对外契约：明确调用方必须满足的前置条件（地址可访问性、初始化顺序、并发约束、`unsafe transfer` 的内存所有权/不重叠要求），以及分配失败时的可观察行为。

## Requirements

### Requirement: 提供全局堆分配器（`#[global_allocator]`）
crate `kernel-alloc` MUST 通过 `#[global_allocator]` 提供一个全局分配器实现，以支持 `alloc` crate（例如 `Vec`、`Box`）在内核中进行堆分配。

该全局分配器 MUST 在内部委托给 buddy allocator；其可观察行为 MUST 符合 Rust `GlobalAlloc` 契约（对齐、布局一致性等由调用方遵循）。

#### Scenario: 使用 `alloc` 类型触发全局分配
- **WHEN** 内核链接了 `kernel-alloc` 且已完成该 crate 要求的初始化（见后续 requirements）
- **THEN** 对 `alloc` 类型（例如 `Box::new(...)`）的堆分配请求 MUST 由该全局分配器处理

### Requirement: 初始化入口 `init(base_address: usize)`
crate `kernel-alloc` MUST 提供 `pub fn init(base_address: usize)` 用于初始化全局堆分配器的内部状态。

调用方 MUST 将 `base_address` 视为动态内存区域的起始位置；并且调用方 MUST 保证 `base_address` 对应地址在内核地址空间中可被安全解引用/写入（即该地址已映射或为直接可访问的恒等映射区域）。

若 `base_address` 为 0 或不可构造为非空指针，当前实现会在内部 `unwrap()` 处 panic；调用方 MUST 将其视为违反前置条件。

#### Scenario: 以非零、可访问地址初始化
- **WHEN** 调用方在首次堆分配前调用 `init(base_address)`
- **AND WHEN** `base_address` 为非零且指向内核可访问的内存
- **THEN** 初始化 MUST 成功完成且后续可通过 `transfer` 托管内存区域

### Requirement: 托管内存块 `unsafe transfer(region: &'static mut [u8])`
crate `kernel-alloc` MUST 提供 `pub unsafe fn transfer(region: &'static mut [u8])` 用于将一段内存块托管给全局堆分配器管理。

调用方 MUST 在调用 `transfer` 前完成 `init(...)`，并 MUST 保证被托管的 `region` 满足以下安全前置条件：
- `region` 的所有权将转移到分配器；因此 `region` MUST NOT 与任何已经托管给分配器的内存块重叠
- `region` MUST NOT 被其他对象引用（包括别名可变引用、共享引用或被 DMA/设备/其他 CPU 并发访问）
- `region` MUST 位于 `init(base_address)` 传入的起始位置之后（语义来自该 crate 文档；当前实现不在运行时显式检查）

#### Scenario: 托管一段新内存用于后续分配
- **WHEN** 调用方已调用 `init(base_address)`
- **AND WHEN** 调用方以满足前置条件的 `region` 调用 `unsafe { transfer(region) }`
- **THEN** 分配器 MUST 将该内存块纳入其管理范围，以供后续 `alloc` 分配使用

### Requirement: 分配失败的可观察行为（`handle_alloc_error`）
当全局分配器无法满足某个 `Layout` 的分配请求时，crate `kernel-alloc` MUST 调用 `alloc::alloc::handle_alloc_error(layout)` 处理该失败。

因此，调用方 MUST 将“分配失败导致 panic/abort（由 `handle_alloc_error` 决定）”视为可观察行为，并在内核策略上自行决定是否允许该行为发生（例如通过保证足够内存或替换 panic 策略）。

#### Scenario: 请求超过当前可用堆容量
- **WHEN** 调用方发起一个无法被分配器满足的分配请求（例如堆内存不足或内部 buddy allocator 返回错误）
- **THEN** 分配器 MUST 触发 `handle_alloc_error(layout)`，程序 MAY panic 或 abort（取决于运行环境）

### Requirement: 释放行为与布局一致性（`dealloc` 前置条件）
全局分配器的 `dealloc(ptr, layout)` MUST 将释放请求委托给内部 buddy allocator。

调用方 MUST 遵循 Rust `GlobalAlloc` 前置条件：传入的 `ptr` MUST 为先前由该分配器分配且尚未释放的指针，且 `layout` MUST 与分配时使用的布局一致。

若 `ptr` 为 null，当前实现会在内部 `unwrap()` 处 panic；调用方 MUST 将其视为违反前置条件。

#### Scenario: 成对的分配与释放
- **WHEN** 调用方通过 `alloc` 类型分配得到指针 `p`（或对象 `x`）
- **AND WHEN** 调用方仅释放一次且使用匹配的 `Layout`（或通过 drop 释放对象）
- **THEN** 分配器 MUST 正确回收该内存，使其可被后续分配再次使用

### Requirement: 并发与重入约束（无锁分配器）
由于该 crate 的全局分配器实现不包含内部锁，调用方 MUST 确保不存在并发的 `alloc/dealloc/transfer` 访问（包括多核并发、抢占/中断上下文重入等），或必须在更高层引入外部同步以串行化访问。

若违反该约束，行为 MAY 导致数据竞争与未定义行为；该风险属于该 crate 的边界条件。

#### Scenario: 单核、无抢占环境下使用
- **WHEN** 内核以单核或已禁用抢占/中断的方式保证分配器调用不并发
- **THEN** 调用 `alloc/dealloc/transfer` MUST 不因并发访问而破坏分配器内部状态

### Requirement: 地址可访问性前置条件（“虚地址覆盖物理地址”的假设）
调用方 MUST 保证传入 `init(base_address)` 与 `transfer(region)` 的地址范围对内核而言是可访问的（可安全读写）。

该 crate 的实现与文档假设是：内核可直接访问到所有将被托管的物理内存（例如“虚地址空间覆盖物理地址空间”或等价的恒等映射/直映窗口）；因此该 crate MUST NOT 负责通过修改页表等方式使其变得可访问。

#### Scenario: 直映窗口下托管物理内存
- **WHEN** 内核将一段物理内存映射到直映窗口 `KSEG + pa`
- **AND WHEN** 调用方以该虚拟地址范围构造 `base_address/region`
- **THEN** 分配器 MUST 能在不修改页表的前提下管理并分配该内存

## Public API

### Functions
- `init(base_address: usize)`: 初始化全局堆分配器（动态内存区域起始位置）
- `unsafe transfer(region: &'static mut [u8])`: 将一段内存块托管给分配器管理（调用方承担不重叠/无引用/地址可访问等前置条件）

### Global allocator
- `#[global_allocator]`: 本 crate 在链接时注册一个全局分配器实现，用于服务 `alloc` crate 的堆分配

## Build Configuration

- build.rs: 无
- 环境变量: 无
- 生成文件: 无

## Dependencies

- Workspace crates: 无
- External crates:
  - `customizable-buddy`: buddy allocator 实现（`BuddyAllocator` 等），为全局分配器提供底层算法
  - `log`: 依赖声明存在；当前实现未在该 crate 的 public 行为中使用
  - `page-table`: 依赖声明存在；当前实现未在该 crate 的 public 行为中使用

