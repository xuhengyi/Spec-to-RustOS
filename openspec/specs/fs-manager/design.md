## Context

`EasyFileSystem` 整合底层组件，对上层提供统一的文件系统服务。

## Goals / Non-Goals

- Goals: 文件系统创建/打开、块分配/回收、运行时状态管理
- Non-Goals: 不支持扩容/缩容、不优化并发

## Decisions

- **Arc<Mutex<...>>**：多 Inode 共享同一文件系统
- **根目录 inode = 0**：第一个分配的 inode
- **分配失败 panic**：简化错误处理

## Workflow

**create 流程**：
1. 计算区域大小（inode 区根据位图容量，数据区取剩余）
2. 清零 [0, total_blocks)
3. 写入 SuperBlock
4. alloc_inode() → 0，初始化为 Directory
5. sync_all

**open 流程**：
1. 读 SuperBlock，校验魔数
2. 解析区域起始位置
3. 构建 Bitmap 和返回
