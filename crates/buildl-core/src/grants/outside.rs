//! The wants a workspace's ceiling does not allow.
//!
//! Its own file because it reports rather than refuses: the plan stops at the first want
//! outside the ceiling, while a report of a workspace's authority lists every one of them.
//!
//! Responsibilities: [`outside`].
//!
//! Non-responsibilities: refusing a build, which the plan's check does; and collecting the
//! wants, which [`wanted`](super::wanted) does.

use std::collections::BTreeSet;

use crate::types::{WantedSet, Workspace};

/// The part of `wanted` that `workspace`'s ceiling does not allow.
///
/// A program is outside when the ceiling does not name it to run, a variable when the ceiling
/// does not name it to read, and a directory when no directory the ceiling allows writing to
/// holds it. The network is never outside: a ceiling has no entry for it.
pub(crate) fn outside(wanted: &WantedSet, workspace: &Workspace) -> WantedSet {
    let ceiling = &workspace.ceiling;
    WantedSet {
        programs: wanted
            .programs
            .iter()
            .filter(|program| !ceiling.run.contains(*program))
            .cloned()
            .collect(),
        env: wanted
            .env
            .iter()
            .filter(|name| !ceiling.env.contains(*name))
            .cloned()
            .collect(),
        write: wanted
            .write
            .iter()
            .filter(|directory| {
                !ceiling
                    .write
                    .iter()
                    .any(|allowed| allowed.contains(directory))
            })
            .cloned()
            .collect(),
        network: BTreeSet::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::outside;
    use crate::grants::fixtures::{dir, env_name, label, program};
    use crate::types::{ApproverMode, Ceiling, Execution, WantedSet, Workspace};

    /// A workspace whose ceiling allows `cc`, `PATH`, reads under `src` and writes under `out`
    /// and `.buildl`.
    fn workspace() -> Workspace {
        Workspace {
            out: dir("out"),
            state: dir(".buildl"),
            ceiling: Ceiling {
                run: BTreeSet::from([program("cc")]),
                env: BTreeSet::from([env_name("PATH")]),
                read: BTreeSet::from([dir("src")]),
                write: BTreeSet::from([dir("out"), dir(".buildl")]),
            },
            execution: Execution::default(),
            approver: ApproverMode::Manifest,
            plugins: std::collections::BTreeMap::new(),
        }
    }

    /// A set wanting `cc`, `PATH`, `out` and `.buildl`: all inside [`workspace`]'s ceiling.
    fn inside() -> WantedSet {
        WantedSet {
            programs: BTreeSet::from([program("cc")]),
            env: BTreeSet::from([env_name("PATH")]),
            write: BTreeSet::from([dir("out"), dir(".buildl")]),
            network: BTreeSet::new(),
        }
    }

    #[test]
    fn a_program_outside_proc_run_is_outside() {
        let mut wanted = inside();
        wanted.programs.insert(program("ld"));
        assert_eq!(
            outside(&wanted, &workspace()).programs,
            BTreeSet::from([program("ld")])
        );
    }

    #[test]
    fn a_variable_outside_env_read_is_outside() {
        let mut wanted = inside();
        wanted.env.insert(env_name("HOME"));
        assert_eq!(
            outside(&wanted, &workspace()).env,
            BTreeSet::from([env_name("HOME")])
        );
    }

    #[test]
    fn a_write_directory_outside_fs_write_is_outside() {
        let mut wanted = inside();
        wanted.write.insert(dir("build"));
        wanted.write.insert(dir("out/sub"));
        assert_eq!(
            outside(&wanted, &workspace()).write,
            BTreeSet::from([dir("build")])
        );
    }

    #[test]
    fn the_network_is_never_outside() {
        let mut wanted = inside();
        wanted.network.insert(label("//:fetch"));
        assert!(outside(&wanted, &workspace()).network.is_empty());
    }

    #[test]
    fn a_set_inside_the_ceiling_has_nothing_outside() {
        assert!(outside(&inside(), &workspace()).is_empty());
    }
}
