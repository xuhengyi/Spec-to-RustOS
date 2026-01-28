## Context

块设备接口层将文件系统与存储设备解耦，支持用户态测试（文件模拟）和内核态运行（virtio-blk）。

## Goals / Non-Goals

- Goals: 最小化接口、支持 `#![no_std]`、支持 `Arc<dyn BlockDevice>`
- Non-Goals: 不处理设备初始化、不支持异步 I/O

## Decisions

- **使用 trait**：使 easy-fs 可脱离内核独立测试
- **要求 Send + Sync + Any**：支持多线程和类型擦除
- **块大小固定 512**：与传统扇区大小一致，简化实现
