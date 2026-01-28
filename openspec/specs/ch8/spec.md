# Capability: ch8

本规格描述 crate `ch8`（第八章内核）的对外契约与边界：其构建时生成的链接脚本、内核启动初始化流程、`initproc` 加载、以及通过 `syscall` 子系统对用户态提供的 I/O/进程/线程/同步/信号/时钟等行为。

## Purpose

为 `ch8` 这一章的内核二进制定义可验证的对外契约：明确启动与调度主循环、文件系统/virtio 块设备、以及 syscall 适配层对用户态的可观察语义与前置条件，并将对 workspace 依赖的必要语义收敛为 Preconditions。

## Requirements

### Requirement: Build script 生成并使用链接脚本
crate `ch8` 的 `build.rs` MUST 将 `linker::SCRIPT` 写入 `$OUT_DIR/linker.ld`，并通过 `cargo:rustc-link-arg=-T<path>` 强制使用该链接脚本参与链接。

#### Scenario: 生成并注入 linker.ld
- **WHEN** Cargo 执行 `build.rs` 并提供环境变量 `OUT_DIR`
- **THEN** `$OUT_DIR/linker.ld` MUST 被写入为 `linker::SCRIPT`
- **AND THEN** 编译产物 MUST 带有链接参数 `-T$OUT_DIR/linker.ld`

### Requirement: Build script 的重跑条件
crate `ch8` 的 `build.rs` MUST 声明在下列变更时重新运行：
- `build.rs` 文件内容变更
- 环境变量 `LOG` 变更
- 环境变量 `APP_ASM` 变更

#### Scenario: LOG 变更触发重跑
- **WHEN** `LOG` 环境变量发生变化
- **THEN** Cargo MUST 重新运行 `build.rs`

### Requirement: 启动初始化与关机行为
内核入口 `rust_main` MUST 完成以下初始化流程（按可观察顺序）：
- MUST 清零 `.bss`
- MUST 初始化 `console` 并将日志级别设置为 `option_env!("LOG")`
- MUST 初始化内核堆并将剩余物理内存转交给分配器
- MUST 构建内核地址空间并安装页表（写入 `satp`）
- MUST 初始化异界传送门（portal）并初始化 `syscall` 各子系统（I/O、进程、调度、时钟、信号、线程、互斥/信号量）
- MUST 读取并加载 `initproc`（若可解析为 RISC-V 可执行 ELF）
- 在无可运行任务时 SHOULD 打印 `no task` 并结束主循环
- 结束主循环后 MUST 通过 `sbi_rt::system_reset(Shutdown, NoReason)` 关机

#### Scenario: 正常启动并关机
- **WHEN** `initproc` 成功加载且任务最终全部退出
- **THEN** 内核 MUST 进入调度/分发循环直至无任务可运行
- **AND THEN** 内核 MUST 发起 `Shutdown`

### Requirement: 内核地址空间映射（内核段/堆/portal/MMIO）
内核 MUST 在地址空间中映射：
- `linker::KernelLayout` 所描述的内核各段，并对不同段应用与其语义匹配的 `VmFlags`
- `[layout.end(), layout.start()+MEMORY)` 作为堆/可用内存区，并以可写映射
- portal 传送门所在虚页 `PROTAL_TRANSIT`，并映射到 portal 的物理页
- `MMIO` 中列出的每个区间

#### Scenario: MMIO 被映射
- **WHEN** 内核完成 `kernel_space(...)` 构建
- **THEN** `MMIO` 中每个 `(base, len)` 区间 MUST 在地址空间中存在对应映射

### Requirement: `initproc` 的读取与 ELF 装载
内核 MUST 通过文件系统打开并读取 `initproc` 文件的全部内容，并在其可解析为 RISC-V 64-bit 可执行 ELF 时：
- MUST 为其建立新的用户态地址空间
- MUST 按 ELF `PT_LOAD` 段映射其内容，并设置 U/R/W/X 权限
- MUST 为其映射用户栈（2 页）
- MUST 映射 portal 页表项
- MUST 创建一个初始 `Thread`，其用户态入口为 ELF `entry_point`，其初始 `sp` 为约定的栈顶

#### Scenario: initproc 加载成功
- **WHEN** `initproc` 是可执行的 RISC-V ELF
- **THEN** 内核 MUST 创建对应 `Process` 与初始 `Thread` 并加入调度

### Requirement: 调度循环与 syscall 分发
在每次选中任务后，内核 MUST 执行该任务上下文并根据 `scause` 处理陷入：
- **IF** `scause` 为 `UserEnvCall`：MUST 读取 `a7` 为 syscall id、`a0..a5` 为参数并调用 `syscall::handle(...)`
- syscall 执行后 MUST 执行一次信号处理（当前实现位置为 syscall 之后）
- MUST 按 syscall id 与返回值更新当前任务状态（退出/阻塞/挂起）
- **ELSE**（非 `UserEnvCall`）：MUST 记录错误并以错误码退出当前任务

