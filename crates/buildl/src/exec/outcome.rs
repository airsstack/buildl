//! How a run ends and how a strategy words its failures.
//!
//! Its own file because every strategy ends a run the same way and words an error the same way,
//! and neither may name another to share the two.
//!
//! Responsibilities: `ended` and `fault`.
//!
//! Non-responsibilities: running a process, which `capture` does, and choosing what to do about
//! an ending, which is the pipeline's.

use core::fmt;
use std::os::unix::process::ExitStatusExt;

use buildl_core::{Diagnostic, Error, Exit, Label, PortName};

use crate::exec::capture::Capture;

/// How a process ended: stopped for running too long, with a status of its own, or by a
/// signal.
///
/// Running too long is read from what the wait recorded, never from the signal that stopped
/// the process: anything can send that signal.
pub(super) fn ended(captured: &Capture) -> Exit {
    if captured.timed_out {
        return Exit::TimedOut;
    }
    captured.status.code().map_or_else(
        // Waiting for a process gives a status or a signal, never neither.
        || Exit::Signaled(captured.status.signal().unwrap_or_default()),
        Exit::Exited,
    )
}

/// The error for the action of `label`: what was being done, and why it could not be.
pub(super) fn fault(label: &Label, doing: &str, why: impl fmt::Display) -> Error {
    Error::Adapter {
        port: PortName::ExecStrategy,
        diagnostic: Diagnostic::new(format!("{label}: {doing}: {why}")),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    use buildl_core::{Exit, Label};

    use super::{ended, fault};
    use crate::exec::capture::Capture;

    /// A capture that ended with the raw wait status `raw`.
    fn capture(raw: i32, timed_out: bool) -> Capture {
        Capture {
            status: ExitStatus::from_raw(raw),
            timed_out,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn a_timed_out_capture_ends_timed_out_whatever_its_status() {
        // Exited 0, and ended by a kill: the wait's record decides, not the status.
        assert_eq!(ended(&capture(0, true)), Exit::TimedOut);
        assert_eq!(ended(&capture(9, true)), Exit::TimedOut);
    }

    #[test]
    fn an_exit_status_ends_exited() {
        // A wait status holds the exit code in its second byte.
        assert_eq!(ended(&capture(3 << 8, false)), Exit::Exited(3));
        assert_eq!(ended(&capture(0, false)), Exit::Exited(0));
    }

    #[test]
    fn a_signal_ends_signaled() {
        assert_eq!(ended(&capture(11, false)), Exit::Signaled(11));
    }

    #[test]
    fn a_fault_names_the_port_label_doing_and_why() {
        let label = Label::parse("//lib:probe").unwrap();

        let error = fault(&label, "doing", "why");

        assert_eq!(error.to_string(), "exec strategy: //lib:probe: doing: why");
    }
}
