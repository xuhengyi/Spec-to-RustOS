## Context

crate `ch8` 是 `no_std` 的内核二进制，负责在 RISC-V（Sv39）上完成启动初始化、建立内核地址空间、初始化 virtio block + easy-fs 文件系统，并通过 `syscall` 子系统向用户态提供基础 OS 能力（进程/线程/同步/信号/时钟/I-O）。

## Goals / Non-Goals

- Goals:
  - 明确启动与调度循环的可观察行为边界
  - 记录关键 unsafe/假设（指针翻译、索引越界、页表所有权标记等）
  - 说明 syscall 适配层的限制与前置条件
- Non-Goals:
  - 不规定 `syscall`/`task-manage`/`kernel-vm` 等依赖 crate 的内部实现
  - 不引入新的功能/修复行为差异（本设计文档仅描述现状约束）

## Architecture Overview

### Boot & Memory Model
- 使用 `linker::boot0!` 定义入口 `rust_main` 与内核栈。
- 通过 `linker::KernelLayout` 获取内核段信息，并将其映射到 `kernel_vm::AddressSpace`。
- 建立 portal（`kernel_context::foreign::MultislotPortal`）并将 portal 页表项复制到每个用户地址空间（`map_portal`）。
- 初始化完成后直接写 `satp` 切换到 Sv39 页表。

### User Execution Model
- 调度器为 `rcore_task_manage::PThreadManager<Process, Thread, ...>`，内核循环使用 `find_next()` 取出下一个线程。
- 线程执行通过 `ForeignContext::execute(portal, ())` 进入用户态执行（或等价路径），之后依赖 `scause` 判断陷入原因。
- 目前仅显式支持 `UserEnvCall` 路径（syscall）；其他 trap 会导致任务以错误码退出。

### Syscall Adaptation Layer
`SyscallContext` 作为一组 `syscall` trait 的实现者，将用户态参数（用户虚拟地址、id、flags）转换为内核内部动作：
- I/O：通过用户地址空间 `translate` 将用户指针映射为内核可访问指针后读写。
- 进程：`fork/exec/wait/exit/getpid` 通过 `Process`/`Thread` 结构与 `PROCESSOR` 完成。
- 线程：按约定范围扫描“未被映射”的栈区并映射新栈，然后创建新线程。
- 同步：使用 `sync` crate 的阻塞语义（返回可唤醒的 tid）并通过 `PROCESSOR.re_enque` 唤醒。
- 信号：通过 `signal` trait object 维护 pending/mask/action，并在 syscall 后执行一次处理。

## Safety / Unsafe Constraints

### 用户指针与跨页问题
- 多处实现仅对用户缓冲区起始地址做一次 `translate`，随后用 `from_raw_parts(_count)`/`from_raw_parts_mut(_count)` 直接访问连续内存。
- **约束**：调用者必须确保缓冲区在用户地址空间中是连续可访问的（至少覆盖 `count` 字节），否则行为可能是错误返回、数据截断、或未定义行为。

### 句柄/ID 越界
- 多处通过 `vec[index]` 或 `unwrap()` 访问 fd/信号量/互斥/条件变量列表项。
- **约束**：用户态传入的 fd/id MUST 是有效范围内且对应条目存在，否则可能触发 panic 或不受控错误。

### mutex_create 的非阻塞模式
- `mutex_create(blocking=false)` 当前返回 `None` 作为互斥实体并仍分配 id；后续 `mutex_lock/unlock` 会 `unwrap()` 该条目。
- **约束**：用户态 MUST 仅请求阻塞式互斥锁（`blocking=true`）。

### Sv39Manager 的资源回收
- `Sv39Manager` 的 `deallocate`/`drop_root` 为 `todo!()`。
- **影响**：页表相关资源回收/销毁路径尚不完整；规格将其视为实现限制。

### 全局可变状态与初始化时序
- `KERNEL_SPACE`/`PROCESSOR` 为 `static mut`，依赖严格初始化顺序。
- `virtio` 的 `virt_to_phys` 依赖 `KERNEL_SPACE` 已正确初始化并可 `translate`。

## Signal Handling Placement

当前信号处理在 syscall 执行之后进行（代码注释已标注这是临时位置）。这意味着：
- 部分异常（如访存异常）不会在“返回用户态之前”被统一处理为信号；
- 实现更接近“每次 syscall 后检查一次信号”的策略。

## Open Questions
- 是否需要在更多 trap 分支上补齐信号处理点（并明确“返回用户态前”语义）？
- 用户缓冲区跨页的正确处理应由哪个层（`syscall` 适配层还是 `kernel-vm`）承担？

