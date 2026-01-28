#!/bin/bash
# 一次性生成所有 crate 的实现 prompt
# 用法: ./scripts/generate_all_prompts.sh [--allow-other-crates]

set -e

WORKSPACE_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$WORKSPACE_ROOT"

ALLOW_OTHER_CRATES="${1:-}"

# Crate 线性实现顺序（基于依赖关系的拓扑排序）
# 此顺序确保每个 crate 的所有依赖都已实现
# 注意：不包含 user 和 xtask
LINEAR_ORDER=(
    "ch1"
    "rcore-console"
    "easy-fs"
    "kernel-alloc"
    "kernel-context"
    "kernel-vm"
    "linker"
    "signal-defs"
    "task-manage"
    "ch1-lab"
    "signal"
    "syscall"
    "sync"
    "signal-impl"
    "ch2"
    "ch3"
    "ch4"
    "ch5"
    "ch6"
    "ch7"
    "ch8"
)

# 所有 crate 列表（用于生成禁止访问列表）
ALL_CRATES=("${LINEAR_ORDER[@]}")

PROMPTS_DIR="$WORKSPACE_ROOT/prompts"
SPEC_DIR="$WORKSPACE_ROOT/openspec/specs"

# 获取 crate 的实际目录名（用于查找 spec 和目录）
get_crate_dir() {
    local crate="$1"
    # console 目录对应 rcore-console crate
    if [ "$crate" = "rcore-console" ]; then
        echo "console"
    else
        echo "$crate"
    fi
}

# 解析依赖关系（从 Cargo.toml）
get_dependencies() {
    local crate="$1"
    local crate_dir=$(get_crate_dir "$crate")
    local cargo_toml="$WORKSPACE_ROOT/$crate_dir/Cargo.toml"
    
    if [ ! -f "$cargo_toml" ]; then
        return
    fi
    
    # 提取 path 依赖（workspace 内的 crate）
    grep -A 1 "path = " "$cargo_toml" | grep -oP '(?<=path = "\.\./)[^"]+' | sort -u || true
}

# 获取已实现的 crate 列表（用于限制访问）
# 对于一次性生成，我们假设所有 crate 都可能已实现
get_all_crates() {
    printf '%s\n' "${ALL_CRATES[@]}" | sort
}

# 检查 crate 是否需要集成测试（在特定章节验证）
# 返回: 如果需要集成测试，返回章节号；否则返回空
get_integration_test_chapter() {
    local crate="$1"
    case "$crate" in
        "kernel-context")
            echo "2"
            ;;
        "kernel-alloc"|"kernel-vm")
            echo "4"
            ;;
        "task-manage")
            echo "5"
            ;;
        "signal"|"signal-impl")
            echo "7"
            ;;
        *)
            echo ""
            ;;
    esac
}

# 获取某个章节需要验证的crate列表
get_crates_to_verify_in_chapter() {
    local chapter="$1"
    case "$chapter" in
        "2")
            echo "kernel-context"
            ;;
        "4")
            echo "kernel-alloc kernel-vm"
            ;;
        "5")
            echo "task-manage"
            ;;
        "7")
            echo "signal signal-impl"
            ;;
        *)
            echo ""
            ;;
    esac
}

