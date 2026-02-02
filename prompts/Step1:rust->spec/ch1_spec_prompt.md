# Spec 生成 Prompt: ch1

## 角色
你是资深 Rust/OS 工程师与规格工程师。

## 任务
为 crate `ch1` 生成 OpenSpec capability specs。

## 输入文件
请阅读以下文件（在项目根目录下）：
- `ch1/Cargo.toml`
- `ch1/src/**/*.rs`
- `ch1/build.rs`（如果存在）
- `ch1/README.md`（如果存在）

## 输出要求

1. 创建 `openspec/specs/ch1/spec.md`，包含：
   - 使用 SHALL/MUST 规范化措辞
   - **每个 Requirement 至少一个 `#### Scenario:`**
   - 明确列出 public API（按模块/feature 分组）
   - 列出 build.rs/环境变量/生成文件（如有）
   - 列出依赖的 workspace 内 crate 及其所需符号（如有）
   - Feature flags 及其影响（如有）

2. 如果 crate 有明显架构/unsafe 约束/feature matrix，创建 `openspec/specs/ch1/design.md`

## 约束
- 不要改代码
- 不要发散到其它 crate
- 只描述该 crate 的对外契约与边界
- 若发现该 crate 依赖 workspace 内其它 crate：只把"它依赖对方的哪些符号/语义"写成前置条件（Preconditions）

## 示例格式

```markdown
# Capability: ch1

## Requirements

### Requirement: [功能名称]
[描述该功能，使用 SHALL/MUST]

#### Scenario: [场景名称]
- **WHEN** [触发条件]
- **THEN** [预期结果]

## Public API

### Types
- `TypeName`: [描述]

### Functions
- `function_name()`: [描述]

## Build Configuration
- build.rs: [描述行为]
- 环境变量: [列出]
- 生成文件: [列出]

## Dependencies
- Workspace crates: [列出]
- External crates: [列出]
```
