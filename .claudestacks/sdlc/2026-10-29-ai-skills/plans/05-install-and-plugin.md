---
status: approved
created: 2026-10-29
depends-on: [02, 04]
built-on: 1703d7070b45
---

# Install Script and Plugin Implementation Plan

**Goal:** one command, `sh install.sh`, installs `buildl`, `airsl` and the two skills, either as a
Claude Code plugin or into a folder another agent reads.

**Architecture:** `.claude-plugin/marketplace.json` makes the repository root a Claude Code plugin
whose `skills` list names the two skill folders, with no `plugin.json` and no copy (spec §5.1).
`skills/install.sh`, the only shell script under `skills/`, installs the two commands with `cargo
install` when they are missing, then the skills through `claude plugin` or by copying (spec §5.2–5.4).
The `skills` task gains the installer's checks, and a local-only `skills-claude` task runs Claude
Code's validator (spec §6.1, §6.5).

**Tech Stack:** POSIX `sh`, `cargo install`, `claude plugin` (Claude Code 2.1.312), `curl`, `tar`,
`shellcheck`.

---

## Files that change

```
.claude-plugin/marketplace.json  — [create] the marketplace and its one plugin entry
skills/install.sh                — [create] the installer
Makefile.toml                    — [modify] `skills` gains the installer checks; new `skills-claude`
.github/workflows/ci.yml         — [modify] the `skills` job installs shellcheck
```

## Facts this plan relies on

| Fact | Source |
|---|---|
| a root-source marketplace entry with a `skills` list loads only those folders; without `plugin.json` the entry is the manifest; the layout passes `claude plugin validate --strict` | spec §1 P7, P8, P9 |
| `claude plugin marketplace add <owner/repo@ref> --sparse <paths...>`; `claude plugin install <plugin>@<marketplace>`, scope `user` by default | spec §1 P10 |
| `cargo install --locked --git <url> --branch <branch> <crate>` | `cargo install --help` (`--git <URL>`, `--branch <BRANCH>`, `--locked`) |
| `cargo install --locked airsl-cli` installs `airsl-cli` 0.1.4 | spec §1 P23 |
| the tarball `https://github.com/airsstack/buildl/archive/<branch>.tar.gz` answers 302 and needs `curl -L` | spec §1 P24 |
| the options, steps and messages of the installer | spec §5.3, §5.4 |
| a component path that does not exist fails validation with `Path not found` | code.claude.com plugins/manifest-reference.md:407 |

## Order of work

### Task 1 — `marketplace.json`

**Files:**
- Create `.claude-plugin/marketplace.json` — the JSON of spec §5.1, verbatim
- Modify `Makefile.toml` — add the task `skills-claude` to the *Skills* section: `command = "claude"`,
  `args = ["plugin", "validate", "--strict", "."]`, with a comment saying it is run by hand before a
  commit that touches `.claude-plugin/` or `skills/`, and is in neither CI nor `dod`

**Change:** Claude Code reads the repository as a marketplace named `buildl` holding one plugin,
`buildl`, whose skills are `skills/buildl` and `skills/buildl-migrate`.

**Proof:**
- `cargo make skills-claude` prints `✔ Validation passed` and exits 0
- by hand, once: a `skills` entry naming `./skills/missing` makes it fail with `Path not found`;
  revert

**Done when:** that holds, and `cargo make dod` and `cargo deny check` are green. Commit
`feat(repo): offer the skills as a Claude Code plugin`.

### Task 2 — `install.sh`

**Files:**
- Create `skills/install.sh` — POSIX `sh`, no `sudo`, `set -eu`

**Change:** the steps of spec §5.4 in order, with the options of spec §5.3:
- argument parsing: `--claude`, `--dir P`, `--ref R` (default `main`), `--dry-run`; anything else, or
  not exactly one of `--claude`/`--dir`, prints the usage to stderr and exits 2;
- a `run` helper prints each command before running it, and under `--dry-run` only prints it;
- `buildl` and `airsl` each skipped when on `PATH`, else installed with the `cargo install` lines of
  spec §5.4; `cargo` is looked for only then, and missing exits 1 with the rustup line;
- `--claude`: `claude` missing exits 1 with its line; then the two `claude plugin` commands;
- `--dir P`: when the script's own folder is named `skills` and holds `buildl/SKILL.md`, it is the
  source; else `curl -fsSL` the tarball of `R` into a temporary folder and `tar -xzf` it there; for
  each `skills/*/` holding `SKILL.md`, `P/<name>` is removed and replaced by a copy; the temporary
  folder is removed on exit;
- a closing report naming what was installed and where.

**Proof:**
- `shellcheck skills/install.sh` reports nothing
- `sh skills/install.sh` with no option exits 2 and prints the usage
- `sh skills/install.sh --claude --dir /tmp/x` exits 2
- `sh skills/install.sh --dry-run --dir "$T"` (a temporary `T`) exits 0, prints the copy commands for
  `buildl` and `buildl-migrate`, and leaves `T` empty
- by hand, once: `sh skills/install.sh --dir "$T"` fills `T/buildl` and `T/buildl-migrate` with copies
  holding `SKILL.md`, and a second run gives the same tree; `skills/tests` is not copied
- by hand, once, after the branch is pushed (spec §8): `sh skills/install.sh --claude --ref
  <branch>` installs, `claude plugin list` shows `buildl@buildl` with both skills, a second run
  ends without error, and `claude plugin uninstall buildl@buildl` with `claude plugin marketplace
  remove buildl` undoes it; the outcome is recorded in the execution record

**Done when:** that holds, and `cargo make dod` and `cargo deny check` are green. Commit
`feat(repo): add the skills installer`.

### Task 3 — The installer in `cargo make skills`

**Files:**
- Modify `Makefile.toml` — the `skills` task: `shellcheck` joins the tools whose absence stops it;
  after its snippet step, `shellcheck skills/install.sh`, then `sh skills/install.sh --dry-run --dir
  "$T2"` with a fresh temporary `T2`, which must exit 0 and leave `T2` empty
- Modify `.github/workflows/ci.yml` — the `skills` job gains a step `sudo apt-get install -y
  shellcheck` before `cargo make skills`

**Change:** CI catches a shell defect or a broken dry run of the installer (spec §6.5).

**Proof:**
- `cargo make skills` passes
- by hand, once: an unquoted variable put in `install.sh` makes `cargo make skills` fail at the
  `shellcheck` step; revert

**Done when:** that holds, and `cargo make dod` and `cargo deny check` are green. Commit
`build(repo): check the installer in cargo make skills`.

## Risks

- **Most risky: Task 2's `--claude` path** cannot be proven without changing the developer's own
  Claude Code setup and a pushed branch. `--dry-run --claude` proves the commands it would run; the
  real install is run once by hand on the merged branch, against the user scope, and undone with
  `claude plugin uninstall buildl@buildl` and `claude plugin marketplace remove buildl`.
- The `--dir` download path needs the branch pushed to GitHub; until then only the clone path is
  provable. The by-hand run in Task 2 uses the clone path.
- `cargo install --git` builds buildl from source: several minutes on a fresh machine. Stated in the
  book page (plan 06).

## Proof

`cargo make skills` and `cargo make skills-claude` pass; the installer's dry runs and the by-hand
`--dir` run behave as Task 2 states.
