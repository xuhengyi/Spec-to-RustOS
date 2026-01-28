## Context

VFS 索引节点层是 easy-fs 面向使用者的最高层抽象，将底层封装成易用接口。

## Goals / Non-Goals

- Goals: 简洁的文件/目录 API、封装锁管理、文件句柄机制
- Non-Goals: 不实现完整 POSIX、不支持多级路径解析

## Decisions

- **Inode 持有 Arc<Mutex<EasyFileSystem>>**：可独立操作
- **所有操作持有 fs 锁**：确保并发安全
- **write_at 自动扩容**：简化调用方逻辑
- **FSManager 由内核实现**：路径解析依赖进程上下文

## Locking

所有 Inode 方法遵循：获取 fs 锁 → 操作 → 释放锁

注意避免：持有缓存锁时再获取 fs 锁（可能死锁）

## FileHandle

```
FileHandle
  ├── inode: 底层 Inode
  ├── read/write: 权限
  └── offset: 当前偏移（read/write 后自动更新）
```
