use alloc::sync::Arc;
use spin::Mutex;

use crate::block_cache::{block_cache_sync_all, get_block_cache};
use crate::block_dev::{BlockDevice, BLOCK_SZ};
use crate::layout::{Bitmap, DiskInode, DiskInodeType, SuperBlock};
use crate::vfs::Inode;

/// 每块的 bit 数量 (512 * 8 = 4096)
const BLOCK_BITS: usize = BLOCK_SZ * 8;

/// 每块可容纳的 inode 数量 (512 / 128 = 4)
const INODES_PER_BLOCK: u32 = (BLOCK_SZ / core::mem::size_of::<DiskInode>()) as u32;

/// easy-fs 文件系统
/// 
/// 整合磁盘布局、管理块分配/回收，提供文件系统创建和打开接口。
pub struct EasyFileSystem {
    /// 块设备引用
    pub block_device: Arc<dyn BlockDevice>,
    /// inode 位图
    pub inode_bitmap: Bitmap,
    /// 数据位图
    pub data_bitmap: Bitmap,
    /// inode 区域起始块号
    pub inode_area_start_block: u32,
    /// 数据区域起始块号
    pub data_area_start_block: u32,
}

impl EasyFileSystem {
    /// 创建文件系统
    /// 
    /// # Arguments
    /// 
    /// * `block_device` - 块设备
    /// * `total_blocks` - 总块数
    /// * `inode_bitmap_blocks` - inode 位图块数
    /// 
    /// # 流程
    /// 
    /// 1. 计算各区域大小
    /// 2. 清零所有块
    /// 3. 初始化 SuperBlock
    /// 4. 分配 inode 0 为根目录（Directory）
    /// 5. 调用 `block_cache_sync_all()`
    /// 6. 返回 `Arc<Mutex<EasyFileSystem>>`
    pub fn create(
        block_device: Arc<dyn BlockDevice>,
        total_blocks: u32,
        inode_bitmap_blocks: u32,
    ) -> Arc<Mutex<Self>> {
        // 计算各区域大小
        // 布局: SuperBlock(1) | inode_bitmap | inode_area | data_bitmap | data_area
        
        // inode 区根据位图容量计算：每个位图块可管理 BLOCK_BITS 个 inode
        // 每块存储 INODES_PER_BLOCK 个 inode
        let inode_area_blocks = 
            inode_bitmap_blocks * BLOCK_BITS as u32 / INODES_PER_BLOCK;
        
        let inode_total_blocks = 1 + inode_bitmap_blocks + inode_area_blocks;
        let data_total_blocks = total_blocks - inode_total_blocks;
        
        // 数据位图块数：需要满足 data_bitmap_blocks * BLOCK_BITS >= data_area_blocks
        // 即 data_bitmap_blocks * (BLOCK_BITS + 1) >= data_total_blocks
        let data_bitmap_blocks = 
            (data_total_blocks + BLOCK_BITS as u32) / (BLOCK_BITS as u32 + 1);
        let data_area_blocks = data_total_blocks - data_bitmap_blocks;
        
        // 清零所有块
        for i in 0..total_blocks {
            get_block_cache(i as usize, Arc::clone(&block_device))
                .lock()
                .modify(0, |data_block: &mut [u8; BLOCK_SZ]| {
                    data_block.fill(0);
                });
        }
        
        // 初始化 SuperBlock
        get_block_cache(0, Arc::clone(&block_device))
            .lock()
            .modify(0, |super_block: &mut SuperBlock| {
                super_block.initialize(
                    total_blocks,
                    inode_bitmap_blocks,
                    inode_area_blocks,
                    data_bitmap_blocks,
                    data_area_blocks,
                );
            });
        
        // 计算起始块号
        let inode_bitmap_start = 1u32;
        let inode_area_start = inode_bitmap_start + inode_bitmap_blocks;
        let data_bitmap_start = inode_area_start + inode_area_blocks;
        let data_area_start = data_bitmap_start + data_bitmap_blocks;
        
        let efs = Self {
            block_device: Arc::clone(&block_device),
            inode_bitmap: Bitmap::new(
                inode_bitmap_start as usize,
                inode_bitmap_blocks as usize,
            ),
            data_bitmap: Bitmap::new(
                data_bitmap_start as usize,
                data_bitmap_blocks as usize,
            ),
            inode_area_start_block: inode_area_start,
            data_area_start_block: data_area_start,
        };
        
        let efs = Arc::new(Mutex::new(efs));
        
        // 分配 inode 0 为根目录
        let root_inode_id = efs.lock().alloc_inode();
        assert_eq!(root_inode_id, 0);
        
        let (root_inode_block, root_inode_offset) = {
            let efs_lock = efs.lock();
            efs_lock.get_disk_inode_pos(root_inode_id)
        };
        
        get_block_cache(root_inode_block as usize, Arc::clone(&block_device))
            .lock()
            .modify(root_inode_offset, |disk_inode: &mut DiskInode| {
                disk_inode.initialize(DiskInodeType::Directory);
            });
        
        // 同步所有缓存
        block_cache_sync_all();
        
        efs
    }
    
