# Capability: syscall

## Purpose

该 capability 描述 workspace crate `syscall` 的对外契约与边界：

- 在 **`user` feature** 下提供 RISC-V `ecall` 系统调用封装与若干常用 wrapper。
- 在 **`kernel` feature** 下提供系统调用分发器 `handle()` 与可注入的 syscall handler traits。
- 在不引入 `std` 的前提下提供 syscall ABI 所需的基础类型与常量（如 `SyscallId`/`TimeSpec`/stdio fds 等）。

## Requirements

### Requirement: Feature gating 与 no_std 约束
crate `syscall` MUST be `#![no_std]` 并在启用 features 时提供明确的编译期约束。

#### Scenario: 同时启用 `kernel` 与 `user`
- **WHEN** `syscall` crate 同时启用 `kernel` 与 `user` features
- **THEN** 编译 MUST 失败（`compile_error!`），以避免在同一构建产物中混用两套角色语义

#### Scenario: 仅启用 `kernel` 或仅启用 `user`
- **WHEN** 仅启用 `kernel` 或仅启用 `user`（或两者均未启用）
- **THEN** crate MUST 成功编译，并仅暴露相应 feature 下的 public API

### Requirement: SyscallId 类型安全与 syscall 号常量
crate MUST 提供 `SyscallId` 作为 syscall 号的包装类型，并提供一组可用的 syscall 号常量（关联常量）以用于发起/分发系统调用。

#### Scenario: 从 `usize` 构造 SyscallId
- **WHEN** 调用 `SyscallId::from(usize)`
- **THEN** MUST 返回 `SyscallId(usize)`，且保持 `#[repr(transparent)]` 的 ABI 语义

#### Scenario: 使用生成的 syscall 号常量
- **WHEN** 代码使用 `SyscallId::WRITE` / `SyscallId::READ` / `SyscallId::CLOCK_GETTIME` 等常量
- **THEN** 这些常量 MUST 存在且其数值 MUST 与生成源（见 Build Configuration）一致

### Requirement: build.rs 生成 syscall 号表
crate MUST 通过 `build.rs` 从输入文件生成 `src/syscalls.rs`，并将其编译进 crate，以提供 syscall 号常量。

#### Scenario: 变更输入文件触发重新生成
- **WHEN** `build.rs` 或 `src/syscall.h.in` 发生变化
- **THEN** Cargo build MUST 重新运行 build script 并刷新生成的 `src/syscalls.rs`

#### Scenario: 正常生成 `src/syscalls.rs`
- **WHEN** 执行 crate 的 build script
- **THEN** build script MUST 生成 `src/syscalls.rs`，其中包含 `impl crate::SyscallId { pub const ... }` 的一组关联常量

### Requirement: `user` feature 下的 RISC-V ecall 原语（native）
在 `user` feature 下，crate MUST 提供最小封装 `native::syscall0..syscall6`，以使用 RISC-V `ecall` 发起系统调用。

#### Scenario: 发起不带参数的系统调用
- **WHEN** 调用 `unsafe native::syscall0(id)`
- **THEN** 实现 MUST 将 `id.0` 放入寄存器 `a7`，执行 `ecall`，并返回 `a0` 作为 `isize`

#### Scenario: 发起带参数的系统调用
- **WHEN** 调用 `unsafe native::syscallN(id, a0..a{N-1})`（N=1..6）
- **THEN** 实现 MUST 将参数放入 `a0..a{N-1}`，将 `id.0` 放入 `a7`，执行 `ecall`，并返回 `a0` 作为 `isize`

### Requirement: `user` feature 下的高层 syscall wrappers
在 `user` feature 下，crate MUST 提供一组安全包装函数（如 `read/write/exit/...`）来调用对应的 syscall id，并将参数按约定传递给 `native::syscall*`。

#### Scenario: `wait`/`waitpid` 的忙等让出
- **WHEN** 调用 `wait(exit_code_ptr)` 或 `waitpid(pid, exit_code_ptr)` 且底层 syscall 返回 `-2`
- **THEN** wrapper MUST 调用 `sched_yield()` 并重试，直到返回值不为 `-2` 后将其作为结果返回