# 生成单个 crate 的 prompt
generate_prompt_for_crate() {
    local crate_name="$1"
    local crate_dir=$(get_crate_dir "$crate_name")
    
    echo "📝 生成 prompt: $crate_name (目录: $crate_dir)"
    
    # 检查 spec 是否存在
    if [ ! -f "$SPEC_DIR/$crate_dir/spec.md" ]; then
        echo "  ⚠️  警告: spec 文件不存在: $SPEC_DIR/$crate_dir/spec.md，跳过"
        return 1
    fi
    
    # 获取当前 crate 的依赖
    local deps=$(get_dependencies "$crate_name")
    local all_crates=$(get_all_crates)
    
    # 创建 prompt 文件
    mkdir -p "$PROMPTS_DIR"
    local prompt_file="$PROMPTS_DIR/${crate_name}_implementation_prompt.md"
    
    cat > "$prompt_file" <<EOF
# 实现生成 Prompt: $crate_name

## 角色
你是 Rust OS crate 的实现者。

## 任务
从 OpenSpec spec 实现 crate \`$crate_name\`。

## 输入文件
- \`openspec/specs/$crate_dir/spec.md\`
EOF

    # 检查是否有 design.md
    if [ -f "$SPEC_DIR/$crate_dir/design.md" ]; then
        echo "- \`openspec/specs/$crate_dir/design.md\`" >> "$prompt_file"
    fi

    cat >> "$prompt_file" <<EOF
- \`$crate_dir/Cargo.toml\`

## 访问规则（重要！）

### 允许的访问
1. **当前 crate 的 spec**: 必须阅读 \`openspec/specs/$crate_dir/spec.md\` 和 design.md（如有）
2. **直接依赖的 spec（可选，需记录）**: 如果当前 crate 的 spec 不足以理解接口，可以阅读直接依赖的 spec：
EOF

    # 添加依赖的 specs
    if [ -n "$deps" ]; then
        for dep in $deps; do
            if [ -f "$SPEC_DIR/$dep/spec.md" ]; then
                echo "   - \`openspec/specs/$dep/spec.md\` (crate: \`$dep\`)" >> "$prompt_file"
            fi
        done
    else
        echo "   - 无直接依赖" >> "$prompt_file"
    fi
    
    cat >> "$prompt_file" <<EOF
   
   **重要**: 如果访问了直接依赖的 spec，必须在实现日志中说明原因。
EOF

    # 如果是章节，检查是否有需要验证的crate
    local crates_to_verify=""
    if [[ "$crate_name" =~ ^ch([1-8])(-lab)?$ ]]; then
        local chapter_num="${BASH_REMATCH[1]}"
        crates_to_verify=$(get_crates_to_verify_in_chapter "$chapter_num")
    fi
    
    # 如果有需要验证的crate，添加特殊访问规则
    if [ -n "$crates_to_verify" ]; then
        cat >> "$prompt_file" <<EOF

3. **需要验证的 crate 代码（集成测试阶段允许）**: 以下 crate 需要在当前章节的集成测试阶段才能验证正确性。
   在实现当前章节后，如果集成测试发现问题，可以访问和修改这些 crate 的代码以修复问题：
EOF
        for crate_to_verify in $crates_to_verify; do
            local verify_crate_dir=$(get_crate_dir "$crate_to_verify")
            cat >> "$prompt_file" <<EOF
   - \`$verify_crate_dir/\` (crate: \`$crate_to_verify\`): 允许访问和修改 \`$verify_crate_dir/src/\` 下的源代码文件
EOF
        done
        cat >> "$prompt_file" <<EOF
   
   **重要**: 如果访问和修改了这些 crate 的代码，必须在实现日志中详细记录：
   - 在集成测试中发现的具体问题
   - 访问和修改了哪些 crate 的哪些文件
   - 修改的原因和解决方案

4. **已生成代码和其余spec（下下策，需记录）**: 只有在测试失败且无法通过 spec 解决问题时，才能访问已实现的代码：
   
   **重要**: 如果访问了已生成的代码，必须在实现日志中详细说明：
   - 为什么需要访问（测试失败的具体原因）
   - 访问了哪些文件
   - 从中学到了什么
   - 为什么这是下下策
EOF
    else
        cat >> "$prompt_file" <<EOF

3. **已生成代码和其余spec（下下策，需记录）**: 只有在测试失败且无法通过 spec 解决问题时，才能访问已实现的代码：
   
   **重要**: 如果访问了已生成的代码，必须在实现日志中详细说明：
   - 为什么需要访问（测试失败的具体原因）
   - 访问了哪些文件
   - 从中学到了什么
   - 为什么这是下下策
EOF
    fi

    cat >> "$prompt_file" <<EOF

## 约束
EOF

    # 如果有需要验证的crate，添加特殊约束说明
    if [ -n "$crates_to_verify" ]; then
        cat >> "$prompt_file" <<EOF
- **集成测试验证**: 以下 crate 需要在当前章节的集成测试阶段才能验证正确性，如果发现问题可以修改：
EOF
        for crate_to_verify in $crates_to_verify; do
            local verify_crate_dir=$(get_crate_dir "$crate_to_verify")
            cat >> "$prompt_file" <<EOF
  - \`$verify_crate_dir/\` (crate: \`$crate_to_verify\`)
EOF
        done
    fi
    
    cat >> "$prompt_file" <<EOF
- **仅实现当前 crate**: 只修改 \`$crate_dir/\` 目录下的文件
- **优先使用当前 crate 的 spec**: 首先尝试仅通过当前 crate 的 spec 实现
- **谨慎使用直接依赖的 spec**: 只有在当前 spec 不足以理解接口时才使用，并在日志中说明
- **最后手段：查看已生成代码**: 只有在测试失败且无法通过 spec 解决问题时使用，必须在日志中详细说明
- 实现 spec 中定义的全部对外契约
- 保持 API 兼容
- 优先最小实现，但必须满足 spec 的行为与不变量
- 不新增非必要依赖
- 不修改其它 crate（除非为了解决编译错误且变化被 spec 允许；这种情况要先报告并请求调整 spec）

## Gate 要求
EOF

    # 判断是否为 ch1-ch8
    if [[ "$crate_name" =~ ^ch([1-8])(-lab)?$ ]]; then
        # ch1-ch8 使用 cargo qemu --ch X 测试
        local chapter_num="${BASH_REMATCH[1]}"
        echo "- \`cargo qemu --ch $chapter_num\` 必须通过" >> "$prompt_file"
        echo "- 访问 \`user/src/bin/\` 下的测试程序代码，验证输出是否符合预期" >> "$prompt_file"
    else
        # 其他 crate 需要验证
        echo "- \`cargo check\` 和 \`cargo test\` 必须通过" >> "$prompt_file"
    fi

    cat >> "$prompt_file" <<EOF

## 输出
只提交该 crate 目录下必要的 Rust 源码/配置（\`src/lib.rs\`/\`src/main.rs\`/必要模块/必要 build.rs 等）。

## 工作流程
1. 阅读 \`openspec/specs/$crate_dir/spec.md\` 和 design.md（如有）
2. 尝试仅基于当前 crate 的 spec 实现
3. 如果当前 spec 不足以理解接口，可以阅读直接依赖的 specs（**必须在日志中说明原因**）
4. 实现 crate（创建或修改 \`$crate_dir/src/lib.rs\` 或 \`$crate_dir/src/main.rs\`）
EOF

    # 判断是否为 ch1-ch8
    if [[ "$crate_name" =~ ^ch([1-8])(-lab)?$ ]]; then
        # ch1-ch8 使用 cargo qemu --ch X 测试
        local chapter_num="${BASH_REMATCH[1]}"
        cat >> "$prompt_file" <<EOF
5. 运行 gate 验证：\`cargo qemu --ch $chapter_num\`
EOF
        # 如果有需要验证的crate，添加验证步骤
        if [ -n "$crates_to_verify" ]; then
            cat >> "$prompt_file" <<EOF
6. **验证相关 crate 的正确性**: 如果集成测试发现问题，检查并修复以下 crate 的实现：
EOF
            for crate_to_verify in $crates_to_verify; do
                local verify_crate_dir=$(get_crate_dir "$crate_to_verify")
                cat >> "$prompt_file" <<EOF
   - \`$verify_crate_dir/\` (crate: \`$crate_to_verify\`): 
     * 访问 \`$verify_crate_dir/src/\` 下的源代码文件，检查实现是否正确
     * 如果发现问题，修改代码以修复集成测试问题
     * **必须在日志中记录**: 发现的问题、修改的文件、修改原因和解决方案
EOF
            done
            cat >> "$prompt_file" <<EOF
7. **验证输出**：访问 \`user/src/bin/\` 目录下的测试程序代码，检查 \`cargo qemu --ch $chapter_num\` 的输出是否符合预期
   - 查看 \`user/cases.toml\` 了解当前章节需要运行的测试用例
   - 阅读 \`user/src/bin/\` 下对应测试程序的源代码，理解预期的输出行为
   - 对比实际运行输出与预期输出，确保所有测试用例的输出都符合预期
8. 如果测试失败或输出不符合预期且无法通过 spec 解决，可以访问已生成的代码（**必须在日志中详细说明**）
9. 更新实现日志：\`implementation_logs/${crate_name}_implementation.log\`
EOF
        else
            cat >> "$prompt_file" <<EOF
6. **验证输出**：访问 \`user/src/bin/\` 目录下的测试程序代码，检查 \`cargo qemu --ch $chapter_num\` 的输出是否符合预期
   - 查看 \`user/cases.toml\` 了解当前章节需要运行的测试用例
   - 阅读 \`user/src/bin/\` 下对应测试程序的源代码，理解预期的输出行为
   - 对比实际运行输出与预期输出，确保所有测试用例的输出都符合预期
7. 如果测试失败或输出不符合预期且无法通过 spec 解决，可以访问已生成的代码（**必须在日志中详细说明**）
8. 更新实现日志：\`implementation_logs/${crate_name}_implementation.log\`
EOF
        fi

        cat >> "$prompt_file" <<EOF

## 验证命令
\`\`\`bash
# 使用 cargo qemu 进行验证
cargo qemu --ch $chapter_num
\`\`\`

## 输出验证
验证 \`cargo qemu --ch $chapter_num\` 的输出是否符合预期：
1. 查看 \`user/cases.toml\` 中 \`[ch$chapter_num]\` 部分，了解需要运行的测试用例列表
2. 访问 \`user/src/bin/\` 目录下对应的测试程序源代码（如 \`00hello_world.rs\`、\`02power.rs\` 等）
3. 理解每个测试程序的预期输出行为
4. 运行 \`cargo qemu --ch $chapter_num\` 并检查实际输出是否与预期一致
5. 确保所有测试用例的输出都正确，没有错误或异常行为
EOF
    else
        # 其他 crate 需要验证
        cat >> "$prompt_file" <<EOF
5. 运行 gate 验证：cd 到 \`$crate_dir\` 目录，执行 \`cargo check\` 和 \`cargo test\`
6. 如果测试失败且无法通过 spec 解决，可以访问已生成的代码（**必须在日志中详细说明**）
7. 更新实现日志：\`implementation_logs/${crate_name}_implementation.log\`

## 验证命令
\`\`\`bash
# cd 到对应文件夹进行验证
cd $crate_dir
cargo check
cargo test
\`\`\`
EOF
    fi

    cat >> "$prompt_file" <<EOF

---

## 实现日志

**必须维护实现日志**: \`implementation_logs/${crate_name}_implementation.log\`

日志应包含：
1. **实现开始时间**
2. **使用的资源**:
   - ✅ 当前 crate 的 spec
   - ⚠️  直接依赖的 spec（如果使用，说明原因）
EOF

    # 如果有需要验证的crate，添加日志说明
    if [ -n "$crates_to_verify" ]; then
        cat >> "$prompt_file" <<EOF
   - 🔧 需要验证的 crate 代码（如果访问和修改，需记录问题、修改的文件、原因和解决方案）:
EOF
        for crate_to_verify in $crates_to_verify; do
            cat >> "$prompt_file" <<EOF
     * \`$crate_to_verify\`
EOF
        done
    fi
    
    cat >> "$prompt_file" <<EOF
   - ❌ 已生成的代码（如果使用，详细说明原因、访问的文件、学到的内容）
3. **实现过程**: 关键决策和遇到的问题，尽量详细包含你每一次动作，如search第三方库，访问除输入外的代码，以及遇到什么具体报错和调试思路
4. **测试结果**: gate 验证是否通过，给出代码是否是仅经过一次生成就通过测试（这里指的是第一次运行测试命令除警告信息外没有报错信息，直接编译成功），如果不是记录你的修改流程
EOF

    # 如果有需要验证的crate，添加集成测试结果说明
    if [ -n "$crates_to_verify" ] && [[ "$crate_name" =~ ^ch([1-8])(-lab)?$ ]]; then
        local chapter_num="${BASH_REMATCH[1]}"
        cat >> "$prompt_file" <<EOF
5. **集成测试结果**: \`cargo qemu --ch $chapter_num\` 是否通过
   - 如果发现问题，记录发现的具体问题
   - 记录修改了哪些 crate 的哪些文件
   - 记录修改原因和解决方案
   - 记录修改后的验证结果
6. **实现完成时间**
7. **日志必须使用中文**
EOF
    else
        cat >> "$prompt_file" <<EOF
5. **实现完成时间**
6. **日志必须使用中文**
EOF
    fi

    cat >> "$prompt_file" <<EOF

## 开始实现

请根据上述 spec 实现 \`$crate_name\` crate。

**重要提醒**:
- 只修改 \`$crate_dir/\` 目录
- 优先使用当前 crate 的 spec
- 谨慎使用直接依赖的 spec，并在日志中说明
- 只有在测试失败时才访问已生成的代码，并在日志中详细说明
- 确保实现满足 spec 中的所有要求
- 维护实现日志
EOF

    echo "  ✅ 已生成: $prompt_file"
}

# 主流程
main() {
    echo "=========================================="
    echo "生成所有 crate 的实现 prompt"
    echo "=========================================="
    echo ""
    
    local total=0
    local success=0
    local failed=0
    
    # 按线性顺序生成
    echo ""
    echo "按依赖关系的线性顺序生成 prompt..."
    echo ""
    
    for crate in "${LINEAR_ORDER[@]}"; do
        total=$((total + 1))
        echo "[$total/${#LINEAR_ORDER[@]}] 生成 prompt: $crate"
        if generate_prompt_for_crate "$crate"; then
            success=$((success + 1))
        else
            failed=$((failed + 1))
        fi
    done
    
    echo ""
    echo "=========================================="
    echo "生成完成"
    echo "=========================================="
    echo "总计: $total"
    echo "成功: $success"
    echo "失败: $failed"
    echo ""
    echo "所有 prompt 文件已生成到: $PROMPTS_DIR"
    echo ""
    echo "使用方法:"
    echo "1. 在 Cursor 中打开对应的 prompt 文件"
    echo "2. 复制内容到 Cursor 对话"
    echo "3. 让 AI 模型根据 prompt 实现 crate"
}

main
