## Context

块缓存层减少磁盘 I/O，通过统一管理实现写操作合并和读操作复用。

## Goals / Non-Goals

- Goals: 透明缓存、自动脏块管理、类型安全访问
- Non-Goals: 不实现 LRU、不支持异步写回、不支持预取

## Decisions

- **Arc<Mutex<BlockCache>>**：支持多方持有和并发访问
- **类 FIFO 替换**：通过 `strong_count` 检查避免替换在用缓存
- **Drop 自动同步**：RAII 模式确保数据不丢失
- **固定容量 16**：简单可预测

## Safety Notes

`get_ref/get_mut` 使用 unsafe 将字节数组转为类型引用，假设：
- `T` 使用 `#[repr(C)]` 布局
- `offset` 满足对齐要求
- 磁盘与 CPU 字节序一致
