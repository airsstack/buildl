---
status: superseded
created: 2026-10-31
built-on: d48b727b98e5
---

# Spec: Publish the four crates to crates.io

The first release of buildl, version `0.1.0`: the four crates published to crates.io in their
dependency order, a GitHub release built with cargo-dist that holds a prebuilt `buildl` for six
Unix targets, each smoke-tested on its own platform before anything is published, a short install
command served from the book's GitHub Pages site, and the `plugin-c-v0.1.0` tag the documentation
already pins the `c` plugin to. The release machinery copies the one airsl already runs, so both
airsstack tools release the same way. The install instructions then lead with the prebuilt binary,
which needs no Rust toolchain.

Evidence below was produced on 2026-10-31 at commit `dbbe5f7`, on `aarch64-apple-darwin`, with
cargo 1.99.0 and cargo-dist 0.33.1 (`dist --version` printed `cargo-dist 0.33.1`). Probes that
needed a dist configuration ran on a scratch copy of the repository (`rsync -a --exclude target
--exclude .git`, then `git init` and one commit), never on this checkout.

## 1. Premises

| # | Fact | Evidence |
|---|---|---|
| P1 | The four crates package and verify as a set in publish order, before any is on crates.io. | `cargo publish --dry-run --workspace` exited 0, printing in order `Verifying buildl-core v0.1.0`, `Verifying buildl-lua v0.1.0`, `Verifying buildl v0.1.0`, `Verifying buildl-cli v0.1.0`, the later three each `Unpacking … (registry …/target/package/tmp-registry)`, then `Uploading` ×4 each followed by `warning: aborting upload due to dry run`. |
| P2 | The four names are free on crates.io. | `cargo info <name>` for `buildl`, `buildl-core`, `buildl-lua`, `buildl-cli` each printed `error: could not find \`<name>\` in registry \`https://github.com/rust-lang/crates.io-index\``. Control: the same index serves `airsl = "0.1.4"`, which the workspace resolves (`Cargo.toml` `[workspace.dependencies]`). |
| P3 | Every crate already carries `description`, `readme`, and workspace `license`/`repository`; none carries `homepage`. | `grep -nE 'description\|readme\|license\|repository\|homepage' crates/*/Cargo.toml` listed `description` (line 4), `readme` (line 5), `license.workspace` (8), `repository.workspace` (9) in each of the four, and no `homepage` line; `grep -n homepage Cargo.toml crates/*/Cargo.toml` exited 1. Control: the same alternation matched the other four keys in every file. |
| P4 | buildl is built and tested on Unix only. | `.github/workflows/ci.yml:32` "Unix only, deliberately — matching airsl, which refuses to build off unix."; `ci.yml:36` `os: [ubuntu-latest, macos-latest]`; `README.md:43` "buildl runs on Linux and macOS." |
| P5 | The install instructions today build from source. | `book/src/ch01-01-installation.md:3` "`buildl` is not on crates.io yet, and there are no prebuilt binaries."; `ch01-01:20` and `README.md:46` `cargo install --git https://github.com/airsstack/buildl buildl-cli --locked`; `skills/install.sh:125` `cargo install --locked --path` (from a clone), `:129` `cargo install --locked --git` (otherwise), `:137` `cargo install --locked airsl-cli`; `install.sh:115-117` exits with "install Rust from https://rustup.rs" when `cargo` is absent and either binary is missing. |
| P6 | `plugin-c-v0.1.0` is pinned by the documentation and does not exist on GitHub. | `rev = "plugin-c-v0.1.0"` at `book/src/ch08-04-the-c-plugin.md:38` and `:469`, `skills/buildl/SKILL.md:168`, `plugins/c/README.md:23`, `examples/c/README.md:45`, `examples/cpp/README.md:67`, and in `examples/c/buildl.toml` and `examples/cpp/buildl.toml`. `git ls-remote --tags origin` printed nothing; control: `git ls-remote --heads origin` lists `refs/heads/main`. `examples/c/prepare.sh` header: "The published tag does not exist before the plugin is released", and it builds a local stand-in repository tagged `plugin-c-v0.1.0` for CI. `examples/c/README.md:68` "Before the tag exists, …". |
| P7 | The book's site is live and its Pages workflow deploys only on book changes. | `curl -sS -o /dev/null -w "%{http_code}" https://airsstack.github.io/buildl/` printed `200`. `.github/workflows/pages.yml` `on.push.paths`: `book/**`, `.github/workflows/pages.yml`; it uploads `book/book`. |
| P8 | airsl's installer pointer is live. | `curl -sSL -o /dev/null -w "%{http_code}"` printed `200` for `https://airsstack.github.io/airsl/install.sh` and for `https://github.com/airsstack/airsl/releases/latest/download/airsl-cli-installer.sh`. |
| P9 | The `skills` task checks `install.sh`. | `Makefile.toml:485` `shellcheck skills/install.sh`; `:486` runs `sh skills/install.sh --dry-run --dir "${T2}"` and fails if it wrote into `T2` (`:488`) or prints no copy of each skill (`:493`); `:498-500` require exit code 2 with no option. |

