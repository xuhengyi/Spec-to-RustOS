# Capability: easy-fs

本规格描述 crate `easy-fs` 的对外契约与边界：其块设备抽象、块缓存写回策略、磁盘布局（superblock/bitmap/inode/data area），以及通过 `Inode` 提供的最小文件/目录访问能力。

## Purpose

为 `easy-fs` 定义可验证的对外语义（创建/打开文件系统、根目录、文件创建、目录枚举、按偏移读写、清空文件），并将对底层块设备与调用约束收敛为 Preconditions。

## Requirements

### Requirement: 块大小与块设备读写
`easy-fs` MUST 以 512 字节为块大小（`BLOCK_SZ == 512`）。

实现 `BlockDevice` 的类型 MUST 以“整块”为单位提供读写：
- `read_block(block_id, buf)` MUST 将编号为 `block_id` 的块内容写入 `buf`
- `write_block(block_id, buf)` MUST 将 `buf` 的内容写入编号为 `block_id` 的块

#### Scenario: 读写一个块
- **WHEN** 调用方对同一 `block_id` 依次调用 `write_block` 写入 512 字节，再调用 `read_block` 读取
- **THEN** 读取到的 512 字节 MUST 与写入内容一致

### Requirement: 创建文件系统并初始化磁盘布局
`EasyFileSystem::create(block_device, total_blocks, inode_bitmap_blocks)` MUST 在 `block_device` 上创建一个新的 easy-fs 文件系统，并完成以下行为：
- MUST 将 `[0, total_blocks)` 范围内的所有块内容清零
- MUST 在块 0 写入 `SuperBlock`，并使其 `is_valid()` 为真
- MUST 分配 inode 0 作为根目录 inode（类型为 Directory）
- MUST 在返回前将上述元数据写回底层块设备（同步所有相关块缓存）

#### Scenario: create 后可打开
- **WHEN** 调用方先调用 `EasyFileSystem::create(...)`，随后对同一 `block_device` 调用 `EasyFileSystem::open(...)`
- **THEN** `open` MUST 通过 superblock 校验并成功返回文件系统句柄

### Requirement: 打开文件系统必须校验 superblock
`EasyFileSystem::open(block_device)` MUST 从块 0 读取 `SuperBlock` 并校验 magic。
- **IF** superblock 校验失败
  - **THEN** 当前实现 MUST 触发断言失败（panic），并拒绝继续使用该块设备作为 easy-fs

#### Scenario: 非 easy-fs 设备被拒绝
- **WHEN** `block_device` 的块 0 不是合法 easy-fs superblock
- **THEN** 调用 `EasyFileSystem::open` MUST panic

### Requirement: 根目录 inode
`EasyFileSystem::root_inode(efs)` MUST 返回 inode id 为 0 的 `Inode` 句柄，并与 `efs` 共享同一个底层 `block_device`。

#### Scenario: root_inode 可枚举目录
- **WHEN** 调用方从 `efs` 获取 `root_inode`
- **THEN** 调用 `root_inode.readdir()` MUST 返回一个可用的目录项列表（允许为空）

### Requirement: 目录项与文件名约束
目录项 MUST 以固定大小 `DIRENT_SZ == 32` 序列化。

对 `Inode::find(name)` / `Inode::create(name)` / `Inode::readdir()`：
- 调用方 MUST 仅在“当前 inode 表示目录”时使用它们（当前实现以断言/约定保证）
- `name` MUST 为 UTF-8 字符串
- `name.len()` MUST 满足 `0 < name.len() <= 27`（超出将导致当前实现 panic 或产生不可解析条目）

#### Scenario: 过长文件名被拒绝
- **WHEN** 调用方传入 `name.len() > 27`
- **THEN** 当前实现对 `Inode::create(name)` MUST panic（由边界检查触发）

### Requirement: 在目录下查找文件
`Inode::find(name)` MUST 在当前目录 inode 的目录项中按顺序查找名称等于 `name` 的条目。
- **IF** 找到
  - **THEN** MUST 返回对应的 `Arc<Inode>`
- **ELSE**
  - **THEN** MUST 返回 `None`

#### Scenario: find 找到已创建文件
- **WHEN** 调用方在某目录下成功 `create("a")`
- **THEN** 随后调用 `find("a")` MUST 返回 `Some(...)`

### Requirement: 在目录下创建文件
`Inode::create(name)` MUST 在当前目录下创建一个新的普通文件（DiskInodeType::File）并写入目录项。

该接口的前置条件为：调用方 SHOULD 先用 `find(name)` 确认不存在同名条目（当前实现不会阻止重名）。

