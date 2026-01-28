//! VFS 索引节点层
//!
//! easy-fs 的最顶层，封装 DiskInode 操作，为内核提供 Inode、FileHandle、FSManager 等
//! 高层文件/目录操作接口。

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use crate::block_cache::{block_cache_sync_all, get_block_cache};
use crate::block_dev::BlockDevice;
use crate::efs::EasyFileSystem;
use crate::layout::{DirEntry, DiskInode, DiskInodeType, DIRENT_SZ};

/// 索引节点
///
/// 封装 DiskInode 操作，提供文件/目录的高层接口。
pub struct Inode {
    /// DiskInode 所在的块号
    block_id: usize,
    /// DiskInode 在块内的偏移
    block_offset: usize,
    /// 文件系统引用
    fs: Arc<Mutex<EasyFileSystem>>,
    /// 块设备引用
    block_device: Arc<dyn BlockDevice>,
}

impl Inode {
    /// 创建新的 Inode
    ///
    /// # Arguments
    ///
    /// * `block_id` - DiskInode 所在的块号
    /// * `block_offset` - DiskInode 在块内的偏移
    /// * `fs` - 文件系统引用
    /// * `block_device` - 块设备引用
    pub fn new(
        block_id: u32,
        block_offset: usize,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        Self {
            block_id: block_id as usize,
            block_offset,
            fs,
            block_device,
        }
    }

    /// 以只读方式访问 DiskInode
    fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .read(self.block_offset, f)
    }

    /// 以可变方式访问 DiskInode
    fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .modify(self.block_offset, f)
    }

    /// 在目录中查找指定名称的条目
    ///
    /// # Arguments
    ///
    /// * `name` - 要查找的文件名
    ///
    /// # Returns
    ///
    /// 如果找到，返回 `Some(Arc<Inode>)`；否则返回 `None`。
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            self.find_inode_id(name, disk_inode).map(|inode_id| {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                Arc::new(Self::new(
                    block_id,
                    block_offset,
                    Arc::clone(&self.fs),
                    Arc::clone(&self.block_device),
                ))
            })
        })
    }

    /// 在 DiskInode 中查找目录项，返回 inode_id
    fn find_inode_id(&self, name: &str, disk_inode: &DiskInode) -> Option<u32> {
        assert!(disk_inode.is_dir());
        let file_count = disk_inode.size as usize / DIRENT_SZ;
        let mut dirent = DirEntry::empty();
        for i in 0..file_count {
            assert_eq!(
                disk_inode.read_at(
                    i * DIRENT_SZ,
                    dirent.as_bytes_mut(),
                    &self.block_device,
                ),
                DIRENT_SZ,
            );
            if dirent.name() == name {
                return Some(dirent.inode_number());
            }
        }
        None
    }

    /// 在当前目录下创建文件
    ///
    /// # Arguments
    ///
    /// * `name` - 要创建的文件名
    ///
    /// # Returns
    ///
    /// 如果创建成功，返回 `Some(Arc<Inode>)`；如果文件已存在，返回 `None`。
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {
        let mut fs = self.fs.lock();
        // 检查文件是否已存在
        let op = self.read_disk_inode(|disk_inode| {
            assert!(disk_inode.is_dir());
            self.find_inode_id(name, disk_inode)
        });
        if op.is_some() {
            return None;
        }
        // 分配新 inode
        let new_inode_id = fs.alloc_inode();
        // 初始化为 File
        let (new_inode_block_id, new_inode_block_offset) = fs.get_disk_inode_pos(new_inode_id);
        get_block_cache(new_inode_block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(new_inode_block_offset, |new_inode: &mut DiskInode| {
                new_inode.initialize(DiskInodeType::File);
            });
        // 追加目录项
        self.modify_disk_inode(|root_inode| {
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            // 扩容
            self.increase_size(new_size as u32, root_inode, &mut fs);
            // 写入目录项
            let dirent = DirEntry::new(name, new_inode_id);
            root_inode.write_at(file_count * DIRENT_SZ, dirent.as_bytes(), &self.block_device);
        });
        // 同步缓存
        block_cache_sync_all();
        // 返回新创建的 Inode
        Some(Arc::new(Self::new(
            new_inode_block_id,
            new_inode_block_offset,
            Arc::clone(&self.fs),
            Arc::clone(&self.block_device),
        )))
    }

    /// 扩容 DiskInode
    fn increase_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut EasyFileSystem,
    ) {
        if new_size <= disk_inode.size {
            return;
        }
        let blocks_needed = disk_inode.blocks_num_needed(new_size);
        let mut v: Vec<u32> = Vec::new();
        for _ in 0..blocks_needed {
            v.push(fs.alloc_data());
        }
        disk_inode.increase_size(new_size, v, &self.block_device);
    }

    /// 返回目录下所有条目名称
    ///
    /// # Returns
    ///
    /// 目录中所有文件/子目录名称的列表，顺序与创建顺序一致。
    pub fn readdir(&self) -> Vec<String> {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            let file_count = disk_inode.size as usize / DIRENT_SZ;
            let mut v: Vec<String> = Vec::new();
            for i in 0..file_count {
                let mut dirent = DirEntry::empty();
                assert_eq!(
                    disk_inode.read_at(
                        i * DIRENT_SZ,
                        dirent.as_bytes_mut(),
                        &self.block_device,
                    ),
                    DIRENT_SZ,
                );
                v.push(String::from(dirent.name()));
            }
            v
        })
    }

    /// 从文件指定偏移读取数据
    ///
    /// # Arguments
    ///
    /// * `offset` - 起始偏移
    /// * `buf` - 目标缓冲区
    ///
    /// # Returns
    ///
    /// 实际读取的字节数。
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }

    /// 向文件指定偏移写入数据
    ///
    /// 自动扩容文件大小以容纳写入数据。
    ///
    /// # Arguments
    ///
    /// * `offset` - 起始偏移
    /// * `buf` - 源缓冲区
    ///
    /// # Returns
    ///
    /// 实际写入的字节数。
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        let size = self.modify_disk_inode(|disk_inode| {
            // 自动扩容
            let new_size = (offset + buf.len()) as u32;
            self.increase_size(new_size, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        });
        block_cache_sync_all();
        size
    }

    /// 清空文件内容
    ///
    /// 回收所有数据块，将文件大小设为 0。
    pub fn clear(&self) {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            let data_blocks_dealloc = disk_inode.clear_size(&self.block_device);
            for data_block in data_blocks_dealloc {
                fs.dealloc_data(data_block);
            }
        });
        block_cache_sync_all();
    }
}

