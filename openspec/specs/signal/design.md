## Context

crate `signal` 是信号子系统的**接口边界**：它不包含具体的信号实现逻辑，仅定义对外可见的 trait/类型，使上层（如 syscall/任务管理）能够以统一方式驱动信号机制。

源码注释表明具体实现位于其它 crate（例如 `signal-impl` 或同类实现 crate）；因此 `signal` 的职责是稳定接口与语义，而不是提供默认实现。

## Goals / Non-Goals

- Goals:
  - 在 `#![no_std]` 环境下提供信号相关公共 API（类型重导出 + `Signal` trait + `SignalResult`）
  - 支持以 `Box<dyn Signal>` 方式在运行时选择/替换具体实现
  - 将“改写/恢复执行上下文”的边界明确落在 `kernel_context::LocalContext`
- Non-Goals:
  - 不规定具体的用户态信号处理 ABI/栈帧布局/寄存器保存格式（由实现与上层共同约定）
  - 不提供默认信号实现

## Decisions

- Decision: 使用 trait object 作为实现替换点
  - `Signal` 以 `Send + Sync` 约束实现，允许其在多核/并发场景中被上层安全持有（具体内部同步由实现承担）。
  - `from_fork -> Box<dyn Signal>` 作为 fork 路径的克隆机制，避免要求实现 `Clone` 或暴露具体类型。

- Decision: 上下文改写通过 `LocalContext` 显式完成
  - `handle_signals` 与 `sig_return` 均接收 `&mut LocalContext`，将“信号导致的控制流切换/恢复”的副作用局限在可审计的接口上。

- Decision: 信号编号/动作定义归一到 `signal-defs`
  - `signal` 仅重导出 `signal-defs` 的公共类型与常量，避免重复定义导致的语义漂移。

## Risks / Trade-offs

- trait object + `alloc::Box` 依赖 `alloc`：在极小运行时/无分配器环境下不可用；但这是为了在内核中更方便地做实现替换与 fork 克隆。
- `LocalContext` 的语义若变化（字段/ABI/保存恢复规则）会直接影响信号实现可否正确改写/恢复上下文；因此它被明确列为前置条件。

