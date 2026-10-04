---
status: done
created: 2026-09-18
depends-on: [01]
---

# `buildl-core` Purity Bans Implementation Plan

**Goal:** Make `buildl-core` fail the gate when it names a filesystem, process, thread,
environment, standard-stream or clock API.

**Architecture:** A crate-local `crates/buildl-core/clippy.toml` enumerates every forbidden item
under clippy's `disallowed-methods`, `disallowed-types` and `disallowed-macros`, each with an
inline `reason` that clippy renders as a `= note:` beneath the offending span. Those three lints
are on by default and need no entry in the workspace lint table. A crate-local `clippy.toml`
scopes to its own crate and **shadows** a workspace-root one rather than merging with it; no root
`clippy.toml` exists today. Because an unresolvable ban entry only *warns* — and still exits 0
under `-D warnings` — a cargo-make wrapper task, `guard-core-purity`, runs clippy and then
asserts four further things: the config exists, no diagnostic points into it, `allow-invalid` is
absent, and no source file suppresses a `disallowed_*` lint. The task attaches as a dependency of
the existing `clippy` task, so `dod` keeps five named steps.

**Tech Stack:** clippy 0.1.98 (rustc 1.98.1), cargo-make 0.37.24, POSIX shell.

---

## Context an implementer needs

`docs/architecture-building-blocks.md` §1.2 states `buildl-core`'s dependency allowance as "`std`,
excluding its I/O, process, thread, environment, and clock APIs", and `docs/architecture.md` §5
rule 4 quarantines the wall clock behind the `Clock` port. Neither is enforced today:
`std::time::SystemTime::now()` compiles inside `buildl-core` right now. The crate boundary
constrains *other crates'* APIs; `std` is in scope for every crate.

`#![no_std]` would be exhaustive and was considered and declined in spec §3.1: it taxes every line
of a crate that is purely host-side, and it contradicts the sibling chain's constraint that
`buildl-core` may use `std` minus the named families. The enumerated ban list is a tripwire, not a
proof — a `std` API stabilised after the list is written is not covered until someone adds it.

**This plan depends on `01-crate-edge-guards` being `done`,** because both plans edit
`[tasks.clippy]`'s `dependencies` list in `Makefile.toml`. Plan 01 introduces that key with
`["guard-crate-edges"]`; task 3 below extends it.

All work happens in the git worktree, on a branch, never on `main`. Commits follow Conventional
Commits: `type(scope): summary`, scope `repo`. One commit per task.

### File map

```
crates/buildl-core/clippy.toml  — [create] the 128 ban entries (task 1)
Makefile.toml                   — [modify] new `[tasks.guard-core-purity]` (task 2);
                                  extend `[tasks.clippy]`'s `dependencies` (task 3)
```

### Where the ban list came from

Most of it is derived mechanically from the `rust-src` component of the pinned stable toolchain,
not transcribed. That component is not installed by default — `rust-toolchain.toml` sets
`profile = "minimal"` with `components = ["rustfmt", "clippy"]` — so reproducing the derivation
needs `rustup component add rust-src` first.

```bash
STD="$(rustc --print sysroot)/lib/rustlib/src/rust/library/std/src"
grep -oE '^pub (unsafe )?fn [a-z_]+'  "$STD"/fs.rs "$STD"/env.rs "$STD"/process.rs
grep -oE '^pub struct [A-Za-z]+'      "$STD"/fs.rs "$STD"/env.rs "$STD"/process.rs "$STD"/time.rs
grep -hoE '^pub fn [a-z_]+'           "$STD"/thread/functions.rs "$STD"/thread/current.rs "$STD"/thread/scoped.rs
grep -hoE '^pub struct [A-Za-z]+'     "$STD"/thread/builder.rs "$STD"/thread/join_handle.rs \
                                      "$STD"/thread/scoped.rs "$STD"/thread/thread.rs "$STD"/thread/local.rs
grep -oE '^pub fn (stdin|stdout|stderr)' "$STD"/io/stdio.rs
grep -oE '    pub fn (exists|try_exists|is_file|is_dir|is_symlink|metadata|symlink_metadata|canonicalize|read_dir)' "$STD"/path.rs
grep -oE '^pub (unsafe )?fn [a-z_]+'  "$STD"/os/unix/fs.rs
grep -oE '^pub trait [A-Za-z]+'       "$STD"/os/unix/fs.rs
```

Counts as derived on rustc 1.98.1, each confirmed against the command above:

