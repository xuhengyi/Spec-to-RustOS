# Capability: block-cache

在内存中缓存磁盘块，提供类型安全的数据访问接口，减少实际 I/O 次数。

## Purpose

块缓存层位于块设备之上，为上层提供带脏标记管理和自动写回的块缓存服务，通过缓存复用减少磁盘 I/O。

## Requirements

### Requirement: BlockCache 结构

`BlockCache` MUST 包含：缓存数组 `[u8; 512]`、`block_id`、`block_device` 引用、`modified` 脏标记。

创建时 MUST 调用 `read_block` 加载数据，`modified` 初始为 false。

#### Scenario: 创建触发读取

- **WHEN** `BlockCache::new(block_id, block_device)`
- **THEN** MUST 从块设备读取该块数据

### Requirement: 类型安全访问

MUST 提供泛型方法：

```rust
fn get_ref<T>(&self, offset: usize) -> &T;
fn get_mut<T>(&mut self, offset: usize) -> &mut T;  // 设置 modified = true
fn read<T, V>(&self, offset: usize, f: impl FnOnce(&T) -> V) -> V;
fn modify<T, V>(&mut self, offset: usize, f: impl FnOnce(&mut T) -> V) -> V;
```

前置条件：`offset + size_of::<T>() <= 512`

#### Scenario: get_mut 设置脏标记

- **WHEN** 调用 `get_mut`
- **THEN** `modified` MUST 变为 true

### Requirement: 同步与 Drop

- `sync()` MUST 在 `modified == true` 时写回块设备并重置标记
- `Drop` MUST 自动调用 `sync()`

#### Scenario: Drop 触发写回

- **WHEN** `modified == true` 的 `BlockCache` 被丢弃
- **THEN** MUST 写回块设备

### Requirement: 全局缓存管理器

`BlockCacheManager` MUST 维护最多 16 个 `(block_id, Arc<Mutex<BlockCache>>)` 缓存。

`get_block_cache(block_id, block_device)` 行为：
- 已缓存 → 返回现有引用
- 未缓存且已满 → 替换 `strong_count == 1` 的条目
- 无可替换 → panic

#### Scenario: 缓存复用

- **WHEN** 连续请求同一 `block_id`
- **THEN** MUST 返回同一缓存实例

### Requirement: 全局同步

`block_cache_sync_all()` MUST 同步所有缓存中的脏块。

#### Scenario: sync_all 写回脏块

- **WHEN** 缓存中有脏块
- **THEN** 调用后所有脏块 MUST 被写回

## Public API

- `BlockCache`: `new`, `get_ref`, `get_mut`, `read`, `modify`, `sync`
- `BlockCacheManager`: `new`, `get_block_cache`
- `get_block_cache(block_id, block_device) -> Arc<Mutex<BlockCache>>`
- `block_cache_sync_all()`
- `BLOCK_CACHE_MANAGER: Lazy<Mutex<BlockCacheManager>>`

## Dependencies

- `block-device`: `BlockDevice`, `BLOCK_SZ`
- `spin`: `Mutex`, `Lazy`
- `alloc`: `Arc`, `VecDeque`
