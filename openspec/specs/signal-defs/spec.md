# Capability: signal-defs

## Purpose

本规格描述 crate `signal-defs` 的对外契约与边界：它在 `#![no_std]` 环境中提供一组“信号编号”定义（`SignalNo`）以及信号处理动作的数据结构（`SignalAction`），供内核/用户态接口层在不引入平台依赖的情况下共享信号相关类型。

## Requirements

### Requirement: `SignalAction` 的内存布局与字段可见性
crate `signal-defs` MUST 定义类型 `SignalAction`，其满足：
- MUST 为 `#[repr(C)]` 结构体，以确保可用于 C ABI/FFI 场景的稳定字段布局。
- MUST 包含且仅包含两个 `pub` 字段，且字段顺序 MUST 为：
  - `handler: usize`
  - `mask: usize`
- MUST 派生（derive）`Debug`、`Clone`、`Copy`、`Default`，从而允许值复制、默认构造与调试打印。

#### Scenario: 构造并在 FFI 边界传递 `SignalAction`
- **WHEN** 调用方在 Rust 中创建 `SignalAction { handler, mask }` 并将其按 `repr(C)` 语义传递到 ABI 边界
- **THEN** 接收方 MUST 观察到字段顺序与大小/对齐符合该平台上两个 `usize` 的 `repr(C)` 布局
- **AND THEN** 调用方可以通过 `Default::default()` 得到一个可用的零初始化默认值（由派生实现决定）

### Requirement: 信号编号枚举 `SignalNo` 的取值集合
crate `signal-defs` MUST 定义 `SignalNo` 作为 `#[repr(u8)]` 的 `pub enum`，其判别值（discriminant） MUST 与下列信号编号一一对应：
- MUST 包含 `ERR = 0`
- MUST 包含传统信号区间 `1..=31`（例如 `SIGHUP = 1`、`SIGKILL = 9`、`SIGSYS = 31`）
- MUST 包含实时信号区间 `32..=63`，并以 `SIGRTMIN = 32`、`SIGRT1 = 33` … `SIGRT31 = 63` 的方式提供

#### Scenario: 使用 `SignalNo` 表达传统信号与实时信号
- **WHEN** 调用方需要表达信号编号 2（中断）与 33（实时信号 1）
- **THEN** 调用方 MUST 能分别使用 `SignalNo::SIGINT` 与 `SignalNo::SIGRT1` 来表示它们

### Requirement: `SignalNo` 的 `TryFrom<u8>` 与 `From<usize>` 转换语义
crate `signal-defs` MUST 为 `SignalNo` 提供如下可观察的转换语义：
- MUST 支持通过 `TryFrom<u8>` 将 `u8` 转换为 `SignalNo`：
  - **IF** `u8` 值等于某个已定义的判别值
  - **THEN** `try_from` MUST 返回对应的 `SignalNo` 变体
  - **ELSE** `try_from` MUST 返回错误（由 `numeric-enum-macro` 生成的实现决定错误类型）
- MUST 提供 `impl From<usize> for SignalNo`，其语义 MUST 等价于：
  - 先将输入 `num` 以 Rust `as` 规则转换为 `u8`（可能发生截断）
  - 再执行 `SignalNo::try_from(u8)`
  - **IF** `try_from` 成功则返回对应变体；**ELSE** MUST 返回 `SignalNo::ERR`

#### Scenario: 将有效/无效编号转换为 `SignalNo`
- **WHEN** 调用方执行 `SignalNo::try_from(15u8)`
- **THEN** MUST 得到 `SignalNo::SIGTERM`
- **WHEN** 调用方执行 `SignalNo::try_from(100u8)`
- **THEN** MUST 得到一个错误结果（因为 100 不在已定义集合中）
- **WHEN** 调用方执行 `SignalNo::from(100usize)`
- **THEN** MUST 得到 `SignalNo::ERR`

#### Scenario: `From<usize>` 的截断行为是可观察的
- **WHEN** 调用方执行 `SignalNo::from(289usize)`
- **THEN** MUST 先发生 `289usize as u8 == 33u8` 的截断
- **AND THEN** MUST 得到 `SignalNo::SIGRT1`（等价于 `SignalNo::try_from(33u8)` 的成功结果）

### Requirement: `MAX_SIG` 常量的含义与值
crate `signal-defs` MUST 暴露 `pub const MAX_SIG: usize = 31`，用于表达“传统信号”的最大编号（与 `SignalNo` 中 `SIGSYS = 31` 对齐）。

#### Scenario: 调用方以 `MAX_SIG` 作为传统信号上限
- **WHEN** 调用方需要在位图/数组中为 `1..=MAX_SIG` 的信号预留槽位
- **THEN** 调用方 MUST 能读取到 `MAX_SIG == 31`

### Requirement: `#![no_std]` 兼容性
crate `signal-defs` MUST 在 `#![no_std]` 环境下可编译与可用，并且其 public API MUST 不依赖标准库类型（`std`）。

#### Scenario: 内核（no_std）依赖 `signal-defs`
- **WHEN** `#![no_std]` 的内核 crate 将 `signal-defs` 作为依赖并引用 `SignalNo`/`SignalAction`
- **THEN** 该依赖关系 MUST 可被编译通过（不需要启用 `std`）

## Public API

### Types
- `SignalAction`: `#[repr(C)]` 的信号处理动作结构体，包含 `handler: usize` 与 `mask: usize` 两个公开字段。
- `SignalNo`: `#[repr(u8)]` 的信号编号枚举，覆盖 `ERR`、传统信号 `1..=31` 与实时信号 `32..=63`。

### Constants
- `MAX_SIG: usize`: 传统信号的最大编号（固定为 31）。

### Trait Implementations (observable)
- `SignalAction`: `Debug`, `Clone`, `Copy`, `Default`
- `SignalNo`: `Eq`, `PartialEq`, `Debug`, `Copy`, `Clone`
- `SignalNo`: `TryFrom<u8>`（由 `numeric-enum-macro` 生成）
- `SignalNo`: `From<usize>`（crate 内显式提供）

## Build Configuration
- build.rs: none
- 环境变量: none
- 生成文件: none
- Feature flags: none

## Dependencies
- Workspace crates: none
- External crates:
  - `numeric-enum-macro`: 用于生成 `SignalNo` 的数值枚举定义与 `TryFrom<u8>` 等转换实现