#### Scenario: `read` 写入缓冲区的调用方约束
- **WHEN** 调用 `read(fd, buffer)` 来接收内核写入的数据
- **THEN** 调用方 MUST 确保 `buffer.as_ptr()` 指向的内存是可写的（例如传入 `&mut [u8]` 并发生到 `&[u8]` 的借用/强转），否则行为未定义或由内核返回错误

#### Scenario: `OpenFlags` 的位语义
- **WHEN** 调用 `open(path, flags)`
- **THEN** wrapper MUST 将 `flags.bits()`（底层 `u32`）按 `usize` 传给底层 syscall，并保持 bitflags 的位语义不被修改

### Requirement: `kernel` feature 下的 handler 注入与一次性初始化
在 `kernel` feature 下，crate MUST 允许内核侧通过 `init_*` API 注册各子系统 handler（trait object），并以一次性初始化（`spin::Once`）语义存储。

#### Scenario: 未初始化 handler 的 syscall 分发
- **WHEN** 调用 `handle(caller, id, args)`，且对应子系统 handler 未通过 `init_*` 注册
- **THEN** `handle` MUST 返回 `SyscallResult::Unsupported(id)`（而不是 panic）

#### Scenario: 初始化后分发成功
- **WHEN** 已通过 `init_*` 注册了对应子系统 handler，且调用 `handle(caller, id, args)`
- **THEN** `handle` MUST 调用相应 trait 方法并返回 `SyscallResult::Done(ret)`

### Requirement: `kernel` feature 下的 syscall id 到 trait 方法映射
在 `kernel` feature 下，crate MUST 将一组已支持的 syscall id 映射到对应 trait 方法，并按固定参数槽位从 `args: [usize; 6]` 取参。

#### Scenario: IO 写调用映射
- **WHEN** `handle(caller, SyscallId::WRITE, [fd, buf, count, ..])` 被调用且 IO handler 已初始化
- **THEN** `handle` MUST 调用 `IO::write(caller, fd, buf, count)` 并返回 `SyscallResult::Done(ret)`

#### Scenario: 未支持的 syscall id
- **WHEN** `handle` 收到未在 match 列表内的 `SyscallId`
- **THEN** MUST 返回 `SyscallResult::Unsupported(id)`

### Requirement: 基础时间类型（ClockId/TimeSpec）
crate MUST 提供基本的时间类型以在 syscall 接口中传递/计算时间。

#### Scenario: `TimeSpec` 加法进位
- **WHEN** 计算 `TimeSpec + TimeSpec` 且 `tv_nsec` 溢出超过 1_000_000_000
- **THEN** 实现 MUST 对 `tv_sec` 进位并将 `tv_nsec` 归一化到合法范围

## Public API

### Root (always available)
- **Types**
  - `SyscallId(pub usize)`: syscall 号包装类型（`#[repr(transparent)]`）
  - `ClockId(pub usize)`: 时钟 ID 包装类型（`#[repr(transparent)]`）
  - `TimeSpec { tv_sec: usize, tv_nsec: usize }`: 时间结构（`#[repr(C)]`）
- **Constants**
  - `STDIN: usize = 0`
  - `STDOUT: usize = 1`
  - `STDDEBUG: usize = 2`
  - `ClockId::CLOCK_*`: 一组时钟 ID 常量
  - `TimeSpec::{ZERO, SECOND, MILLSECOND, MICROSECOND, NANOSECOND}`
  - `SyscallId::{...}`: 由 `build.rs` 生成的 syscall 号常量集合（如 `READ/WRITE/EXIT/...`）
- **Functions**
  - `TimeSpec::from_millsecond(millsecond: usize) -> TimeSpec`

### Re-exports (from workspace crate `signal-defs`)
- `SignalAction`
- `SignalNo`
- `MAX_SIG`

### `user` feature
- **Types**
  - `OpenFlags`: `bitflags!` 定义的打开标志（底层 `u32`）
