//! The `buildl` binary — the command-line entry point to the `buildl` library.
//!
//! All build logic lives in the library crates; this binary depends on the `buildl` crate
//! alone and reaches core types through its re-export. It reads a command line, runs the
//! command on the workspace at or above the directory it was started in, and ends with a status
//! that says whose mistake a failure was.
//!
//! # Commands
//!
//! | Command | Writes to standard output | Writes to standard error |
//! |---|---|---|
//! | `buildl check` | nothing | `ok: 7 declarations in 2 build files` |
//! | `buildl graph [--json \| --dot]` | the target graph: canonical JSON (the default) or Graphviz DOT | nothing, or `//svc:pkgs has not run; run buildl build` for each discovery target that has not run |
//! | `buildl query <EXPR> [--json]` | the targets the expression reaches, one per line, or as canonical JSON | nothing, or the same line for each discovery target that has not run |
//! | `buildl plan [TARGET]... [--set NAME=VALUE]... [--json]` | what a build would run and why, or the plan as canonical JSON | nothing |
//! | `buildl build [TARGET]... [--set NAME=VALUE]... [-k \| --keep-going] [--stream] [--remote-upload] [--remote-strict] [--no-remote]` | nothing | a block for each action that printed or failed, and a summary |
//! | `buildl watch [TARGET]... [--set NAME=VALUE]... [-k \| --keep-going] [--stream] [--remote-upload] [--remote-strict] [--no-remote]` | nothing | what `build` writes for each cycle, and a `watch:` line for each cycle's start and each wait |
//! | `buildl test [TARGET]... [--set NAME=VALUE]... [--stop] [--stream] [--rerun] [--verbose] [--remote-upload] [--remote-strict] [--no-remote]` | nothing | a block for each failing test (and each test that printed, with `--verbose`) and for each other target that printed or failed, and a line counting the tests |
//! | `buildl verify [TARGET]... [--set NAME=VALUE]... [--sample N]` | nothing | a line for each checked target that was verified, mismatched, failed, skipped or not put back, and a line counting them and the targets left out |
//! | `buildl doctor` | the workspace root, its approver, its run settings, its remote cache (`remote: none`, or its settings, whether its token is set, and the upload and strict variables as this machine sets them; it connects to nothing), its ceiling, and one row per want saying whether the ceiling allows it and whether it is approved, with the plugins whose targets want it under each row, and the wants a plugin does not allow under `plugin violations` | nothing, or the same line for each discovery target that has not run |
//! | `buildl log <TARGET> [--stdout \| --stderr]` | the stored stdout, or the one stream asked for | the header, a warning of damaged lines, and the stored stderr when no flag is given |
//! | `buildl clean` | nothing | `clean: removed out` (naming the manifest's `out` directory), or `clean: out holds nothing` |
//! | `buildl gc` | nothing | `gc: dropped 1 row, removed 5 blobs (2048 bytes)` |
//! | `buildl plugins fetch` | nothing | a line for each git plugin, by name: `fetched NAME COMMIT` or `unchanged NAME COMMIT` |
//!
//! A target is written `//dir:name`, or `:name` and `name` for one in the directory the command
//! was started in; naming none means every target but tests for `plan` and `build`, and every
//! test for `test`, and `log` takes exactly one. `--set` gives a build setting a value for
//! one run and may be repeated, once per setting. `--keep-going` builds everything a failure
//! does not stop, and `--stream` shows one running action's output as it is printed. `--stop`
//! starts nothing new after the first failure. `--rerun` runs the selected tests even when
//! nothing they depend on changed. `--verbose` shows what every test printed, not only what a
//! failing one did. In a workspace whose `buildl.toml` has a `[remote]` table, `build` and `test`
//! look results up in that remote cache; `--remote-upload` also sends what the run produced
//! (`BUILDL_REMOTE_UPLOAD=1` does the same for every run on a machine), `--remote-strict` fails
//! the run when a call to the cache fails instead of warning and building locally
//! (`BUILDL_REMOTE_STRICT=1`), and `--no-remote` leaves the cache out of one run, whatever the
//! variables say; it cannot be combined with the other two. `--sample N`, with `N` at least one, makes `verify` re-run only the `N` cached
//! actions whose key digests are lowest, the same ones every time; `verify` runs under
//! `strategy = "sandbox"` only and records nothing. Every command takes
//! `--color auto|always|never`: under `auto`, what is printed is coloured only when standard
//! error is a terminal whose `TERM` is set and not `dumb`, and `NO_COLOR` is unset or empty.
//!
//! `watch` takes the flags of `build`. It builds, then builds again when a file the build reads
//! changes, or when the build files declare a different graph, reading the workspace anew each
//! cycle; changes under `out`, the state directory and `.git` are ignored. A cycle that fails is
//! reported and followed by the next change. It ends on Ctrl-C, with 2 for a command line that is
//! wrong, or with 3 when the file watcher fails.
//!
//! An expression is `deps(TARGET)`, every target TARGET depends on, or `rdeps(TARGET)`, every
//! target that depends on TARGET. Either reaches any number of steps away, and TARGET itself is
//! not in the answer. `query`, like `check` and `graph`, takes no lock and writes nothing.
//!
//! `doctor` evaluates every build file once (twice when a discovery target is declared), takes no
//! lock, writes nothing and asks nothing. It reports the wants of every target at the settings'
//! defaults, and ends with 0 even when a want is outside the ceiling or not approved; what stops
//! it from making the report ends with the status that mistake has everywhere: 5 for a build
//! file or an output two targets claim, 6 for a remembered approvals file that is not what buildl
//! writes, and 3 when that file cannot be read or there is no data directory to find it in.
//!
//! `plugins fetch` pins every git plugin `buildl.toml` names in `buildl.lock` and stores its files
//! under the state directory, so that a build afterwards needs no network. It holds the state
//! directory while it runs, reads no build file and asks no approver. It runs the `git` on the
//! `PATH`, and only for a plugin that has to be fetched; a `git` that is missing or refuses ends
//! with 3. A plugin that fetches as other files than the lock pins, as when its tag has
//! moved, ends with 6, and one whose files hold a link with 5.
//!
//! `log` takes no lock and loads no build file, and writes nothing but the store's own removal of
//! a blob that fails its digest; it does not follow an alias.
//!
//! # Exit status
//!
//! | Status | Means |
//! |---|---|
//! | 0 | done |
//! | 1 | an action failed: fix the code being built |
//! | 2 | the command line is wrong |
//! | 3 | the builder or the machine failed: fix it, or run again; a `git` that is missing or fails is one |
//! | 4 | the build wants more than `buildl.toml`'s ceiling or a plugin's caps allow, or than was approved |
//! | 5 | a build file or `buildl.toml` is wrong, a plugin is not fetched, or the discovery targets changed between the two passes |
//! | 6 | a file in `.buildl/` (a fetched plugin's included), or the remembered approvals file, is not what buildl writes |
//! | 7 | buildl itself is at fault |
//!
//! Under `--keep-going`, which `test` has unless `--stop` is given, the most severe status wins:
//! 7, then 3, then 1. `verify` ends with the most severe status of its checks as well: 1 for a
//! mismatch, the status a build would give for a re-run that failed, and 3 for a target that was
//! skipped or whose outputs could not be put back. It ends with 0 when it found nothing wrong,
//! and so when there was nothing cached to check.
//!
//! # Platforms
//!
//! The binary builds and is tested on Linux and macOS.
//!
//! This file holds the module list and `main`, which reads what only the process knows and
//! hands it on; it decides nothing, so it carries no logic to unit-test.

use std::env;
use std::io::{self, IsTerminal};
use std::process::ExitCode;

mod cli;
mod console;
mod doctor_text;
mod exit;
mod remote_switches;
mod run;
mod terminal;

fn main() -> ExitCode {
    let terminal = terminal::Terminal {
        interactive: io::stderr().is_terminal(),
        term: env::var_os("TERM"),
        no_color: env::var_os("NO_COLOR"),
    };
    let remote = remote_switches::RemoteSwitches {
        upload: env::var_os("BUILDL_REMOTE_UPLOAD"),
        strict: env::var_os("BUILDL_REMOTE_STRICT"),
        execute: env::var_os("BUILDL_REMOTE_EXECUTE"),
    };
    let exit = run::run(
        env::args_os(),
        env::current_dir(),
        &terminal,
        &remote,
        &mut io::stdout(),
        &mut io::stderr(),
        &mut console::StdConsole,
    );
    ExitCode::from(exit as u8)
}
