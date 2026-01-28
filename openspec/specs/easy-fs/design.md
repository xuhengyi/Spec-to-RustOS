## Context

`easy-fs` 是一个 `#![no_std]` 的最小文件系统实现，旨在被内核以“块设备 + 自旋锁”方式集成。它包含：
- 磁盘布局（superblock/bitmap/inode/data）
- 基于固定大小块的缓存与写回
- `Inode` 提供的最小目录/文件操作

## Goals / Non-Goals

- Goals:
  - 提供可在教学内核中使用的最小持久化文件读写能力
  - 通过块缓存降低重复 I/O

- Non-Goals:
  - 崩溃一致性（journal/事务）与强一致性语义
  - 完整 POSIX 语义（权限、时间戳、硬链接管理、层级目录创建/删除等）
  - 并发可伸缩性（细粒度锁、异步 I/O）

## On-disk Layout（实现约束）

当前实现将块设备划分为：
- Block 0: `SuperBlock`（含 magic 与各区域大小）
- Inode bitmap: 从 block 1 开始，共 `inode_bitmap_blocks`
- Inode area: 紧随 inode bitmap，用于顺序存放 `DiskInode`
- Data bitmap: 紧随 inode area，用于分配数据区块
- Data area: 数据块区（`EasyFileSystem::get_data_block_id` 采用线性偏移）

约束/注意：
- 目录项为定长 32 字节，文件名采用 0 终止字节串；文件名长度限制为 27 字节以保证可解析
- `DiskInode` 使用 direct + indirect1 + indirect2 的块索引结构

## Safety / Unsafe Notes

实现中存在多处 `unsafe` 的“字节块视图”转换：
- `BlockCache::{get_ref,get_mut}` 将 `[u8; BLOCK_SZ]` 的某个偏移强转为 `&T/&mut T`
- `DirEntry::{as_bytes,as_bytes_mut}` 将结构体视为原始字节序列

这些做法隐含如下假设（当前实现并未在类型层面强制保证）：
- 假设被映射的类型 `T` 采用稳定的内存布局（例如 `#[repr(C)]`）
- 假设偏移位置满足 `T` 的对齐要求（否则在 Rust 语义下可能构成未定义行为）
- 假设磁盘与 CPU 端字节序一致（字段如 `u32` 将以平台端序写入/读出）

本项目将这些假设视为 **实现约束**：在目标教学平台与当前使用方式下可工作，但不保证跨平台/跨编译器优化级别的完全可移植性。

## Concurrency Model

- 块缓存管理器为全局 `spin::Mutex` 保护的队列，容量固定为 16。
- 缓存条目的替换策略基于 `Arc::strong_count == 1`（即仅被缓存管理器持有）才能被驱逐。
- 当缓存已满且没有可驱逐条目时，当前实现选择 panic（而非阻塞/回收）。

## Feature Matrix

- Feature flags: 无
- build.rs: 无