    /// 打开文件系统
    /// 
    /// # Arguments
    /// 
    /// * `block_device` - 块设备
    /// 
    /// # 流程
    /// 
    /// 1. 读取 Block 0 的 SuperBlock
    /// 2. 校验魔数，失败则 panic
    /// 3. 构建 `EasyFileSystem` 并返回
    pub fn open(block_device: Arc<dyn BlockDevice>) -> Arc<Mutex<Self>> {
        // 读取 Block 0 的 SuperBlock
        get_block_cache(0, Arc::clone(&block_device))
            .lock()
            .read(0, |super_block: &SuperBlock| {
                // 校验魔数
                assert!(super_block.is_valid(), "Invalid easy-fs magic number");
                
                // 计算起始块号
                let inode_bitmap_start = 1u32;
                let inode_area_start = inode_bitmap_start + super_block.inode_bitmap_blocks;
                let data_bitmap_start = inode_area_start + super_block.inode_area_blocks;
                let data_area_start = data_bitmap_start + super_block.data_bitmap_blocks;
                
                let efs = Self {
                    block_device: Arc::clone(&block_device),
                    inode_bitmap: Bitmap::new(
                        inode_bitmap_start as usize,
                        super_block.inode_bitmap_blocks as usize,
                    ),
                    data_bitmap: Bitmap::new(
                        data_bitmap_start as usize,
                        super_block.data_bitmap_blocks as usize,
                    ),
                    inode_area_start_block: inode_area_start,
                    data_area_start_block: data_area_start,
                };
                
                Arc::new(Mutex::new(efs))
            })
    }
    
    /// 获取磁盘 inode 的位置
    /// 
    /// # Arguments
    /// 
    /// * `inode_id` - inode 编号
    /// 
    /// # Returns
    /// 
    /// `(block_id, offset)` - 块号和块内偏移
    /// 
    /// # Formula
    /// 
    /// block_id = inode_area_start + inode_id / 4
    /// offset = (inode_id % 4) * 128
    pub fn get_disk_inode_pos(&self, inode_id: u32) -> (u32, usize) {
        (
            self.inode_area_start_block + inode_id / INODES_PER_BLOCK,
            (inode_id % INODES_PER_BLOCK) as usize * core::mem::size_of::<DiskInode>(),
        )
    }
    
    /// 获取数据块的磁盘块号
    /// 
    /// # Arguments
    /// 
    /// * `data_block_id` - 数据块相对编号
    /// 
    /// # Returns
    /// 
    /// 磁盘绝对块号 = data_area_start + data_block_id
    pub fn get_data_block_id(&self, data_block_id: u32) -> u32 {
        self.data_area_start_block + data_block_id
    }
    
    /// 分配一个 inode
    /// 
    /// 从 inode 位图分配，返回 inode id。
    /// 分配失败则 panic。
    pub fn alloc_inode(&mut self) -> u32 {
        self.inode_bitmap.alloc(&self.block_device).unwrap() as u32
    }
    
    /// 分配一个数据块
    /// 
    /// 从数据位图分配，返回磁盘绝对块号。
    /// 分配失败则 panic。
    pub fn alloc_data(&mut self) -> u32 {
        let data_block_id = self.data_bitmap.alloc(&self.block_device).unwrap() as u32;
        self.get_data_block_id(data_block_id)
    }
    
    /// 回收一个数据块
    /// 
    /// 清零块内容并回收到数据位图。
    /// 
    /// # Arguments
    /// 
    /// * `block_id` - 磁盘绝对块号
    pub fn dealloc_data(&mut self, block_id: u32) {
        // 清零块内容
        get_block_cache(block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(0, |data_block: &mut [u8; BLOCK_SZ]| {
                data_block.fill(0);
            });
        
        // 计算相对块号并回收
        let data_block_id = block_id - self.data_area_start_block;
        self.data_bitmap.dealloc(&self.block_device, data_block_id as usize);
    }

    /// 获取根目录的 Inode
    /// 
    /// # Arguments
    /// 
    /// * `efs` - 文件系统引用
    /// 
    /// # Returns
    /// 
    /// 根目录的 Inode
    pub fn root_inode(efs: &Arc<Mutex<Self>>) -> Inode {
        let block_device = Arc::clone(&efs.lock().block_device);
        let (block_id, block_offset) = efs.lock().get_disk_inode_pos(0);
        Inode::new(block_id, block_offset, Arc::clone(efs), block_device)
    }
}
