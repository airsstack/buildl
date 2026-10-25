//! Whether a failed call has turned the remote cache off for the rest of a command.
//!
//! Its own file because the answer is shared by every worker of a run, and by both runs of a
//! command that builds its discovery targets first, and the step that turns the remote off must
//! be one indivisible operation: two workers failing at the same moment must not both be the one
//! that did it, or the build would warn twice.
//!
//! Responsibilities: [`RemoteSwitch`], which says whether the remote is off and turns it off
//! exactly once.
//!
//! Non-responsibilities: deciding that a call failed, and saying so to the build, which the
//! worker that met the failure does.

use core::sync::atomic::{AtomicBool, Ordering};

/// Whether the remote cache has been turned off, shared by the workers of a command.
///
/// It starts on. [`RemoteSwitch::turn_off`] is the only way to change it, and it never turns the
/// remote on again.
#[derive(Debug, Default)]
pub struct RemoteSwitch(AtomicBool);

impl RemoteSwitch {
    /// A switch with the remote on.
    #[must_use]
    pub const fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    /// Whether a failed call has turned the remote off.
    #[must_use]
    pub fn is_off(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// Turns the remote off, and returns `true` only to the one caller whose call turned it off.
    ///
    /// A caller that finds it already off, however long ago or however close behind another
    /// caller, gets `false`.
    pub fn turn_off(&self) -> bool {
        self.0
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::RemoteSwitch;

    #[test]
    fn a_new_switch_has_the_remote_on() {
        assert!(!RemoteSwitch::new().is_off());
        assert!(!RemoteSwitch::default().is_off());
    }

    #[test]
    fn only_the_first_call_to_turn_it_off_is_told_so() {
        let switch = RemoteSwitch::new();

        assert!(switch.turn_off(), "the call that turned it off");
        assert!(switch.is_off());
        assert!(!switch.turn_off(), "a later call finds it off");
        assert!(switch.is_off(), "and does not turn it on");
    }
}
