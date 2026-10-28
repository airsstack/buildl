---
status: approved
created: 2026-10-28
built-on: 4f3397c93dde
---

# Spec: The buildl book

This chain replaces `docs/` with an mdbook book, laid out like airsl's, that teaches buildl to
someone who has never read its source and carries the design and architecture in an "Under the
Hood" part. CI builds it on every pull request and push to `main`; a Pages workflow publishes it
at `https://airsstack.github.io/buildl/`. Every reference to `docs/` or a spec `§` number
outside the chain history is repointed to the chapter that now holds the content, and `docs/`
is deleted.

Evidence comes from probes run on 2026-10-28 with the debug `buildl` binary built from commit
`d6ba45a` (macOS arm64, mdBook 0.4.52), deleted afterwards; their output is quoted where it
backs a claim.

## 1. Premises proven

|Premise|Evidence|
|---|---|
|airsl's book is `book/book.toml` plus `book/src/SUMMARY.md`, `introduction.md`, `chNN-MM-<topic>.md` chapter files and `appendix-NN-<topic>.md` files, 43 files, 2,876 lines|`ls /Users/hiraq/Projects/airsstack/airsl/book/src \| wc -l` → `43`; `wc -l book/src/*.md` → `2876 total`; `book/src/SUMMARY.md` read in full|
|airsl's `book.toml` sets `title`, `authors = ["airsstack"]`, `language = "en"`, `src = "src"`, and under `[output.html]` `site-url = "/airsl/"`, `git-repository-url`, `edit-url-template`|`airsl/book/book.toml`, read in full|
|airsl pins mdBook `0.4.52` in three places and keeps them in step by comment|`airsl/.github/workflows/ci.yml:129-134` (`tool: mdbook@0.4.52`), `airsl/.github/workflows/pages.yml` (same pin), `airsl/Makefile.toml:173,180` (`min_version = "0.4.52"`)|
|airsl's Pages workflow deploys from an Actions artifact: a `build` job (`contents: read`) runs `mdbook build book` and `actions/upload-pages-artifact@v5` on `book/book`; a `deploy` job alone holds `pages: write` and `id-token: write` and runs `actions/deploy-pages@v5`; `concurrency: { group: pages, cancel-in-progress: false }`; triggered by a push to `main` touching its paths, and by `workflow_dispatch`|`airsl/.github/workflows/pages.yml`, lines 1-70 read; its header says "The repository's Pages source must be set to "GitHub Actions" for the deploy job to succeed"|
|airsl's `Makefile.toml` has `book` (`mdbook serve book --open`) and `book-build` (`mdbook build book`), each installing mdBook on first use through `install_crate`|`airsl/Makefile.toml:170-182`|
|mdBook 0.4.52 does not draw Mermaid: a ` ```mermaid ` block becomes `<code class="language-mermaid">` and no Mermaid script is shipped|throwaway book built with `mdbook build`: `class="language-mermaid"`; `find book -iname '*mermaid*'` → `0`; control `find book -name highlight.js` → `1`|
|`mdbook-mermaid` 0.16.2 is the last release built on mdBook 0.4; 0.17.0 and 0.17.1 need mdBook 0.5|crates.io dependencies API: `0.16.2 [('mdbook', '^0.4.36')]`, `0.17.0`/`0.17.1 [('mdbook-preprocessor', '^0.5.0')]`|
|`mdbook-mermaid` 0.16.2 works with mdBook 0.4.52, and `mdbook-mermaid install <dir>` writes `mermaid.min.js` and `mermaid-init.js` beside `book.toml` and adds `[preprocessor.mermaid] command = "mdbook-mermaid"` and `[output.html] additional-js = ["mermaid.min.js", "mermaid-init.js"]`|installed with `cargo install mdbook-mermaid --version 0.16.2 --locked` into the scratchpad; after `install`, `book.toml` held exactly those keys; `mdbook build` rendered `<pre class="mermaid">graph LR` and copied both scripts into the output|
|`taiki-e/install-action` installs `mdbook-mermaid`|`taiki-e/install-action` `TOOLS.md:84` (fetched raw): `[**mdbook-mermaid**](https://github.com/badboy/mdbook-mermaid) \| $CARGO_HOME/bin \| GitHub Releases`|
|`docs/` holds four documents, 2,882 lines, 38 Mermaid diagrams|`wc -l docs/*` → `773 architecture-building-blocks.md`, `750 architecture.md`, `1061 design.md`, `298 fundamental-walkthrough.md`; `grep -c '```mermaid'` → design 14, walkthrough 10, architecture 7, building-blocks 7|
|Outside `docs/` and the chain history, six files cite the spec documents, on 48 matching lines; no Rust source does|`grep -rIl "§\|design\.md\|architecture\.md\|architecture-building-blocks\|fundamental-walkthrough\|docs/design\|docs/arch\|docs/fundamental"` excluding `target`, `.git`, `.claudestacks`, `.claude`, `docs` → `Cargo.toml`, `CLAUDE.md`, `crates/buildl-core/clippy.toml`, `deny.toml`, `Makefile.toml`, `README.md`; `grep -c` with the same pattern per file → 7, 12, 9, 10, 3, 7. Beyond those, `docs/` appears as plain text, not a link, at `README.md:245` (a tree listing), `README.md:269` ("open an issue against `docs/design.md` or `docs/architecture.md`") and `CLAUDE.md:92` (the Mermaid rule); `README.md:284` cites `https://bazel.build/docs/sandboxing`, and five vendored files under `crates/buildl/proto/` and `crates/buildl/generated/` hold Google `/docs/` URLs — none of these last six is a reference to buildl's documents. `grep -rIl "§" crates examples ci tools .github` → only `crates/buildl-core/clippy.toml` (the control: the same search finds its 9 lines). `deny.toml` also abbreviates the building-blocks document as `ABB` (`deny.toml:84-114`); `grep -rn "ABB\b"` finds it nowhere else|
|`crates/buildl-core/src/types/changed_path.rs` mentions `docs/read me.md` as sample data, not a citation|`changed_path.rs:23,112,135,138`|
|CI runs on a push to `main`, on every pull request and on `workflow_dispatch`; it has jobs `dod`, `examples`, `deny`, `msrv`, `reapi-codegen`, `remote-execution`, `examples-remote`|`.github/workflows/ci.yml:3-7` and `grep -n "^  [a-z-]*:$"`|
|The repository is public and has no Pages site yet|`gh repo view airsstack/buildl --json visibility` → `"PUBLIC"`; `gh api repos/airsstack/buildl/pages` → `404 Not Found`|
|The binary has 13 commands besides `help`: `check`, `graph`, `query`, `plan`, `build`, `watch`, `test`, `verify`, `doctor`, `log`, `clean`, `gc`, `plugins`; `build` takes `--set <NAME=VALUE>`, `-k/--keep-going`, `--stream`, `--remote-upload`, `--remote-strict`, `--no-remote`, `--remote-execute`, `--no-remote-execute`, `--remote-jobs <N>`|`buildl --help` and `buildl build --help`, debug build of `d6ba45a`; `buildl --version` → `buildl 0.1.0`|
|`Makefile.toml` already has an `install` task: `cargo install --path crates/buildl-cli --locked --force`|`Makefile.toml:385-389`|
|The workspace takes airsl from crates.io (`airsl = "0.1.4"`), so `cargo install --git` needs no other repository|`Cargo.toml:27`. Whether `cargo install --git https://github.com/airsstack/buildl buildl-cli --locked` succeeds end to end is **not verified** (§8)|
|`.gitignore` ignores neither `book/book/` nor anything under `book/`|`.gitignore`, read in full|

## 2. The book's structure

Tutorial first, then the features, then Under the Hood, then reference — airsl's shape. Each
row is one file under `book/src/`; each `chNN-00` file opens its chapter and introduces its
sections.

|File|Title|Holds|
|---|---|---|
|`introduction.md`|Introduction|what buildl is; declare, then execute; nothing is ambient; who the book is for|
|`ch01-00-getting-started.md`|Getting Started||
|`ch01-01-installation.md`|Installation|`cargo install --git https://github.com/airsstack/buildl buildl-cli --locked`; `cargo make install` from a checkout; the Rust version the workspace needs|
|`ch01-02-hello-buildl.md`|Hello, buildl|a one-target workspace: `buildl.toml`, `build.lua`, `buildl build`, `out/`|
|`ch01-03-what-did-it-do.md`|What Did It Do?|`plan`, `log`, `doctor` on that workspace|
|`ch02-00-first-project.md`|A First Project|`examples/go` walked through: build, test, edit, `plan` naming the edited input, the second `test` served from cache; the Rust and Python starters as the same in two more languages|
|`ch03-00-build-files.md`|Build Files||
|`ch03-01-targets-and-labels.md`|Targets and Labels|`buildl.target`, `buildl.rule`, `buildl.alias` (installed at `crates/buildl-lua/src/module.rs:161,170`), labels `//dir:name`, relative labels, a directory-only `//dir` label rejected, `graph`, `query`|
|`ch03-02-inputs-outputs-deps.md`|Inputs, Outputs and Dependencies|`$in`, `$out`, `$deps`, sources, the input rule that an input stays under its declaring directory|
|`ch03-03-toolbox.md`|The Toolbox|the `airsstack` modules a build file gets|
|`ch03-04-many-directories.md`|Many Directories|`subdir`, explicit only — a build file is evaluated only because another named its directory; labels across directories; `examples/polyglot`|
|`ch03-05-build-settings.md`|Build Settings|`--set NAME=VALUE` and the options a build file declares|
|`ch04-00-asking-permission.md`|Asking Permission||
|`ch04-01-nothing-is-ambient.md`|Nothing Is Ambient|the wanted set; tools are dependencies too; an untrusted clone|
|`ch04-02-the-ceiling.md`|`buildl.toml`, the Ceiling|every table of `buildl.toml` that bounds a build|
|`ch04-03-approvers.md`|Approvers|`manifest`, `interactive`, `deny`; `doctor`|
|`ch05-00-caching.md`|Caching||
|`ch05-01-action-key.md`|The Action Key|what the key folds; `plan` explaining a dirty target|
|`ch05-02-early-cutoff.md`|Early Cutoff|a dependency rebuilt to the same bytes; `examples/polyglot`'s comment edit|
|`ch05-03-tests.md`|Tests|`test`, cached tests|
|`ch05-04-clean-gc-verify.md`|`clean`, `gc` and `verify`|removing `out/`, pruning the store, re-running cached actions in the sandbox and naming mismatches|
|`ch06-00-sandbox.md`|The Sandbox|the isolation ladder; tier 2 by default; pinned `TZ`, `LC_ALL`, `SOURCE_DATE_EPOCH`; `strategy = "bare"`; tiers refused|
|`ch07-00-discovery-targets.md`|Discovery Targets|why a declaration cannot run a tool; `buildl.discover`, `buildl.discovered`, `stdout = true`, `buildl.decode_all`; `examples/go-discovery`; the depfile gap|
|`ch08-00-plugins.md`|Plugins||
|`ch08-01-writing-a-plugin.md`|Writing a Plugin|`extension.toml`, `buildl.use`, the three layers of engineer-owned logic, what a stack plugin encodes, plugin API versioning (within `api = 1` the `buildl` table only gains names); `examples/plugins`|
|`ch08-02-plugins-from-git.md`|Plugins from Git|`[plugins.<name>]`, `buildl plugins fetch`, `buildl.lock`|
|`ch08-03-how-buildl-decides.md`|How buildl Decides|the plugin's manifest, the workspace's cap, the ceiling; what `plan`, `doctor` and `graph` name|
|`ch09-00-remote.md`|Remote Cache and Execution|`[remote]`, `fetched`, `--remote-upload`, `--remote-strict`, `--no-remote`, `--remote-execute`, jobs and retries; `examples/remote`|
|`ch10-00-watch.md`|Watch Mode|`buildl watch`, what it ignores, how it ends|
|`ch11-00-under-the-hood.md`|Under the Hood|for the curious and for contributors; none of it is needed to use buildl|
|`ch11-01-declare-then-execute.md`|Declare, Then Execute|Load → Resolve → Plan → Execute → Record; phases handing values; the scheduler; parallel loading|
|`ch11-02-crash-safety.md`|Crash Safety and the Store|one action's lifecycle; cas first, cache row after, log always; output capture and failure|
|`ch11-03-four-crates.md`|Four Crates, Ports and Adapters|the four crates, dependency inversion, what `buildl-core` may depend on and its per-family counts, the C4 views, the port catalog, the dependency rules (numbered as today), publishing order|
|`ch11-04-errors.md`|Errors|the error architecture|
|`ch11-05-determinism-rules.md`|Determinism Rules|the determinism engineering rules (numbered as today); declaration determinism; guarantee, mitigate, verify; the explicitness ledger|
|`ch11-06-decision-records.md`|Decision Records|every decision record of both documents|
|`ch11-07-testing-layers.md`|Testing Layers|unit, fake-port pipeline, adapter, end-to-end and example layers, and which crate holds each|
|`ch11-08-where-buildl-sits.md`|Where buildl Sits|the à la Carte taxonomy, scheduler and rebuilder, buildl as a build system first with task-runner use a degenerate case, the capability gradient, the summary card|
|`ch11-09-what-it-promises.md`|What buildl Can and Cannot Promise|the scope note (no per-file C/C++ compilation in v1, discovery targets instead of depfiles); what each isolation tier does not guarantee|
|`appendix-00.md`|Appendix||
|`appendix-01-cli.md`|A. Command-Line Reference|every command and flag, read from the binary's `--help`|
|`appendix-02-buildl-toml.md`|B. `buildl.toml` Reference|every table and key the parser accepts|
|`appendix-03-lua-api.md`|C. Build File API|every function the `buildl` table installs|
|`appendix-04-exit-codes.md`|D. Exit Codes|every exit code the binary returns and what causes it|
|`appendix-05-references.md`|E. References|the documents' numbered reference lists, merged and renumbered|

Appendices A to D are read from the code (the binary's `--help`, the `buildl.toml` parser, the
`buildl` module's install, the exit-code mapping), never from the old documents.

## 3. Where each part of `docs/` goes

Every heading of the four documents, from `grep -n "^#" docs/*.md`.

### `docs/design.md`

|Heading|Goes to|
|---|---|
|1 Overview; 1.1 Why Lua, why airsl|`introduction.md`; `ch11-06` (the Lua/airsl choice as a decision)|
|2 The two-phase model|`introduction.md`, `ch11-01`|
|3 Workspace layout and artifacts|`ch01-02`, `ch03-02`|
|4 `buildl.toml`|`ch04-02`, `appendix-02`|
|5 The Lua declaration API|`ch03-01` to `ch03-03`, `appendix-03`|
|6 Grant negotiation|`ch04-01`, `ch04-03`|
|7 The build workflow|`ch01-03`, `ch05-03`, `ch05-04`|
|8.1 Load; 8.2 Resolve; 8.4 Execute|`ch11-01`|
|8.3 Plan: the action key|`ch05-01`, `ch11-01`|
|8.5 One action's lifecycle; 8.7 Output capture and failure|`ch11-02`|
|8.6 Watch mode|`ch10-00`|
|9 Worked examples; 9.1 Go|`ch02-00`|
|9.2 TypeScript with Bun|**dropped** — never built; no example exists|
|9.3 Rust|`ch02-00`, in the form `examples/rust` uses (`cargo install --root $out`); design's own `cargo build --release` form does not build (examples spec §1)|
|9.4 Docker|**dropped** — never built; no example exists|
|9.5 A polyglot monorepo|`ch03-04`|
|9.6 Toolchains: tools are dependencies too|`ch04-01`|
|10 Plugins; 10.1 Customization|`ch08-01`, `ch08-03`; discovery targets → `ch07-00`|
|10.2 Build settings|`ch03-05`|
|10.3 Stack plugins|`ch08-01`|
|11 Scale; 11.1 Big monorepos|`ch11-01`|
|11.2 Remote cache, then remote execution|`ch09-00`|
|12.1 Declaration phase|`ch11-05`|
|12.2 Isolation is a ladder|`ch06-00`|
|12.3 Guarantee, mitigate, verify|`ch06-00`, `ch11-05`|
|12.4 The explicitness ledger|`ch11-05`, `ch11-09`|
|13 Sequencing|**dropped** — a build-order history the roadmap under `.claudestacks/sdlc/roadmaps/buildl/` already records; its two scope decisions are kept: no per-file C/C++ compilation in v1 → `ch11-09`; a build system first, task-runner use a degenerate case → `ch11-08`|
|14 Open questions|its settled rules are kept: a directory-only `//dir` label is rejected → `ch03-01`; `subdir` is explicit only → `ch03-04`; plugin API versioning → `ch08-01` (`docs/design.md:1040-1044`); relative labels and the discovery second-pass rule are already in architecture §2 and §3.6; the questions themselves are **dropped**|
|References|`appendix-05`|

### `docs/architecture.md`

|Heading|Goes to|
|---|---|
|1 Workspace layout; 1.1 Crate dependency graph|`ch11-03`|
|2 Core types; 2.1 The target state machine|`ch11-01`|
|3 Phase implementations; 3.1 `declare`; 3.2 `graph`; 3.3 `plan`; 3.4 `exec`|`ch11-01`|
|3.5 `store`|`ch11-02`|
|3.6 `discovery`|`ch07-00`, `ch11-01`|
|3.7 `plugins`|`ch08-03`|
|3.8 `remote`|`ch09-00`|
|3.9 `watch`|`ch10-00`|
|4 Error architecture|`ch11-04`|
|5 Determinism engineering rules|`ch11-05`, rule numbers kept|
|6 Decision records|`ch11-06`|
|7 Testing strategy|`ch11-07`|
|8 Open implementation questions|**dropped** — as design §14|
|References|`appendix-05`|

### `docs/architecture-building-blocks.md`

|Heading|Goes to|
|---|---|
|1 Principles; 1.1 Dependency inversion; 1.2 What `buildl-core` may depend on; 1.3 The name `buildl`|`ch11-03`, with 1.2's derivation and per-family counts|
|2 System context; 3 Containers; 4 Components; 5 Code (5.1 Domain data, 5.2 Ports, 5.3 `Pipeline` and `Ports`, 5.4 Pure logic)|`ch11-03`|
|6 Port catalog|`ch11-03`|
|7 Dependency rules|`ch11-03`, rule numbers kept|
|8 Testing layers|`ch11-07`|
|9 Publishing|`ch11-03`|
|10 Decision records|`ch11-06`|
|11 Open questions|**dropped** — as design §14|
|References|`appendix-05`|

### `docs/fundamental-walkthrough.md`

|Heading|Goes to|
|---|---|
|1 The à la Carte taxonomy; 2 Scheduler + rebuilder|`ch11-08`|
|3 The action key: one concrete trace|`ch05-01`|
|4 Untrusted clone|`ch04-01`|
|5 The polyglot monorepo ripple|`ch05-02`|
|6 The depfile problem|`ch07-00`, `ch11-09`|
|7 buildl as task runner; 8 The capability gradient; 9 Summary card|`ch11-08`|
|References|`appendix-05`|

The documents' `**Author:** … · **Date:** … · **Reviewers:** —` headers are dropped. Where an
old document says something the binary no longer does, the book says what the binary does.

## 4. Tooling

```text
book/
  book.toml        [book] title = "The buildl Book", authors = ["airsstack"], language = "en", src = "src"
                   [preprocessor.mermaid] command = "mdbook-mermaid"
                   [output.html] site-url = "/buildl/", git-repository-url, edit-url-template
                                 (airsl's form, naming airsstack/buildl), additional-js
  mermaid.min.js   written by `mdbook-mermaid install book`, committed
  mermaid-init.js  〃
  src/             §2
```

|Piece|Change|
|---|---|
|`.gitignore`|ignores `book/book/`, the build output|
|`.github/workflows/ci.yml`|a `book` job: checkout, `taiki-e/install-action@v2` with `tool: mdbook@0.4.52,mdbook-mermaid@0.16.2`, `mdbook build book`|
|`.github/workflows/pages.yml`|new, airsl's workflow without its installer step: `build` (`contents: read`; install both tools; `mdbook build book`; `actions/upload-pages-artifact@v5` on `book/book`) and `deploy` (`pages: write`, `id-token: write`, `environment: github-pages`, `actions/deploy-pages@v5`); on a push to `main` touching `book/**` or `.github/workflows/pages.yml`, and `workflow_dispatch`; `concurrency: { group: pages, cancel-in-progress: false }`|
|`Makefile.toml`|`book` (`mdbook serve book --open`) and `book-build` (`mdbook build book`), in a `Book` category, installing both tools on first use at the exact versions through cargo-make's `install_crate = { …, version = "<v>" }`, not airsl's `min_version` floor: a floor lets a missing `mdbook-mermaid` install as 0.17.x, which needs mdBook 0.5 (cargo-make `README.md:2177-2183`, fetched raw: "If you want to ensure a specific version is used, you can define the **version** attribute instead")|

Both versions — mdBook `0.4.52` and `mdbook-mermaid` `0.16.2` — are written exactly in `ci.yml`,
`pages.yml` and `Makefile.toml`, each place carrying a comment naming the other two, as airsl
does. Diagrams stay Mermaid: each of the 38 is carried into its chapter as a ` ```mermaid `
block, redrawn where the binary has moved on.

Turning on Pages is the user's act: the repository's Pages source set to "GitHub Actions".
Until then the `deploy` job fails; the `book` job in CI does not depend on it.

## 5. Repointing what cites `docs/`

The six files of §1 are changed so that no line outside `.claudestacks/` names `docs/`, a spec
document or a spec `§` number:

|File|Change|
|---|---|
|`Cargo.toml`, `deny.toml`, `Makefile.toml`, `crates/buildl-core/clippy.toml`|each citation names the chapter file instead, e.g. `architecture.md §5 rule 2` → `book/src/ch11-05-determinism-rules.md rule 2`, `ABB §7 rule 1` → `book/src/ch11-03-four-crates.md, dependency rule 1`; rule numbers are kept in the chapters so these stay true|
|`README.md`|links to `docs/*.md` (`README.md:7` and the rest of the 7 lines) point at the matching `book/src/*.md` files; the `docs/` entry of the tree listing (`:245`) becomes `book/`; "from `design.md` §13" (`:256`) becomes a link to `.claudestacks/sdlc/roadmaps/buildl/roadmap.md`, which holds the sequencing; "open an issue against `docs/design.md` or `docs/architecture.md`" (`:269`) becomes "open an issue"; `:284`'s Bazel URL stays; no other edit (the readmes chain rewrites it)|
|`CLAUDE.md`|"Documents and their roles" (`:13-16`) names the book and its parts instead of the four documents; `:9` cites `book/src/ch07-00-discovery-targets.md` and `book/src/ch11-09-what-it-promises.md` for the depfile reasoning and the scope note; `:17`'s "design §13 items 1–8" becomes "the milestones `01`–`08`"; the rule at `:20` becomes: chapter files under `book/src/` are cited by name from `Cargo.toml`, `deny.toml`, `Makefile.toml` and `clippy.toml`, and numbered rules keep their numbers — keep both when restructuring the book; the Conventions bullet at `:47` becomes: the book's references are `appendix-05-references.md`, and new content cites into that list (the author headers are gone); the "Chat ASCII, document Mermaid" rule (`:92`) names `book/` instead of `docs/`; no other edit|

`docs/` is then deleted. `.claudestacks/` — chains, roadmap, and `rfcs/`'s older copies of the
four documents — is left as it is.

## 6. How it is checked

|Check|How|
|---|---|
|The book builds|`mdbook build book` exits 0 locally (`cargo make book-build`) and in the CI `book` job|
|Every output shown is real|each shell transcript in the book is captured by running the debug binary of the chain's head, on `examples/` or a scratch workspace, while the chapter is written; none is typed from memory|
|No reference to the old documents remains|`grep -rIl "§\|\(^\|[^/]\)docs/\|design\.md\|architecture\.md\|architecture-building-blocks\|fundamental-walkthrough\|ABB" --exclude-dir=target --exclude-dir=.git --exclude-dir=.claudestacks --exclude-dir=.claude --exclude-dir=docs .` finds only `crates/buildl-core/src/types/changed_path.rs` (its sample path); the `[^/]` keeps URL paths such as `bazel.build/docs/` and the vendored protos' Google URLs out. Control, run 2026-10-28 before any change: the same command finds exactly `Cargo.toml`, `CLAUDE.md`, `crates/buildl-core/clippy.toml`, `crates/buildl-core/src/types/changed_path.rs`, `deny.toml`, `Makefile.toml`, `README.md`|
|Nothing dropped silently|§3 names a destination or a reason for every heading; a plan marks each row done as its chapter lands|
|Diagrams render|the built `book/book/` holds `mermaid.min.js`, and a chapter with a diagram renders `<pre class="mermaid">`|
|The workspace gate still passes|`cargo make dod` and `cargo deny check` green after the citation edits (comments in `Cargo.toml`, `deny.toml`, `clippy.toml` and `Makefile.toml` only)|

## 7. Order of work

The book is written in parts that each build on their own: tooling and an empty skeleton first,
so `mdbook build book` passes from the first commit; then the tutorial, the feature chapters,
Under the Hood and the appendix; then the citation repointing and the deletion of `docs/` last,
once every row of §3 has landed.

## 8. Risks for the plan

- `cargo install --git https://github.com/airsstack/buildl buildl-cli --locked` is not verified
  end to end; the Installation chapter's command is run once before it is written.
- `mdbook-mermaid` 0.16.2's bundled Mermaid version may not parse every diagram in `docs/`; each
  chapter carrying a diagram is checked in the built HTML.
- The old documents describe parts of buildl that later milestones changed; the binary is the
  authority, and a chapter that finds a mismatch writes what the binary does.
- A gap found in buildl while writing — a command behaving differently from its `--help` —
  is raised to the user, not worked around in the text.

## 9. Non-goals

- Rewriting `README.md`, the crate READMEs or `CLAUDE.md` beyond §5 (the readmes chain).
- Publishing the crates (the first-publish chain); the Installation chapter shows the ways that
  work while the crates are unpublished.
- API documentation for docs.rs.
- A custom domain, translations, versioned copies of the book.
- Moving to mdBook 0.5.
- Changing buildl's behaviour.
