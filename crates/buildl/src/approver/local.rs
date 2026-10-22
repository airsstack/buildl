//! The approver a workspace names, whichever of the three that is.
//!
//! Its own file because the ports are generic over one approver type: the several ways to answer
//! have to meet in one. Each method does what its variant does and nothing besides.
//!
//! Responsibilities: [`LocalApprover`].
//!
//! Non-responsibilities: choosing the variant, which the mode in the manifest does where the
//! ports are assembled; and answering, which each variant does.

use buildl_core::{Approval, Approver, Result, WantedSet};

use crate::approver::console::Console;
use crate::approver::deny::DenyApprover;
use crate::approver::interactive::InteractiveApprover;
use crate::approver::manifest::ManifestApprover;

/// The approver a workspace runs its builds under.
#[derive(Debug)]
pub enum LocalApprover<C: Console> {
    /// Approves every build the ceiling allows.
    Manifest(ManifestApprover),
    /// Refuses every build.
    Deny(DenyApprover),
    /// Asks the person at the console about what is new.
    Interactive(InteractiveApprover<C>),
}

impl<C: Console> Approver for LocalApprover<C> {
    fn approve(&mut self, wanted: &WantedSet) -> Result<Approval> {
        match self {
            Self::Manifest(approver) => approver.approve(wanted),
            Self::Deny(approver) => approver.approve(wanted),
            Self::Interactive(approver) => approver.approve(wanted),
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use buildl_core::{Approval, Approver, Program, WantedSet};

    use super::LocalApprover;
    use crate::approver::console::Scripted;
    use crate::approver::data_dir::DataDir;
    use crate::approver::deny::DenyApprover;
    use crate::approver::interactive::InteractiveApprover;
    use crate::approver::manifest::ManifestApprover;

    #[test]
    fn each_mode_answers_as_its_approver() {
        let data = tempfile::tempdir().unwrap();
        let wanted = WantedSet {
            programs: BTreeSet::from([Program::parse("cc").unwrap()]),
            ..WantedSet::default()
        };
        let mut console = Scripted::answering(&["n"]);

        let mut manifest = LocalApprover::<Scripted>::Manifest(ManifestApprover);
        let mut deny = LocalApprover::<Scripted>::Deny(DenyApprover);
        let mut interactive = LocalApprover::Interactive(InteractiveApprover::new(
            &mut console,
            DataDir::At(data.path().to_path_buf()),
            PathBuf::from("/work/one"),
        ));

        assert_eq!(manifest.approve(&wanted).unwrap(), Approval::Granted);
        assert_eq!(
            deny.approve(&wanted).unwrap(),
            DenyApprover.approve(&wanted).unwrap()
        );
        assert!(matches!(
            interactive.approve(&wanted).unwrap(),
            Approval::Denied { .. }
        ));
        assert_eq!(console.prompts.len(), 1);
    }
}
