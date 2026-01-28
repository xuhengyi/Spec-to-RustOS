//! easy-fs crate 功能性验证测试
//! 
//! 这些测试验证 easy-fs crate 对外提供的 API 的正确性。
//! 测试在用户态环境运行，使用 std。
//! 
//! 注意：easy-fs 是一个 no_std crate，但测试使用 std 来创建 mock 块设备。

use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use easy_fs::{BlockDevice, EasyFileSystem, FileHandle, Inode, OpenFlags, UserBuffer, BLOCK_SZ};

// Mock 块设备实现，用于测试
struct MockBlockDevice {
    blocks: Arc<StdMutex<Vec<Vec<u8>>>>,
}

impl MockBlockDevice {
    fn new(block_size: usize, num_blocks: usize) -> Self {
        let mut blocks = Vec::new();
        for _ in 0..num_blocks {
            blocks.push(vec![0u8; block_size]);
        }
        Self {
            blocks: Arc::new(StdMutex::new(blocks)),
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

const TEST_TOTAL_BLOCKS: u32 = 4096;
const TEST_INODE_BITMAP_BLOCKS: u32 = 1;

static TEST_LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
static TEST_DEVICE: OnceLock<Arc<MockBlockDevice>> = OnceLock::new();

fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK
        .get_or_init(|| StdMutex::new(()))
        .lock()
        .unwrap_or_else(|err| err.into_inner())
}

fn test_device() -> Arc<MockBlockDevice> {
    TEST_DEVICE
        .get_or_init(|| Arc::new(MockBlockDevice::new(BLOCK_SZ, TEST_TOTAL_BLOCKS as usize)))
        .clone()
}

fn with_test_device<T>(f: impl FnOnce(Arc<MockBlockDevice>) -> T) -> T {
    let _guard = test_lock();
    let device = test_device();
    f(device)
}

fn with_test_fs<T>(f: impl FnOnce(Arc<MockBlockDevice>, Inode) -> T) -> T {
    let _guard = test_lock();
    let device = test_device();
    let efs = EasyFileSystem::create(device.clone(), TEST_TOTAL_BLOCKS, TEST_INODE_BITMAP_BLOCKS);
    let root = EasyFileSystem::root_inode(&efs);
    f(device, root)
}

#[test]
fn test_block_size_constant() {
    // 测试 BLOCK_SZ 常量
    assert_eq!(BLOCK_SZ, 512);
    assert!(BLOCK_SZ > 0);
}

#[test]
fn test_block_device_trait() {
    // 测试 BlockDevice trait 的基本功能
    let device = Arc::new(MockBlockDevice::new(BLOCK_SZ, 10));
    
    // 测试写入
    let test_data = vec![0xAA; BLOCK_SZ];
    device.write_block(0, &test_data);
    
    // 测试读取
    let mut read_buf = vec![0u8; BLOCK_SZ];
    device.read_block(0, &mut read_buf);
    
    assert_eq!(read_buf, test_data);
}

#[test]
fn test_block_device_multiple_blocks() {
    // 测试多个块的读写
    let device = Arc::new(MockBlockDevice::new(BLOCK_SZ, 5));
    
    // 写入多个块
    for i in 0..5 {
        let data = vec![i as u8; BLOCK_SZ];
        device.write_block(i, &data);
    }
    
    // 读取并验证
    for i in 0..5 {
        let mut buf = vec![0u8; BLOCK_SZ];
        device.read_block(i, &mut buf);
        assert_eq!(buf, vec![i as u8; BLOCK_SZ]);
    }
}

#[test]
fn test_open_flags_basic() {
    // 测试 OpenFlags bitflags
    let rdonly = OpenFlags::RDONLY;
    assert_eq!(rdonly.bits(), 0);
    
    let wronly = OpenFlags::WRONLY;
    assert_eq!(wronly.bits(), 1);
    
    let rdwr = OpenFlags::RDWR;
    assert_eq!(rdwr.bits(), 2);
    
    let create = OpenFlags::CREATE;
    assert_eq!(create.bits(), 512);
    
    let trunc = OpenFlags::TRUNC;
    assert_eq!(trunc.bits(), 1024);
}

#[test]
fn test_open_flags_combinations() {
    // 测试 OpenFlags 的组合
    let flags1 = OpenFlags::WRONLY | OpenFlags::CREATE | OpenFlags::TRUNC;
    assert!(flags1.contains(OpenFlags::WRONLY));
    assert!(flags1.contains(OpenFlags::CREATE));
    assert!(flags1.contains(OpenFlags::TRUNC));
    
    let flags2 = OpenFlags::RDWR | OpenFlags::CREATE;
    assert!(flags2.contains(OpenFlags::RDWR));
    assert!(flags2.contains(OpenFlags::CREATE));
}

#[test]
fn test_open_flags_read_write() {
    // 测试 read_write() 方法
    let rdonly = OpenFlags::RDONLY;
    let (readable, writable) = rdonly.read_write();
    assert!(readable);
    assert!(!writable);
    
    let wronly = OpenFlags::WRONLY;
    let (readable, writable) = wronly.read_write();
    assert!(!readable);
    assert!(writable);
    
    let rdwr = OpenFlags::RDWR;
    let (readable, writable) = rdwr.read_write();
    assert!(readable);
    assert!(writable);
}

#[test]
fn test_file_handle_new() {
    // 测试 FileHandle::new 与读写偏移
    with_test_fs(|_device, root| {
        let inode = root.create("handle_file").unwrap();
        let mut handle = FileHandle::new(true, true, inode);

        let write_slice: &'static mut [u8] = Box::leak(Box::new([b'a', b'b', b'c']));
        let write_buf = UserBuffer::new(vec![write_slice]);
        let write_len = handle.write(write_buf);
        assert_eq!(write_len, 3);
        assert_eq!(handle.offset, 3);

        handle.offset = 0;
        let read_box = Box::new([0u8; 3]);
        let read_ptr = read_box.as_ptr();
        let read_slice: &'static mut [u8] = Box::leak(read_box);
        let read_buf = UserBuffer::new(vec![read_slice]);
        let read_len = handle.read(read_buf);
        assert_eq!(read_len, 3);
        let read_back = unsafe { std::slice::from_raw_parts(read_ptr, 3) };
        assert_eq!(read_back, b"abc");
        assert!(handle.readable());
        assert!(handle.writable());
    });
}

#[test]
fn test_file_handle_empty() {
    // 测试 FileHandle::empty
    let handle = FileHandle::empty(true, true);
    assert!(handle.readable());
    assert!(handle.writable());
    assert_eq!(handle.offset, 0);
    assert!(handle.inode.is_none());
}

#[test]
fn test_file_handle_readable_writable() {
    // 测试 FileHandle 的 readable 和 writable 方法
    // 注意：由于 Inode 没有实现 Clone，我们主要测试 empty 方法
    let handle1 = FileHandle::empty(true, false);
    assert!(handle1.readable());
    assert!(!handle1.writable());
    
    let handle2 = FileHandle::empty(false, true);
    assert!(!handle2.readable());
    assert!(handle2.writable());
    
    let handle3 = FileHandle::empty(true, true);
    assert!(handle3.readable());
    assert!(handle3.writable());
}

#[test]
fn test_user_buffer_new() {
    // 测试 UserBuffer::new
    // 注意：UserBuffer 需要 'static 生命周期，使用 Box::leak
    let buf1 = Box::leak(Box::new(vec![0u8; 10]));
    let buf2 = Box::leak(Box::new(vec![0u8; 20]));
    let buffers = vec![buf1.as_mut_slice(), buf2.as_mut_slice()];
    let user_buf = UserBuffer::new(buffers);
    
    assert_eq!(user_buf.len(), 30);
}

#[test]
fn test_user_buffer_len() {
    // 测试 UserBuffer::len
    // 注意：UserBuffer 需要 'static 生命周期，使用 Box::leak
    let buf1 = Box::leak(Box::new(vec![0u8; 5]));
    let buf2 = Box::leak(Box::new(vec![0u8; 15]));
    let buf3 = Box::leak(Box::new(vec![0u8; 10]));
    let buffers = vec![
        buf1.as_mut_slice(),
        buf2.as_mut_slice(),
        buf3.as_mut_slice(),
    ];
    let user_buf = UserBuffer::new(buffers);
    
    assert_eq!(user_buf.len(), 30);
}

#[test]
fn test_user_buffer_empty() {
    // 测试空的 UserBuffer
    let user_buf = UserBuffer::new(vec![]);
    assert_eq!(user_buf.len(), 0);
}

#[test]
fn test_easy_filesystem_create() {
    // 测试 EasyFileSystem::create
    with_test_fs(|_device, root| {
        // 根 inode 应该可以列出目录（即使为空）
        let entries = root.readdir();
        // 初始时应该是空的
        assert_eq!(entries.len(), 0);
    });
}

#[test]
fn test_easy_filesystem_root_inode() {
    // 测试获取根 inode
    with_test_fs(|_device, root| {
        // 根目录应该可以列出内容
        let entries = root.readdir();
        // 初始时应该是空的
        assert_eq!(entries.len(), 0);
    });
}

#[test]
fn test_inode_find_not_exist() {
    // 测试查找不存在的文件
    with_test_fs(|_device, root| {
        let result = root.find("nonexistent");
        assert!(result.is_none());
    });
}

#[test]
fn test_inode_create_file() {
    // 测试创建文件
    with_test_fs(|_device, root| {
        // 创建文件（create 总是创建文件）
        let file = root.create("test_file").unwrap();

        // 验证文件存在
        let found = root.find("test_file");
        assert!(found.is_some());

        // 验证可以写入和读取（文件特性）
        file.write_at(0, b"test data");
        let mut buf = vec![0u8; 9];
        let len = file.read_at(0, &mut buf);
        assert_eq!(len, 9);
        assert_eq!(&buf[..len], b"test data");
    });
}

#[test]
#[should_panic]
fn test_inode_create_name_too_long_panics() {
    // 超过 NAME_LENGTH_LIMIT 的文件名应触发 panic
    with_test_fs(|_device, root| {
        let name = "a".repeat(29);
        let _ = root.create(&name);
    });
}

#[test]
fn test_inode_create_multiple_files() {
    // 测试创建多个文件
    with_test_fs(|_device, root| {
        // 创建多个文件
        root.create("file1").unwrap();
        root.create("file2").unwrap();
        root.create("file3").unwrap();

        // 验证所有文件都存在
        assert!(root.find("file1").is_some());
        assert!(root.find("file2").is_some());
        assert!(root.find("file3").is_some());
    });
}

#[test]
fn test_inode_read_write() {
    // 测试文件读写
    with_test_fs(|_device, root| {
        // 创建文件
        let file = root.create("test_file").unwrap();

        // 写入数据
        let test_data = b"Hello, easy-fs!";
        file.write_at(0, test_data);

        // 读取数据
        let mut read_buf = vec![0u8; test_data.len()];
        let read_len = file.read_at(0, &mut read_buf);

        assert_eq!(read_len, test_data.len());
        assert_eq!(&read_buf[..read_len], test_data);
    });
}

#[test]
fn test_inode_read_write_large() {
    // 测试跨越直接块的读写（触发一级间接块）
    with_test_fs(|_device, root| {
        // 创建文件
        let file = root.create("large_file").unwrap();

        // 写入大量数据（超过直接块范围）
        let test_data_len = BLOCK_SZ * 28 + 1;
        let test_data: Vec<u8> = (0..test_data_len).map(|i| (i % 256) as u8).collect();
        file.write_at(0, &test_data);

        // 读取数据
        let mut read_buf = vec![0u8; test_data.len()];
        let read_len = file.read_at(0, &mut read_buf);

        assert_eq!(read_len, test_data.len());
        assert_eq!(&read_buf[..read_len], &test_data);
    });
}

#[test]
fn test_inode_readdir() {
    // 测试列出目录内容（顺序与创建一致）
    with_test_fs(|_device, root| {
        // 创建几个文件
        root.create("file1").unwrap();
        root.create("file2").unwrap();
        root.create("file3").unwrap();

        // 列出目录内容
        let names = root.readdir();

        // 验证顺序与创建一致
        assert_eq!(
            names,
            vec!["file1".to_string(), "file2".to_string(), "file3".to_string()]
        );
    });
}

#[test]
fn test_easy_filesystem_open() {
    // 测试打开已存在的文件系统
    with_test_device(|device| {
        // 创建文件系统
        let efs1 =
            EasyFileSystem::create(device.clone(), TEST_TOTAL_BLOCKS, TEST_INODE_BITMAP_BLOCKS);
        let root1 = EasyFileSystem::root_inode(&efs1);
        root1.create("test_file").unwrap();

        // 重新打开文件系统
        let efs2 = EasyFileSystem::open(device.clone());
        let root2 = EasyFileSystem::root_inode(&efs2);

        // 验证文件仍然存在
        let found = root2.find("test_file");
        assert!(found.is_some());
    });
}

#[test]
fn test_inode_clear() {
    // 测试清空文件
    with_test_fs(|_device, root| {
        // 创建文件并写入数据
        let file = root.create("test_file").unwrap();
        file.write_at(0, b"test data");

        // 验证数据已写入
        let mut buf1 = vec![0u8; 9];
        assert_eq!(file.read_at(0, &mut buf1), 9);

        // 清空文件
        file.clear();

        // 验证文件为空
        let mut buf2 = vec![0u8; 10];
        let read_len = file.read_at(0, &mut buf2);
        assert_eq!(read_len, 0);
    });
}

#[test]
fn test_inode_size() {
    // 测试文件大小（通过读写验证）
    with_test_fs(|_device, root| {
        // 创建文件
        let file = root.create("test_file").unwrap();

        // 初始时读取应该返回 0
        let mut buf = vec![0u8; 10];
        let len = file.read_at(0, &mut buf);
        assert_eq!(len, 0);

        // 写入数据
        let test_data = b"Hello, World!";
        file.write_at(0, test_data);

        // 验证可以读取全部数据
        let mut read_buf = vec![0u8; test_data.len()];
        let read_len = file.read_at(0, &mut read_buf);
        assert_eq!(read_len, test_data.len());
        assert_eq!(&read_buf[..read_len], test_data);
    });
}

#[test]
fn test_inode_read_at_offset() {
    // 测试从偏移量读取
    with_test_fs(|_device, root| {
        // 创建文件
        let file = root.create("test_file").unwrap();

        // 写入数据
        file.write_at(0, b"Hello, World!");

        // 从偏移量 7 读取
        let mut buf = vec![0u8; 5];
        let read_len = file.read_at(7, &mut buf);

        assert_eq!(read_len, 5);
        assert_eq!(&buf[..read_len], b"World");
    });
}

#[test]
fn test_inode_read_at_eof_returns_zero() {
    // 测试读取文件末尾返回 0
    with_test_fs(|_device, root| {
        let file = root.create("test_file").unwrap();
        file.write_at(0, b"abc");

        let mut buf = [0u8; 1];
        let read_len = file.read_at(3, &mut buf);
        assert_eq!(read_len, 0);
    });
}

#[test]
fn test_inode_write_at_offset() {
    // 测试从偏移量写入
    with_test_fs(|_device, root| {
        // 创建文件
        let file = root.create("test_file").unwrap();

        // 写入初始数据
        file.write_at(0, b"Hello, World!");

        // 从偏移量 7 覆盖写入（只覆盖4个字节，原始是"World" 5字节）
        file.write_at(7, b"Rust");

        // 读取并验证
        let mut buf = vec![0u8; 13];
        let read_len = file.read_at(0, &mut buf);

        assert_eq!(read_len, 13);
        // 结果应该是 "Hello, Rustd!" (覆盖了 "Worl" 变成 "Rust"，"d"保留)
        assert_eq!(&buf[..read_len], b"Hello, Rustd!");
    });
}
