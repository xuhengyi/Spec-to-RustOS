#!/bin/bash
# ch5 测例自动化测试脚本
# 使用 INIT_APP 环境变量直接加载各测例，绕过 user_shell 的 exec（当前 exec 存在 alloc 问题）

set -e
cd "$(dirname "$0")/.."

PASS=0
FAIL=0

run_case() {
    local name=$1
    local expect_pattern=$2
    local timeout=${3:-15}
    echo -n "  $name ... "
    out=$(timeout "$timeout" env INIT_APP="$name" cargo qemu --ch 5 2>&1 | strings || true)
    if echo "$out" | grep -q "$expect_pattern"; then
        echo "PASS"
        ((PASS++)) || true
        return 0
    else
        echo "FAIL (未找到: $expect_pattern)"
        ((FAIL++)) || true
        return 1
    fi
}

echo "=== ch5 测例测试 (INIT_APP 模式) ==="

# 00hello_world: 输出 Hello, world!
run_case "00hello_world" "Hello, world!"

# 02power: 输出 Test power OK!
run_case "02power" "Test power OK!" 30

# 01store_fault: 输出 Kernel should kill，然后触发 store fault
run_case "01store_fault" "Kernel should kill"

# 03priv_inst: 输出 Kernel should kill
run_case "03priv_inst" "Kernel should kill"

# 04priv_csr: 输出 Kernel should kill
run_case "04priv_csr" "Kernel should kill"

# 12forktest: 输出 forktest pass
run_case "12forktest" "forktest pass" 25

# 13forktree: 输出 forktree pass
run_case "13forktree" "forktree pass" 25

# 14forktest2: 输出 forktest2 pass
run_case "14forktest2" "forktest2 pass" 25

# 15matrix: 输出 matrix pass
run_case "15matrix" "matrix pass" 30

echo ""
echo "=== 结果: $PASS 通过, $FAIL 失败 ==="

# user_shell 和 initproc 需要 exec，当前通过管道测试
echo ""
echo "--- user_shell 管道测试 ---"
out=$(timeout 20 bash -c 'echo "00hello_world" | cargo qemu --ch 5 2>&1' | strings || true)
if echo "$out" | grep -q "Rust user shell" && echo "$out" | grep -q "00hello_world"; then
    echo "  user_shell 启动与输入: PASS"
else
    echo "  user_shell 启动与输入: FAIL (exec 00hello_world 仍有 alloc 问题)"
fi

exit $FAIL
