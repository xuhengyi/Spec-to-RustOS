use alloc::sync::Arc;
use alloc::vec::Vec;
use core::mem::size_of;

use crate::block_cache::get_block_cache;
use crate::block_dev::{BlockDevice, BLOCK_SZ};

/// easy-fs 魔数
pub const EFS_MAGIC: u32 = 0x3b800001;
/// 直接索引数量
pub const INODE_DIRECT_COUNT: usize = 28;
/// 文件名最大长度（不含终止符）
pub const NAME_LENGTH_LIMIT: usize = 27;
/// 目录项大小（字节）
pub const DIRENT_SZ: usize = 32;

/// 每块可存储的间接索引数量 (512 / 4 = 128)
const INODE_INDIRECT1_COUNT: usize = BLOCK_SZ / size_of::<u32>();
/// 直接索引边界
const DIRECT_BOUND: usize = INODE_DIRECT_COUNT;
/// 一级间接索引边界 (28 + 128 = 156)
const INDIRECT1_BOUND: usize = DIRECT_BOUND + INODE_INDIRECT1_COUNT;
/// 每块的 bit 数量 (512 * 8 = 4096)
const BLOCK_BITS: usize = BLOCK_SZ * 8;

/// 超级块
/// 
/// 存储文件系统元数据，位于磁盘的第 0 块。
#[repr(C)]
pub struct SuperBlock {
    /// 魔数，用于标识 easy-fs 文件系统
    magic: u32,
    /// 总块数
    pub total_blocks: u32,
    /// inode 位图块数
    pub inode_bitmap_blocks: u32,
    /// inode 区域块数
    pub inode_area_blocks: u32,
    /// 数据位图块数
    pub data_bitmap_blocks: u32,
    /// 数据区域块数
    pub data_area_blocks: u32,
}

impl SuperBlock {
    /// 初始化超级块
    pub fn initialize(
        &mut self,
        total_blocks: u32,
        inode_bitmap_blocks: u32,
        inode_area_blocks: u32,
        data_bitmap_blocks: u32,
        data_area_blocks: u32,
    ) {
        *self = Self {
            magic: EFS_MAGIC,
            total_blocks,
            inode_bitmap_blocks,
            inode_area_blocks,
            data_bitmap_blocks,
            data_area_blocks,
        };
    }

    /// 验证魔数是否正确
    pub fn is_valid(&self) -> bool {
        self.magic == EFS_MAGIC
    }
}

/// 位图
/// 
/// 管理 inode 或数据块的分配状态。
pub struct Bitmap {
    /// 起始块号
    start_block_id: usize,
    /// 块数
    blocks: usize,
}

impl Bitmap {
    /// 创建新的位图
    pub fn new(start_block_id: usize, blocks: usize) -> Self {
        Self {
            start_block_id,
            blocks,
        }
    }

    /// 分配一个空闲 bit
    /// 
    /// 找到第一个为 0 的 bit，将其置 1 并返回编号。
    /// 如果没有空闲 bit，返回 None。
    pub fn alloc(&self, block_device: &Arc<dyn BlockDevice>) -> Option<usize> {
        for block_id in 0..self.blocks {
            let pos = get_block_cache(
                self.start_block_id + block_id,
                Arc::clone(block_device),
            )
            .lock()
            .modify(0, |bitmap_block: &mut BitmapBlock| {
                if let Some((bits64_pos, inner_pos)) = bitmap_block
                    .iter()
                    .enumerate()
                    .find(|(_, bits64)| **bits64 != u64::MAX)
                    .map(|(bits64_pos, bits64)| (bits64_pos, bits64.trailing_ones() as usize))
                {
                    // 设置该 bit
                    bitmap_block[bits64_pos] |= 1u64 << inner_pos;
                    Some(block_id * BLOCK_BITS + bits64_pos * 64 + inner_pos)
                } else {
                    None
                }
            });
            if pos.is_some() {
                return pos;
            }
        }
        None
    }

    /// 释放指定 bit
    /// 
    /// 将指定 bit 清零。
    pub fn dealloc(&self, block_device: &Arc<dyn BlockDevice>, bit: usize) {
        let block_id = bit / BLOCK_BITS;
        let bits64_pos = (bit % BLOCK_BITS) / 64;
        let inner_pos = bit % 64;
        get_block_cache(
            self.start_block_id + block_id,
            Arc::clone(block_device),
        )
        .lock()
        .modify(0, |bitmap_block: &mut BitmapBlock| {
            assert!(bitmap_block[bits64_pos] & (1u64 << inner_pos) > 0);
            bitmap_block[bits64_pos] -= 1u64 << inner_pos;
        });
    }

