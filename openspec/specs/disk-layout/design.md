## Context

磁盘数据结构层定义 easy-fs 在磁盘上的存储格式，是实现难度最高的一层。

## Goals / Non-Goals

- Goals: 简洁的磁盘布局、三级索引支持大文件、稳定的内存布局
- Non-Goals: 不支持日志/事务、不支持稀疏文件、不支持扩展属性

## Decisions

- **块大小 512 字节**：与扇区一致
- **DiskInode 128 字节**：每块 4 个，28 个直接索引
- **三级索引**：直接 14KB + 一级 64KB + 二级 8MB
- **目录项 32 字节**：文件名 27 + 终止符 1 + inode 4

## Implementation Notes

**难点 1：get_block_id 边界计算**
```
DIRECT_BOUND = 28
INDIRECT1_BOUND = 28 + 128 = 156
```

**难点 2：increase_size 顺序**
先填 direct → 分配 indirect1 并填充 → 分配 indirect2 及其子块

**难点 3：clear_size 反向回收**
回收顺序：data blocks → indirect1 blocks → indirect2 blocks
