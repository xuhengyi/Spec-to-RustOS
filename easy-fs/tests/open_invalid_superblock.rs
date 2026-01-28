//! easy-fs superblock 校验测试

use std::sync::{Arc, Mutex};

use easy_fs::{BlockDevice, EasyFileSystem, BLOCK_SZ};

// 简化的 Mock 块设备实现
struct MockBlockDevice {
    blocks: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl MockBlockDevice {
    fn new(num_blocks: usize) -> Self {
        let mut blocks = Vec::new();
        for _ in 0..num_blocks {
            blocks.push(vec![0u8; BLOCK_SZ]);
        }
        Self {
            blocks: Arc::new(Mutex::new(blocks)),
        }
    }
}

impl BlockDevice for MockBlockDevice {
    fn read_block(&self, block_id: usize, buf: &mut [u8]) {
        let blocks = self.blocks.lock().unwrap();
        if block_id < blocks.len() {
            let block = &blocks[block_id];
            let len = buf.len().min(block.len());
            buf[..len].copy_from_slice(&block[..len]);
        }
    }

    fn write_block(&self, block_id: usize, buf: &[u8]) {
        let mut blocks = self.blocks.lock().unwrap();
        if block_id < blocks.len() {
            let block = &mut blocks[block_id];
            let len = buf.len().min(block.len());
            block[..len].copy_from_slice(&buf[..len]);
        }
    }
}

#[test]
#[should_panic]
fn test_open_invalid_superblock_panics() {
    // 未初始化的设备不应被 open 接受
    let device = Arc::new(MockBlockDevice::new(32));
    let _ = EasyFileSystem::open(device);
}