- **Functions (syscall wrappers)**
  - `write(fd: usize, buffer: &[u8]) -> isize`
  - `read(fd: usize, buffer: &[u8]) -> isize`
  - `open(path: &str, flags: OpenFlags) -> isize`
  - `close(fd: usize) -> isize`
  - `exit(exit_code: i32) -> isize`
  - `sched_yield() -> isize`
  - `clock_gettime(clockid: ClockId, tp: *mut TimeSpec) -> isize`
  - `fork() -> isize`
  - `exec(path: &str) -> isize`
  - `wait(exit_code_ptr: *mut i32) -> isize`
  - `waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize`
  - `getpid() -> isize`
  - `kill(pid: isize, signum: SignalNo) -> isize`
  - `sigaction(signum: SignalNo, action: *const SignalAction, old_action: *const SignalAction) -> isize`
  - `sigprocmask(mask: usize) -> isize`
  - `sigreturn() -> isize`
  - `thread_create(entry: usize, arg: usize) -> isize`
  - `gettid() -> isize`
  - `waittid(tid: usize) -> isize`
  - `semaphore_create(res_count: usize) -> isize`
  - `semaphore_up(sem_id: usize) -> isize`
  - `semaphore_down(sem_id: usize) -> isize`
  - `mutex_create(blocking: bool) -> isize`
  - `mutex_lock(mutex_id: usize) -> isize`
  - `mutex_unlock(mutex_id: usize) -> isize`
  - `condvar_create() -> isize`
  - `condvar_signal(condvar_id: usize) -> isize`
  - `condvar_wait(condvar_id: usize, mutex_id: usize) -> isize`
- **Module**
  - `native`: 最小 syscall 原语
    - `unsafe fn syscall0(id: SyscallId) -> isize`
    - `unsafe fn syscall1(id: SyscallId, a0: usize) -> isize`
    - `unsafe fn syscall2(id: SyscallId, a0: usize, a1: usize) -> isize`
    - `unsafe fn syscall3(id: SyscallId, a0: usize, a1: usize, a2: usize) -> isize`
    - `unsafe fn syscall4(id: SyscallId, a0: usize, a1: usize, a2: usize, a3: usize) -> isize`
    - `unsafe fn syscall5(id: SyscallId, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> isize`
    - `unsafe fn syscall6(id: SyscallId, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize) -> isize`

### `kernel` feature
- **Types**
  - `Caller { entity: usize, flow: usize }`
  - `SyscallResult::{Done(isize), Unsupported(SyscallId)}`
- **Traits**
  - `Process`, `IO`, `Memory`, `Scheduling`, `Clock`, `Signal`, `Thread`, `SyncMutex`
- **Functions**
  - `init_process(&'static dyn Process)`
  - `init_io(&'static dyn IO)`
  - `init_memory(&'static dyn Memory)`
  - `init_scheduling(&'static dyn Scheduling)`
  - `init_clock(&'static dyn Clock)`
  - `init_signal(&'static dyn Signal)`
  - `init_thread(&'static dyn Thread)`
  - `init_sync_mutex(&'static dyn SyncMutex)`
  - `handle(caller: Caller, id: SyscallId, args: [usize; 6]) -> SyscallResult`

## Build Configuration

- **Features**
  - `kernel`: 启用内核侧分发与 handler 注入 API
  - `user`: 启用用户侧 `ecall` 原语与 syscall wrappers
  - `kernel` 与 `user` MUST NOT 同时启用
- **build.rs**
  - 输入文件: `src/syscall.h.in`
  - 生成文件: `src/syscalls.rs`
  - rerun-if-changed: `build.rs` 与 `src/syscall.h.in`
- **环境变量**
  - 无（build.rs 未声明/读取环境变量）

## Dependencies

- **Workspace crates**
  - `signal-defs`
    - **Preconditions**
      - MUST 提供 `SignalAction` 且具有 `#[repr(C)]` 布局（至少包含 `handler: usize` 与 `mask: usize` 字段），以便在用户/内核 ABI 边界传递。
      - MUST 提供 `SignalNo` 且具有 `#[repr(u8)]` 语义（可转换为数值用于 syscall 参数传递）。
      - MUST 提供 `MAX_SIG` 常量以指示最大信号编号范围。
- **External crates**
  - `spin`: 用于 `spin::Once`（内核侧 handler 一次性注入）
  - `bitflags`: 用于 `OpenFlags` 的位标志定义

