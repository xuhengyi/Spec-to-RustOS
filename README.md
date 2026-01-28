# Spec-to-RustOS 验证实验

本项目是一个 **Spec-to-RustOS 验证实验**，旨在验证从 OpenSpec 规范自动生成 Rust OS 实现的可行性。实验采用 **rcore → spec → rcore** 的双向验证流程，使用 AI 辅助完成实现。

> **基于项目**: 本项目基于 [rCore-Tutorial-in-single-workspace](https://github.com/rcore-os/rCore-Tutorial-in-single-workspace) 进行验证实验。

## 实验概述

### 实验流程

1. **Phase A：从现有实现生成 OpenSpec 规范和测试用例**
   - 分析 [rCore-Tutorial-in-single-workspace](https://github.com/rcore-os/rCore-Tutorial-in-single-workspace) 的 crate 实现
   - 为每个 crate 生成结构化的 OpenSpec 规范（`openspec/specs/<crate>/spec.md`）
   - 规范包含 API 契约、行为要求、场景示例等
   - 基于原有实现生成单元测试用例（`<crate>/tests/api_tests.rs`）
   - 测试用例使用 Mock 对象模拟硬件或内核支持的功能，在用户态环境运行

2. **Phase B：从规范重新生成实现**
   - 在新目录中，**仅基于 OpenSpec 规范**重新实现所有 crate
   - 根据 spec 生成代码
   - 严格限制：实现过程中**禁止访问原始实现代码**，只能通过 spec 了解接口
   - 按依赖顺序逐个实现，并通过 gate 验证（`cargo check`）

3. **验证与测试**
   - 每个 crate 实现后立即进行 gate 验证（`cargo check` 和 `cargo test`）
   - 使用 Phase A 生成的测试用例验证实现的正确性
   - 完成 ch1 后开始集成测试（`cargo qemu --ch 1`）
   - 所有章节完成后运行完整集成测试套件

### 实验目标

- ✅ **验证 spec 的完整性**：如果仅通过 spec 能成功实现，说明 spec 足够详细
- ✅ **测试 AI 辅助开发能力**：验证 AI 模型基于结构化规范实现复杂系统的能力
- ✅ **验证测试用例的有效性**：通过 AI 生成的测试用例验证实现的正确性
- ✅ **建立可复用的规范体系**：为未来的 OS 开发提供标准化的规范模板和测试用例模板

## 项目结构说明

本项目包含以下类型的 crate：

- **核心库 crate**：linker, console, easy-fs, kernel-context, kernel-alloc, kernel-vm, task-manage, signal-defs, signal, signal-impl, sync, syscall
- **章节 crate**：ch1, ch1-lab, ch2, ch3, ch4, ch5, ch6, ch7, ch8
- **工具 crate**：xtask（构建工具，来自原仓库）
- **用户程序 crate**：user（用户程序集合，来自原仓库）

**注意**：`user` 和 `xtask` 两个 crate 来自原仓库的复制，不属于本实验的实现范围，仅用于构建和测试。
## 实现流程

### Crate 实现顺序

**必须按依赖顺序实现，否则会导致编译错误。** 推荐以下线性实现顺序：

```
1.  linker ✅
2.  task-manage 
3.  console ✅
4.  easy-fs ✅        (需要5个相关spec，见下方说明)
5.  kernel-context ✅
6.  kernel-alloc 
7.  kernel-vm 
8.  signal-defs ✅
9.  syscall ✅        (依赖: signal-defs)
10. sync              (依赖: task-manage)
11. signal            (依赖: kernel-context, signal-defs)
12. signal-impl       (依赖: kernel-context, signal)
13.  ch1 ✅
14. ch1-lab           (依赖: console)
15. ch2 ✅            (依赖: linker, console, kernel-context, syscall)
16. ch3  ✅           (依赖: linker, console, kernel-context, syscall) [同 ch2，有 coop feature]
17. ch4               (依赖: ch2 的依赖 + kernel-alloc, kernel-vm)
18. ch5               (依赖: ch4 的依赖 + task-manage)
19. ch6               (依赖: ch5 的依赖 + easy-fs)
20. ch7               (依赖: ch6 的依赖 + signal, signal-impl)
21. ch8               (依赖: ch7 的依赖 + sync)
```

### 实现约束

**强烈建议：禁止访问其他已实现 crate 的源码。**

**允许的访问：**
- ✅ 当前 crate 的 spec.md 和 design.md
- ✅ 直接依赖 crate 的 spec.md（仅了解接口，不看实现）
- ✅ 当前 crate 目录下的文件

**禁止的访问：**
- ❌ 其他已实现 crate 的源码（`src/**/*.rs`）
- ❌ 其他已实现 crate 的内部实现细节
- ❌ 间接依赖的源码（只能通过 spec 了解）

### Easy-FS 特殊说明

`easy-fs` crate 实现复杂度较高，被拆分为 5 个增量步骤，需要参考以下 5 个相关 spec：

1. **block-device** (`openspec/specs/block-device/spec.md`) - 块设备抽象层
2. **block-cache** (`openspec/specs/block-cache/spec.md`) - 块缓存层
3. **disk-layout** (`openspec/specs/disk-layout/spec.md`) - 磁盘布局层
4. **fs-manager** (`openspec/specs/fs-manager/spec.md`) - 文件系统管理层
5. **vfs-inode** (`openspec/specs/vfs-inode/spec.md`) - VFS inode 层

实现顺序：block-device → block-cache → disk-layout → fs-manager → vfs-inode

详细步骤请参考 `prompts/easy-fs/README.md` 和对应的分步 prompt 文件。

### 单个 Crate 实现流程

```bash
# 1. 查看实现 prompt
#    所有 prompt 可以通过以下命令一次性生成：
./scripts/generate_all_prompts.sh
#    或者手动查看 prompts/<crate>_implementation_prompt.md
#    对于 easy-fs，查看 prompts/easy-fs/ 目录下的分步 prompt

# 2. 在 Cursor 中使用 prompt
#    打开对应的 prompt 文件
#    复制内容到 Cursor 对话
#    让 AI 实现

# 3. 验证
cargo check
cargo test
```


## 测试说明

### 单元测试

每个 crate 都包含单元测试（`tests/api_tests.rs`），用于验证 API 的正确性。测试在用户态环境运行，使用 `std`。

**测试限制说明**：部分 crate 的单元测试因架构原因不能单独测试。**kernel-context**、**signal** 和 **signal-impl** 包含 RISC-V 特定的内联汇编代码，需要 RISC-V 64 位平台才能编译和运行（在 x86_64 主机上无法编译，在 `riscv64gc-unknown-none-elf` 目标上无法运行）。**kernel-vm** 需要 `PageManager` trait 的具体实现，而这些实现通常需要特定的架构支持（如 RISC-V Sv39），单元测试主要验证类型和基本 API 的存在性。对于这些 crate，推荐使用 `cargo check` 验证编译，或通过集成测试（如 `cargo qemu --ch 7` 或 `ch8`）在实际内核环境中验证功能。其他大部分 crate 的单元测试可以在标准 Rust 环境中运行，使用 Mock 对象模拟硬件或内核支持的功能。

### 集成测试

**每完成一个章节后测试**：
```bash
cargo qemu --ch 1
cargo qemu --ch 2
cargo qemu --ch 3
# ... 以此类推
```

## 当前状态

- ✅ **Phase A**：基于原仓库所有 crate 的 OpenSpec 规范已生成
- ✅ 基于原仓库 crate 的测例已生成

- ✅ **Phase B**：正在按顺序实现各个 crate
  - ✅ 已完成：linker, console, easy-fs, kernel-context, signal-defs, syscall, ch1, ch2，ch3
  - ✅ 集成测试：ch1-ch3 已通过 QEMU 测试


