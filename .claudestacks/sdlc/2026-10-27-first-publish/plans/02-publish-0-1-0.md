---
status: approved
created: 2026-10-31
depends-on: [01]
built-on: a9f5951ee52c
---

# Publish 0.1.0 Implementation Plan

**Goal:** `buildl` `0.1.0` is out: the four crates on crates.io, and a GitHub release with
smoke-tested binaries built from the same commit.

**Architecture:** one commit on `main`, R, is gated, published from and tagged. The author
publishes the crates from their own machine, then pushes `plugin-c-v0.1.0` and `buildl-cli-v0.1.0`
at R, and the release workflow from plan 01 builds the release. Crates go before tags because the
publish is the step that cannot be undone (spec section 7). Every task here runs commands; only
Task 1 can change a file.

**Tech Stack:** cargo, cargo-make, cargo-deny, git, the GitHub CLI (`gh`), cargo-dist's generated
installer.

**Content authority:** spec section 7 steps 3-8, and section 8.

---

## Files that change

```
CHANGELOG.md   — [modify] the 0.1.0 heading's date becomes the publish date, only if it differs
```

## Order of work

### Task 1 — Fix commit R

**Files:**
- Modify `CHANGELOG.md` — only when the `## 0.1.0 — <date>` heading from plan 01 Task 4 is not
  today's date: the date becomes today's, on a branch, through a pull request

**Change:** `main`'s head becomes R, the commit that will be gated, published and tagged. When the
changelog date is already today, R is `main`'s head after PR A's merge, and nothing changes.
Otherwise a one-line pull request re-dates the heading, keeping the version first so dist still
reads the notes, and its merge commit is R. Record R's full hash in the plan's execution notes. The changelog's
release date is the day R is fixed: when the go-ahead in Task 3 comes on a later day, R is not
re-dated, since re-dating would make a new R and repeat Task 2.

**Proof:**
- `heading form` — `dist plan --tag buildl-cli-v0.1.0 --output-format=json` at R gives an
  `announcement_title` of `0.1.0 — <today>` and a non-empty `announcement_changelog`
- `R is main` — `git rev-parse origin/main` equals R

**Done when:** both checks pass. Commit, only when the date changed,
`docs(repo): date the 0.1.0 changelog entry`.

### Task 2 — Gate R

**Change:** at a clean checkout of R, run the gates spec section 7 step 3 names. Nothing is
changed. A failure stops the plan: it is fixed through a pull request, and Task 1 starts again
with the new head.

**Proof:**
- `dod at R` — `cargo make dod` exits 0
- `deny at R` — `cargo deny check` exits 0
- `publish dry run at R` — `cargo make publish-dry-run` exits 0, printing
  `warning: aborting upload due to dry run` for `buildl-core`, `buildl-lua`, `buildl` and
  `buildl-cli`, in that order
- `names still free` — `cargo info buildl`, `cargo info buildl-core`, `cargo info buildl-lua` and
  `cargo info buildl-cli` each print `could not find`

**Done when:** all four checks pass at R. No commit.

### Task 3 — CHECKPOINT: the author's go-ahead, then publish the crates

**Change:** **Hard stop.** Show the author R, the four green gates from Task 2, and what happens
next: `cargo publish --workspace` at R, then the two tags at R pushed to GitHub (Task 4).
Publishing cannot be undone: a version can be yanked but never deleted or replaced. Go on only on
the author's explicit yes, which covers this task and Task 4. On the yes, the author, logged in
with `cargo login`, runs `cargo publish --workspace` at a clean checkout of R. It publishes
`buildl-core`, `buildl-lua`, `buildl` and `buildl-cli` in that order.

If it stops partway, follow spec section 8 row 2. If the cause is outside the crates (network,
credentials), publish the rest from R with `cargo publish -p <crate>` in order. If it needs a change
to crate source, stop this plan: the fix ships as `0.1.1` of every crate through a new chain, and
no tag is pushed.

**Proof:**
- `on crates.io` — `cargo info buildl-core@0.1.0`, `buildl-lua@0.1.0`, `buildl@0.1.0` and
  `buildl-cli@0.1.0` each show version `0.1.0`

**Done when:** all four crates show `0.1.0` on crates.io. No commit.

### Task 4 — Tag R and start the release

**Change:** covered by Task 3's go-ahead. Create two lightweight tags at R, `plugin-c-v0.1.0` and
`buildl-cli-v0.1.0`, and push both to `origin`. The `buildl-cli-v0.1.0` push starts
`buildl-cli-release.yml`. The `plugin-c-v0.1.0` push starts nothing, because the release trigger is
namespaced (plan 01 Task 3).

