#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use kernel_context::LocalContext;
use signal::{Signal, SignalAction, SignalNo, SignalResult, MAX_SIG};

/// Bitset helper for pending/mask signal sets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SignalSet(pub usize);

impl SignalSet {
    #[inline]
    pub fn add_bit(&mut self, bit: usize) {
        if bit < usize::BITS as usize {
            self.0 |= 1usize << bit;
        }
    }

    #[inline]
    pub fn remove_bit(&mut self, bit: usize) {
        if bit < usize::BITS as usize {
            self.0 &= !(1usize << bit);
        }
    }

    #[inline]
    pub fn contain_bit(&self, bit: usize) -> bool {
        bit < usize::BITS as usize && (self.0 & (1usize << bit)) != 0
    }

    #[inline]
    pub fn union(self, rhs: SignalSet) -> SignalSet {
        SignalSet(self.0 | rhs.0)
    }

    #[inline]
    pub fn difference(self, rhs: SignalSet) -> SignalSet {
        SignalSet(self.0 & !rhs.0)
    }

    #[inline]
    pub fn find_first_one(&self, mask: SignalSet) -> Option<usize> {
        let deliverable = self.difference(mask).0;
        if deliverable == 0 {
            None
        } else {
            Some(deliverable.trailing_zeros() as usize)
        }
    }
}

/// In-progress signal handling state.
#[derive(Clone)]
pub enum HandlingSignal {
    /// Process is suspended by SIGSTOP and waiting for SIGCONT.
    Frozen,
    /// Process is running a user signal handler, with pre-handler context saved.
    UserSignal(LocalContext),
}

/// Per-process signal implementation.
pub struct SignalImpl {
    pub received: SignalSet,
    pub mask: SignalSet,
    pub handling: Option<HandlingSignal>,
    pub actions: [Option<SignalAction>; MAX_SIG + 1],
}

impl SignalImpl {
    #[inline]
    pub fn new() -> Self {
        Self {
            received: SignalSet(0),
            mask: SignalSet(0),
            handling: None,
            actions: [None; MAX_SIG + 1],
        }
    }

    #[inline]
    fn valid_index(signum: SignalNo) -> Option<usize> {
        let idx = signum as usize;
        if idx == 0 || idx > MAX_SIG {
            None
        } else {
            Some(idx)
        }
    }

    #[inline]
    fn kill_code(signum: SignalNo) -> i32 {
        -(signum as i32)
    }

    #[inline]
    fn should_ignore_by_default(signum: SignalNo) -> bool {
        matches!(signum, SignalNo::SIGCHLD | SignalNo::SIGURG | SignalNo::SIGCONT)
    }

    #[inline]
    fn take_deliverable_signal(&mut self) -> Option<SignalNo> {
        let bit = self.received.find_first_one(self.mask)?;
        self.received.remove_bit(bit);
        let signum = SignalNo::from(bit);
        if Self::valid_index(signum).is_some() {
            Some(signum)
        } else {
            None
        }
    }

    #[inline]
    fn handle_frozen(&mut self) -> SignalResult {
        let sigcont = SignalNo::SIGCONT as usize;
        if self.received.contain_bit(sigcont) && !self.mask.contain_bit(sigcont) {
            self.received.remove_bit(sigcont);
            self.handling = None;
            SignalResult::Handled
        } else {
            SignalResult::ProcessSuspended
        }
    }
}

impl Default for SignalImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl Signal for SignalImpl {
    fn from_fork(&mut self) -> Box<dyn Signal> {
        Box::new(Self {
            received: SignalSet(0),
            mask: self.mask,
            handling: None,
            actions: self.actions,
        })
    }

    fn clear(&mut self) {
        self.received = SignalSet(0);
        self.mask = SignalSet(0);
        self.handling = None;
        self.actions = [None; MAX_SIG + 1];
    }

    fn add_signal(&mut self, signal: SignalNo) {
        if let Some(idx) = Self::valid_index(signal) {
            self.received.add_bit(idx);
        }
    }

    fn is_handling_signal(&self) -> bool {
        self.handling.is_some()
    }

    fn set_action(&mut self, signum: SignalNo, action: &SignalAction) -> bool {
        if matches!(signum, SignalNo::SIGKILL | SignalNo::SIGSTOP) {
            return false;
        }
        let Some(idx) = Self::valid_index(signum) else {
            return false;
        };
        self.actions[idx] = Some(*action);
        true
    }

    fn get_action_ref(&self, signum: SignalNo) -> Option<SignalAction> {
        if matches!(signum, SignalNo::SIGKILL | SignalNo::SIGSTOP) {
            return None;
        }
        let idx = Self::valid_index(signum)?;
        Some(self.actions[idx].unwrap_or_default())
    }

    fn update_mask(&mut self, mask: usize) -> usize {
        let old = self.mask.0;
        self.mask = SignalSet(mask);
        old
    }

    fn handle_signals(&mut self, current_context: &mut LocalContext) -> SignalResult {
        let sigkill_idx = SignalNo::SIGKILL as usize;
        if self.received.contain_bit(sigkill_idx) && !self.mask.contain_bit(sigkill_idx) {
            self.received.remove_bit(sigkill_idx);
            return SignalResult::ProcessKilled(Self::kill_code(SignalNo::SIGKILL));
        }

        match self.handling.as_ref() {
            Some(HandlingSignal::Frozen) => return self.handle_frozen(),
            Some(HandlingSignal::UserSignal(_)) => return SignalResult::IsHandlingSignal,
            None => {}
        }

        let Some(signum) = self.take_deliverable_signal() else {
            return SignalResult::NoSignal;
        };

        match signum {
            SignalNo::SIGKILL => SignalResult::ProcessKilled(Self::kill_code(signum)),
            SignalNo::SIGSTOP => {
                self.handling = Some(HandlingSignal::Frozen);
                SignalResult::ProcessSuspended
            }
            _ => {
                let idx = signum as usize;
                let action = self.actions[idx].unwrap_or_default();
                if action.handler != 0 {
                    self.handling = Some(HandlingSignal::UserSignal(current_context.clone()));
                    *current_context.pc_mut() = action.handler;
                    *current_context.a_mut(0) = idx;
                    SignalResult::Handled
                } else if Self::should_ignore_by_default(signum) {
                    SignalResult::Ignored
                } else {
                    SignalResult::ProcessKilled(Self::kill_code(signum))
                }
            }
        }
    }

    fn sig_return(&mut self, current_context: &mut LocalContext) -> bool {
        let Some(handling) = self.handling.take() else {
            return false;
        };

        match handling {
            HandlingSignal::Frozen => {
                self.handling = Some(HandlingSignal::Frozen);
                false
            }
            HandlingSignal::UserSignal(saved_ctx) => {
                *current_context = saved_ctx;
                true
            }
        }
    }
}
