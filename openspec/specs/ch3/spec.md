# Capability: ch3

## Purpose

`ch3` 是一个 `#![no_std]` RISC-V Supervisor-mode (S-mode) **多道程序执行**内核 *binary crate*。它通过 workspace 内 `linker` 提供的启动入口 `_start` 引导进入 `rust_main`，初始化 console/log 与 syscall 宿主实现，将编译期通过 `APP_ASM` 内联进来的用户应用装载/枚举为任务，并以 **轮转调度**（可选抢占）方式运行它们，最终通过 SBI 请求关机。

## Requirements

### Requirement: Boot entry and non-returning kernel main
该二进制 MUST 通过 `linker::boot0!` 导出名为 `_start` 的 S-mode 入口符号，并 MUST 在可用启动栈上进入 `rust_main() -> !`；`rust_main` MUST NOT return。

#### Scenario: SEE transfers control to the kernel entry
- **WHEN** SEE 按链接脚本将控制权转移到 `.text.entry` 的 `_start`
- **THEN** `_start` 在启动栈上跳转进入 `rust_main`
- **AND THEN** `rust_main` 不返回

### Requirement: BSS zeroing before subsystem initialization
内核 MUST 在初始化任何依赖零初始化静态存储的子系统之前，将 `.bss` 清零。

#### Scenario: Zero BSS on boot
- **WHEN** `rust_main` 开始执行
- **THEN** 它调用 `linker::KernelLayout::locate().zero_bss()` 将 `.bss` 清零
- **AND THEN** 才开始初始化 console/log 与 syscall

### Requirement: Console and logging initialization
内核 MUST 初始化 `rcore_console` 后端，并 MUST 使用可选的编译期环境变量 `LOG` 配置日志级别，使 `print!/println!` 与 `rcore_console::log::*` 可用。

#### Scenario: Console backend is ready for printing
- **WHEN** `rust_main` 进入 console 初始化流程
- **THEN** 内核调用 `rcore_console::init_console(...)` 安装 console 后端
- **AND THEN** 之后的 `print!/println!` 与日志输出均通过该后端输出

### Requirement: Syscall subsystem initialization
在执行任何用户任务之前，内核 MUST 初始化 syscall 子系统的 IO / Process / Scheduling / Clock 四个宿主接口实现。

#### Scenario: Syscall handlers are ready before the first task runs
- **WHEN** `rust_main` 完成 console 初始化
- **THEN** 它依次调用 `syscall::init_io(...)`、`syscall::init_process(...)`、`syscall::init_scheduling(...)`、`syscall::init_clock(...)`
- **AND THEN** 用户态 `ecall` 触发的系统调用可经由 `syscall::handle(...)` 分发

### Requirement: Embedded application bundle inclusion
内核 MUST 通过 `global_asm!(include_str!(env!("APP_ASM")))` 将由 `APP_ASM` 指定的应用程序集内联进最终内核二进制。

#### Scenario: Build embeds a concrete application bundle
- **WHEN** 编译时提供环境变量 `APP_ASM`
- **THEN** 该路径指向的汇编文本被 `include_str!(env!("APP_ASM"))` 内联到二进制中

### Requirement: Application enumeration and task initialization
内核 MUST 通过 `linker::AppMeta::locate().iter()` 枚举用户应用，并 MUST 为每个枚举到的应用初始化一个 `TaskControlBlock`：
- 初始用户入口 PC MUST 取该应用镜像的起始地址；
- MUST 为任务分配并清零一个固定大小的用户栈；
- MUST 将用户上下文的 `sp` 设为该栈顶。

#### Scenario: Create task contexts for all embedded apps
- **WHEN** `linker::AppMeta::locate().iter()` 产生 N 个应用镜像
- **THEN** 内核为每个应用建立用户上下文（`LocalContext::user(entry)`）并设置用户栈顶 `sp`
- **AND THEN** N 个任务都处于“可执行/未完成”状态，等待调度

### Requirement: Timer interrupt enabling
内核 MUST 开启 supervisor timer interrupt 使其具备（至少在未启用 `coop` feature 时）基于定时器的抢占能力。