成功创建后：
- MUST 分配一个新的 inode id
- MUST 初始化新 inode 为 File
- MUST 将 `(name, inode_id)` 追加到当前目录的目录项列表末尾
- MUST 在返回前同步写回相关块缓存

#### Scenario: create 后 readdir 可见
- **WHEN** 调用方对目录调用 `create("a")` 并成功返回
- **THEN** `readdir()` 的返回列表 MUST 包含 `"a"`

### Requirement: 枚举目录
`Inode::readdir()` MUST 读取当前目录 inode 中的所有目录项，并按目录项顺序返回名称列表。

#### Scenario: readdir 返回顺序与插入一致
- **WHEN** 调用方依次创建 `a`、`b`
- **THEN** `readdir()` MUST 以 `a` 在前、`b` 在后的顺序返回

### Requirement: 按偏移读取文件
`Inode::read_at(offset, buf)` MUST 从当前 inode 的数据区读取数据到 `buf`，其行为为：
- 实际读取范围 MUST 被截断在文件大小以内
- **IF** `offset >= file_size`
  - **THEN** MUST 返回 0
- **ELSE**
  - **THEN** MUST 返回实际读取的字节数（`<= buf.len()`）

#### Scenario: 读取越界返回 0
- **WHEN** 文件大小为 N，且调用 `read_at(N, buf)`
- **THEN** MUST 返回 0

### Requirement: 按偏移写入文件（必要时扩容）
`Inode::write_at(offset, buf)` MUST 将 `buf` 写入到当前 inode 的数据区，并在需要时扩容文件：
- MUST 将文件大小至少扩展到 `offset + buf.len()`
- MUST 通过数据位图分配所需的新数据块（以及必要的间接块）
- MUST 在返回前同步写回所有块缓存

#### Scenario: 写入触发扩容
- **WHEN** 对空文件调用 `write_at(0, buf)` 且 `buf.len() > 0`
- **THEN** 随后调用 `read_at(0, out)` MUST 读回与 `buf` 一致的数据

### Requirement: 清空文件并回收数据块
`Inode::clear()` MUST 将当前 inode 的大小置零，并回收该 inode 占用的数据块（以及间接块），同时将被回收的数据块内容清零。

清空完成后：
- MUST 同步写回所有相关块缓存

#### Scenario: clear 后文件内容为空
- **WHEN** 调用方先写入任意数据，再调用 `clear()`
- **THEN** 随后 `read_at(0, buf)` MUST 返回 0

### Requirement: 块缓存写回与容量限制（实现约束）
`easy-fs` 的块缓存层 MUST 在以下时机将脏块写回底层设备：
- 当 `Inode::create` / `Inode::write_at` / `Inode::clear` / `EasyFileSystem::create` 完成时（通过同步所有块缓存）
- 当某个 `BlockCache` 被丢弃（Drop）时（仅对已修改块）

当前实现的块缓存容量上限为 16 块。
- **IF** 缓存已满且所有缓存条目仍被外部持有（无法被替换）
  - **THEN** 当前实现 MUST panic

#### Scenario: 缓存耗尽触发 panic
- **WHEN** 系统已缓存 16 个不同 `block_id` 且这些缓存均被外部 `Arc` 持有
- **THEN** 再请求新的 `block_id` 缓存时 MUST panic

## Public API

### Constants
- `BLOCK_SZ: usize`: 块大小常量（固定为 512）

### Traits
- `BlockDevice`: 块设备抽象（`read_block`/`write_block`）
- `FSManager`: 文件系统管理接口（`open/find/link/unlink/readdir`；`easy-fs` 仅定义该 trait，不在本 crate 内提供实现）

### Types
- `EasyFileSystem`: easy-fs 文件系统对象（`create/open/root_inode` 等）
- `Inode`: VFS inode 句柄（`find/create/readdir/read_at/write_at/clear`）
- `UserBuffer`: 用户缓冲区（由多个 `&'static mut [u8]` 组成）
- `OpenFlags`: 打开标志位（`RDONLY/WRONLY/RDWR/CREATE/TRUNC`）
- `FileHandle`: 文件句柄（缓存 inode/读写权限/偏移；提供 `read/write`）

## Build Configuration
- **build.rs**: 无
- **环境变量**: 无
- **生成文件**: 无
- **Feature flags**: 无

## Dependencies

### Workspace crates
- 无

### External crates
- `spin`: `Mutex`/`Lazy`（自旋锁与全局缓存管理器）
- `bitflags`: `OpenFlags` 位标志定义
- `alloc`: `Arc/Vec/String`（`no_std` 下的堆分配支持）
