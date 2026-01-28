## Context

`ch3` 是一个不依赖标准库与运行时的 RISC-V S-mode 内核 binary。它通过 workspace `linker` 的 `boot0!` 宏导出 `_start` 并进入 `rust_main`，运行期不使用虚拟内存抽象；用户应用以内联汇编（`APP_ASM`）的方式静态链接进内核镜像，并通过 `linker::AppMeta` 提供的元数据进行枚举/（可选）拷贝装载。

## Goals / Non-Goals

- Goals:
  - 提供可审计的“清 BSS → 初始化 console/log/syscall → 多任务轮转执行 → 关机”最小多道内核路径。
  - 支持两种调度模式：默认抢占（timer slice）与 `coop` 协作式（yield 驱动）。
- Non-Goals:
  - 不提供隔离的地址空间/安全的用户指针翻译；用户缓冲区的可达性与合法性由平台/测试用例保证。
  - 不提供完整的类 Unix 进程/文件系统语义；syscall 仅提供本章所需的最小宿主实现。

## Unsafe / ABI Invariants

- **启动入口不变量**:
  - `_start` 为 `#[unsafe(naked)]`，并位于 `.text.entry`；它假设 SEE 将控制权转移到链接脚本安排的入口地址。
  - 启动栈通过在 `.boot.stack` 放置静态数组来“占位”，并通过链接符号 `__end` 作为 `sp` 初值；因此链接脚本/符号布局属于该 crate 的强前置条件。

- **应用元数据不变量**:
  - `linker::AppMeta::locate()` 依赖内联汇编提供 `apps` 符号以及相应布局（base/step/count/first + 边界数组）。
  - `AppMeta::iter()` 可能将应用镜像从链接位置拷贝到 `base + i*step` 的装载地址，并将装载区域剩余部分清零；因此用户入口地址与可执行内存的布局由该元数据与平台内存地图共同决定。

- **上下文切换/陷入不变量**:
  - `kernel_context::LocalContext::execute()` 会修改 CSR（`sscratch/sepc/sstatus/stvec`）并在 trap 后返回；调度器依赖 `scause` 判断 trap 原因。
  - syscall 返回路径通过 `LocalContext::move_next()` 前移 `sepc`，该实现假设触发 trap 的指令为 **非压缩指令**（固定 +4）。

- **用户指针不变量（无隔离）**:
  - syscall 宿主实现直接将用户参数 `buf/tp` 解释为内核可直接解引用的地址（`from_raw_parts` / `from_utf8_unchecked` / 写 `TimeSpec`）。
  - 因此本章内核把“用户指针指向可读写、对齐正确、且（对 write）为有效 UTF-8”视为前置条件；否则行为未定义。

## Scheduling Model

- **默认模式（非 `coop`）**:
  - 在每次进入用户执行前设置 `set_timer(time::read64() + 12500)`，以 `SupervisorTimer` 中断实现抢占式时间片轮转。
  - 在收到 `SupervisorTimer` 中断后将 timer 置为 `u64::MAX`（等价于禁用），并切换到下一任务。

- **`coop` feature**:
  - 不设置每次时间片的 timer；调度主要依赖用户任务显式 `SCHED_YIELD` 或终止事件（`EXIT`/kill）。

