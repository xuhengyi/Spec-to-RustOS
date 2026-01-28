# 实现生成 Prompt: ch1-lab

## 角色
你是 Rust OS crate 的实现者。

## 任务
从 OpenSpec spec 实现 crate `ch1-lab`。

## 输入文件
- `openspec/specs/ch1-lab/spec.md`
- `openspec/specs/ch1-lab/design.md`
- `ch1-lab/Cargo.toml`

## 访问规则（重要！）

### 允许的访问
1. **当前 crate 的 spec**: 必须阅读 `openspec/specs/ch1-lab/spec.md` 和 design.md（如有）
2. **直接依赖的 spec（可选，需记录）**: 如果当前 crate 的 spec 不足以理解接口，可以阅读直接依赖的 spec：
   - `openspec/specs/console/spec.md` (crate: `console`)
   - `openspec/specs/kernel-alloc/spec.md` (crate: `kernel-alloc`)
   
   **重要**: 如果访问了直接依赖的 spec，必须在实现日志中说明原因。

3. **已生成代码和其余spec（下下策，需记录）**: 只有在测试失败且无法通过 spec 解决问题时，才能访问已实现的代码：
   
   **重要**: 如果访问了已生成的代码，必须在实现日志中详细说明：
   - 为什么需要访问（测试失败的具体原因）
   - 访问了哪些文件
   - 从中学到了什么
   - 为什么这是下下策

## 约束
- **仅实现当前 crate**: 只修改 `ch1-lab/` 目录下的文件
- **优先使用当前 crate 的 spec**: 首先尝试仅通过当前 crate 的 spec 实现
- **谨慎使用直接依赖的 spec**: 只有在当前 spec 不足以理解接口时才使用，并在日志中说明
- **最后手段：查看已生成代码**: 只有在测试失败且无法通过 spec 解决问题时使用，必须在日志中详细说明
- 实现 spec 中定义的全部对外契约
- 保持 API 兼容
- 优先最小实现，但必须满足 spec 的行为与不变量
- 不新增非必要依赖
- 不修改其它 crate（除非为了解决编译错误且变化被 spec 允许；这种情况要先报告并请求调整 spec）

## Gate 要求
- `cargo qemu --ch 1` 必须通过
- 访问 `user/src/bin/` 下的测试程序代码，验证输出是否符合预期

## 输出
只提交该 crate 目录下必要的 Rust 源码/配置（`src/lib.rs`/`src/main.rs`/必要模块/必要 build.rs 等）。

## 工作流程
1. 阅读 `openspec/specs/ch1-lab/spec.md` 和 design.md（如有）
2. 尝试仅基于当前 crate 的 spec 实现
3. 如果当前 spec 不足以理解接口，可以阅读直接依赖的 specs（**必须在日志中说明原因**）
4. 实现 crate（创建或修改 `ch1-lab/src/lib.rs` 或 `ch1-lab/src/main.rs`）
5. 运行 gate 验证：`cargo qemu --ch 1`
6. **验证输出**：访问 `user/src/bin/` 目录下的测试程序代码，检查 `cargo qemu --ch 1` 的输出是否符合预期
   - 查看 `user/cases.toml` 了解当前章节需要运行的测试用例
   - 阅读 `user/src/bin/` 下对应测试程序的源代码，理解预期的输出行为
   - 对比实际运行输出与预期输出，确保所有测试用例的输出都符合预期
7. 如果测试失败或输出不符合预期且无法通过 spec 解决，可以访问已生成的代码（**必须在日志中详细说明**）
8. 更新实现日志：`implementation_logs/ch1-lab_implementation.log`

## 验证命令
```bash
# 使用 cargo qemu 进行验证
cargo qemu --ch 1
```

## 输出验证
验证 `cargo qemu --ch 1` 的输出是否符合预期：
1. 查看 `user/cases.toml` 中 `[ch1]` 部分，了解需要运行的测试用例列表
2. 访问 `user/src/bin/` 目录下对应的测试程序源代码（如 `00hello_world.rs`、`02power.rs` 等）
3. 理解每个测试程序的预期输出行为
4. 运行 `cargo qemu --ch 1` 并检查实际输出是否与预期一致
5. 确保所有测试用例的输出都正确，没有错误或异常行为

---

## 实现日志

**必须维护实现日志**: `implementation_logs/ch1-lab_implementation.log`

日志应包含：
1. **实现开始时间**
2. **使用的资源**:
   - ✅ 当前 crate 的 spec
   - ⚠️  直接依赖的 spec（如果使用，说明原因）
   - ❌ 已生成的代码（如果使用，详细说明原因、访问的文件、学到的内容）
3. **实现过程**: 关键决策和遇到的问题，尽量详细包含你每一次动作，如search第三方库，访问除输入外的代码，以及遇到什么具体报错和调试思路
4. **测试结果**: gate 验证是否通过，给出代码是否是仅经过一次生成就通过测试（这里指的是第一次运行测试命令除警告信息外没有报错信息，直接编译成功），如果不是记录你的修改流程
5. **实现完成时间**
6. **日志必须使用中文**

## 开始实现

请根据上述 spec 实现 `ch1-lab` crate。

**重要提醒**:
- 只修改 `ch1-lab/` 目录
- 优先使用当前 crate 的 spec
- 谨慎使用直接依赖的 spec，并在日志中说明
- 只有在测试失败时才访问已生成的代码，并在日志中详细说明
- 确保实现满足 spec 中的所有要求
- 维护实现日志