#### Scenario: Enable supervisor timer interrupt
- **WHEN** 内核完成任务初始化
- **THEN** 它设置 `sie.stimer` 以允许 SupervisorTimer 中断进入内核

### Requirement: Round-robin scheduling loop
内核 MUST 对所有未完成任务执行轮转调度：每次选择一个未完成任务进入执行，直到所有任务均完成为止。

#### Scenario: Rotate across runnable tasks
- **WHEN** 存在多个未完成任务
- **THEN** 内核按固定顺序轮转选择任务并执行
- **AND THEN** 任务完成后不再被调度，直到所有任务完成

### Requirement: Preemptive time slicing (default, without `coop`)
当 **未启用** feature `coop` 时，内核 MUST 在每次进入用户执行前设置一次 SBI timer（以 `time::read64() + 12500` 为下一次触发点）以实现抢占式时间片。

#### Scenario: Program the timer before running a slice
- **WHEN** feature `coop` 未启用且内核将要执行某个任务
- **THEN** 内核调用 `sbi_rt::set_timer(time::read64() + 12500)`
- **AND THEN** 该任务在时间片耗尽时可通过 `SupervisorTimer` 中断返回内核

### Requirement: Cooperative scheduling mode (`coop` feature)
当 **启用** feature `coop` 时，内核 MUST NOT 在每次任务执行前设置上述时间片定时器；任务切换 MUST 仅由 `yield/exit/kill` 等事件触发。

#### Scenario: `coop` disables per-slice timer programming
- **WHEN** feature `coop` 启用
- **THEN** 内核在进入任务执行前不调用 `sbi_rt::set_timer(time::read64() + 12500)`
- **AND THEN** 调度仅在显式 `yield` 或任务终止等事件后发生

### Requirement: Trap cause handling policy
每次从用户执行返回后，内核 MUST 读取 `scause` 并按以下策略处理：
- `Interrupt::SupervisorTimer`：MUST 视为时间片耗尽（非完成），并 MUST 禁用后续定时器（设置 `set_timer(u64::MAX)`）后切换到下一个任务；
- `Exception::UserEnvCall`：MUST 进入 syscall 分发路径；
- 其他 Exception 或 Interrupt：MUST 视为任务被杀死（完成），并记录日志。

#### Scenario: Timer interrupt preempts a task
- **WHEN** 某任务因 `SupervisorTimer` 中断返回内核
- **THEN** 内核调用 `sbi_rt::set_timer(u64::MAX)` 禁用定时器并记录超时日志
- **AND THEN** 该任务保持未完成并让出 CPU 给下一任务

#### Scenario: Non-ecall trap kills a task
- **WHEN** 某任务以非 `UserEnvCall` 的 trap 返回内核
- **THEN** 内核记录错误日志描述该 trap
- **AND THEN** 该任务被标记为完成且不再被调度

### Requirement: Syscall dispatch ABI (register convention)
在 `UserEnvCall` 场景下，内核 MUST 从用户上下文寄存器读取：
- syscall ID：`a7`
- 参数：`a0..a5`
并 MUST 调用 `syscall::handle(Caller { entity: 0, flow: 0 }, id, args)` 分发系统调用。

#### Scenario: Read syscall ID and args from the user context
- **WHEN** 用户态执行 `ecall` 并陷入内核
- **THEN** 内核从 `a7` 读取 syscall ID，从 `a0..a5` 读取参数数组
- **AND THEN** 内核以 `Caller { entity: 0, flow: 0 }` 调用 `syscall::handle(...)`

### Requirement: Syscall completion, yield, and exit semantics
当 `syscall::handle(...)` 返回 `SyscallResult::Done(ret)` 时，内核 MUST 按 syscall ID 处理：
- `EXIT`：MUST 将任务视为完成；退出码 MUST 取自用户寄存器 `a0`；
- `SCHED_YIELD`：MUST 将 `ret` 写回用户 `a0`，MUST 将用户 PC 前移到下一条指令，并 MUST 触发一次调度让出 CPU；
- 其他 syscall：MUST 将 `ret` 写回用户 `a0`，MUST 将用户 PC 前移到下一条指令，并 MUST 继续执行同一任务（不切换）。
当 `syscall::handle(...)` 返回 `SyscallResult::Unsupported(id)` 时，内核 MUST 记录日志并 MUST 将任务视为完成（终止）。