#### Scenario: 用户态发起 syscall 并返回
- **WHEN** 当前任务触发 `UserEnvCall`
- **THEN** 内核 MUST 调用 `syscall::handle` 并将返回值写回用户态 `a0`（对需要返回值的 syscall）
- **AND THEN** 当前任务 MUST 被标记为挂起以便后续再次调度

### Requirement: 文件描述符 I/O（read/write/open/close）
通过 `SyscallContext` 的 I/O 适配层，系统 MUST 提供最小可用的类 Unix 文件描述符 I/O 能力（含标准输入/输出与基于 easy-fs 的普通文件）。
- `write` MUST 支持向 `STDOUT`/`STDDEBUG` 输出；对普通 fd，若对应文件不可写 MUST 返回错误
- `read` MUST 支持从 `STDIN` 读取字符；对普通 fd，若对应文件不可读 MUST 返回错误
- `open` MUST 将用户态传入的 C 字符串路径解析为 Rust 字符串，并通过 `FS.open(path, OpenFlags)` 打开文件；成功时 MUST 分配一个新的 fd 并返回
- `close` MUST 关闭有效 fd；对无效 fd MUST 返回错误

#### Scenario: open 一个文件并写入
- **WHEN** 用户态对某路径调用 `open` 并获得有效 fd
- **AND WHEN** 用户态对该 fd 调用 `write`
- **THEN** 内核 MUST 将数据写入对应文件

### Requirement: 进程管理（fork/exec/exit/wait/getpid）
通过 `SyscallContext` 的进程适配层，系统 MUST 支持最小的进程管理能力（fork/exec/exit/wait/getpid）。
- `fork` MUST 克隆当前进程地址空间与必要状态并创建子进程；子进程返回值 MUST 为 0，父进程返回值 MUST 为子进程 pid
- `exec` MUST 从文件系统加载指定程序；若程序不存在 MUST 打印可用程序列表并返回错误
- `exit` MUST 使当前进程退出并携带退出码
- `wait` MUST 等待指定子进程退出；成功时 MUST 将退出码写回用户指针并返回已退出子进程 pid
- `getpid` MUST 返回当前进程 pid

#### Scenario: fork 后父子返回值不同
- **WHEN** 用户态调用 `fork`
- **THEN** 子进程 MUST 观察到返回值为 0
- **AND THEN** 父进程 MUST 观察到返回值为子进程 pid

### Requirement: 时钟查询（clock_gettime）
通过 `SyscallContext` 的时钟适配层，系统 MUST 支持对单调时钟的查询并将结果写回用户空间。
- 对 `CLOCK_MONOTONIC`，`clock_gettime` MUST 写入一个 `TimeSpec { tv_sec, tv_nsec }` 到用户指针，并返回成功

#### Scenario: 查询单调时钟
- **WHEN** 用户态调用 `clock_gettime(CLOCK_MONOTONIC, tp)`
- **THEN** `*tp` MUST 被写入合法的 `TimeSpec`

### Requirement: 信号（kill/sigaction/sigprocmask/sigreturn）
通过 `SyscallContext` 的信号适配层，系统 MUST 支持最小的信号投递、处理动作管理与上下文恢复语义。
- `kill` MUST 向目标进程投递指定信号（若 pid/信号号无效 MUST 返回错误）
- `sigaction` MUST 支持读取/设置指定信号的处理动作（action/old_action 为 0 表示不读/不写）
- `sigprocmask` MUST 更新当前进程的信号屏蔽字并返回更新结果
- `sigreturn` MUST 将被信号处理保存的上下文恢复到当前线程上下文（失败则返回错误）

#### Scenario: kill 投递信号
- **WHEN** 用户态对存在的 pid 调用 `kill(pid, signum)`
- **THEN** 目标进程的信号模块 MUST 记录该信号待处理

### Requirement: 线程（thread_create/gettid/waittid）
通过 `SyscallContext` 的线程适配层，系统 MUST 支持同一进程内创建/查询/等待线程。
- `thread_create` MUST 为新线程分配用户栈并创建线程，且将 `arg` 写入新线程用户态 `a0`
- `gettid` MUST 返回当前线程 tid
- `waittid` MUST 等待指定 tid 线程退出并返回退出码；线程 MUST NOT 等待自身

#### Scenario: 创建线程并传参
- **WHEN** 用户态调用 `thread_create(entry, arg)`
- **THEN** 新线程 MUST 从 `entry` 开始执行并在其 `a0` 中观察到 `arg`