## 2. The model: airsl's release

airsl's release (repository `airsstack/airsl`, read 2026-10-31) is the template. What is copied:

| airsl file | What it does | Cited |
|---|---|---|
| `dist-workspace.toml` | cargo-dist `0.33.1`, `ci = "github"`, `installers`, `targets`, `install-path = "~/.local/bin"`, `hosting = "github"`, `install-updater = false`, `tag-namespace = "airsl-cli"`, `github-attestations = true`, `global-artifacts-jobs = ["./airsl-cli-smoke"]`, `pr-run-mode = "upload"` | whole file, 35 lines |
| `.github/workflows/airsl-cli-release.yml` | generated by dist ("This file was autogenerated by dist", line 1); trigger `tags: - 'airsl-cli**[0-9]+.[0-9]+.[0-9]+*'` | lines 1, `on:` block |
| `.github/workflows/airsl-cli-smoke.yml` | hand-written; `on: workflow_call` with input `plan`; job `targets` derives the matrix from the plan's `executable-zip` artifacts and a fixed target→runner table, failing on an archive with no runner; job `smoke` per archive: download the `artifacts-build-local-*` artifacts, `sha256sum -c` (or `shasum -a 256 -c`), unpack, run musl builds inside `docker run … alpine:3.20`, check `--version` equals `airsl $VERSION`, then exercise the binary | whole file |
| `install/install.sh` | a pointer: resolves `AIRSL_VERSION` (default `latest`) to `…/releases/latest/download/airsl-cli-installer.sh` or `…/releases/download/airsl-cli-v$version/airsl-cli-installer.sh`, rejects a value outside `[0-9A-Za-z.+-]`, downloads with curl or wget, runs it with the given arguments (`sh "$tmp/$installer" "$@"`) | whole file, 78 lines |
| `.github/workflows/pages.yml` | also triggers on `install/**`; step "Add the installer wrappers": `cp install/install.sh install/install.ps1 book/book/` | lines 3-4, 15, 48-49 |
| `CHANGELOG.md` | Keep a Changelog | lines 1-6 |

What is not copied: Windows (`x86_64-pc-windows-msvc`, the PowerShell installer, the CRT import
check), because buildl is Unix only (P4); airsl's per-crate tag pair, because buildl's four crates
share one version.

## 3. Release configuration

`dist-workspace.toml` at the repository root, created by `dist init` and then edited to:

| Key | Value | Why |
|---|---|---|
| `cargo-dist-version` | `"0.33.1"` | airsl's version; the one installed and probed |
| `ci` | `"github"` | |
| `installers` | `["shell"]` | Unix only (P4) |
| `targets` | `aarch64-apple-darwin`, `x86_64-apple-darwin`, `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-musl`, `x86_64-unknown-linux-musl` | the author's choice: both macOS, both glibc Linux, both static musl Linux |
| `install-path` | `"~/.local/bin"` | airsl's choice: no `~/.cargo/bin` on a machine without Rust |
| `hosting` | `"github"` | |
| `install-updater` | `false` | |
| `tag-namespace` | `"buildl-cli"` | only `buildl-cli-v*` tags start a release (below) |
| `github-attestations` | `true` | |
| `global-artifacts-jobs` | `["./buildl-cli-smoke"]` | the smoke workflow (section 4) |
| `pr-run-mode` | `"upload"` | every pull request builds and smoke-tests all six archives, publishing nothing |

Proven on the scratch copy:

- `dist init --yes --ci github --installer shell --hosting github` printed
  `✔ added [profile.dist] to your workspace Cargo.toml` and `✔ added [dist] to your root dist-workspace.toml`;
  the added profile reads `[profile.dist]` / `inherits = "release"` / `lto = "thin"`. The workspace's
  `[profile.release]` (`Cargo.toml:128-131`) is `lto = "thin"`, `codegen-units = 1`,
  `strip = "symbols"`, so release archives get all three.
