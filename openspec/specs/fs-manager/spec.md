# Capability: fs-manager

磁盘块管理器层，整合磁盘布局、管理块分配/回收，提供文件系统创建和打开接口。

## Purpose

作为 easy-fs 的核心管理组件，整合位图和块缓存等底层组件，负责文件系统的创建、打开、以及 inode 和数据块的分配与回收。

## Requirements

### Requirement: EasyFileSystem 结构

`EasyFileSystem` MUST 包含：
- `block_device: Arc<dyn BlockDevice>`
- `inode_bitmap: Bitmap`
- `data_bitmap: Bitmap`
- `inode_area_start_block: u32`
- `data_area_start_block: u32`

#### Scenario: 运行时状态

- **WHEN** 打开文件系统
- **THEN** MUST 正确记录各区域起始块号

### Requirement: 创建文件系统

`create(block_device, total_blocks, inode_bitmap_blocks)` MUST：
1. 计算各区域大小
2. 清零所有块
3. 初始化 SuperBlock
4. 分配 inode 0 为根目录（Directory）
5. 调用 `block_cache_sync_all()`
6. 返回 `Arc<Mutex<EasyFileSystem>>`

#### Scenario: create 后可 open

- **WHEN** `create` 后对同一设备 `open`
- **THEN** MUST 成功打开

### Requirement: 打开文件系统

`open(block_device)` MUST：
1. 读取 Block 0 的 SuperBlock
2. 校验魔数，失败则 panic
3. 构建 `EasyFileSystem` 并返回

#### Scenario: 非法设备

- **WHEN** Block 0 魔数不匹配
- **THEN** MUST panic

### Requirement: 获取根目录

`root_inode(efs)` MUST 返回 inode id 为 0 的 `Inode` 实例。

#### Scenario: root_inode 可用

- **WHEN** 获取 root_inode
- **THEN** `readdir()` MUST 正常工作

### Requirement: 位置计算

`EasyFileSystem` MUST 提供位置计算方法：
- `get_disk_inode_pos(inode_id)` MUST 返回 `(inode_area_start + inode_id/4, (inode_id%4)*128)`
- `get_data_block_id(data_block_id)` MUST 返回 `data_area_start + data_block_id`

#### Scenario: inode 位置

- **WHEN** `inode_area_start = 2`, `inode_id = 5`
- **THEN** MUST 返回 `(3, 128)`

### Requirement: 分配与回收

- `alloc_inode()` MUST 从 inode 位图分配，返回 inode id
- `alloc_data()` MUST 从数据位图分配，返回磁盘绝对块号
- `dealloc_data(block_id)` MUST 清零块内容并回收

#### Scenario: 分配返回绝对块号

- **WHEN** `data_area_start = 100`，分配第一个数据块
- **THEN** MUST 返回 100

## Public API

- `EasyFileSystem`
  - `create(block_device, total_blocks, inode_bitmap_blocks) -> Arc<Mutex<Self>>`
  - `open(block_device) -> Arc<Mutex<Self>>`
  - `root_inode(efs) -> Inode`
  - `get_disk_inode_pos(inode_id) -> (u32, usize)`
  - `get_data_block_id(data_block_id) -> u32`
  - `alloc_inode(&mut self) -> u32`
  - `alloc_data(&mut self) -> u32`
  - `dealloc_data(&mut self, block_id)`

## Dependencies

- `block-device`, `block-cache`, `disk-layout`
- `spin`: `Mutex`
- `alloc`: `Arc`