    /// 返回位图能管理的最大 bit 数量
    pub fn maximum(&self) -> usize {
        self.blocks * BLOCK_BITS
    }
}

/// 位图块类型（每块 64 个 u64）
type BitmapBlock = [u64; 64];

/// 索引节点类型
#[derive(PartialEq, Clone, Copy)]
pub enum DiskInodeType {
    File,
    Directory,
}

/// 磁盘索引节点
/// 
/// 存储文件或目录的元数据和块索引，大小为 128 字节。
#[repr(C)]
pub struct DiskInode {
    /// 文件大小（字节）
    pub size: u32,
    /// 直接索引
    pub direct: [u32; INODE_DIRECT_COUNT],
    /// 一级间接索引块号
    pub indirect1: u32,
    /// 二级间接索引块号
    pub indirect2: u32,
    /// 类型（文件/目录）
    type_: DiskInodeType,
}

impl DiskInode {
    /// 初始化索引节点
    pub fn initialize(&mut self, type_: DiskInodeType) {
        self.size = 0;
        self.direct = [0u32; INODE_DIRECT_COUNT];
        self.indirect1 = 0;
        self.indirect2 = 0;
        self.type_ = type_;
    }

    /// 是否是目录
    pub fn is_dir(&self) -> bool {
        self.type_ == DiskInodeType::Directory
    }

    /// 是否是文件
    pub fn is_file(&self) -> bool {
        self.type_ == DiskInodeType::File
    }

    /// 计算存储当前大小需要的数据块数量
    pub fn data_blocks(&self) -> u32 {
        Self::_data_blocks(self.size)
    }

    fn _data_blocks(size: u32) -> u32 {
        (size + BLOCK_SZ as u32 - 1) / BLOCK_SZ as u32
    }

    /// 计算存储当前大小需要的总块数（含间接索引块）
    pub fn total_blocks(size: u32) -> u32 {
        let data_blocks = Self::_data_blocks(size) as usize;
        let mut total = data_blocks;
        // 需要一级间接索引块
        if data_blocks > DIRECT_BOUND {
            total += 1;
        }
        // 需要二级间接索引块
        if data_blocks > INDIRECT1_BOUND {
            // 二级间接索引块本身 + 其下属的一级间接块
            let indirect2_data = data_blocks - INDIRECT1_BOUND;
            let indirect1_blocks = (indirect2_data + INODE_INDIRECT1_COUNT - 1) / INODE_INDIRECT1_COUNT;
            total += 1 + indirect1_blocks;
        }
        total as u32
    }

    /// 计算从当前大小扩容到新大小需要的新块数量
    pub fn blocks_num_needed(&self, new_size: u32) -> u32 {
        assert!(new_size >= self.size);
        Self::total_blocks(new_size) - Self::total_blocks(self.size)
    }

    /// 获取文件内部块号对应的磁盘块号
    pub fn get_block_id(&self, inner_id: u32, block_device: &Arc<dyn BlockDevice>) -> u32 {
        let inner_id = inner_id as usize;
        if inner_id < DIRECT_BOUND {
            self.direct[inner_id]
        } else if inner_id < INDIRECT1_BOUND {
            get_block_cache(self.indirect1 as usize, Arc::clone(block_device))
                .lock()
                .read(0, |indirect_block: &IndirectBlock| {
                    indirect_block[inner_id - DIRECT_BOUND]
                })
        } else {
            let inner_id = inner_id - INDIRECT1_BOUND;
            let indirect1_id = inner_id / INODE_INDIRECT1_COUNT;
            let indirect2_id = inner_id % INODE_INDIRECT1_COUNT;
            let indirect1 = get_block_cache(self.indirect2 as usize, Arc::clone(block_device))
                .lock()
                .read(0, |indirect2: &IndirectBlock| indirect2[indirect1_id]);
            get_block_cache(indirect1 as usize, Arc::clone(block_device))
                .lock()
                .read(0, |indirect1: &IndirectBlock| indirect1[indirect2_id])
        }
    }