| Family | Methods | Types | Derived by |
|---|---|---|---|
| `std::fs` | 22 | 10 | grep |
| `std::env` | 15 | 6 | grep — `set_var`/`remove_var` are `unsafe fn` in edition 2024 and already unreachable under `unsafe_code = "forbid"`; banned anyway |
| `std::process` | 3 | 12 | grep |
| `std::thread` | 13 | 7 | grep |
| `std::time` | — | 3 | grep — `Instant`, `SystemTime`, `SystemTimeError`. `Duration` is `core` and stays allowed |
| `std::io` | 3 | 3 | grep — `stdin`/`stdout`/`stderr` and their handle types |
| `std::path` | 9 | — | grep — `Path`'s own inherent methods that hit the filesystem. `Path` and `PathBuf` themselves stay allowed: they are string manipulation and `buildl-core` needs them |
| `std::net` | — | 3 | hand-picked — `TcpListener`, `TcpStream`, `UdpSocket` |
| `std::os::unix::fs` | 6 | 7 traits | grep |
| macros | 6 | — | hand-picked — `env!`, `option_env!`, `print!`, `println!`, `eprint!`, `eprintln!` |
| **total** | **71** | **51 + 6 macros** | **128 entries** |

Two families the `std::fs`/`std::io` greps do not reach are included because they are reachable
**today**, not future drift: `Path::exists` and its eight siblings touch the filesystem through a
type the crate legitimately uses, and the printing macros write to the handles this list bans
without naming them — `println!` expands to a private path, so a ban on `std::io::stdout` does not
catch it.

`include_str!` and `include_bytes!` are deliberately **not** banned: they read files at compile
time, which is deterministic and visible in the source.

**Every one of the 128 entries resolves on rustc 1.98.1** — verified by running clippy over the
finished file and getting zero diagnostics, with a control probe (an entry naming
`std::fs::definitely_not_real`) proving the config was actually read. No entry needs pruning. Spec
§3.2 anticipated that some unstable or deprecated names would fail to resolve; on this toolchain
none do, including `sleep_ms`, `park_timeout_ms`, `sleep_until`, `set_times_nofollow` and
`fs::Dir`.

---

## Task 1 — Write the ban list

**Files:**
- Create `crates/buildl-core/clippy.toml`

**Steps:**

1. Reproduce the gap first. Create `crates/buildl-core/src/probe.rs`:

   ```rust
   pub fn probe_clock() -> std::time::SystemTime { std::time::SystemTime::now() }
   pub fn probe_print() { println!("x"); }
   pub fn probe_path(p: &std::path::Path) -> bool { p.exists() }
   ```

   and append `pub mod probe;` to `crates/buildl-core/src/lib.rs`. Then:

   ```
   $ cargo clippy -p buildl-core --all-targets --all-features -- -D warnings 2>&1 | grep -c disallowed
   0
   ```

   Nothing objects to the clock, the printing or the filesystem probe. (Unrelated
   `missing-docs` and `must-use-candidate` errors will also appear — those come from the existing
   workspace lint table, not from this plan.) Leave the probe in place for step 3.

