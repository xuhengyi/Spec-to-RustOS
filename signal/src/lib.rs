#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use kernel_context::LocalContext;

pub use signal_defs::{SignalAction, SignalNo, MAX_SIG};

/// Result of one signal-handling attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalResult {
    /// No deliverable signal is pending.
    NoSignal,
    /// A signal handler is already running and nested delivery is blocked.
    IsHandlingSignal,
    /// A signal was consumed and ignored.
    Ignored,
    /// A signal was handled and context is updated accordingly.
    Handled,
    /// Current process should be terminated with the provided exit code.
    ProcessKilled(i32),
    /// Current process should stay suspended.
    ProcessSuspended,
}

/// Abstract signal subsystem bound to one process/task.
pub trait Signal: Send + Sync {
    /// Clone signal state for a forked child.
    fn from_fork(&mut self) -> Box<dyn Signal>;

    /// Clear exec-discarded state.
    fn clear(&mut self);

    /// Add one pending signal.
    fn add_signal(&mut self, signal: SignalNo);

    /// Whether this process is currently handling a signal.
    fn is_handling_signal(&self) -> bool;

    /// Install action for a signal.
    fn set_action(&mut self, signum: SignalNo, action: &SignalAction) -> bool;

    /// Query action for a signal.
    fn get_action_ref(&self, signum: SignalNo) -> Option<SignalAction>;

    /// Replace signal mask and return old mask.
    fn update_mask(&mut self, mask: usize) -> usize;

    /// Try to handle one pending signal.
    fn handle_signals(&mut self, current_context: &mut LocalContext) -> SignalResult;

    /// Return from user signal handler.
    fn sig_return(&mut self, current_context: &mut LocalContext) -> bool;
}