- With `tag-namespace = "buildl-cli"`, `dist generate` wrote
  `.github/workflows/buildl-cli-release.yml` whose trigger is
  `tags: - 'buildl-cli**[0-9]+.[0-9]+.[0-9]+*'`. Without the key the file is `release.yml` and the
  trigger is `'**[0-9]+.[0-9]+.[0-9]+*'`, which `plugin-c-v0.1.0` matches; `dist plan --tag
  plugin-c-v0.1.0` then fails: `× Couldn't parse the version from the provided announcement tag
  (plugin-c-v0.1.0)` / `unexpected character 'p' while parsing major version number`. The
  namespace is what keeps plugin tags from starting a failing release run.
- `dist plan --tag buildl-cli-v0.1.0` printed `announcing v0.1.0` / `buildl-cli 0.1.0`, then
  `source.tar.gz`, `buildl-cli-installer.sh`, `sha256.sum` and one
  `buildl-cli-<target>.tar.xz` per target, each holding `[bin] buildl`, `[misc] LICENSE,
  README.md` and a `.sha256`. The three library crates are not announced; the release is
  `buildl-cli`'s alone.
- The plan's `ci.github.artifacts_matrix` assigns runners `macos-14` (aarch64-apple-darwin),
  `macos-15-intel` (x86_64-apple-darwin), `ubuntu-22.04` (x86_64 gnu and musl),
  `ubuntu-22.04-arm` (aarch64 gnu and musl), and for both musl targets `packages_install`
  `sudo apt-get update` / `sudo apt-get install musl-tools`. GitHub's runner list
  (`github/docs` `data/reusables/actions/supported-github-runners.md`, fetched 2026-10-31) lists
  `macos-15-intel` under Intel (line 74), `macos-14` under arm64 (line 86), and
  `ubuntu-22.04-arm` under arm64 (line 52).
- `dist build --tag buildl-cli-v0.1.0 --artifacts=global` (with `install-path = "~/.local/bin"`)
  wrote `target/distrib/buildl-cli-installer.sh`, which installs to `$HOME/.local/bin`
  (installer lines 129, 754-756) and reads `BUILDL_CLI_INSTALL_DIR`, `BUILDL_CLI_NO_MODIFY_PATH`
  and the other `BUILDL_CLI_*` variables.
- With `github-attestations = true`, `global-artifacts-jobs = ["./buildl-cli-smoke"]` and
  `pr-run-mode = "upload"` added, `dist generate` (run by the spec reviewer on a scratch copy,
  2026-10-31) wrote a job `custom-buildl-cli-smoke` that `uses:
  ./.github/workflows/buildl-cli-smoke.yml` with `needs: [plan, build-local-artifacts]`; the
  `host` job lists it in `needs`, and its `if` accepts the smoke job's `success` or `skipped`;
  `on: pull_request` is present with `pr_run_mode` `upload`; `actions/attest@v4` is present; and
  `dist plan` with no tag announces `buildl-cli 0.1.0` alone with six `executable-zip` artifacts.

`.github/workflows/buildl-cli-release.yml` is the output of `dist generate` and is never edited
by hand; a change to the release goes through `dist-workspace.toml` and `dist generate`.

## 4. Smoke workflow

`.github/workflows/buildl-cli-smoke.yml`, written by hand in the shape of airsl's
(section 2): called by the release workflow with the plan, it runs after every archive is built
and before the GitHub release exists, and dist makes the publishing job wait for it, so an
archive that fails is never published.

```text
targets ── plan's executable-zip artifacts × runner table ──► matrix (fails on a target with no runner)
smoke (one per archive, fail-fast off)
  download artifacts-build-local-* ─► sha256 check ─► unpack ─► BUILDL=<binary>
     (musl: BUILDL wraps `docker run alpine:3.20`)
  ├─ buildl --version  ==  "buildl <plan's app_version>"
  ├─ in .github/smoke/:  build ─► test ─► build (all cached) ─► verify
  └─ in each of examples/{go,go-discovery,plugins,polyglot,python,rust,remote}: check, doctor
```

Runner table: the six targets on the runners dist builds them on (section 3). The version string
compared is what `buildl --version` prints: this checkout's debug binary printed `buildl 0.1.0`.

`.github/smoke/` is a buildl workspace whose actions run only `sh`, `tr` and `grep`, so no
compiler or language toolchain is needed on the runner or in Alpine. Proven with this checkout's
debug `buildl` and a workspace of the same shape (one `buildl.target` running
`sh -c 'tr a-z A-Z < greeting.txt > "$1"'`, one `buildl.test` running
`sh -c 'grep -q hello greeting.txt'`, ceiling `proc.run = ["sh"]`), run under
`env -i HOME=$HOME PATH=/usr/bin:/bin`:

