# Easy-FS 分步实现指南

由于 easy-fs 实现复杂度较高，将其拆分为 5 个增量步骤。

## 模块依赖

```
vfs-inode (Step 5)
    ↓
fs-manager (Step 4)
    ↓
disk-layout (Step 3)
    ↓
block-cache (Step 2)
    ↓
block-device (Step 1)
```

## 实现步骤

| 步骤 | Spec | Prompt |
|-----|------|--------|
| 1 | `openspec/specs/block-device/` | `step1_block_device_prompt.md` |
| 2 | `openspec/specs/block-cache/` | `step2_block_cache_prompt.md` |
| 3 | `openspec/specs/disk-layout/` | `step3_disk_layout_prompt.md` |
| 4 | `openspec/specs/fs-manager/` | `step4_fs_manager_prompt.md` |
| 5 | `openspec/specs/vfs-inode/` | `step5_vfs_inode_prompt.md` |

## 使用方法

按顺序执行各步骤的 prompt，最终在 Step 5 完成后通过 `cargo test`。
