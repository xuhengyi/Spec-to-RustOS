# Capability: block-device

定义块设备抽象接口层，是 easy-fs 的最底层，使文件系统与具体块设备驱动解耦。

## Purpose

提供以 512 字节块为单位的读写抽象，供块缓存层调用。调用方需实现 `BlockDevice` trait。

## Requirements

### Requirement: 块大小常量

`BLOCK_SZ: usize = 512` MUST 作为公开常量导出。

#### Scenario: 常量可用

- **WHEN** 引用 `easy_fs::BLOCK_SZ`
- **THEN** MUST 返回 512

### Requirement: BlockDevice trait

`BlockDevice` trait MUST 定义如下接口并要求 `Send + Sync + Any`：

```rust
pub trait BlockDevice: Send + Sync + Any {
    fn read_block(&self, block_id: usize, buf: &mut [u8]);
    fn write_block(&self, block_id: usize, buf: &[u8]);
}
```

#### Scenario: 读写一致性

- **WHEN** 对 `block_id` 先 `write_block` 再 `read_block`
- **THEN** 读取内容 MUST 与写入一致

### Requirement: 块读写语义

- `read_block(block_id, buf)` MUST 读取 512 字节到 `buf`
- `write_block(block_id, buf)` MUST 写入 512 字节到设备
- 调用方 MUST 确保 `buf.len() == 512`

#### Scenario: 缓冲区大小

- **WHEN** `buf.len() != 512`
- **THEN** 行为未定义（实现可 panic）

## Public API

- `BLOCK_SZ: usize = 512`
- `trait BlockDevice: Send + Sync + Any`
  - `read_block(&self, block_id: usize, buf: &mut [u8])`
  - `write_block(&self, block_id: usize, buf: &[u8])`

## Dependencies

- `core::any::Any`