```text
== buildl build   →  1 built                exit=0
== buildl test    →  tests: 1 passed        exit=0
== buildl build   →  1 cached               exit=0
== buildl verify  →  verified //:upper
                     1 verified, 0 mismatched, 0 failed, 0 skipped, 0 not cached, 0 not sampled   exit=0
out/upper: HELLO
```

`build` and `verify` run in the input sandbox by default (CLAUDE.md, Milestone 3), so this
covers the sandbox, the cache and the store in the shipped binary.

The example checks were chosen by running each command in every example with
`env -i HOME=$HOME PATH=/nonexistent`: `check` and `doctor` exited 0 in `go`, `go-discovery`,
`plugins`, `polyglot`, `python`, `rust` and `remote`; `plan` exited 3 in each
(`program go was not found`, `program cargo was not found`, `program python3 was not found`), so
it is left out; in `c` and `cpp` all three exited 5 (`plugin c is not fetched: buildl.lock does
not pin https://github.com/airsstack/buildl at plugin-c-v0.1.0 …`), so those two are left out.

## 5. Install pointer

`install/install.sh`, copied from airsl's (section 2) with its names changed:

| airsl | buildl |
|---|---|
| `AIRSL_VERSION` | `BUILDL_VERSION` |
| `airsl-cli-installer.sh` | `buildl-cli-installer.sh` |
| `…/releases/download/airsl-cli-v$version/…` | `…/releases/download/buildl-cli-v$version/…` |
| `https://airsstack.github.io/airsl/install.sh` | `https://airsstack.github.io/buildl/install.sh` |

`.github/workflows/pages.yml` gains `install/**` in its paths and a step copying
`install/install.sh` into `book/book/` before the upload, so the command is

```console
$ curl -LsSf https://airsstack.github.io/buildl/install.sh | sh
```

## 6. Documentation and installer changes

| File | Change |
|---|---|
| `CHANGELOG.md` (new) | Keep a Changelog. One entry, headed `## 0.1.0 — <release date>`, covering the four crates and the `c` plugin `0.1.0`. dist takes the GitHub release's title and notes from it: on the scratch copy, with the heading `## 0.1.0 — 2026-11-01`, `dist plan --tag buildl-cli-v0.1.0 --output-format=json` gave `announcement_title` `'0.1.0 — 2026-11-01'` and `announcement_changelog` holding the entry's body; with `## buildl-cli 0.1.0 — 2026-11-01` or `## buildl 0.1.0 — 2026-11-01` it gave `announcement_changelog` `None`. The heading therefore starts with the version. |
| `Cargo.toml` (workspace) | `[profile.dist]` from `dist init`; `homepage = "https://airsstack.github.io/buildl/"` under `[workspace.package]` |
| `crates/*/Cargo.toml` | `homepage.workspace = true` |
| `Makefile.toml` | task `publish-dry-run`: `cargo publish --dry-run --workspace` (P1) |
| `crates/buildl-cli/README.md` | its one install line, `cargo install --git https://github.com/airsstack/buildl buildl-cli --locked` (lines 10-12), becomes the curl command first and `cargo install --locked buildl-cli` second. It is the crate's `readme` (`crates/buildl-cli/Cargo.toml:5`), so it is the crates.io page of `buildl-cli` 0.1.0 and fixed once published. The other three crate READMEs hold no install line. |
| `README.md` | install section: the curl command first, `cargo install --locked buildl-cli` second, git and checkout last |
| `book/src/ch01-01-installation.md` | rewritten in airsl's order: prebuilt (curl, `BUILDL_VERSION`, `BUILDL_CLI_NO_MODIFY_PATH`, installs to `~/.local/bin`), then from crates.io (needs Rust 1.94 and a C compiler), then from GitHub and from a checkout; the release's archives, checksums and attestations named as the manual route |
| `book/src/ch01-04-with-an-ai-agent.md` | says `install.sh` installs the prebuilt `buildl` and `airsl`, and that Rust is needed only when installing from a clone |
| `skills/install.sh` | `buildl`: from a clone, `cargo install --locked --path` as now; with `--ref` left at `main`, run `https://airsstack.github.io/buildl/install.sh` (the latest release); with any other `--ref`, `cargo install --locked --git … --branch <ref>` as now, so `--ref` still names the buildl that is installed as well as the skills. `airsl`: run `https://airsstack.github.io/airsl/install.sh` (P8). The `cargo` requirement applies only to the clone and branch routes. `--dry-run`, `--dir`, `--claude` and the exit code 2 keep their behaviour, so the `skills` task's checks (P9) hold. |
| `examples/c/README.md`, `examples/cpp/README.md` | the tag is published; `prepare.sh` stays, as the way CI runs the checkout's own plugin |
| `examples/c/prepare.sh`, `examples/cpp/prepare.sh` | the header's "The published tag does not exist before the plugin is released" (line 6) becomes a statement that CI runs the checkout's plugin rather than the published tag |

