#![no_std]

extern crate alloc;

mod block_cache;
mod block_dev;
mod efs;
mod layout;
mod vfs;

pub use block_cache::{
    block_cache_sync_all, get_block_cache, BlockCache, BlockCacheManager, BLOCK_CACHE_MANAGER,
};
pub use block_dev::{BlockDevice, BLOCK_SZ};
pub use efs::EasyFileSystem;
pub use layout::{
    Bitmap, DirEntry, DiskInode, DiskInodeType, SuperBlock,
    DIRENT_SZ, EFS_MAGIC, INODE_DIRECT_COUNT, NAME_LENGTH_LIMIT,
};
pub use vfs::{FSManager, FileHandle, Inode, OpenFlags, UserBuffer};
