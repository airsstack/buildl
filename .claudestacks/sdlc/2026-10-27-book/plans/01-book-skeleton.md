---
status: executing
created: 2026-10-28
built-on: 053dd1fe3030
disproves: 053dd1fe3030
---

# Book Skeleton Implementation Plan

**Goal:** An mdbook book with every chapter file of the spec present as a titled stub builds the
same way locally, in CI and in a Pages workflow.

**Architecture:** `book/` copies airsl's layout (`/Users/hiraq/Projects/airsstack/airsl/book/`):
`book.toml`, `src/SUMMARY.md`, one Markdown file per chapter and section. Mermaid diagrams render
through the `mdbook-mermaid` preprocessor, whose two scripts are committed beside `book.toml`.
mdBook `0.4.52` and `mdbook-mermaid` `0.16.2` are pinned exactly in three places — `ci.yml`,
`pages.yml`, `Makefile.toml` — each with a comment naming the other two. Every later chapter plan
fills a stub this plan creates, so they never touch `SUMMARY.md`.

**Tech Stack:** mdBook 0.4.52, mdbook-mermaid 0.16.2, cargo-make, GitHub Actions
(`taiki-e/install-action@v2`, `actions/upload-pages-artifact@v5`, `actions/deploy-pages@v5`).

---

## Files that change

```
book/book.toml                 — [create] book metadata, mermaid preprocessor, html output settings
book/mermaid.min.js            — [create] written by `mdbook-mermaid install book`
book/mermaid-init.js           — [create] written by `mdbook-mermaid install book`
book/src/SUMMARY.md            — [create] the table of contents: all 45 files below
book/src/*.md (45 files)       — [create] one stub per chapter/section, holding only its `# Title`
.gitignore                     — [modify] ignore `book/book/`
Makefile.toml                  — [modify] `book`, `book-build`, `book-mdbook` and `book-mermaid` tasks
.github/workflows/ci.yml       — [modify] a `book` job
.github/workflows/pages.yml    — [create] build and deploy the book to GitHub Pages
```

## Order of work

### Task 1 — Create the book and its stubs

**Files:**
- Create `book/book.toml`, `book/mermaid.min.js`, `book/mermaid-init.js`, `book/src/SUMMARY.md`
- Create the 45 stub files listed below under `book/src/`
- Modify `.gitignore`

**Change:**
- `book/book.toml`:
  - `[book]`: `title = "The buildl Book"`, `authors = ["airsstack"]`, `language = "en"`, `src = "src"`.
  - `[output.html]`: `site-url = "/buildl/"`, `git-repository-url = "https://github.com/airsstack/buildl"`, `edit-url-template = "https://github.com/airsstack/buildl/edit/main/book/{path}"`. This is airsl's form (`airsl/book/book.toml`), including its comment on why `site-url` carries the prefix.
- Install both tools at the exact versions:

  ```
  cargo install mdbook --version 0.4.52 --locked
  cargo install mdbook-mermaid --version 0.16.2 --locked
  ```

  Skip either one if `--version` already prints that version. Then run `mdbook-mermaid install book`. It writes the two scripts and adds `[preprocessor.mermaid] command = "mdbook-mermaid"` and `[output.html] additional-js = ["mermaid.min.js", "mermaid-init.js"]` to `book.toml`. Keep its keys, and tidy the layout so each table appears once.
- `SUMMARY.md` follows airsl's form (`airsl/book/src/SUMMARY.md`): `[Introduction](introduction.md)`, then the numbered chapters with their sections nested, then a `---` separator and the appendix. The files and their titles, in order:

  ```
  introduction.md                     Introduction
  ch01-00-getting-started.md          Getting Started
    ch01-01-installation.md           Installation
    ch01-02-hello-buildl.md           Hello, buildl
    ch01-03-what-did-it-do.md         What Did It Do?
  ch02-00-first-project.md            A First Project
  ch03-00-build-files.md              Build Files
    ch03-01-targets-and-labels.md     Targets and Labels
    ch03-02-inputs-outputs-deps.md    Inputs, Outputs and Dependencies
    ch03-03-toolbox.md                The Toolbox
    ch03-04-many-directories.md       Many Directories
    ch03-05-build-settings.md         Build Settings
  ch04-00-asking-permission.md        Asking Permission
    ch04-01-nothing-is-ambient.md     Nothing Is Ambient
    ch04-02-the-ceiling.md            buildl.toml, the Ceiling
    ch04-03-approvers.md              Approvers
  ch05-00-caching.md                  Caching
    ch05-01-action-key.md             The Action Key
    ch05-02-early-cutoff.md           Early Cutoff
    ch05-03-tests.md                  Tests
    ch05-04-clean-gc-verify.md        clean, gc and verify
  ch06-00-sandbox.md                  The Sandbox
  ch07-00-discovery-targets.md        Discovery Targets
  ch08-00-plugins.md                  Plugins
    ch08-01-writing-a-plugin.md       Writing a Plugin
    ch08-02-plugins-from-git.md       Plugins from Git
    ch08-03-how-buildl-decides.md     How buildl Decides
  ch09-00-remote.md                   Remote Cache and Execution
  ch10-00-watch.md                    Watch Mode
  ch11-00-under-the-hood.md           Under the Hood
    ch11-01-declare-then-execute.md   Declare, Then Execute
    ch11-02-crash-safety.md           Crash Safety and the Store
    ch11-03-four-crates.md            Four Crates, Ports and Adapters
    ch11-04-errors.md                 Errors
    ch11-05-determinism-rules.md      Determinism Rules
    ch11-06-decision-records.md       Decision Records
    ch11-07-testing-layers.md         Testing Layers
    ch11-08-where-buildl-sits.md      Where buildl Sits
    ch11-09-what-it-promises.md       What buildl Can and Cannot Promise
  --- appendix-00.md                  Appendix
    appendix-01-cli.md                A. Command-Line Reference
    appendix-02-buildl-toml.md        B. buildl.toml Reference
    appendix-03-lua-api.md            C. Build File API
    appendix-04-exit-codes.md         D. Exit Codes
    appendix-05-references.md         E. References
  ```

  That is 45 files, the spec's §2 table row for row. Each stub holds only its `# Title`. Titles that name a file or command (`buildl.toml`, `clean`, `gc`, `verify`) are set in backticks inside the stub heading.