### Requirement: 同步原语（信号量/互斥/条件变量）
通过 `SyscallContext` 的同步适配层，系统 MUST 提供阻塞式同步原语（信号量/互斥/条件变量）并与调度器联动完成阻塞与唤醒。
- `semaphore_create` MUST 创建一个计数信号量并返回其 id
- `semaphore_down` 在无法获取资源时 MUST 使当前线程进入阻塞路径（以返回值 `-1` 表示需要阻塞）
- `semaphore_up` 在释放资源后 SHOULD 唤醒一个等待线程（若存在）
- `mutex_create` MUST 创建“阻塞式互斥锁”；`mutex_lock` 在无法获取锁时 MUST 进入阻塞路径（以返回值 `-1` 表示需要阻塞）；`mutex_unlock` SHOULD 唤醒一个等待线程（若存在）
- `condvar_create` MUST 创建条件变量并返回其 id
- `condvar_wait` MUST 以“释放互斥锁并等待条件”语义阻塞当前线程，并在需要时唤醒相关线程
- `condvar_signal` SHOULD 唤醒一个等待线程（若存在）

#### Scenario: mutex_lock 竞争导致阻塞
- **WHEN** 线程 A 已持有互斥锁
- **AND WHEN** 线程 B 调用 `mutex_lock` 试图获取同一把锁
- **THEN** B 的 `mutex_lock` MUST 走阻塞路径（以返回 `-1` 表示）

## Public API

`ch8` 为 `#![no_std]`、`#![no_main]` 的二进制 crate，不提供稳定的对外库 API。其对外可观察接口主要体现在：
- 构建产物（链接脚本注入）
- 通过 `syscall` 子系统向用户态暴露的系统调用语义
- 通过 SBI/virtio/MMIO 与外设交互的行为

### Crate Root Exports
- `PROCESSOR`: 全局调度/任务管理器实例（来自 `rcore_task_manage::PThreadManager`）
- `MMIO`: 设备 MMIO 映射区间列表

## Build Configuration
- **build.rs**: 生成 `$OUT_DIR/linker.ld` 并通过 `-T` 注入链接脚本
- **环境变量**:
  - `OUT_DIR`: Cargo 提供，用于输出 `linker.ld`
  - `LOG`: 触发 build 重跑；运行时用于设置日志级别（通过 `option_env!("LOG")`）
  - `APP_ASM`: 触发 build 重跑（具体语义由上游构建/链接链路决定）
- **生成文件**:
  - `$OUT_DIR/linker.ld`

## Dependencies

### Workspace crates（Preconditions）
- `linker`:
  - MUST 提供 `SCRIPT`（链接脚本文本）
  - MUST 提供 `boot0!` 宏以定义内核入口与栈
  - MUST 提供 `KernelLayout::locate()`、`KernelLayout::iter()` 与段信息（含 `KernelRegionTitle` 与段地址范围）
- `rcore-console`（`console`）:
  - MUST 提供 `init_console(...)`、`set_log_level(...)` 与 `test_log()`
  - `println!`/日志宏 MUST 可在 `no_std` 环境工作
- `kernel-alloc`:
  - MUST 提供 `init(heap_start)` 初始化堆
  - MUST 提供 `transfer(&mut [u8])` 接管剩余内存
- `kernel-vm`:
  - MUST 提供 `AddressSpace` 以及 `map/map_extern/translate/cloneself/root/root_ppn` 等语义
  - MUST 提供 `Sv39`、`VAddr`、`VPN`、`PPN`、`VmFlags` 等页表/地址类型
- `kernel-context`:
  - MUST 提供 `LocalContext::user(entry)` 以创建用户上下文
  - MUST 提供 `foreign::MultislotPortal` 以创建并执行 `ForeignContext`
- `syscall`:
  - MUST 提供 `init_*` 初始化函数、`handle(...)` 分发函数
  - MUST 提供各 trait（`IO/Process/Scheduling/Clock/Signal/Thread/SyncMutex`）与类型（`SyscallId/SyscallResult/Caller/ClockId/TimeSpec` 等）
- `rcore-task-manage`（`task-manage`）:
  - MUST 提供 `PThreadManager` 及其线程/进程管理与调度接口（如 `add_proc/add/find_next/current/get_current_proc/make_current_*` 等）
  - MUST 提供 `ProcId/ThreadId` 标识类型
- `easy-fs`:
  - MUST 提供 `EasyFileSystem`、`Inode`、`FileHandle`、`OpenFlags`、`UserBuffer` 与 `FSManager` trait
- `signal`:
  - MUST 提供信号编号与上限（如 `MAX_SIG`）以及 `Signal` trait 的所需行为（投递、mask、sigreturn 等）
  - MUST 提供 `SignalResult`（用于调度循环中决定是否杀死进程）
- `signal-impl`:
  - MUST 提供 `SignalImpl::new()` 且可作为 `dyn Signal`
  - MUST 支持 `from_fork()` 以在 `fork` 时克隆信号状态
- `sync`:
  - MUST 提供 `Semaphore/MutexBlocking/Condvar` 及其阻塞/唤醒语义（返回可唤醒的 tid）

### External crates
- `virtio-drivers`: virtio block 设备驱动与 `Hal` 接口
- `sbi-rt`: SBI 调用（console、system_reset）
- `riscv`: CSR 访问（`satp/scause/time`）
- `xmas-elf`: ELF 解析
- `spin`: `Lazy/Mutex` 自旋同步原语