2. Create `crates/buildl-core/clippy.toml` with exactly this content:

   ```toml
   #
   # Purity bans for buildl-core.
   #
   # architecture-building-blocks.md §1.2: this crate may use `std`, excluding its
   # I/O, process, thread, environment and clock APIs. The crate boundary does not
   # enforce that — `std` is in scope for every crate — so the rule is enumerated
   # here and asserted by `cargo make guard-core-purity`.
   #
   # A crate-local clippy.toml SHADOWS a workspace-root one rather than merging
   # with it. There is no root clippy.toml today; if one is ever added, this crate
   # will not see it.
   #
   # There is no escape hatch. `#[allow]`/`#[expect]` naming a `disallowed_*` lint
   # and `allow-invalid = true` are both rejected by the guard task. The documented
   # path for a legitimate need is to move the call into an adapter behind a port,
   # or — failing that — to delete the entry, amend §1.2 and add a decision record
   # to architecture.md §6.
   #
   # Derivation and per-family counts: the chain's spec §3.2 and plan 02.
   #
   disallowed-methods = [
       { path = "std::fs::read", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::read_to_string", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::write", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::set_times", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::set_times_nofollow", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::remove_file", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::metadata", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::symlink_metadata", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::rename", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::copy", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::hard_link", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::soft_link", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::read_link", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::canonicalize", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::create_dir", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::create_dir_all", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::remove_dir", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::remove_dir_all", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::read_dir", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::set_permissions", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::set_permissions_nofollow", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::exists", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::env::current_dir", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::set_current_dir", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::vars", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::vars_os", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::var", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::var_os", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::set_var", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::remove_var", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::split_paths", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::join_paths", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::home_dir", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::temp_dir", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::current_exe", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::args", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::args_os", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::process::exit", reason = "running a process is the Exec port's; buildl-core only describes actions" },
       { path = "std::process::abort", reason = "running a process is the Exec port's; buildl-core only describes actions" },
       { path = "std::process::id", reason = "running a process is the Exec port's; buildl-core only describes actions" },
       { path = "std::thread::scope", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::thread::current_id", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::thread::current", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::thread::spawn", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::thread::yield_now", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::thread::panicking", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::thread::sleep_ms", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::thread::sleep", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::thread::sleep_until", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::thread::park", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::thread::park_timeout_ms", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::thread::park_timeout", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::thread::available_parallelism", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::io::stdin", reason = "the standard streams are the Reporter port's" },
       { path = "std::io::stdout", reason = "the standard streams are the Reporter port's" },
       { path = "std::io::stderr", reason = "the standard streams are the Reporter port's" },
       { path = "std::path::Path::exists", reason = "this Path method hits the filesystem; the Storage port answers it" },
       { path = "std::path::Path::try_exists", reason = "this Path method hits the filesystem; the Storage port answers it" },
       { path = "std::path::Path::is_file", reason = "this Path method hits the filesystem; the Storage port answers it" },
       { path = "std::path::Path::is_dir", reason = "this Path method hits the filesystem; the Storage port answers it" },
       { path = "std::path::Path::is_symlink", reason = "this Path method hits the filesystem; the Storage port answers it" },
       { path = "std::path::Path::metadata", reason = "this Path method hits the filesystem; the Storage port answers it" },
       { path = "std::path::Path::symlink_metadata", reason = "this Path method hits the filesystem; the Storage port answers it" },
       { path = "std::path::Path::canonicalize", reason = "this Path method hits the filesystem; the Storage port answers it" },
       { path = "std::path::Path::read_dir", reason = "this Path method hits the filesystem; the Storage port answers it" },
       { path = "std::os::unix::fs::symlink", reason = "platform filesystem extensions belong to an adapter, never to core" },
       { path = "std::os::unix::fs::chown", reason = "platform filesystem extensions belong to an adapter, never to core" },
       { path = "std::os::unix::fs::fchown", reason = "platform filesystem extensions belong to an adapter, never to core" },
       { path = "std::os::unix::fs::lchown", reason = "platform filesystem extensions belong to an adapter, never to core" },
       { path = "std::os::unix::fs::chroot", reason = "platform filesystem extensions belong to an adapter, never to core" },
       { path = "std::os::unix::fs::mkfifo", reason = "platform filesystem extensions belong to an adapter, never to core" },
   ]

   disallowed-types = [
       { path = "std::fs::File", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::Dir", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::Metadata", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::ReadDir", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::DirEntry", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::OpenOptions", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::FileTimes", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::Permissions", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::FileType", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::fs::DirBuilder", reason = "the filesystem is the Storage port's; buildl-core reads and writes nothing" },
       { path = "std::env::Vars", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::VarsOs", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::SplitPaths", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::JoinPathsError", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::Args", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::env::ArgsOs", reason = "the environment is ambient authority; every value reaches core through a port" },
       { path = "std::process::Child", reason = "running a process is the Exec port's; buildl-core only describes actions" },
       { path = "std::process::ChildStdin", reason = "running a process is the Exec port's; buildl-core only describes actions" },
       { path = "std::process::ChildStdout", reason = "running a process is the Exec port's; buildl-core only describes actions" },
       { path = "std::process::ChildStderr", reason = "running a process is the Exec port's; buildl-core only describes actions" },
       { path = "std::process::Command", reason = "running a process is the Exec port's; buildl-core only describes actions" },
       { path = "std::process::CommandArgs", reason = "running a process is the Exec port's; buildl-core only describes actions" },
       { path = "std::process::CommandEnvs", reason = "running a process is the Exec port's; buildl-core only describes actions" },
       { path = "std::process::Output", reason = "running a process is the Exec port's; buildl-core only describes actions" },
       { path = "std::process::Stdio", reason = "running a process is the Exec port's; buildl-core only describes actions" },
       { path = "std::process::ExitStatus", reason = "running a process is the Exec port's; buildl-core only describes actions" },
       { path = "std::process::ExitStatusError", reason = "running a process is the Exec port's; buildl-core only describes actions" },
       { path = "std::process::ExitCode", reason = "running a process is the Exec port's; buildl-core only describes actions" },
       { path = "std::thread::Scope", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::thread::ScopedJoinHandle", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::thread::JoinHandle", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::thread::Builder", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::thread::Thread", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::thread::LocalKey", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::thread::AccessError", reason = "scheduling is the executor adapter's; core logic is single-threaded and pure" },
       { path = "std::time::Instant", reason = "the wall clock is the Clock port's (architecture.md 5.4)" },
       { path = "std::time::SystemTime", reason = "the wall clock is the Clock port's (architecture.md 5.4)" },
       { path = "std::time::SystemTimeError", reason = "the wall clock is the Clock port's (architecture.md 5.4)" },
       { path = "std::io::Stdin", reason = "the standard streams are the Reporter port's" },
       { path = "std::io::Stdout", reason = "the standard streams are the Reporter port's" },
       { path = "std::io::Stderr", reason = "the standard streams are the Reporter port's" },
       { path = "std::net::TcpListener", reason = "buildl-core opens no sockets" },
       { path = "std::net::TcpStream", reason = "buildl-core opens no sockets" },
       { path = "std::net::UdpSocket", reason = "buildl-core opens no sockets" },
       { path = "std::os::unix::fs::FileExt", reason = "platform filesystem extensions belong to an adapter, never to core" },
       { path = "std::os::unix::fs::PermissionsExt", reason = "platform filesystem extensions belong to an adapter, never to core" },
       { path = "std::os::unix::fs::OpenOptionsExt", reason = "platform filesystem extensions belong to an adapter, never to core" },
       { path = "std::os::unix::fs::MetadataExt", reason = "platform filesystem extensions belong to an adapter, never to core" },
       { path = "std::os::unix::fs::FileTypeExt", reason = "platform filesystem extensions belong to an adapter, never to core" },
       { path = "std::os::unix::fs::DirEntryExt", reason = "platform filesystem extensions belong to an adapter, never to core" },
       { path = "std::os::unix::fs::DirBuilderExt", reason = "platform filesystem extensions belong to an adapter, never to core" },
   ]

   disallowed-macros = [
       { path = "std::env", reason = "compile-time environment reads are ambient authority" },
       { path = "std::option_env", reason = "compile-time environment reads are ambient authority" },
       { path = "std::print", reason = "printing bypasses the Reporter port and the standard streams ban" },
       { path = "std::println", reason = "printing bypasses the Reporter port and the standard streams ban" },
       { path = "std::eprint", reason = "printing bypasses the Reporter port and the standard streams ban" },
       { path = "std::eprintln", reason = "printing bypasses the Reporter port and the standard streams ban" },
   ]
   ```

3. Confirm the probe from step 1 is now caught, with the reasons rendered:

   ```
   $ cargo clippy -p buildl-core --all-targets --all-features -- -D warnings 2>&1 | grep -E 'error: use|note: the|note: printing|note: this Path'
   error: use of a disallowed type `std::time::SystemTime`
     = note: the wall clock is the Clock port's (architecture.md 5.4)
   error: use of a disallowed macro `std::println`
     = note: printing bypasses the Reporter port and the standard streams ban
   error: use of a disallowed method `std::path::Path::exists`
     = note: this Path method hits the filesystem; the Storage port answers it
   ```

   Note that `SystemTime::now()` is caught by the *type* ban — banning a type covers its inherent
   methods. Aliased calls are caught too: `use std::fs; fs::metadata(…)` trips the ban on
   `std::fs::metadata`.

4. Remove the probe:

   ```
   $ rm crates/buildl-core/src/probe.rs
   $ git checkout crates/buildl-core/src/lib.rs
   ```

5. Run the control that proves the file is actually being read — this is the step that would
   otherwise let a silently-ignored config pass as a clean one. Insert one bogus entry at the top
   of `disallowed-methods`:

   ```toml
       { path = "std::fs::definitely_not_real", reason = "control probe" },
   ```

   Then force a re-lint (clippy caches aggressively; touching the source is what makes it
   re-evaluate):

   ```
   $ touch crates/buildl-core/src/lib.rs
   $ cargo clippy -p buildl-core --all-targets --all-features -- -D warnings
   warning: `std::fs::definitely_not_real` does not refer to a reachable function
    --> /…/crates/buildl-core/clippy.toml:2:5
     |
   2 |     { path = "std::fs::definitely_not_real", reason = "control probe" },
     |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
     |
     = help: add `allow-invalid = true` to the entry to suppress this warning
   ...
       Finished `dev` profile
   $ echo $?
   0
   ```

   Two things this establishes: the config is read, **and** a rotted ban warns while the run still
   exits **0** even under `-D warnings`. That exit code is the whole reason task 2 exists.

6. Delete the control entry and confirm the file is clean:

   ```
   $ touch crates/buildl-core/src/lib.rs
   $ cargo clippy -p buildl-core --all-targets --all-features -- -D warnings
       Checking buildl-core v0.1.0 (…)
       Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.50s
   $ git status --short
   ?? crates/buildl-core/clippy.toml
   ```

7. Commit `build(buildl-core): ban the I/O, process, thread, environment and clock APIs`.

---

## Task 2 — Add the `guard-core-purity` task

**Files:**
- Modify `Makefile.toml`

**Steps:**

1. Add the task to `Makefile.toml`, in the `# Architecture guards` section plan 01 created,
   beside `[tasks.guard-crate-edges]`:

   ```toml
   [tasks.guard-core-purity]
   category = "Gate"
   description = "Assert buildl-core names no I/O, process, thread, environment or clock API"
   # `cargo make clippy` already fails on a real violation. What it does not fail
   # on is a ban entry that stopped resolving: clippy warns and exits 0, so every
   # ban could rot away under a green gate. Four assertions close that and the
   # three ways a ban can be switched off.
   script_runner = "@shell"
   script = '''
   config="crates/buildl-core/clippy.toml"
   if [ ! -f "${config}" ]; then
       echo "guard: ${config} is missing — every purity ban is gone" >&2
       exit 1
   fi

   out=$(cargo clippy -p buildl-core --all-targets --all-features -- -D warnings 2>&1) || {
       echo "${out}"
       exit 1
   }
   echo "${out}"

   # Any diagnostic whose span points at the config file means a ban stopped resolving.
   if echo "${out}" | grep -qE 'clippy\.toml:[0-9]+'; then
       echo "guard: a ban in crates/buildl-core/clippy.toml no longer resolves" >&2
       exit 1
   fi

   if grep -q 'allow-invalid' "${config}"; then
       echo "guard: allow-invalid disables rot detection for a ban entry" >&2
       exit 1
   fi

   if grep -rnE --include='*.rs' '#!?\[[^]]*disallowed_' crates/buildl-core; then
       echo "guard: a purity ban was suppressed in source" >&2
       echo "       move the call to an adapter behind a port, or delete the ban" >&2
       echo "       and record the decision in architecture.md §6" >&2
       exit 1
   fi
   '''
   ```

   Four details in that script are load-bearing rather than incidental:

   | Detail | Why |
   |---|---|
   | the `[ ! -f ]` check comes first | without it, deleting `clippy.toml` leaves the task printing `grep: …: No such file or directory` and **exiting 0** — every ban gone, gate green. That is the same failure this chain exists to remove. |
   | the rot check keys on the span path, not the message text | clippy prints config diagnostics with `--> …/crates/buildl-core/clippy.toml:L:C`, and no legitimate diagnostic points there. Matching message text would trip on an unrelated rustc E0573/E0574, whose "expected a type, found" phrasing is common, and send the reader to the wrong file. |
   | the suppression regex is `#!?\[`, not `#\[` | the inner-attribute form interposes `!`, so `#\[` misses `#![allow(clippy::disallowed_methods)]` — one line at the top of `lib.rs` that would silence every ban and leave the guard green. The `[^]]*` body also catches `#[cfg_attr(test, allow(clippy::disallowed_methods))]`. |
   | the grep scans `crates/buildl-core`, not `crates/buildl-core/src` | the clippy run beside it is `--all-targets`, so it lints `tests/` and `benches/` too; a narrower grep would leave a suppression there linted but never checked. `--include='*.rs'` keeps it to source files. |

   Clippy itself advertises the loophole the suppression check closes: its own diagnostic ends
   ``help: to override `-D warnings` add #[allow(clippy::disallowed_types)]``, so a contributor
   following the compiler's advice lands exactly on the thing the guard forbids.

2. Confirm it passes on the clean tree:

   ```
   $ cargo make guard-core-purity
   [cargo-make] INFO - Running Task: guard-core-purity
       Finished `dev` profile …
   [cargo-make] INFO - Build Done in …
   ```

3. Falsify failure mode 1 — a real violation. Create `crates/buildl-core/src/probe.rs` with:

   ```rust
   //! Probe.
   /// Probe.
   #[must_use]
   pub fn probe_clock() -> std::time::SystemTime { std::time::SystemTime::now() }
   ```

   append `pub mod probe;` to `crates/buildl-core/src/lib.rs`, then:

   ```
   $ cargo make guard-core-purity
   error: use of a disallowed type `std::time::SystemTime`
     = note: the wall clock is the Clock port's (architecture.md 5.4)
   [cargo-make] ERROR - Error while executing command, exit code: 101
   $ echo $?
   105
   ```

4. Falsify failure mode 2 — an inner-attribute suppression. Add
   `#![allow(clippy::disallowed_types)]` as the first line of `crates/buildl-core/src/probe.rs`
   (after the `//!` doc comment), then:

   ```
   $ cargo make guard-core-purity
       Finished `dev` profile …
   crates/buildl-core/src/probe.rs:2:#![allow(clippy::disallowed_types)]
   guard: a purity ban was suppressed in source
          move the call to an adapter behind a port, or delete the ban
          and record the decision in architecture.md §6
   $ echo $?
   105
   ```

   Clippy exits clean — the suppression worked — and the guard catches it anyway. That is the
   point of the third assertion.

5. Remove the probe and confirm green:

   ```
   $ rm crates/buildl-core/src/probe.rs
   $ git checkout crates/buildl-core/src/lib.rs
   $ cargo make guard-core-purity
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `build(repo): add the buildl-core purity guard task`.

---

## Task 3 — Falsify the config-side failure modes and run the guard before every commit

**Files:**
- Modify `Makefile.toml`

**Steps:**

1. Falsify failure mode 3 — a rotted ban, on a **warm** cache. Change one entry's path in
   `crates/buildl-core/clippy.toml`, for example `std::fs::metadata` →
   `std::fs::nonexistent_fn`, then run the guard **three times with no source change between
   runs**:

   ```
   $ cargo make guard-core-purity
   warning: `std::fs::nonexistent_fn` does not refer to a reachable function
    --> /…/crates/buildl-core/clippy.toml:…
   guard: a ban in crates/buildl-core/clippy.toml no longer resolves
   $ echo $?
   105
   ```

   All three runs must fail identically. Cargo replays cached diagnostics rather than dropping
   them, which is what makes the span-path check reliable rather than a cold-cache-only trick.

2. Falsify failure mode 4 — the rot opt-out. Restore the path, then add `allow-invalid = true` to
   any entry:

   ```toml
       { path = "std::fs::metadata", reason = "…", allow-invalid = true },
   ```

   ```
   $ cargo make guard-core-purity
   guard: allow-invalid disables rot detection for a ban entry
   $ echo $?
   105
   ```

   Clippy suggests exactly this flag in its own rot warning (`help: add allow-invalid = true to
   the entry to suppress this warning`), which is why the guard rejects it by name.

3. Falsify failure mode 5 — a missing config:

   ```
   $ mv crates/buildl-core/clippy.toml /tmp/clippy.toml.bak
   $ cargo make guard-core-purity
   guard: crates/buildl-core/clippy.toml is missing — every purity ban is gone
   $ echo $?
   105
   $ mv /tmp/clippy.toml.bak crates/buildl-core/clippy.toml
   ```

4. Confirm the config is back to its committed state and the guard is green:

   ```
   $ git status --short
   $ cargo make guard-core-purity
   [cargo-make] INFO - Build Done in …
   ```

   `git status --short` must print nothing for `crates/buildl-core/clippy.toml`. If it does, an
   edit from steps 1–2 was not reverted — run `git checkout crates/buildl-core/clippy.toml`.

5. Wire the guard into the gate. Plan 01 left `[tasks.clippy]` with
   `dependencies = ["guard-crate-edges"]`; extend it:

   ```toml
   dependencies = ["guard-core-purity", "guard-crate-edges"]
   ```

   The guards attach to `clippy` rather than to `dod` because `Makefile.toml`'s `dod` task
   deliberately mirrors the rust-guidelines five-command Definition of Done, and the `deny` task's
   comment records why no sixth step is added. `cargo make`, `cargo make dod` and
   `cargo make clippy` all run both guards.

6. Confirm the full gate:

   ```
   $ cargo make dod
   [cargo-make] INFO - Running Task: fmt-check
   [cargo-make] INFO - Running Task: guard-core-purity
   [cargo-make] INFO - Running Task: guard-crate-edges
   [cargo-make] INFO - Running Task: clippy
   [cargo-make] INFO - Running Task: doc
   [cargo-make] INFO - Running Task: test
   [cargo-make] INFO - Running Task: test-doc
   [cargo-make] INFO - Running Task: dod
   [cargo-make] INFO - Build Done in …
   $ cargo deny check
   advisories ok
   bans ok
   licenses ok
   sources ok
   ```

7. Commit `build(repo): run the buildl-core purity guard before every commit`.


---

## Review findings

One reviewer round over the whole diff, then one fix round. Its central result: **the guard
as this plan specified it failed open in three ways** — each one a state in which a purity ban
is absent, unresolvable or suppressed and `cargo make dod` still exits 0. That is precisely the
failure this plan exists to remove, so all three were treated as blocking. Every hole was
reproduced before the fix and re-checked after.

| # | Tier | Finding | Disposition |
|---|---|---|---|
| 1 | 🔴 | `grep -qE '^[^#]*allow-invalid'` fails open: `^[^#]*` rejects any line with an earlier `#`, so an entry whose `reason` contains one (`"see issue #42"`) escapes the check | applied — strip comment lines first, then match plainly |
| 2 | 🔴 | the source-suppression grep binds attribute and lint name to one line; rustfmt's own multi-line `#![allow(\n    clippy::disallowed_types\n)]` splits them and evades it | applied — match the lint name with no attribute context |
| 3 | 🟡 | a blanket `#![allow(clippy::all)]` or `#![allow(clippy::style)]` names no `disallowed_` text at all, and `--include='*.rs'` never sees `Cargo.toml`, where `[lints.clippy] disallowed_types = "allow"` silences a ban identically | applied — both group names added to the pattern; a fifth assertion added for the manifest |
| 4 | 🟡 | the comment claimed "four assertions close that and the three ways a ban can be switched off", and `clippy.toml`'s header claimed "There is no escape hatch" — completeness guarantees findings 1–3 falsify | applied — both rewritten to state coverage, not exhaustiveness |
| 5 | 🟡 | `echo "${out}"` under the `@shell` runner's `sh` interprets backslash escapes; a `\c` in clippy output truncates the rest and can drop the `clippy.toml:NN` span the rot check greps for | applied — `printf '%s\n'` at all three capture sites |
| 6 | 🟡 | nothing catches removal of `guard-core-purity` from `[tasks.clippy]`'s `dependencies` | declined — new guard machinery, outside this plan's one objective |
| 7 | 🟡 | `clippy.toml:19` cited "the chain's spec §3.2 and plan 02" — an internal planning path in a file that ships inside the published crate (doc-comment-discipline) | applied — replaced with a pointer to `docs/architecture-building-blocks.md` §1.2 |
| 8 | 🟡 | nothing catches a reversion of the ban list; deleting any of the 128 entries leaves the gate green, where plan 01's sibling guard machine-enforces the same class with a golden file | declined — new machinery, outside this plan's objective |
| 9 | 🔵 | `split_paths`/`join_paths`/`JoinPathsError` reasons claimed "the environment is ambient authority"; these three read nothing from the environment | applied |
| 10 | 🔵 | `process::id` and `thread::panicking` reasons described process execution and scheduling; neither does either | applied |
| 11 | 🔵 | three reasons cited "(architecture.md 5.4)", which resolves to no heading — §5 has no subsections | applied — now `architecture.md §5 rule 4` |
| 12 | 🔵 | the guard's `cargo clippy -p buildl-core` duplicates `[tasks.clippy]`'s `--workspace` run; deliberate but undocumented | applied — noted in the task comment |
| 13 | ❓ | `clippy.toml` ships inside the published crate (no manifest `exclude`), so downstream vendorers inherit all 128 bans | raised to the author; not decided here |

Verified clean by the reviewer and not changed: 128 entries (71/51/6), zero duplicates, valid
TOML, every entry resolves on rustc 1.98.1, all three lint tables fire under a positive control,
rot detection survives a warm cache, `set -e` is on under `@shell` and `out=$(…) || {…}` does not
swallow clippy's exit code, `dbg!` is not a gap, and no interaction breakage with plan 01's
`guard-crate-edges` or `crates/expected-edges.txt`.

## Probe results

Every falsification the plan specifies was run, plus three the plan did not anticipate. Task 2
step 2, task 2 step 5 and task 3 step 4 are the green-path runs; the rest are failures, and
cargo-make reports a failed task as exit **105**.

| Falsification | Plan step | Result |
|---|---|---|
| clean tree, guard passes | T2.2 | exit 0 |
| a real violation — `SystemTime::now()` in `buildl-core` | T2.3 | exit 105 |
| inner-attribute suppression `#![allow(clippy::disallowed_types)]` | T2.4 | exit 105 — clippy itself exits clean; the guard catches it anyway |
| probe removed, guard green again | T2.5 | exit 0 |
| a rotted ban, three consecutive runs on a warm cache | T3.1 | exit 105 on all three, identically — cargo replays the cached diagnostic, so the span-path check is not a cold-cache trick |
| `allow-invalid = true` on an entry | T3.2 | exit 105 |
| missing `clippy.toml` | T3.3 | exit 105 |
| config restored, guard green | T3.4 | exit 0 |
| full gate | T3.6 | `cargo make dod` exit 0; `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok` |
| **`allow-invalid = true` with a `#` in the reason** | — | **not in the plan.** Before: exit 0 (fail-open). After: exit 105 |
| **multi-line `#![allow(\n clippy::disallowed_types\n)]`** | — | **not in the plan.** Before: exit 0 with `cargo fmt --check` also green. After: exit 105 |
| **`[lints.clippy] disallowed_types = "allow"` in the manifest** | — | **not in the plan.** Before: exit 0. After: exit 105 |

`clippy::allow_attributes` was checked as a negative control against the new pattern's `\b`
anchors and is correctly **not** matched, so the broadened grep introduces no false positive.

Two predicted outputs in the plan were loose and are not defects. cargo-make prints both
`Task: <name>` and `Running Task: <name>` for a script task invoked by name, where the plan shows
only the second; and `cargo deny check` prints one summary line, not four. A `SystemTime` return
type plus a `SystemTime::now()` call also fires the type ban twice, not once — the plan's step
predicted three diagnostic lines and reality gives four.

## Deviations

1. **The `allow-invalid` check is not the plan's.** The plan's `grep -q 'allow-invalid' "${config}"`
   (line 402) matches the header comment its own task 1 body writes into `clippy.toml` (line 151),
   so the guard could never pass. The shipped check strips comment lines and then matches:

   ```sh
   if grep -vE '^[[:space:]]*#' "${config}" | grep -q 'allow-invalid'; then
   ```

   An intermediate attempt, `grep -qE '^[^#]*allow-invalid'`, was written and then replaced: it
   fixed the false positive but introduced a false negative on any entry whose `reason` contains
   a `#`. Both states were reproduced.

2. **The source-suppression pattern is not the plan's.** `#!?\[[^]]*disallowed_` (line 421) is
   line-scoped and blind to blanket group allows. The shipped check is

   ```sh
   if grep -rnE --include='*.rs' 'clippy::(disallowed_|all\b|style\b)' crates/buildl-core; then
   ```

   `clippy::all` and `clippy::style` are named because `clippy-driver -Whelp` shows
   `disallowed-methods`, `disallowed-types` and `disallowed-macros` as members of exactly those
   two groups.

3. **A fifth assertion was added.** The plan specifies four; a crate-local `[lints]` override in
   `crates/buildl-core/Cargo.toml` defeats all four, so the guard now asserts that table is
   exactly `workspace = true`.

4. **`printf '%s\n'` replaces `echo` at the three diagnostic-capture sites**, so a backslash
   escape in clippy's output cannot truncate the text the rot check greps.

5. **Task 3 step 3's backup path** was the session scratchpad rather than `/tmp`, per this
   environment's constraint. `clippy.toml` is untracked, so `git status` cannot prove it was
   restored; restoration was proven by `diff` against a pristine copy instead of the plan's
   `git status --short` assertion.

The plan's task 2 and task 3 bodies still show the superseded `allow-invalid` and suppression
patterns inline. They are left as written — the plan is the record of what was specified, and
these deviations are the record of what shipped. The chain's `spec.md` §3.3 quotes the same
superseded guard script and should be reconciled when plan 04 amends the documentation.
