# Capability: signal

本规格描述 crate `signal` 的对外契约与边界：它在 `#![no_std]` 环境下提供信号相关的**公共类型重导出**、`SignalResult` 结果枚举、以及由实现方提供的 `Signal` trait（信号状态机接口）。

## Purpose

`signal` crate MUST 作为信号子系统的接口边界，使内核/任务/系统调用层能够在不依赖具体实现细节的前提下：
- 表达信号编号与处理动作（通过重导出 `signal-defs` 的类型与常量）；
- 以统一的结果类型（`SignalResult`）承载“忽略/已处理/需要杀死/需要暂停”等控制流语义；
- 通过 `Signal` trait 驱动具体的“信号投递、处理动作管理、mask 管理、上下文改写与 sigreturn 恢复”等行为。

## Requirements

### Requirement: 重导出信号定义（signal-defs）
crate `signal` MUST 重导出 workspace crate `signal-defs` 的下列 public API：
- `SignalAction`
- `SignalNo`
- `MAX_SIG`

#### Scenario: 下游通过 signal 直接引用信号定义
- **WHEN** 下游 crate 编译并执行 `use signal::{SignalAction, SignalNo, MAX_SIG};`
- **THEN** 这些符号 MUST 可被解析且其语义 MUST 等同于来自 `signal-defs` 的对应定义

### Requirement: 提供信号处理结果枚举（SignalResult）
crate `signal` MUST 提供 `pub enum SignalResult`，用于表达一次信号处理尝试对调度/执行流的影响。

`SignalResult` MUST 至少包含以下变体，并满足对应语义：
- `NoSignal`: 表示没有可处理的信号
- `IsHandlingSignal`: 表示当前正在处理信号，因而不能开始处理其他信号
- `Ignored`: 表示已处理（消费）了一个信号，且可直接返回用户态且无需改写用户上下文
- `Handled`: 表示已处理了一个信号，且已经对用户上下文进行了改写（例如切换到用户信号处理函数入口）
- `ProcessKilled(i32)`: 表示需要结束当前进程，并给出退出码（或等价 errno）
- `ProcessSuspended`: 表示需要暂停当前进程，直到收到继续执行的信号

#### Scenario: 信号处理请求杀死进程
- **WHEN** `Signal::handle_signals(...)` 返回 `SignalResult::ProcessKilled(code)`
- **THEN** 调用方 MUST 将当前进程置为退出状态并传播 `code`（传播路径由上层决定）

### Requirement: 提供 `Signal` trait 作为实现替换点
crate `signal` MUST 定义 `pub trait Signal: Send + Sync`，作为具体信号实现的对外接口边界。

#### Scenario: 下游以 trait object 持有信号实现
- **WHEN** 下游以 `Box<dyn Signal>` 方式持有某个信号实现
- **THEN** 下游 MUST 可通过 trait 方法完成投递/处理/管理动作与 mask 等操作，而无需依赖实现的具体类型

### Requirement: fork 语义（Signal::from_fork）
`Signal::from_fork(&mut self) -> Box<dyn Signal>` MUST 生成一个用于“fork 后子任务/子进程”的新信号模块实例，并满足：
- 新实例 MUST 继承父实例的信号处理动作配置（handlers / actions）
- 新实例 MUST 继承父实例的信号掩码（mask）

#### Scenario: fork 后继承动作与 mask
- **WHEN** 父任务的信号模块配置了非默认的 action 与 mask
- **AND WHEN** 调用 `from_fork` 为子任务生成新信号模块
- **THEN** 子任务信号模块 MUST 观察到与父任务一致的 action 与 mask 配置

### Requirement: exec 语义（Signal::clear）
`Signal::clear(&mut self)` MUST 清理当前信号模块中会被 `exec` 丢弃的状态，使 `exec` 后的任务不继承 `exec` 前的信号处理动作与 mask 配置。

#### Scenario: exec 清空动作与 mask
- **WHEN** 某任务在 `exec` 前已配置了 signal actions 与 mask
- **AND WHEN** 上层在 `exec` 路径调用 `Signal::clear`
- **THEN** 后续 `get_action_ref` 与 `update_mask` 的可观察结果 MUST 等价于“未配置/默认配置”的状态

### Requirement: 信号投递（Signal::add_signal）
`Signal::add_signal(&mut self, signal: SignalNo)` MUST 将 `signal` 记录为待处理信号（pending），以便后续由 `handle_signals` 消费/处理。

#### Scenario: 投递后可被处理路径观察到
- **WHEN** 对某个信号模块调用 `add_signal(sig)`
- **AND WHEN** 随后调用 `handle_signals(&mut ctx)` 尝试处理
- **THEN** 返回值 MUST NOT 永远保持为 `SignalResult::NoSignal`（除非 `sig` 被 mask/动作配置规则显式忽略并被消费为 `Ignored` 等）

### Requirement: 处理态查询（Signal::is_handling_signal）
`Signal::is_handling_signal(&self) -> bool` MUST 反映该信号模块是否处于“正在处理信号”的状态，以支持上层避免在信号处理期间重入处理逻辑。

#### Scenario: 正在处理信号时查询为 true
- **WHEN** 信号模块已切换到“正在处理信号”的状态（由实现定义具体触发点）
- **THEN** `is_handling_signal()` MUST 返回 `true`