**Proof:**
- `tags at R` — `git ls-remote --tags origin` lists `refs/tags/plugin-c-v0.1.0` and
  `refs/tags/buildl-cli-v0.1.0`, both at R
- `one release run` — `gh run list --workflow buildl-cli-release.yml` shows one run for
  `buildl-cli-v0.1.0` and none for `plugin-c-v0.1.0`

**Done when:** both checks pass. No commit.

### Task 5 — See the release through

**Change:** watch the tag run to the end. If a smoke check fails, follow spec section 8 row 3. A
fix that touches only files outside the crates (workflows, `.github/smoke/`, `dist-workspace.toml`)
lands through a pull request as R′. Task 2's gates run on R′, `buildl-cli-v0.1.0` is deleted locally
and on `origin`, and it is pushed again at R′, only while no release exists. A fix that touches
crate source ships as `0.1.1` through a new chain.

**Proof:**
- `run green` — `gh run view` of the tag run shows `plan`, the six `build-local-artifacts` jobs,
  `build-global-artifacts`, the six smoke jobs, `host` and `announce` passing
- `release contents` — `gh release view buildl-cli-v0.1.0 --json name,body,assets` gives the name
  `0.1.0 — <date>`; a body whose headings are, in order, `## Release Notes` (holding the changelog
  entry's `### Added` text), `## Install buildl-cli 0.1.0`, `## Download buildl-cli 0.1.0` and
  `## Verifying GitHub Artifact Attestations` (spec section 7); and assets holding the six
  `buildl-cli-<target>.tar.xz`, a `.sha256` for each, `buildl-cli-installer.sh`, `sha256.sum` and
  `source.tar.gz`
- `attested` — `gh attestation verify` of the downloaded `buildl-cli-aarch64-apple-darwin.tar.xz`
  with `--repo airsstack/buildl` succeeds
- `no attribution` — the release body has no line matching `claude|co-authored-by`
  (case-insensitive); not `generated with`, which dist's own attestation section contains
  (spec section 7)

**Done when:** all four checks pass. No commit.

### Task 6 — Check the release as a user would

**Change:** install from each published route into a temporary folder, never over the author's own
`buildl`. Run the plugin tag's first real fetch.

**Proof:**
- `crates.io install` — `cargo install --locked buildl-cli --root <tmp1>` succeeds, and
  `<tmp1>/bin/buildl --version` prints `buildl 0.1.0`
- `release installer` — downloading
  `https://github.com/airsstack/buildl/releases/download/buildl-cli-v0.1.0/buildl-cli-installer.sh`
  and running it with `BUILDL_CLI_INSTALL_DIR=<tmp2>` and `BUILDL_CLI_NO_MODIFY_PATH=1` installs a
  `buildl` under `<tmp2>` that prints `buildl 0.1.0`
- `published plugin tag` — in a fresh copy of `examples/c` (its `buildl.toml` unchanged, so it
  names `github = "airsstack/buildl"` and `rev = "plugin-c-v0.1.0"`), `buildl plugins fetch` writes
  `buildl.lock`, and `buildl build` exits 0

**Done when:** all three checks pass. If one fails, the release is out: the fix ships as `0.1.1`
through a new chain (spec section 8 row 4). No commit.

---

## Risks

- **Task 3 is irreversible**, so it is the plan's one checkpoint, and the gates of Task 2 run on
  exactly the commit it publishes.
- **A partial `cargo publish`** leaves some crates out and some not. The plan only resumes from R
  for causes outside the crates; any source fix becomes `0.1.1`.
- **Re-tagging after a failed tag run** is allowed only before any release exists, and only for
  fixes outside the crates, so the binaries never differ from the crates.io source.
- **From Task 3 until plan 03 merges,** the crates.io page of `buildl-cli` names a curl command that
  Pages does not serve yet; `cargo install --locked buildl-cli` works throughout (spec section 7).
- Not done here: crates.io trusted publishing or any CI publish (a spec non-goal).

## Proof

- crates.io lists `buildl-core`, `buildl-lua`, `buildl` and `buildl-cli` at `0.1.0`, all published
  from R.
- `buildl-cli-v0.1.0` and `plugin-c-v0.1.0` point at R; the GitHub release holds six attested,
  smoke-tested archives, with the changelog entry as its Release Notes section.
- Installed from crates.io and from the release installer, `buildl --version` prints `buildl 0.1.0`.