- `.gitignore`: add `book/book/` under a comment saying it is the build output and Pages builds its own.

**Proof:**
- `mdbook build book` exits 0.
- `ls book/src/*.md | wc -l` gives `46` (45 stubs plus `SUMMARY.md`).
- `book/book/mermaid.min.js` exists.
- `git status --short book/book` prints nothing, because the output is ignored.

**Done when:** all of the above hold. Commit `docs(repo): the book's skeleton`.

### Task 2 — Book tasks in `Makefile.toml`

**Files:** Modify `Makefile.toml`.

**Change:** Add a `Book` section, modelled on `airsl/Makefile.toml:160-182`, with four tasks:
- `book-mdbook`: a script that runs `mdbook --version` and, unless it prints `mdbook v0.4.52`, runs `cargo install mdbook --version 0.4.52 --locked --force`; if the binary on PATH is still not that version, it fails and names the binary.
- `book-mermaid`: the same for `mdbook-mermaid` at exactly `0.16.2`.
- `book`: `mdbook serve book --open`. It depends on `book-mdbook` and `book-mermaid`.
- `book-build`: `mdbook build book`. It depends on `book-mdbook` and `book-mermaid`.

Versions are exact, not airsl's `min_version` floor: under a floor, a missing `mdbook-mermaid` installs as 0.17.x, which needs mdBook 0.5. cargo-make's `install_crate { …, version = … }` key is not used: the Task 2 probe (see `## Probe results`) showed it reads the version from cargo's install record, not from the binary on PATH. (Amended 2026-10-28 during execution.)

The section's comment names the two other places the versions are pinned (`ci.yml` job `book`, `pages.yml`). Add `cargo make book` and `cargo make book-build` to the usage list in the header (`Makefile.toml:1-14`).

**Proof:** `cargo make book-build` exits 0, and `cargo make --list-all-steps` lists `book` and `book-build` under `Book`.

**Done when:** both hold. Commit `build(repo): book tasks in Makefile.toml`.

### Task 3 — A `book` job in CI

**Files:** Modify `.github/workflows/ci.yml`.

**Change:** Add a job `book` named `Book` on `ubuntu-latest` with `timeout-minutes: 10`. Steps:
1. `actions/checkout@v5`.
2. `taiki-e/install-action@v2` with `tool: mdbook@0.4.52,mdbook-mermaid@0.16.2`.
3. `mdbook build book`.

Add a comment, as in `airsl/.github/workflows/ci.yml:120-137`, saying it is the same build `pages.yml` runs after a merge, and naming the other two pin sites.

**Proof:** `ruby -ryaml -e 'YAML.load_file(".github/workflows/ci.yml")'` exits 0. The job's green run is the plan-wide proof below.

**Done when:** it parses. Commit `ci(repo): build the book`.

### Task 4 — Publish the book with a Pages workflow

**Files:** Create `.github/workflows/pages.yml`.