### Requirement: 信号处理动作管理（Signal::set_action / get_action_ref）
`Signal::set_action(&mut self, signum: SignalNo, action: &SignalAction) -> bool` MUST 尝试为 `signum` 设置处理动作，并返回是否成功：
- 返回 `false` MUST 表示 `signum` 或 `action` 无效（上层通常将其映射为 `EINVAL`）

`Signal::get_action_ref(&self, signum: SignalNo) -> Option<SignalAction>` MUST 返回 `signum` 当前的处理动作快照；当 `signum` 无效或不可查询时 MUST 返回 `None`。

#### Scenario: 无效 signum 导致 set_action 失败
- **WHEN** 上层对无效信号号调用 `set_action`
- **THEN** `set_action` MUST 返回 `false`

### Requirement: 信号掩码管理（Signal::update_mask）
`Signal::update_mask(&mut self, mask: usize) -> usize` MUST 将当前信号掩码更新为 `mask`，并返回更新前的旧掩码值。

#### Scenario: update_mask 返回旧值
- **WHEN** 当前 mask 为 `old`
- **AND WHEN** 调用 `update_mask(new)`
- **THEN** 返回值 MUST 等于 `old`
- **AND THEN** 后续处理路径 MUST 以 `new` 作为当前 mask

### Requirement: 信号处理主流程（Signal::handle_signals）
`Signal::handle_signals(&mut self, current_context: &mut kernel_context::LocalContext) -> SignalResult` MUST 表达一次信号处理尝试对“用户态返回/调度/退出”的影响，并遵循：
- **IF** 返回 `NoSignal`：表示没有可处理的信号
- **IF** 返回 `IsHandlingSignal`：表示当前处于处理态，调用方 SHOULD 避免开始新的处理（调用方策略由上层决定）
- **IF** 返回 `Ignored`：表示一个信号已被消费且无需改写 `current_context`
- **IF** 返回 `Handled`：实现 MUST 已按其约定改写 `current_context`，使后续执行流进入被安装的用户信号处理路径（具体 ABI/栈布局由实现与上层共同约定）
- **IF** 返回 `ProcessKilled(code)`：调用方 MUST 终止当前进程并传播 `code`
- **IF** 返回 `ProcessSuspended`：调用方 MUST 将当前进程/任务置为暂停态直到收到继续类信号

#### Scenario: Handled 必须改写用户上下文
- **WHEN** `handle_signals(&mut ctx)` 返回 `SignalResult::Handled`
- **THEN** `ctx` MUST 被实现改写为与“进入用户信号处理函数”一致的状态（例如 pc/sp/寄存器等的改变）

### Requirement: 从信号处理函数返回（Signal::sig_return）
`Signal::sig_return(&mut self, current_context: &mut kernel_context::LocalContext) -> bool` MUST 尝试将用户上下文从“信号处理态”恢复到被打断前的状态，并返回是否成功：
- 返回 `true` MUST 表示恢复成功且 `current_context` 已被更新为恢复后的上下文
- 返回 `false` MUST 表示恢复失败（上层通常将其映射为错误返回）

#### Scenario: sigreturn 恢复并返回用户态
- **WHEN** 任务在用户信号处理函数中触发 `sigreturn` 路径
- **AND WHEN** 上层调用 `sig_return(&mut ctx)` 且其返回 `true`
- **THEN** `ctx` MUST 对应恢复后的用户上下文，使后续执行流从被打断点继续（或等价的恢复点）

## Public API

### Re-exports (from `signal-defs`)
- `SignalAction`
- `SignalNo`
- `MAX_SIG`

### Types
- `SignalResult`: 信号处理结果枚举（见 Requirements）

### Traits
- `Signal`:
  - `from_fork(&mut self) -> Box<dyn Signal>`
  - `clear(&mut self)`
  - `add_signal(&mut self, signal: SignalNo)`
  - `is_handling_signal(&self) -> bool`
  - `set_action(&mut self, signum: SignalNo, action: &SignalAction) -> bool`
  - `get_action_ref(&self, signum: SignalNo) -> Option<SignalAction>`
  - `update_mask(&mut self, mask: usize) -> usize`
  - `handle_signals(&mut self, current_context: &mut LocalContext) -> SignalResult`
  - `sig_return(&mut self, current_context: &mut LocalContext) -> bool`

## Build Configuration

- build.rs: （无）
- 环境变量: （无）
- 生成文件: （无）

## Dependencies

### Workspace crates（Preconditions）
- `kernel-context`:
  - MUST 提供 `LocalContext` 类型，代表可被信号机制改写/恢复的“当前执行上下文”
  - `LocalContext` 的可变借用 MUST 允许实现方在 `handle_signals`/`sig_return` 中对上下文进行更新
- `signal-defs`:
  - MUST 定义并导出 `SignalNo`（信号编号类型）、`SignalAction`（处理动作描述类型）、`MAX_SIG`（信号数量/上限常量）
  - `SignalNo` 与 `SignalAction` 的有效性判定规则 MUST 与上层 syscall 语义保持一致（例如无效值映射为 `EINVAL`）

### External crates
- `alloc`:
  - MUST 可用（`signal` 为 `#![no_std]` crate，且 `Signal::from_fork` 依赖 `Box`）

