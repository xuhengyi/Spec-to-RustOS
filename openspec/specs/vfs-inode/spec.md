# Capability: vfs-inode

索引节点层，提供 `Inode`、`FileHandle`、`FSManager` 等高层文件操作接口。

## Purpose

easy-fs 的最顶层，封装 DiskInode 操作，为内核提供 Inode、FileHandle、FSManager 等高层文件/目录操作接口。

## Requirements

### Requirement: Inode 结构

`Inode` MUST 包含：`block_id`、`block_offset`、`fs: Arc<Mutex<EasyFileSystem>>`、`block_device`。

提供辅助方法：
- `read_disk_inode(f)` 以只读方式访问 DiskInode
- `modify_disk_inode(f)` 以可变方式访问 DiskInode

#### Scenario: 创建 Inode

- **WHEN** `Inode::new(block_id, offset, fs, device)`
- **THEN** MUST 返回可用的 Inode 实例

### Requirement: 目录查找

`find(name)` MUST 在目录中查找指定名称的条目，返回 `Option<Arc<Inode>>`。

#### Scenario: 查找已创建文件

- **WHEN** 创建文件 "a" 后调用 `find("a")`
- **THEN** MUST 返回 `Some(...)`

### Requirement: 文件创建

`create(name)` MUST 在当前目录下创建文件：
1. 分配新 inode
2. 初始化为 File
3. 追加目录项
4. 同步缓存
5. 返回 `Some(Arc<Inode>)`

#### Scenario: create 后可查找

- **WHEN** `create("test")`
- **THEN** `find("test")` MUST 返回 Some

### Requirement: 目录枚举

`readdir()` MUST 返回目录下所有条目名称的 `Vec<String>`，顺序与创建顺序一致。

#### Scenario: 枚举顺序

- **WHEN** 依次创建 "a", "b"
- **THEN** MUST 返回 `["a", "b"]`

### Requirement: 文件读写

- `read_at(offset, buf)` MUST 读取文件数据，返回实际字节数
- `write_at(offset, buf)` MUST 写入数据（自动扩容），同步后返回字节数

#### Scenario: 自动扩容

- **WHEN** 对空文件 `write_at(0, data)`
- **THEN** 文件大小 MUST 变为 `data.len()`

### Requirement: 文件清空

`clear()` MUST 清空文件内容，回收所有数据块，同步缓存。

#### Scenario: 清空后为空

- **WHEN** 调用 `clear()`
- **THEN** `read_at(0, buf)` MUST 返回 0

### Requirement: FileHandle

`FileHandle` MUST 包含：`inode: Option<Arc<Inode>>`、`read`、`write` 权限、`offset`。

- `read(buf: UserBuffer)` 从 offset 读取，更新 offset
- `write(buf: UserBuffer)` 从 offset 写入，更新 offset
- `readable()`/`writable()` 返回权限

#### Scenario: 读取更新偏移

- **WHEN** `offset = 0`，读取 100 字节
- **THEN** `offset` MUST 变为 100

### Requirement: UserBuffer

`UserBuffer` MUST 封装 `Vec<&'static mut [u8]>`，表示分散的用户缓冲区。

- `len()` MUST 返回所有切片长度之和

#### Scenario: 长度计算

- **WHEN** 包含 100 + 200 字节的两个切片
- **THEN** `len()` MUST 返回 300

### Requirement: OpenFlags

`OpenFlags` MUST 定义：`RDONLY(0)`、`WRONLY(1)`、`RDWR(2)`、`CREATE(512)`、`TRUNC(1024)`。

- `read_write()` 返回 `(readable, writable)`

#### Scenario: 权限解析

- **WHEN** `OpenFlags::WRONLY`
- **THEN** `read_write()` MUST 返回 `(false, true)`

### Requirement: FSManager trait

`FSManager` MUST 定义（由内核实现）：
- `open(path, flags) -> Option<Arc<FileHandle>>`
- `find(path) -> Option<Arc<Inode>>`
- `link(src, dst) -> isize`
- `unlink(path) -> isize`
- `readdir(path) -> Option<Vec<String>>`

#### Scenario: 内核实现

- **WHEN** 内核实现 FSManager
- **THEN** MUST 提供路径解析逻辑

## Public API

- `Inode`: `new`, `find`, `create`, `readdir`, `read_at`, `write_at`, `clear`
- `FileHandle`: `new`, `empty`, `readable`, `writable`, `read`, `write`
- `UserBuffer`: `new`, `len`
- `OpenFlags`: `RDONLY`, `WRONLY`, `RDWR`, `CREATE`, `TRUNC`, `read_write`
- `trait FSManager`

## Dependencies

- `block-device`, `block-cache`, `disk-layout`, `fs-manager`
- `spin`: `Mutex`, `MutexGuard`
- `bitflags`
- `alloc`: `Arc`, `Vec`, `String`