**Change:** airsl's `pages.yml` (`airsl/.github/workflows/pages.yml`), minus its installer-wrapper step and the `install/**` path:
- `on`: a push to `main` touching `book/**` or `.github/workflows/pages.yml`, and `workflow_dispatch`.
- Top-level `permissions: contents: read`, and `concurrency: { group: pages, cancel-in-progress: false }`.
- Job `build`: checkout with `persist-credentials: false`; `taiki-e/install-action@v2` with `tool: mdbook@0.4.52,mdbook-mermaid@0.16.2`; `mdbook build book`; `actions/upload-pages-artifact@v5` with `path: book/book`.
- Job `deploy` (`needs: build`): `permissions: { pages: write, id-token: write }`; `environment: { name: github-pages, url: ${{ steps.deployment.outputs.page_url }} }`; `actions/deploy-pages@v5` with `id: deployment`.

The header comment keeps airsl's two points: the site is deployed from an artifact, never a branch; and the repository's Pages source must be "GitHub Actions". It names `https://airsstack.github.io/buildl/` and the other two pin sites.

**Proof:** `ruby -ryaml -e 'YAML.load_file(".github/workflows/pages.yml")'` exits 0.

**Done when:** it parses. Commit `ci(repo): publish the book to GitHub Pages`.

## Risks

- `mdbook-mermaid install` may write `book.toml` keys in an order or layout that differs from airsl's file. Its keys are what matter. Task 1 tidies the layout and keeps the keys.
- `mermaid.min.js` is a large vendored file. That is accepted: the spec commits it (§4).
- The Pages `deploy` job can only be proven once the user sets the repository's Pages source to "GitHub Actions" and the workflow runs on `main`. GitHub's default `github-pages` environment allows deploys from the default branch only, so a `workflow_dispatch` on `worktree-prep-publish` is not expected to deploy. This plan proves the `build` half through the CI `book` job (the same commands), and leaves the deploy to the first push to `main`.
- Planning cannot settle whether cargo-make's `version` key reinstalls a tool found at a different version, or only installs one that is missing. Task 2 probes it once: put a different `mdbook` first on `PATH` and see which version runs. If cargo-make keeps the wrong version, `book-mermaid` (and a matching mdBook step) checks `--version` itself and installs with `cargo install --locked --version`.

## Proof

- `cargo make book-build` exits 0 on a clean checkout.
- One CI run on the branch (`gh workflow run ci.yml --ref worktree-prep-publish`) shows the `Book` job green. Every other job stays as it was.
- `.gitignore` keeps `book/book/` out of `git status`.

## Review findings

- risk — the version-check scripts reinstalled on every run and still built with a wrong binary
  first on PATH; each now re-checks after installing and fails naming the binary —
  `Makefile.toml` `book-mdbook`/`book-mermaid`. Fixed; verified with the fake `mdbook v0.4.40`
  first on PATH: `mdbook on PATH is …/fakebin/mdbook, not v0.4.52; put ~/.cargo/bin first`, exit 1.
- nit — section comment said "Both tasks"; now names `book` and `book-build`. Fixed.
- nit — `book-mermaid` description said "unless it is on PATH"; now "unless that version is the one
  on PATH". Fixed.
- nit — no blank line before `---` in `book/src/SUMMARY.md`; added. Fixed.
- plan hygiene — Task 2 and the Files list still named `install_crate … version`; amended. Fixed.
- spec hygiene — spec §4's `Makefile.toml` row (`spec.md:204`) names `install_crate … version` as
  the mechanism; the probe below disproves it. Recorded as `disproves` in this plan's frontmatter;
  the spec amendment is the `design` skill's.

## Probe results

- Claim (spec §4, plan Risks): cargo-make's `install_crate { version = "<v>" }` makes the task use
  that version. Command: a fake `mdbook` printing `mdbook v0.4.40` first on PATH, then
  `cargo make book-build`. Output: `Execute Command: "mdbook-mermaid" "--version"`,
  `WARN - Unable to read currently installed version for crate: mdbook-mermaid`,
  `Execute Command: "mdbook" "build" "book"`, `mdbook v0.4.40`. **Against the plan**: the key
  reads cargo's install record (`~/.cargo/.crates.toml`), not the binary; the wrong `mdbook` ran
  and nothing was installed.
- `mdbook build book` exited 0; `ls book/src/*.md | wc -l` → `46`; `book/book/mermaid.min.js`
  exists; `git status --short book/book` printed nothing.
- `cargo make --list-all-steps` lists `book`, `book-build`, `book-mdbook`, `book-mermaid` under
  `Book`; `cargo make book-build` exited 0.
- `ruby -ryaml` parsed `ci.yml` and `pages.yml`.
- CI run 40695748537 at `4028365`: the `Book` job and all 9 other jobs green.

## Deviations

- 2026-10-28 — Task 2 uses version-checking scripts (`book-mdbook`, `book-mermaid`) instead of
  `install_crate`, per the plan's Risks fallback, after the probe above.
- 2026-10-28 — `mdbook-mermaid` 0.16.2 was put on PATH by copying the binary built earlier into
  the scratchpad, not by a second `cargo install`.
