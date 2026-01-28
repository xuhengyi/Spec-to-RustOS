## Context

crate `syscall` 在同一份代码中同时覆盖两类使用者：

- `user` feature：用户态/运行库侧，用 RISC-V `ecall` 触发系统调用，并提供若干常用 syscall wrapper。
- `kernel` feature：内核态侧，提供 syscall id 分发器 `handle()`，并通过一组 trait object 将具体实现注入进分发器。

该 crate 为 `#![no_std]`，因此适用于内核与裸机/嵌入式场景。

## Goals / Non-Goals

- **Goals**
  - 为 syscall ABI 提供一个小而清晰的边界层：类型（如 `SyscallId`/`TimeSpec`）、常量（syscall ids / clock ids / stdio fds）、以及 user/kernel 两侧的最小 glue。
  - 在 kernel 侧以“可插拔 handler”形式隔离具体实现，避免 `syscall` crate 依赖内核其它子系统实现细节。
- **Non-Goals**
  - 不定义完整 POSIX/Linux syscall 语义；wrapper 的语义以“参数传递与返回值透传”为主。
  - 不保证跨架构可移植性；`native` 当前绑定 RISC-V `ecall` 调用约定。

## Decisions

- **Decision: 用 feature 做角色分离（`kernel` vs `user`）**
  - 通过 `#[cfg(feature = "...")]` 在编译期裁剪不同角色 API。
  - 使用 `compile_error!` 防止两者同时启用，避免语义混用。

- **Decision: syscall id 由 build.rs 生成**
  - `build.rs` 从 `src/syscall.h.in` 中解析 `#define __NR_*` 生成 `src/syscalls.rs` 的关联常量，确保 syscall 号表可追溯且可批量维护。

- **Decision: kernel 分发采用 Once 注入的 trait object**
  - 每个子系统（Process/IO/Memory/Signal/...）通过 `init_*` 注入 `&'static dyn Trait`，存储在 `spin::Once` 中。
  - 未注入时返回 `SyscallResult::Unsupported(id)`，用结果类型而非 panic 表达“未实现/未注册”。

## Safety / Portability Notes

- **Unsafe assembly**
  - `user::native::syscall*` 使用 `core::arch::asm!` 执行 `ecall`，依赖 RISC-V 寄存器约定（`a0..a5` 传参、`a7` 传 syscall id、`a0` 返回）。
  - 这些函数被标记为 `unsafe`；调用方必须保证参数指针、内存可访问性与 ABI 约定满足内核侧期待。

- **Pointer & string contracts**
  - wrapper 将 `&str` 的 `as_ptr()` 直接作为 `usize` 传给内核侧；调用方必须与内核实现约定字符串编码与终止方式（如是否要求 NUL 结尾、长度如何获得等）。

## Feature Matrix

- **No feature**：仅暴露 `SyscallId`、基础常量与时间/信号相关类型 re-export（不包含 user/kernel 的额外 API）。
- **`user`**：增加 `native` 原语与 syscall wrappers、`OpenFlags`。
- **`kernel`**：增加 `Caller`、`SyscallResult`、各子系统 traits、`init_*` 与 `handle()` 分发器。
- **Mutual exclusion**：`kernel` 与 `user` 不能同时启用。