    /// 扩容文件大小
    /// 
    /// 将文件大小扩展到 new_size，使用 new_blocks 提供的新块。
    pub fn increase_size(
        &mut self,
        new_size: u32,
        new_blocks: Vec<u32>,
        block_device: &Arc<dyn BlockDevice>,
    ) {
        let mut current_blocks = self.data_blocks() as usize;
        self.size = new_size;
        let mut total_blocks = self.data_blocks() as usize;
        let mut new_blocks = new_blocks.into_iter();

        // 填充直接索引
        while current_blocks < total_blocks.min(DIRECT_BOUND) {
            self.direct[current_blocks] = new_blocks.next().unwrap();
            current_blocks += 1;
        }

        // 需要一级间接索引
        if total_blocks > DIRECT_BOUND {
            if current_blocks == DIRECT_BOUND {
                // 分配一级间接索引块
                self.indirect1 = new_blocks.next().unwrap();
            }
            current_blocks -= DIRECT_BOUND;
            total_blocks -= DIRECT_BOUND;
        } else {
            return;
        }

        // 填充一级间接索引
        get_block_cache(self.indirect1 as usize, Arc::clone(block_device))
            .lock()
            .modify(0, |indirect1: &mut IndirectBlock| {
                while current_blocks < total_blocks.min(INODE_INDIRECT1_COUNT) {
                    indirect1[current_blocks] = new_blocks.next().unwrap();
                    current_blocks += 1;
                }
            });

        // 需要二级间接索引
        if total_blocks > INODE_INDIRECT1_COUNT {
            if current_blocks == INODE_INDIRECT1_COUNT {
                // 分配二级间接索引块
                self.indirect2 = new_blocks.next().unwrap();
            }
            current_blocks -= INODE_INDIRECT1_COUNT;
            total_blocks -= INODE_INDIRECT1_COUNT;
        } else {
            return;
        }

        // 填充二级间接索引
        let mut a0 = current_blocks / INODE_INDIRECT1_COUNT;
        let mut b0 = current_blocks % INODE_INDIRECT1_COUNT;
        let a1 = total_blocks / INODE_INDIRECT1_COUNT;
        let b1 = total_blocks % INODE_INDIRECT1_COUNT;
        
        // 分配新的一级间接索引块
        get_block_cache(self.indirect2 as usize, Arc::clone(block_device))
            .lock()
            .modify(0, |indirect2: &mut IndirectBlock| {
                while (a0 < a1) || (a0 == a1 && b0 < b1) {
                    if b0 == 0 {
                        indirect2[a0] = new_blocks.next().unwrap();
                    }
                    // 填充该一级间接块
                    get_block_cache(indirect2[a0] as usize, Arc::clone(block_device))
                        .lock()
                        .modify(0, |indirect1: &mut IndirectBlock| {
                            indirect1[b0] = new_blocks.next().unwrap();
                        });
                    b0 += 1;
                    if b0 == INODE_INDIRECT1_COUNT {
                        b0 = 0;
                        a0 += 1;
                    }
                }
            });
    }

    /// 清空文件，返回待回收的块编号列表
    pub fn clear_size(&mut self, block_device: &Arc<dyn BlockDevice>) -> Vec<u32> {
        let mut v: Vec<u32> = Vec::new();
        let mut data_blocks = self.data_blocks() as usize;
        self.size = 0;
        let mut current_blocks = 0usize;

        // 回收直接索引块
        while current_blocks < data_blocks.min(DIRECT_BOUND) {
            v.push(self.direct[current_blocks]);
            self.direct[current_blocks] = 0;
            current_blocks += 1;
        }

        // 回收一级间接索引块
        if data_blocks > DIRECT_BOUND {
            v.push(self.indirect1);
            data_blocks -= DIRECT_BOUND;
            current_blocks = 0;
        } else {
            return v;
        }

        get_block_cache(self.indirect1 as usize, Arc::clone(block_device))
            .lock()
            .modify(0, |indirect1: &mut IndirectBlock| {
                while current_blocks < data_blocks.min(INODE_INDIRECT1_COUNT) {
                    v.push(indirect1[current_blocks]);
                    current_blocks += 1;
                }
            });
        self.indirect1 = 0;

        // 回收二级间接索引块
        if data_blocks > INODE_INDIRECT1_COUNT {
            v.push(self.indirect2);
            data_blocks -= INODE_INDIRECT1_COUNT;
        } else {
            return v;
        }

        let a1 = data_blocks / INODE_INDIRECT1_COUNT;
        let b1 = data_blocks % INODE_INDIRECT1_COUNT;
        get_block_cache(self.indirect2 as usize, Arc::clone(block_device))
            .lock()
            .modify(0, |indirect2: &mut IndirectBlock| {
                for entry in indirect2.iter_mut().take(a1) {
                    v.push(*entry);
                    get_block_cache(*entry as usize, Arc::clone(block_device))
                        .lock()
                        .modify(0, |indirect1: &mut IndirectBlock| {
                            for entry in indirect1.iter() {
                                v.push(*entry);
                            }
                        });
                }
                // 最后一个一级间接块（可能未满）
                if b1 > 0 {
                    v.push(indirect2[a1]);
                    get_block_cache(indirect2[a1] as usize, Arc::clone(block_device))
                        .lock()
                        .modify(0, |indirect1: &mut IndirectBlock| {
                            for entry in indirect1.iter().take(b1) {
                                v.push(*entry);
                            }
                        });
                }
            });
        self.indirect2 = 0;
        v
    }