#### Scenario: Non-yield syscall returns and continues the same task
- **WHEN** 用户任务触发一个受支持且非 `EXIT`/`SCHED_YIELD` 的 syscall
- **THEN** 内核将返回值写回 `a0` 并将用户 PC 前移到下一条指令
- **AND THEN** 内核继续执行同一任务而不发生任务切换

#### Scenario: `SCHED_YIELD` yields to the scheduler
- **WHEN** 用户任务调用 `SCHED_YIELD` 且 syscall 子系统返回 `Done(ret)`
- **THEN** 内核写回 `a0 = ret` 并前移用户 PC
- **AND THEN** 内核切换到下一任务执行

#### Scenario: Unsupported syscall terminates a task
- **WHEN** 用户任务调用一个 `syscall::handle` 不支持的 syscall ID
- **THEN** 内核记录 "unsupported syscall" 日志
- **AND THEN** 该任务被终止并不再被调度

### Requirement: Syscall host: write to console
内核提供的 syscall 宿主实现 MUST 支持 `WRITE` 到 `STDOUT` 与 `STDDEBUG`：
- 成功时 MUST 将用户缓冲区输出到 console；
- 成功时 MUST 返回 `count`；
- 对不支持的 `fd` MUST 返回负值并记录日志。

#### Scenario: `write` prints user bytes to console
- **WHEN** 用户任务调用 `write(STDOUT, buf, count)` 或 `write(STDDEBUG, buf, count)`
- **THEN** 内核将缓冲区内容输出到 console
- **AND THEN** 返回值为 `count`

#### Scenario: `write` rejects unsupported file descriptors
- **WHEN** 用户任务调用 `write(fd, buf, count)` 且 `fd` 非 `STDOUT/STDDEBUG`
- **THEN** 内核记录不支持的 fd
- **AND THEN** 返回负值

### Requirement: Syscall host: clock_gettime (monotonic)
内核提供的 syscall 宿主实现 MUST 支持 `CLOCK_GETTIME(CLOCK_MONOTONIC, tp)`：成功时 MUST 将单调时钟值写入 `*tp` 并返回 0；对其他 `clock_id` MUST 返回负值。

#### Scenario: `clock_gettime` writes monotonic time
- **WHEN** 用户任务调用 `clock_gettime(CLOCK_MONOTONIC, tp)`
- **THEN** 内核将单调时间写入 `*tp`（`TimeSpec { tv_sec, tv_nsec }`）
- **AND THEN** 返回 0

### Requirement: End-of-run shutdown request
当所有任务均完成后，内核 MUST 请求正常关机 `sbi_rt::system_reset(Shutdown, NoReason)` 且 MUST NOT return。

#### Scenario: Clean shutdown after the last task finishes
- **WHEN** 最后一个任务被标记为完成
- **THEN** 内核调用 `system_reset(Shutdown, NoReason)`
- **AND THEN** 不返回到任何 Rust 调用者

### Requirement: Panic handling
发生 panic 时，内核 MUST 输出 panic 信息，并 MUST 以失败原因请求关机 `system_reset(Shutdown, SystemFailure)`；panic handler MUST NOT return。

#### Scenario: Panic path prints and shuts down
- **WHEN** panic handler 被调用
- **THEN** 它输出 `PanicInfo`
- **AND THEN** 它调用 `system_reset(Shutdown, SystemFailure)` 且不返回

## Public API

该 crate 为 `bin` crate（`#![no_main]`），不提供稳定的对外 Rust library API。其对外契约主要体现为导出的入口符号与构建期接口：

### Linker / symbol-level interface
- `_start() -> !`: 由 `linker::boot0!` 导出、位于 `.text.entry` 的内核入口符号。

### Environment / build-time interface
- `APP_ASM`（环境变量，编译期必须提供）: 指向要内联的应用程序集文件路径。
- `LOG`（环境变量，编译期可选）: 用于配置日志级别。

