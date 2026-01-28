# Capability: disk-layout

定义磁盘数据结构层：超级块、位图、磁盘索引节点、目录项等。

## Purpose

定义 easy-fs 在磁盘上的组织方式，包括超级块、位图、索引节点、目录项等核心数据结构的布局和读写接口。

## Requirements

### Requirement: 磁盘布局

磁盘 MUST 按顺序分为 5 个连续区域：
1. SuperBlock（Block 0）
2. Inode Bitmap
3. Inode Area
4. Data Bitmap
5. Data Area

#### Scenario: 布局连续

- **WHEN** 创建文件系统
- **THEN** 各区域 MUST 无间隙顺序排列

### Requirement: SuperBlock

`SuperBlock` MUST 使用 `#[repr(C)]`，包含：`magic`、`total_blocks`、`inode_bitmap_blocks`、`inode_area_blocks`、`data_bitmap_blocks`、`data_area_blocks`。

- `initialize(...)` MUST 设置 `magic = 0x3b800001`
- `is_valid()` MUST 返回 `magic == 0x3b800001`

#### Scenario: 魔数校验

- **WHEN** magic 不匹配
- **THEN** `is_valid()` MUST 返回 false

### Requirement: Bitmap

`Bitmap` MUST 封装起始块号和块数，每块 4096 bits。

- `alloc(block_device)` MUST 找到第一个 0 bit，置 1 并返回编号
- `dealloc(block_device, bit)` MUST 将指定 bit 清零
- `maximum()` MUST 返回 `blocks * 4096`

#### Scenario: 分配回收

- **WHEN** 分配后回收同一 bit
- **THEN** 再次分配 MUST 返回该 bit

### Requirement: DiskInode 结构

`DiskInode` MUST 使用 `#[repr(C)]`，大小 128 字节：
- `size: u32`
- `direct: [u32; 28]`（直接索引）
- `indirect1: u32`（一级间接）
- `indirect2: u32`（二级间接）
- `type_: DiskInodeType`（File/Directory）

每块容纳 4 个 DiskInode。

#### Scenario: 类型判断

- **WHEN** 初始化为 Directory
- **THEN** `is_dir()` MUST 返回 true

### Requirement: 块索引

`get_block_id(inner_id, block_device)` MUST 根据文件内部块号返回磁盘块号：
- `inner_id < 28` → `direct[inner_id]`
- `28 <= inner_id < 156` → 从 `indirect1` 块读取
- `inner_id >= 156` → 从 `indirect2` 两级查找

总容量：28 + 128 + 128×128 = 16540 块 ≈ 8MB

#### Scenario: 跨越间接边界

- **WHEN** `inner_id = 29`
- **THEN** MUST 从 `indirect1` 块的第 1 项读取

### Requirement: 扩容与清空

- `increase_size(new_size, new_blocks, block_device)` MUST 按顺序填充 direct/indirect1/indirect2
- `clear_size(block_device)` MUST 返回所有待回收块编号（含间接块）

#### Scenario: 扩容分配间接块

- **WHEN** 从 27 块扩容到 29 块
- **THEN** MUST 分配 indirect1 块

### Requirement: 文件读写

- `read_at(offset, buf, block_device)` MUST 读取 `[offset, min(offset+buf.len(), size))` 范围数据
- `write_at(offset, buf, block_device)` MUST 写入数据（调用方先确保 size 足够）

#### Scenario: 越界读取

- **WHEN** `offset >= size`
- **THEN** MUST 返回 0

### Requirement: DirEntry

`DirEntry` MUST 使用 `#[repr(C)]`，大小 32 字节：`name[28]` + `inode_number: u32`。

- `new(name, inode_number)` 创建目录项
- `as_bytes()`/`as_bytes_mut()` 序列化
- `name()`/`inode_number()` 访问字段

文件名限制：`0 < name.len() <= 27`

#### Scenario: 目录项序列化

- **WHEN** 调用 `as_bytes()`
- **THEN** MUST 返回 32 字节切片

## Public API

- `SuperBlock`: `initialize`, `is_valid`
- `Bitmap`: `new`, `alloc`, `dealloc`, `maximum`
- `DiskInode`: `initialize`, `is_dir`, `is_file`, `data_blocks`, `total_blocks`, `blocks_num_needed`, `get_block_id`, `increase_size`, `clear_size`, `read_at`, `write_at`
- `DiskInodeType`: `File`, `Directory`
- `DirEntry`: `empty`, `new`, `as_bytes`, `as_bytes_mut`, `name`, `inode_number`
- 常量：`EFS_MAGIC = 0x3b800001`、`INODE_DIRECT_COUNT = 28`、`NAME_LENGTH_LIMIT = 27`、`DIRENT_SZ = 32`

## Dependencies

- `block-cache`: `get_block_cache`
- `alloc`: `Vec`