/// 用户缓冲区
///
/// 封装分散的用户空间缓冲区切片。
pub struct UserBuffer {
    /// 缓冲区切片列表
    buffers: Vec<&'static mut [u8]>,
}

impl UserBuffer {
    /// 创建新的 UserBuffer
    pub fn new(buffers: Vec<&'static mut [u8]>) -> Self {
        Self { buffers }
    }

    /// 返回所有切片长度之和
    pub fn len(&self) -> usize {
        self.buffers.iter().map(|b| b.len()).sum()
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl IntoIterator for UserBuffer {
    type Item = *mut u8;
    type IntoIter = UserBufferIterator;

    fn into_iter(self) -> Self::IntoIter {
        UserBufferIterator {
            buffers: self.buffers,
            current: 0,
            inner: 0,
        }
    }
}

/// UserBuffer 迭代器
pub struct UserBufferIterator {
    buffers: Vec<&'static mut [u8]>,
    current: usize,
    inner: usize,
}

impl Iterator for UserBufferIterator {
    type Item = *mut u8;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current >= self.buffers.len() {
            return None;
        }
        let r = &mut self.buffers[self.current][self.inner] as *mut _;
        self.inner += 1;
        if self.inner >= self.buffers[self.current].len() {
            self.current += 1;
            self.inner = 0;
        }
        Some(r)
    }
}

bitflags::bitflags! {
    /// 文件打开标志
    pub struct OpenFlags: u32 {
        /// 只读
        const RDONLY = 0;
        /// 只写
        const WRONLY = 1 << 0;
        /// 读写
        const RDWR = 1 << 1;
        /// 创建
        const CREATE = 1 << 9;
        /// 截断
        const TRUNC = 1 << 10;
    }
}

impl OpenFlags {
    /// 根据标志返回读写权限
    ///
    /// # Returns
    ///
    /// `(readable, writable)` 元组。
    pub fn read_write(&self) -> (bool, bool) {
        if self.is_empty() {
            (true, false)
        } else if self.contains(Self::WRONLY) {
            (false, true)
        } else {
            (true, true)
        }
    }
}

/// 文件句柄
///
/// 包含 Inode 引用、权限和当前偏移。
pub struct FileHandle {
    /// 底层 Inode
    pub inode: Option<Arc<Inode>>,
    /// 可读
    readable: bool,
    /// 可写
    writable: bool,
    /// 当前偏移
    pub offset: usize,
}

impl FileHandle {
    /// 创建新的文件句柄
    ///
    /// # Arguments
    ///
    /// * `readable` - 是否可读
    /// * `writable` - 是否可写
    /// * `inode` - Inode 引用
    pub fn new(readable: bool, writable: bool, inode: Arc<Inode>) -> Self {
        Self {
            inode: Some(inode),
            readable,
            writable,
            offset: 0,
        }
    }

