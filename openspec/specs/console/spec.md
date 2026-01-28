# Capability: console

本规格描述 crate `rcore-console`（目录 `console/`）的对外契约与边界：提供可定制实现的 `print!`、`println!` 与 `log::Log`，并以单例方式将输出重定向到用户提供的控制台实现。

## Purpose

为 `rcore-console` 定义可验证的对外契约：明确初始化顺序与单例约束、打印与日志的可观察输出语义、日志级别配置入口，以及对外部实现（`Console` trait）的前置条件。

## Requirements

### Requirement: 控制台输出接口 `Console`
crate `rcore-console` MUST 通过 trait `Console` 抽象“向控制台输出字节序列”的能力，并要求实现者满足并发安全（`Sync`）。

`Console` 实现者 MUST 提供 `put_char(u8)` 以输出单个字节；默认的 `put_str(&str)` MUST 逐字节调用 `put_char` 输出 `s` 的 UTF-8 原始字节序列。

#### Scenario: 默认 `put_str` 的逐字节输出
- **WHEN** 用户实现 `Console::put_char` 且未覆盖 `Console::put_str`
- **THEN** 调用 `Console::put_str("A")` MUST 导致一次 `put_char(b'A')` 调用

### Requirement: 单例初始化与日志注册
crate `rcore-console` MUST 提供 `init_console(console: &'static dyn Console)` 以设置全局输出目标，并在初始化时注册其 `log::Log` 实现为全局 logger。

`init_console` MUST 将 `console` 保存为全局单例引用（`&'static dyn Console`），以供后续打印与日志输出使用。

#### Scenario: 初始化后可输出日志
- **WHEN** 用户先调用 `init_console(my_console)`
- **AND WHEN** 随后调用 `log::info!("hi")`
- **THEN** 输出 MUST 通过 `my_console` 被写入

### Requirement: `init_console` 的幂等性与重复调用行为
`init_console` 的单例存储（基于 `spin::Once`）MUST 只接受首次设置的 `Console` 引用；后续调用 MUST NOT 替换已保存的 `Console` 引用。

此外，本 crate 使用 `log::set_logger(&Logger).unwrap()` 注册全局 logger；因此调用方 MUST 将 `init_console` 视为“最多调用一次”的初始化例程：重复调用 MAY 因 logger 已被注册而 panic（当前实现会在 `unwrap()` 处 panic）。

#### Scenario: 重复调用 `init_console` 可能 panic 且不替换 console
- **WHEN** 用户已调用过一次 `init_console(console_a)`
- **AND WHEN** 再次调用 `init_console(console_b)`
- **THEN** 程序 MAY panic（当前实现可能 panic）
- **AND THEN** 若未 panic，后续输出 MUST 仍通过 `console_a` 而非 `console_b` 被写入

### Requirement: 未初始化时的前置条件（panic 行为）
`print!`/`println!`/日志输出 MUST 依赖已初始化的全局控制台单例；因此调用方 MUST 在首次输出前调用 `init_console(...)`。

若在未初始化时发生打印或日志输出，本 crate 当前实现会对空单例执行 `unwrap()`，因此行为是 panic；该 panic 行为属于可观察结果，调用方 MUST 将其视为违反前置条件。

#### Scenario: 未初始化直接输出导致 panic
- **WHEN** 用户未调用 `init_console(...)`
- **AND WHEN** 调用 `println!("x")` 或触发任意 `log::*` 输出
- **THEN** 程序 MAY panic（当前实现会 panic）

### Requirement: `print!` 与 `println!` 的格式化输出
crate `rcore-console` MUST 提供宏 `print!` 与 `println!` 用于格式化输出，并通过内部函数 `#[doc(hidden)] _print(fmt::Arguments)` 将格式化内容写入控制台。

`println!` MUST 在输出完格式化内容后额外输出一个换行（`\n`）；`println!()`（无参数）MUST 仅输出一个换行。

#### Scenario: `println!` 自动补换行
- **WHEN** 用户已调用 `init_console(...)`
- **AND WHEN** 调用 `println!("a")`
- **THEN** 控制台输出 MUST 以字节序列 `b"a\n"` 结尾

### Requirement: 日志输出格式与颜色码
crate `rcore-console` 的 logger MUST 接受所有日志记录（`enabled()` 恒为 true），并在 `log()` 中将日志按以下格式输出：

- 输出 MUST 以 `println!` 产生一行（末尾包含换行）
- 行内容 MUST 形如：`\x1b[{color}m[{level:>5}] {args}\x1b[0m`
- `color` MUST 按日志级别映射：`Error -> 31`、`Warn -> 93`、`Info -> 34`、`Debug -> 32`、`Trace -> 90`

#### Scenario: `Error` 级别使用红色（31）
- **WHEN** 用户已调用 `init_console(...)`
- **AND WHEN** 调用 `log::error!("boom")`
- **THEN** 输出行 MUST 以 ANSI 转义序列 `\x1b[31m` 开始并以 `\x1b[0m` 结束

### Requirement: 日志级别配置入口
crate `rcore-console` MUST 提供 `set_log_level(env: Option<&str>)` 以设置全局最大日志级别：
- **IF** `env` 可被解析为 `log::LevelFilter`：MUST 将最大级别设为该值
- **ELSE**：MUST 将最大级别设为 `Trace`

本 crate MUST NOT 直接读取环境变量；它仅消费调用方传入的字符串。

#### Scenario: 无配置时默认为 Trace
- **WHEN** 用户调用 `set_log_level(None)`
- **THEN** 全局最大日志级别 MUST 被设置为 `Trace`

### Requirement: `test_log()` 的可观察输出
crate `rcore-console` MUST 提供 `test_log()` 用于打印一段 ASCII art，并依次输出 `trace/debug/info/warn/error` 五条日志，最后额外输出一个空行。

#### Scenario: `test_log()` 产生五条日志
- **WHEN** 用户已调用 `init_console(...)`
- **AND WHEN** 调用 `test_log()`
- **THEN** 控制台输出 MUST 包含五行分别以 `[TRACE]`、`[DEBUG]`、`[ INFO]`、`[ WARN]`、`[ERROR]` 级别标签格式化的日志行

## Public API

### Re-exports
- `pub extern crate log`: 重新导出依赖 crate `log`

### Traits
- `Console`: 控制台输出抽象（`Sync`），要求实现 `put_char(u8)`；可选覆盖 `put_str(&str)`

### Functions
- `init_console(console: &'static dyn Console)`: 初始化全局控制台单例并注册 logger
- `set_log_level(env: Option<&str>)`: 解析并设置 `log` 的全局最大日志级别（默认 `Trace`）
- `test_log()`: 输出测试 banner 与五条不同等级日志

### Macros
- `print!(...)`: 格式化输出（无自动换行）
- `println!(...)`: 格式化输出并自动追加换行；`println!()` 输出单独换行

## Build Configuration

- build.rs: 无
- 环境变量: 无（但调用方可将例如 `LOG` 的环境变量值传入 `set_log_level`）
- 生成文件: 无

## Dependencies

- Workspace crates: 无
- External crates:
  - `log`: 日志门面与 `log::Log` trait
  - `spin`: `spin::Once` 用于保存全局单例控制台引用