### Feature flags
- `coop`: 启用后关闭每次时间片的 `set_timer(time::read64() + 12500)` 设置，调度退化为以 `yield/exit/kill` 为主的协作式切换。

## Build Configuration

### build.rs
- build script MUST 将 `linker::SCRIPT` 写入 `$OUT_DIR/linker.ld`。
- build script MUST 通过 `cargo:rustc-link-arg=-T...` 将该 linker script 传递给链接器。
- build script MUST 通过 `cargo:rerun-if-changed=build.rs` 在 `build.rs` 变更时触发重建。
- build script MUST 通过 `cargo:rerun-if-env-changed` 在环境变量 `LOG` 或 `APP_ASM` 变更时触发重建。

#### Scenario: Cargo build emits and uses the linker script
- **WHEN** Cargo 构建 `ch3`
- **THEN** `build.rs` 生成 `<OUT_DIR>/linker.ld`
- **AND THEN** 最终链接使用 `-T<OUT_DIR>/linker.ld`

### Environment variables
- `OUT_DIR`（Cargo 提供）: build script 输出目录。
- `APP_ASM`: 见 Public API。
- `LOG`: 见 Public API。

### Generated files
- `<OUT_DIR>/linker.ld`: 内容等于 `linker::SCRIPT`。

## Dependencies

### Workspace crates (Preconditions)
`ch3` 依赖以下 workspace 内 crate；它们 MUST 提供所述符号/语义：

- **`linker`**:
  - MUST 提供 `SCRIPT: &[u8]` 作为链接脚本文本（供 `build.rs` 写入）。
  - MUST 提供 `boot0!(...; stack = N)` 宏，并导出 `_start` 入口符号（`.text.entry`），在启动栈上跳转进入 `rust_main`。
  - MUST 提供 `KernelLayout::locate()` 与 `KernelLayout::zero_bss()` 用于定位并清零 `.bss`。
  - MUST 提供 `AppMeta::locate().iter()`，枚举应用镜像字节切片；若其实现执行了拷贝装载，则返回切片起始地址 MUST 可作为用户入口 PC。

- **`rcore-console`**（`rcore_console`）:
  - MUST 提供 `init_console`, `set_log_level(Option<&'static str>)`, `test_log`，以及 `print!/println!` 与 `rcore_console::log::*`。
  - MUST 定义 `rcore_console::Console` trait，至少含 `put_char(u8)`。

- **`kernel-context`**:
  - MUST 提供 `LocalContext::{empty,user}` 创建上下文。
  - MUST 提供 `sp_mut()`、`a(i)`、`a_mut(i)` 与 `move_next()` 以支持 syscall ABI 约定与返回。
  - MUST 提供 `unsafe fn execute(&mut self) -> usize`，用于进入用户执行并在 trap 后返回内核。

- **`syscall`**（feature: `kernel`）:
  - MUST 提供 `init_io`, `init_process`, `init_scheduling`, `init_clock` 初始化入口。
  - MUST 提供 `handle(Caller, SyscallId, [usize; 6]) -> SyscallResult`。
  - MUST 定义 `Caller { entity, flow }`、`SyscallId`（至少含 `EXIT`, `SCHED_YIELD`）与 `SyscallResult::{Done, Unsupported}`。
  - MUST 提供 `STDOUT` 与 `STDDEBUG` 常量，以及 `ClockId`, `TimeSpec`, `ClockId::CLOCK_MONOTONIC`。

### External crates
- **`sbi-rt`**（feature: `legacy`）:
  - MUST 提供 `legacy::console_putchar`、`set_timer` 与 `system_reset`，以及 `Shutdown/NoReason/SystemFailure` 枚举值。
- **`riscv`**:
  - MUST 提供读取 `time`（含 `read64`）以及读取/设置 `sie.stimer`、读取 `scause` cause 的接口。

### Platform/SEE (Preconditions)
- 运行环境 MUST 以 RISC-V S-mode 启动该二进制，并提供可用的 SBI（支持 `legacy::console_putchar`、`set_timer`、`system_reset`）。
- 运行环境 MUST 正确报告 `scause`（至少区分 `Exception::UserEnvCall` 与 `Interrupt::SupervisorTimer`）。

