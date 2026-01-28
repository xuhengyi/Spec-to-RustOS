## Context
`linker` crate 的核心职责是把“链接脚本与链接符号”收敛为 Rust 侧可复用的契约，从而让内核代码避免手写 `extern "C"` 符号声明或散落的 `.ld/.S` 约定。

该 crate 是 `#![no_std]`，并直接依赖链接阶段生成/导出的符号地址，因此其正确性高度依赖最终链接产物是否采用了对应的链接脚本与符号布局。

## Goals / Non-Goals
- **Goals**:
  - 统一内核的段布局与关键符号命名（`__start/__rodata/.../__end`）。
  - 提供启动入口宏 `boot0!`，为早期启动提供可控的启动栈与跳转目标。
  - 提供 `KernelLayout` 与 `.bss` 清零逻辑，支持早期初始化。
  - 提供对“链接进内核的用户 app 镜像”的迭代访问与可选重定位拷贝。
- **Non-Goals**:
  - 不负责生成用户 app 的 `apps` 元数据与镜像本体（仅消费该符号）。
  - 不负责更高层的内存管理/页表映射（仅做地址区间描述与原地清零/拷贝）。

## Decisions

### Decision: 固定内核镜像段布局与导出符号
链接脚本将 `.text` 固定在 `0x80200000`，并将 `.rodata/.data/.bss/.boot` 以明确的对齐与顺序排列，最后导出 `__end`。

该设计允许：
- 在运行时通过 `KernelLayout::locate()` 获取镜像布局；
- 在早期启动时以 `__end` 作为启动栈栈顶；
- 将 `.boot` 作为“启动后可回收的临时区域”（例如换栈后可纳入动态内存）。

### Decision: `boot0!` 以 `__end` 作为启动栈栈顶
`boot0!` 在 `.boot.stack` 段中放置静态启动栈，并在 `_start` 中将 `sp` 设置为 `__end`。

该设计依赖链接脚本保证：
- `.boot` 段位于镜像末尾；
- `.boot.stack` 内容被 `KEEP`，且位于 `.boot` 段内；
- `__end` 位于 `.boot` 段（包含启动栈）之后，从而 `sp=__end` 指向“启动栈顶部”。

### Decision: `.bss` 清零使用 volatile 写
`KernelLayout::zero_bss()` 使用 `write_volatile(0)` 逐字节清零，以避免多核场景下因编译器优化导致的可见性问题。

### Decision: `apps` 元数据布局与可选重定位拷贝
`AppMeta` 约定 `apps` 符号指向一个 `repr(C)` 结构，随后紧跟一个 `count+1` 长度的地址数组：
- 该数组以“相邻指针差”给出每个 app 的大小；
- 当 `base != 0` 时，迭代器会将 app 镜像拷贝到固定槽位 `base + i*step`；
- 同时将该槽位剩余的 `0x20_0000 - size` 清零，为后续运行提供干净的空间（例如映射为用户地址空间）。

## Risks / Trade-offs
- **Risk: 链接脚本/符号不匹配导致未定义行为** → 通过 spec 强制 consumer 使用 `SCRIPT` 并列出必须存在的符号；运行时不做检查（`no_std`/早期启动限制）。
- **Risk: `boot0!` 依赖裸函数与内联汇编能力** → consumer 需要匹配的 Rust 工具链与目标平台支持；否则编译期失败。
- **Risk: App 重定位写入非法内存** → `base/step` 与槽位大小 `0x20_0000` 都是强前置条件；若不满足会导致内存破坏。

## Migration Plan
无（该文档描述现有实现与契约，不引入行为变更）。

## Open Questions
- `boot0!` 的宏参数 `$entry` 在当前实现中未被使用（固定跳转 `rust_main`）；后续若要支持任意入口符号，需要明确是否允许破坏性变更以及迁移策略。