## 7. Release procedure

The work lands as two pull requests, so nothing on `main` or the Pages site points at an install
that does not exist yet:

| PR | Holds | Merged |
|---|---|---|
| A — release | section 3, section 4, `CHANGELOG.md`, the `Cargo.toml` and `Makefile.toml` rows and `crates/buildl-cli/README.md` from section 6 | before the publish (step 2) |
| B — install | section 5 (`install/install.sh`, `pages.yml`), and the `README.md`, book, `skills/install.sh`, examples and `prepare.sh` rows of section 6 | after the release exists (step 9) |

PR A touches nothing under `book/` or `install/`, so merging it does not deploy Pages (P7).

```text
1  PR A ─► release workflow's PR run: 6 archives built + smoke-tested, nothing published
2  merge PR A
3  on main at commit R: cargo make dod · cargo deny check · cargo make publish-dry-run  (all green)
4  ⚠ the author's explicit go-ahead
5  the author runs, at R: cargo publish --workspace     buildl-core → buildl-lua → buildl → buildl-cli
6  git tag plugin-c-v0.1.0 R && git tag buildl-cli-v0.1.0 R, push both
7  buildl-cli-release.yml: plan → build ×6 → smoke ×6 → GitHub release (CHANGELOG notes, archives,
   checksums, attestations, installer)
8  check: cargo install --locked buildl-cli ; buildl --version
          the release's installer for this machine ; buildl --version
          in a copy of examples/c: buildl plugins fetch ; buildl build
9  PR B, gated as usual, merged ─► Pages serves install.sh ─► curl …/buildl/install.sh | sh ; buildl --version
```

Crates go out before tags because step 5 is the one that cannot be undone. If it fails partway,
no release yet advertises an install that does not work. The release (step 7) has already been
built and smoke-tested once on PR A's run (step 1). From step 5 until PR B is merged in step 9, the crates.io page
of `buildl-cli` names a curl command that Pages does not serve yet; its second command,
`cargo install --locked buildl-cli`, works from step 5 on.

No commit, tag, CHANGELOG entry or release note carries AI attribution (intent, Constraints).
dist builds the release notes from the CHANGELOG entry alone (section 6).

## 8. Error handling

The binaries and the crates on crates.io must be the same source, and the gates must be green on
the tagged commit (intent, Constraints). So the response to a failure depends on whether the fix
touches what was published, meaning anything packaged into the four crates.

| Failure | Effect | Response |
|---|---|---|
| A target fails to build, or a smoke check fails, on PR A's run | PR A is red; nothing is published | fix in PR A. The musl targets have never been built by buildl's CI, and this run is where that is first proven. If a target cannot be made to work, dropping it is the author's call. |
| `cargo publish --workspace` stops partway | the crates before the failure are on crates.io at R, the rest are not | if the cause is outside the crates (network, credentials): publish the rest from R. If it needs a change to crate source: none of the four is tagged `0.1.0`; the fix ships as `0.1.1` of every crate, through a new chain. |
| A smoke check fails on the tag run | dist's publishing job does not run; no GitHub release exists | if the fix touches only files outside the crates (workflows, `.github/smoke/`, `dist-workspace.toml`): land it as commit R′, run step 3's gates on R′, delete `buildl-cli-v0.1.0` and push it at R′. If it touches crate source: the fix ships as `0.1.1`, through a new chain. |
| A step-8 or step-9 check fails | the release is out | fixed in a `0.1.1`, through a new chain |

## 9. Testing

| What | Proves |
|---|---|
| `cargo make dod`, `cargo deny check` | the gate, unchanged |
| `cargo make publish-dry-run` | P1, again, on the commit that is published |
| `cargo make skills` | `install.sh` passes shellcheck and its dry-run and usage checks (P9) |
| `cargo make book-build` | the rewritten pages build |
| the release workflow's run on PR A | all six archives build, match their checksums, report `buildl 0.1.0`, and pass section 4's checks on their own platforms |
| steps 8 and 9 of section 7 | the published crates, the installer, the Pages pointer and the plugin tag work for a user |

## 10. Non-goals

- Windows, and installers other than the shell installer.
- Homebrew, apt, Nix and other distribution channels.
- Publishing crates from CI, or crates.io trusted publishing: the author publishes from their
  own machine.
- Releasing the `c` plugin by any means other than its tag.
- Feature work from any other milestone.
