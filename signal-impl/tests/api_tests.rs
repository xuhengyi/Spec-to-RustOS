//! signal-impl crate 功能性验证测试
//! 
//! 这些测试验证 signal-impl crate 对外提供的 API 的正确性。
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
//!    cargo check -p signal-impl --target riscv64gc-unknown-none-elf
//!    ```
//! 
//! 2. **集成测试**：在实际内核环境中验证功能
//!    ```bash
//!    cargo qemu --ch 7  # 或 ch8，这些章节使用了 signal
//!    ```
//! 
//! 3. **RISC-V 64 位主机**：如果有 RISC-V 64 位主机，可以直接运行单元测试
//!    ```bash
//!    cargo test -p signal-impl --test api_tests
//!    ```
//! 
//! **注意**：signal-impl crate 依赖于 kernel-context，而 kernel-context 包含 RISC-V 特定的内联汇编代码。
//! 这些测试应该在 RISC-V 目标平台上运行。

#[cfg(target_arch = "riscv64")]
mod tests {
    use signal_impl::*;
    use signal::{Signal, SignalAction, SignalNo, SignalResult, MAX_SIG};

    #[test]
    fn test_signal_impl_new() {
        // 测试 SignalImpl::new()
        let sig_impl = SignalImpl::new();
        assert_eq!(sig_impl.received.0, 0);
        assert_eq!(sig_impl.mask.0, 0);
        assert!(sig_impl.handling.is_none());
        assert_eq!(sig_impl.actions.len(), MAX_SIG + 1);
    }

    #[test]
    fn test_signal_set_via_signal_impl() {
        // 测试 SignalSet 的功能（通过 SignalImpl 的字段访问）
        let mut sig_impl = SignalImpl::new();
        
        // 测试 received 字段（SignalSet）
        assert_eq!(sig_impl.received.0, 0);
        sig_impl.received.add_bit(1);
        assert!(sig_impl.received.contain_bit(1));
        assert_eq!(sig_impl.received.0, 0b10);
        
        // 测试 mask 字段（SignalSet）
        assert_eq!(sig_impl.mask.0, 0);
        sig_impl.mask.add_bit(2);
        assert!(sig_impl.mask.contain_bit(2));
        assert_eq!(sig_impl.mask.0, 0b100);
    }

    #[test]
    fn test_signal_impl_add_signal() {
        // 测试 SignalImpl::add_signal()
        let mut sig_impl = SignalImpl::new();
        sig_impl.add_signal(SignalNo::SIGINT);
        assert!(sig_impl.received.contain_bit(SignalNo::SIGINT as usize));
    }

    #[test]
    fn test_signal_impl_is_handling_signal() {
        // 测试 SignalImpl::is_handling_signal()
        let sig_impl = SignalImpl::new();
        assert!(!sig_impl.is_handling_signal());
    }

    #[test]
    fn test_signal_impl_set_action() {
        // 测试 SignalImpl::set_action()
        let mut sig_impl = SignalImpl::new();
        let action = SignalAction {
            handler: 0x1000,
            mask: 0x2000,
        };
        
        // 测试设置普通信号
        assert!(sig_impl.set_action(SignalNo::SIGINT, &action));
        assert_eq!(sig_impl.actions[SignalNo::SIGINT as usize], Some(action));
        
        // 测试设置 SIGKILL 应该失败
        assert!(!sig_impl.set_action(SignalNo::SIGKILL, &action));
        
        // 测试设置 SIGSTOP 应该失败
        assert!(!sig_impl.set_action(SignalNo::SIGSTOP, &action));
    }

    #[test]
    fn test_signal_impl_get_action_ref() {
        // 测试 SignalImpl::get_action_ref()
        let mut sig_impl = SignalImpl::new();
        let action = SignalAction {
            handler: 0x1000,
            mask: 0x2000,
        };
        
        // 测试获取未设置的信号（应该返回默认值）
        let default_action = sig_impl.get_action_ref(SignalNo::SIGINT);
        assert_eq!(default_action, Some(SignalAction::default()));
        
        // 测试设置后获取
        sig_impl.set_action(SignalNo::SIGINT, &action);
        let retrieved_action = sig_impl.get_action_ref(SignalNo::SIGINT);
        assert_eq!(retrieved_action, Some(action));
        
        // 测试获取 SIGKILL 应该返回 None
        assert_eq!(sig_impl.get_action_ref(SignalNo::SIGKILL), None);
        
        // 测试获取 SIGSTOP 应该返回 None
        assert_eq!(sig_impl.get_action_ref(SignalNo::SIGSTOP), None);
    }

    #[test]
    fn test_signal_impl_update_mask() {
        // 测试 SignalImpl::update_mask()
        let mut sig_impl = SignalImpl::new();
        let old_mask = sig_impl.update_mask(0x1234);
        assert_eq!(old_mask, 0);
        assert_eq!(sig_impl.mask.0, 0x1234);
        
        let old_mask2 = sig_impl.update_mask(0x5678);
        assert_eq!(old_mask2, 0x1234);
        assert_eq!(sig_impl.mask.0, 0x5678);
    }

    #[test]
    fn test_signal_impl_clear() {
        // 测试 SignalImpl::clear()
        let mut sig_impl = SignalImpl::new();
        let action = SignalAction {
            handler: 0x1000,
            mask: 0x2000,
        };
        sig_impl.set_action(SignalNo::SIGINT, &action);
        sig_impl.clear();
        
        // 验证所有 actions 都被清空
        for action in &sig_impl.actions {
            assert!(action.is_none());
        }
    }

    #[test]
    fn test_signal_impl_from_fork() {
        // 测试 SignalImpl::from_fork()
        let mut sig_impl = SignalImpl::new();
        let action = SignalAction {
            handler: 0x1000,
            mask: 0x2000,
        };
        sig_impl.set_action(SignalNo::SIGINT, &action);
        sig_impl.update_mask(0x1234);
        
        let new_sig_impl = sig_impl.from_fork();
        
        // 验证新实例继承了 mask 和 actions
        assert_eq!(new_sig_impl.mask.0, 0x1234);
        assert_eq!(new_sig_impl.get_action_ref(SignalNo::SIGINT), Some(action));
        
        // 验证新实例的 received 是空的
        assert_eq!(new_sig_impl.received.0, 0);
        
        // 验证新实例的 handling 是 None
        assert!(!new_sig_impl.is_handling_signal());
    }

    #[test]
    fn test_signal_result_variants() {
        // 测试 SignalResult 枚举的所有变体
        let _no_signal = SignalResult::NoSignal;
        let _is_handling = SignalResult::IsHandlingSignal;
        let _ignored = SignalResult::Ignored;
        let _handled = SignalResult::Handled;
        let _killed = SignalResult::ProcessKilled(-9);
        let _suspended = SignalResult::ProcessSuspended;
    }
}
