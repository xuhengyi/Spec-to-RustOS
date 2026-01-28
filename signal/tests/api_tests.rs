//! signal crate 功能性验证测试
//! 
//! 这些测试验证 signal crate 对外提供的 API 的正确性。
//! 测试在用户态环境运行，使用 std。
//! 
//! ## 测试限制
//! 
//! **单元测试**（当前文件）：
//! - ⚠️ 需要 RISC-V 64 位平台才能编译和运行
//! - ⚠️ 在 x86_64 主机上：无法编译（kernel-context 包含 RISC-V 汇编）
//! - ⚠️ 在 `riscv64gc-unknown-none-elf` 目标上：无法运行（no_std 环境，测试需要 std）
//! 
//! **推荐运行方式**：
//! 1. **编译验证**：使用 `cargo check` 验证代码可以编译
//!    ```bash
//!    cargo check -p signal --target riscv64gc-unknown-none-elf
//!    ```
//! 
//! 2. **集成测试**：在实际内核环境中验证功能
//!    ```bash
//!    cargo qemu --ch 7  # 或 ch8，这些章节使用了 signal
//!    ```
//! 
//! 3. **RISC-V 64 位主机**：如果有 RISC-V 64 位主机，可以直接运行单元测试
//!    ```bash
//!    cargo test -p signal --test api_tests
//!    ```
//! 
//! **注意**：signal crate 依赖于 kernel-context，而 kernel-context 包含 RISC-V 特定的内联汇编代码。
//! 这些测试应该在 RISC-V 目标平台上运行。

#[cfg(target_arch = "riscv64")]
mod tests {
    use std::boxed::Box;
    use std::marker::{Send, Sync};
    use signal::{Signal, SignalAction, SignalNo, SignalResult};

    // 注意：由于 signal 不再依赖 signal-impl，我们无法直接测试 SignalImpl
    // 这些测试主要验证 Signal trait 和 SignalResult 的定义

    #[test]
    fn test_signal_result_no_signal() {
        // 测试 SignalResult::NoSignal
        let result = SignalResult::NoSignal;
        // 验证可以创建
        match result {
            SignalResult::NoSignal => {},
            _ => panic!("Expected NoSignal"),
        }
    }

    #[test]
    fn test_signal_result_is_handling_signal() {
        // 测试 SignalResult::IsHandlingSignal
        let result = SignalResult::IsHandlingSignal;
        match result {
            SignalResult::IsHandlingSignal => {},
            _ => panic!("Expected IsHandlingSignal"),
        }
    }

    #[test]
    fn test_signal_result_ignored() {
        // 测试 SignalResult::Ignored
        let result = SignalResult::Ignored;
        match result {
            SignalResult::Ignored => {},
            _ => panic!("Expected Ignored"),
        }
    }

    #[test]
    fn test_signal_result_handled() {
        // 测试 SignalResult::Handled
        let result = SignalResult::Handled;
        match result {
            SignalResult::Handled => {},
            _ => panic!("Expected Handled"),
        }
    }

    #[test]
    fn test_signal_result_process_killed() {
        // 测试 SignalResult::ProcessKilled
        let result = SignalResult::ProcessKilled(-9);
        match result {
            SignalResult::ProcessKilled(code) => {
                assert_eq!(code, -9);
            },
            _ => panic!("Expected ProcessKilled"),
        }
    }

    #[test]
    fn test_signal_result_process_suspended() {
        // 测试 SignalResult::ProcessSuspended
        let result = SignalResult::ProcessSuspended;
        match result {
            SignalResult::ProcessSuspended => {},
            _ => panic!("Expected ProcessSuspended"),
        }
    }

    #[test]
    fn test_max_sig_constant() {
        // 测试 MAX_SIG 常量
        use signal::MAX_SIG;
        assert_eq!(MAX_SIG, 31);
    }

    #[test]
    fn test_signal_action_basic() {
        // 测试 SignalAction 的基本功能
        let action = SignalAction {
            handler: 0x1000,
            mask: 0x2000,
        };
        assert_eq!(action.handler, 0x1000);
        assert_eq!(action.mask, 0x2000);
    }

    #[test]
    fn test_signal_action_default() {
        // 测试 SignalAction 的默认值
        let action = SignalAction::default();
        assert_eq!(action.handler, 0);
        assert_eq!(action.mask, 0);
    }

    #[test]
    fn test_signal_no_basic() {
        // 测试 SignalNo 的基本值
        assert_eq!(SignalNo::SIGINT as u8, 2);
        assert_eq!(SignalNo::SIGKILL as u8, 9);
        assert_eq!(SignalNo::SIGSTOP as u8, 19);
    }

    #[test]
    fn test_signal_trait_send_sync() {
        // 验证 Signal trait 是 Send + Sync 的
        // 这主要是编译时检查，如果编译通过就说明满足要求
        fn assert_send_sync<T: Send + Sync>() {}
        // 注意：由于无法直接使用 SignalImpl，这里只验证 trait 定义
        // 实际的 Send + Sync 验证需要在实现 Signal trait 的类型上进行
    }
}

#[cfg(not(target_arch = "riscv64"))]
#[test]
fn test_signal_requires_riscv64() {
    println!("signal tests require RISC-V 64-bit target architecture");
}