    /// 从文件指定偏移读取数据
    /// 
    /// 返回实际读取的字节数。
    pub fn read_at(
        &self,
        offset: usize,
        buf: &mut [u8],
        block_device: &Arc<dyn BlockDevice>,
    ) -> usize {
        let mut start = offset;
        let end = (offset + buf.len()).min(self.size as usize);
        if start >= end {
            return 0;
        }
        let mut start_block = start / BLOCK_SZ;
        let mut read_size = 0usize;
        loop {
            // 计算当前块的读取范围
            let end_current_block = ((start / BLOCK_SZ) + 1) * BLOCK_SZ;
            let block_read_size = end.min(end_current_block) - start;
            let dst = &mut buf[read_size..read_size + block_read_size];
            get_block_cache(
                self.get_block_id(start_block as u32, block_device) as usize,
                Arc::clone(block_device),
            )
            .lock()
            .read(0, |data_block: &DataBlock| {
                let src = &data_block[start % BLOCK_SZ..start % BLOCK_SZ + block_read_size];
                dst.copy_from_slice(src);
            });
            read_size += block_read_size;
            if end <= end_current_block {
                break;
            }
            start_block += 1;
            start = end_current_block;
        }
        read_size
    }

    /// 向文件指定偏移写入数据
    /// 
    /// 返回实际写入的字节数。调用方需确保 size 足够。
    pub fn write_at(
        &mut self,
        offset: usize,
        buf: &[u8],
        block_device: &Arc<dyn BlockDevice>,
    ) -> usize {
        let mut start = offset;
        let end = (offset + buf.len()).min(self.size as usize);
        assert!(start <= end);
        let mut start_block = start / BLOCK_SZ;
        let mut write_size = 0usize;
        loop {
            // 计算当前块的写入范围
            let end_current_block = ((start / BLOCK_SZ) + 1) * BLOCK_SZ;
            let block_write_size = end.min(end_current_block) - start;
            get_block_cache(
                self.get_block_id(start_block as u32, block_device) as usize,
                Arc::clone(block_device),
            )
            .lock()
            .modify(0, |data_block: &mut DataBlock| {
                let src = &buf[write_size..write_size + block_write_size];
                let dst = &mut data_block[start % BLOCK_SZ..start % BLOCK_SZ + block_write_size];
                dst.copy_from_slice(src);
            });
            write_size += block_write_size;
            if end <= end_current_block {
                break;
            }
            start_block += 1;
            start = end_current_block;
        }
        write_size
    }
}

/// 间接索引块类型
type IndirectBlock = [u32; BLOCK_SZ / size_of::<u32>()];
/// 数据块类型
type DataBlock = [u8; BLOCK_SZ];

/// 目录项
/// 
/// 存储目录中的文件名和对应的 inode 编号。
#[repr(C)]
pub struct DirEntry {
    /// 文件名（含终止符）
    name: [u8; NAME_LENGTH_LIMIT + 1],
    /// inode 编号
    inode_number: u32,
}

impl DirEntry {
    /// 创建空目录项
    pub fn empty() -> Self {
        Self {
            name: [0u8; NAME_LENGTH_LIMIT + 1],
            inode_number: 0,
        }
    }

    /// 创建新目录项
    pub fn new(name: &str, inode_number: u32) -> Self {
        let mut bytes = [0u8; NAME_LENGTH_LIMIT + 1];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        Self {
            name: bytes,
            inode_number,
        }
    }

    /// 获取目录项的字节切片表示
    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                self as *const _ as *const u8,
                DIRENT_SZ,
            )
        }
    }

    /// 获取目录项的可变字节切片表示
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self as *mut _ as *mut u8,
                DIRENT_SZ,
            )
        }
    }

    /// 获取文件名
    pub fn name(&self) -> &str {
        let len = self.name.iter().position(|&c| c == 0).unwrap_or(NAME_LENGTH_LIMIT);
        core::str::from_utf8(&self.name[..len]).unwrap()
    }

    /// 获取 inode 编号
    pub fn inode_number(&self) -> u32 {
        self.inode_number
    }
}