    /// 创建空的文件句柄
    ///
    /// # Arguments
    ///
    /// * `readable` - 是否可读
    /// * `writable` - 是否可写
    pub fn empty(readable: bool, writable: bool) -> Self {
        Self {
            inode: None,
            readable,
            writable,
            offset: 0,
        }
    }

    /// 是否可读
    pub fn readable(&self) -> bool {
        self.readable
    }

    /// 是否可写
    pub fn writable(&self) -> bool {
        self.writable
    }

    /// 从当前偏移读取数据到 UserBuffer
    ///
    /// 读取后更新偏移。
    ///
    /// # Arguments
    ///
    /// * `buf` - 用户缓冲区
    ///
    /// # Returns
    ///
    /// 实际读取的字节数。
    pub fn read(&mut self, buf: UserBuffer) -> usize {
        let mut total_read_size = 0usize;
        if let Some(inode) = &self.inode {
            for slice in buf.buffers.iter() {
                let len = slice.len();
                // 需要使用 unsafe 来获取可变引用进行写入
                let slice_ptr = slice.as_ptr() as *mut u8;
                let slice_mut = unsafe { core::slice::from_raw_parts_mut(slice_ptr, len) };
                let read_size = inode.read_at(self.offset, slice_mut);
                if read_size == 0 {
                    break;
                }
                self.offset += read_size;
                total_read_size += read_size;
            }
        }
        total_read_size
    }

    /// 从 UserBuffer 写入数据到当前偏移
    ///
    /// 写入后更新偏移。
    ///
    /// # Arguments
    ///
    /// * `buf` - 用户缓冲区
    ///
    /// # Returns
    ///
    /// 实际写入的字节数。
    pub fn write(&mut self, buf: UserBuffer) -> usize {
        let mut total_write_size = 0usize;
        if let Some(inode) = &self.inode {
            for slice in buf.buffers.iter() {
                let write_size = inode.write_at(self.offset, slice);
                assert_eq!(write_size, slice.len());
                self.offset += write_size;
                total_write_size += write_size;
            }
        }
        total_write_size
    }
}

/// 文件系统管理器 trait
///
/// 由内核实现，提供路径解析和文件操作接口。
pub trait FSManager: Send + Sync {
    /// 打开文件
    ///
    /// # Arguments
    ///
    /// * `path` - 文件路径
    /// * `flags` - 打开标志
    ///
    /// # Returns
    ///
    /// 如果成功，返回 `Some(Arc<FileHandle>)`；否则返回 `None`。
    fn open(&self, path: &str, flags: OpenFlags) -> Option<Arc<FileHandle>>;

    /// 查找文件
    ///
    /// # Arguments
    ///
    /// * `path` - 文件路径
    ///
    /// # Returns
    ///
    /// 如果找到，返回 `Some(Arc<Inode>)`；否则返回 `None`。
    fn find(&self, path: &str) -> Option<Arc<Inode>>;

    /// 创建硬链接
    ///
    /// # Arguments
    ///
    /// * `src` - 源路径
    /// * `dst` - 目标路径
    ///
    /// # Returns
    ///
    /// 成功返回 0，失败返回 -1。
    fn link(&self, src: &str, dst: &str) -> isize;

    /// 删除文件
    ///
    /// # Arguments
    ///
    /// * `path` - 文件路径
    ///
    /// # Returns
    ///
    /// 成功返回 0，失败返回 -1。
    fn unlink(&self, path: &str) -> isize;

    /// 读取目录
    ///
    /// # Arguments
    ///
    /// * `path` - 目录路径
    ///
    /// # Returns
    ///
    /// 如果成功，返回 `Some(Vec<String>)`；否则返回 `None`。
    fn readdir(&self, path: &str) -> Option<Vec<String>>;
}
