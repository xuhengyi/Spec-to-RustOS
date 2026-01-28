use alloc::collections::VecDeque;
use alloc::sync::Arc;
use spin::{Lazy, Mutex};

use crate::block_dev::{BlockDevice, BLOCK_SZ};

/// 块缓存结构
/// 
/// 在内存中缓存磁盘块，提供类型安全的数据访问接口。
pub struct BlockCache {
    /// 缓存的块数据
    cache: [u8; BLOCK_SZ],
    /// 块编号
    block_id: usize,
    /// 块设备引用
    block_device: Arc<dyn BlockDevice>,
    /// 脏标记，表示缓存是否被修改
    modified: bool,
}

impl BlockCache {
    /// 创建新的块缓存
    /// 
    /// 创建时从块设备读取指定块的数据，modified 初始为 false。
    pub fn new(block_id: usize, block_device: Arc<dyn BlockDevice>) -> Self {
        let mut cache = [0u8; BLOCK_SZ];
        block_device.read_block(block_id, &mut cache);
        Self {
            cache,
            block_id,
            block_device,
            modified: false,
        }
    }

    /// 获取缓存中指定偏移处的不可变引用
    /// 
    /// # Safety
    /// 调用者需确保：
    /// - T 使用 #[repr(C)] 布局
    /// - offset 满足对齐要求
    /// - offset + size_of::<T>() <= 512
    pub fn get_ref<T>(&self, offset: usize) -> &T {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SZ);
        let addr = &self.cache[offset] as *const u8 as *const T;
        unsafe { &*addr }
    }

    /// 获取缓存中指定偏移处的可变引用
    /// 
    /// 调用此方法会设置 modified = true。
    /// 
    /// # Safety
    /// 调用者需确保：
    /// - T 使用 #[repr(C)] 布局
    /// - offset 满足对齐要求
    /// - offset + size_of::<T>() <= 512
    pub fn get_mut<T>(&mut self, offset: usize) -> &mut T {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SZ);
        self.modified = true;
        let addr = &mut self.cache[offset] as *mut u8 as *mut T;
        unsafe { &mut *addr }
    }

    /// 读取缓存中指定偏移处的数据
    /// 
    /// 通过闭包访问数据，返回闭包的返回值。
    pub fn read<T, V>(&self, offset: usize, f: impl FnOnce(&T) -> V) -> V {
        f(self.get_ref(offset))
    }

    /// 修改缓存中指定偏移处的数据
    /// 
    /// 通过闭包修改数据，设置 modified = true，返回闭包的返回值。
    pub fn modify<T, V>(&mut self, offset: usize, f: impl FnOnce(&mut T) -> V) -> V {
        f(self.get_mut(offset))
    }

    /// 同步缓存到块设备
    /// 
    /// 如果 modified 为 true，将缓存写回块设备并重置 modified 标记。
    pub fn sync(&mut self) {
        if self.modified {
            self.block_device.write_block(self.block_id, &self.cache);
            self.modified = false;
        }
    }
}

impl Drop for BlockCache {
    fn drop(&mut self) {
        self.sync();
    }
}

/// 块缓存管理器
/// 
/// 管理最多 16 个块缓存，提供缓存复用和替换策略。
pub struct BlockCacheManager {
    /// 缓存队列，每个元素为 (block_id, Arc<Mutex<BlockCache>>)
    queue: VecDeque<(usize, Arc<Mutex<BlockCache>>)>,
}

/// 块缓存管理器最大容量
const BLOCK_CACHE_SIZE: usize = 16;

impl BlockCacheManager {
    /// 创建新的块缓存管理器
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    /// 获取指定块的缓存
    /// 
    /// 行为：
    /// - 已缓存：返回现有引用
    /// - 未缓存且未满：创建新缓存
    /// - 未缓存且已满：替换 strong_count == 1 的条目
    /// - 无可替换：panic
    pub fn get_block_cache(
        &mut self,
        block_id: usize,
        block_device: Arc<dyn BlockDevice>,
    ) -> Arc<Mutex<BlockCache>> {
        // 检查是否已缓存
        if let Some(pair) = self.queue.iter().find(|pair| pair.0 == block_id) {
            return Arc::clone(&pair.1);
        }

        // 未缓存，检查是否已满
        if self.queue.len() == BLOCK_CACHE_SIZE {
            // 查找可替换的缓存（strong_count == 1）
            if let Some((idx, _)) = self
                .queue
                .iter()
                .enumerate()
                .find(|(_, pair)| Arc::strong_count(&pair.1) == 1)
            {
                self.queue.remove(idx);
            } else {
                panic!("Run out of BlockCache!");
            }
        }

        // 创建新缓存
        let block_cache = Arc::new(Mutex::new(BlockCache::new(block_id, Arc::clone(&block_device))));
        self.queue.push_back((block_id, Arc::clone(&block_cache)));
        block_cache
    }
}

/// 全局块缓存管理器
pub static BLOCK_CACHE_MANAGER: Lazy<Mutex<BlockCacheManager>> =
    Lazy::new(|| Mutex::new(BlockCacheManager::new()));

/// 获取指定块的缓存
/// 
/// 这是对全局 BlockCacheManager 的便捷访问接口。
pub fn get_block_cache(
    block_id: usize,
    block_device: Arc<dyn BlockDevice>,
) -> Arc<Mutex<BlockCache>> {
    BLOCK_CACHE_MANAGER
        .lock()
        .get_block_cache(block_id, block_device)
}

/// 同步所有缓存中的脏块
/// 
/// 遍历所有缓存，将脏块写回块设备。
pub fn block_cache_sync_all() {
    let manager = BLOCK_CACHE_MANAGER.lock();
    for (_, cache) in manager.queue.iter() {
        cache.lock().sync();
    }
}
