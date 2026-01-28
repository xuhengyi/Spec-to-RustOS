use core::any::Any;

/// 块大小常量，固定为 512 字节
pub const BLOCK_SZ: usize = 512;

/// 块设备抽象接口
/// 
/// 提供以 512 字节块为单位的读写抽象，供块缓存层调用。
/// 调用方需实现此 trait。
pub trait BlockDevice: Send + Sync + Any {
    /// 读取指定块的内容到缓冲区
    /// 
    /// # 参数
    /// - `block_id`: 块编号
    /// - `buf`: 目标缓冲区，长度必须为 512 字节
    fn read_block(&self, block_id: usize, buf: &mut [u8]);

    /// 将缓冲区内容写入指定块
    /// 
    /// # 参数
    /// - `block_id`: 块编号
    /// - `buf`: 源缓冲区，长度必须为 512 字节
    fn write_block(&self, block_id: usize, buf: &[u8]);
}
