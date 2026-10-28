# Decision Records

Most of `buildl`'s shape comes from a few dozen decisions, each made once, for a reason, and
written down so nobody has to make it again. This page holds all of them. Each row gives the
question, the choice, and why. If you want to change one, the reason is what you have to argue
with.

They are grouped by subject. A decision that changed later says so in its row.

## The language

| Decision | Choice | Because |
|---|---|---|
| What build files are written in | **Lua, evaluated on airsl** | Plain data such as YAML or TOML cannot express a loop or a condition, and a full scripting language can express things a build file should never do. Lua in a sandbox sits between them: real loops and functions, with a surface the host decides. airsl supplies exactly that: Lua 5.4 linked in, with no interpreter to install and integers kept apart from floats, so JSON stays byte-stable; a policy model of language surface, grants and resource limits; a `HostModule` seam, so `buildl`'s functions install as their own module without forking airsl; sorted JSON and sorted directory walks; and engines cheap enough (about 128 µs to build one) to give every build file its own. xmake shows people want Lua for build files; `buildl` differs by making the sandbox, not convenience, the organising idea. |

## Crates and structure

| Decision | Choice | Because |
|---|---|---|
| Where the core lives | **A crate, `buildl-core`, not a `core` module** | A crate-root `mod core` hides Rust's built-in `core` crate: `use core::fmt::Debug;` fails with `error[E0432]: unresolved import core::fmt`. A crate boundary also turns the dependency rules into compiler errors. |
| Crate cut | **Four crates** | Only airsl brings a C build and an upgrade risk of its own, so only the Lua adapter earns its own crate; the other adapters share `buildl` until there is a concrete reason to split. `cargo test -p buildl-core` never compiles Lua. |
| Structure | **Four dependency-inverted crates** | Every flow can be tested against fake ports. [Four Crates, Ports and Adapters](ch11-03-four-crates.md) has the whole picture. |
| Lua exposure | **The `buildl-lua` crate only** | An airsl or mlua upgrade touches one crate, and `buildl-core` compiles without Lua because airsl is not among its dependencies. |
| Composition root | **`buildl`, not `buildl-cli`** | Editors, other tools and watch mode get a wired `Workspace` without copying the command line's wiring. Watch mode is built this way: `Workspace::watch` keeps the root, the approvals directory and the search path, and opens a `Workspace` for each cycle. The end-to-end tests live beside the real adapters. |
| Re-exporting the core | **`pub use buildl_core;`, so the crate is reached as `buildl::buildl_core`** | `pub use buildl_core as core` cannot work: in a crate root that also names `core::fmt` it is `error[E0432]`, and a crate that glob-imports it and names `core::fmt` gets `error[E0659]: core is ambiguous`. A third name would invent vocabulary. The facade `buildl::Workspace` and the manifest value `buildl::buildl_core::Workspace` stay apart. |
| Adapter isolation (building blocks) | **Checked by `cargo make guard-adapter-isolation`** | Seventeen modules in one crate, sixteen adapters and `plugins`, is where a cross-import slips in. What they share lives in `state` and `search_path`, which implement no port. |
| Adapter isolation inside `buildl` (architecture) | **A gate task, not review** | An adapter module may name the two shared modules, `state` and `search_path`, and nothing else of the crate. A rule a reviewer has to remember is a rule that slips; the gate runs on every change. |
| The plugin lock and store (building blocks) | **`plugins`, a module of `buildl` that serves no port** | Plan never reads the lock or the store, so no trait stands between the core and them: `local_ports` runs the check once and hands the directories to the declaration source, and `workspace` runs the fetch. The module is isolated like an adapter. |
| Where the plugin lock and store live (architecture) | **The same `plugins` module, which names only `state` and `search_path`** | Only `Wiring::source` and `Workspace::fetch_plugins` touch them, so a port would be a trait with one caller. The values it shares with `manifest` live in `buildl-core`, so the isolation gate covers it like any adapter. |
| Diagram notation | **Mermaid `graph` with C4 element labels** | It renders wherever the other Mermaid renders; Mermaid's own C4 syntax is marked experimental. |

## Ports

| Decision | Choice | Because |
|---|---|---|
| Port dispatch | **Generics, with one `Ports` bundle** | Static dispatch, without a type parameter per port on `Pipeline`. |
| Port openness | **Open traits, not sealed** | Adapters are written in other crates, and a sealed trait cannot be implemented outside its own. Ports grow through methods with default bodies. |
| When ports are wired | **Per call: every port for `plan`, `build`, `test`, `verify` and `gc`; a source of build files, the log and the cas for `check`, `graph` and `query`, plus the manifest for `doctor`; the log and the cas for `log`; none for `clean`** | `Pipeline` needs every port, and a command that reads only build files and discovery results must not need, lock or be stopped by a state file: a corrupt `cache.json` does not stop `check`. The state lock is held for one call, so an editor that keeps a `Workspace` open does not hold the directory while idle. |
| Who owns the reporter | **The caller: `Workspace::build` is lent `&mut R` for one build** | Only `build` reports, and an embedder brings its own reporter. `LocalPorts<R, C>` is generic over it; the `Console` an interactive approver asks on is lent the same way, as `C`. |
| Scheduler | **A pure `Schedule`, and a `Dispatcher` port** | Keep-going and skipping can be tested without threads, and the scheduler is the only writer, so its counters need no atomics. |
| Load granularity | **`DeclarationSource` evaluates one file** | The queue of directories and the sorted merge decide determinism, so they stay in `buildl-core` where fakes can test them. |
| Source globbing | **On the host side, in `buildl-lua`** | `b.sources()` walks with `globset` and `walkdir`, so the declaration engine holds no grant and `airsstack.glob.walk` is refused there; sorting, symlinks and staying inside the workspace are decided in one place. The directories the walk skips are a required argument of `LuaSource::new`, filled by the composition root with the output and state directories, so `buildl-lua` reads no manifest. |
| Reaching discovery results | **A second argument of `DeclarationSource::evaluate`, a `Discovered` that Load hands unchanged to every evaluation** | An evaluation stays a function of its file and that one value, on whatever worker it runs. Nothing is published yet, so the signature changed in place and the compiler named every implementor. |
| What an approver is told | **`Approver::approve(wanted, attribution)`: the attribution explains, the wanted set decides** | An approval is remembered by the wanted set alone, so an approver that ignores the attribution (`ManifestApprover`, `DenyApprover`) is correct, and one that shows it (`InteractiveApprover`) cannot change what a yes covers. |
| The `Ports::Log` bound | **`EventLog + RunHistory`** | `Pipeline::plan`, `Pipeline::verify` and `Pipeline::build` read discovery results from the pipeline's own log rather than through a port of their own, and the log adapter implements both. |
| The remote cache's port | **`RemoteCache`, blocking, with the checks in the core and `BlobSource` a `dyn` callback** | Workers on plain threads call it at once, and a blocking port keeps the scheduler and the core synchronous whatever the adapter needs below. The core judges every answer against a fake. The adapter may not import the content store, so an upload names the blobs it needs through a callback the core hands across; with the dispatcher's progress closure, that is one of the two places a trait object is passed. |

## Threads and concurrency

| Decision | Choice | Because |
|---|---|---|
| Async runtime | **None: plain threads for the build and its scheduler. Changed later: a tokio runtime confined to the remote adapter, reached only through `block_on`** | The work is starting processes and hashing files: about as many workers as cores, with no fan-out beyond that. An async runtime buys nothing and costs a dependency tree. The remote cache changed it, because tonic, the gRPC client, needs a runtime. The `remote` module creates and owns it, workers call it through `block_on`, and nothing else names tokio. `deny.toml` keeps tonic, prost and uuid inside `buildl`, and `guard-crate-edges` keeps every other member off tokio. Remote execution, which was expected to reopen the question, did not: it is more threads blocking on the one runtime, up to `[remote] jobs` at once. |
| Worker and scheduler channel | **std `mpsc`** | One set of producers, one consumer, no `select` needed. |
| Parallel hashing | **rayon, confined to the `Digester` adapter** | Chunks of CPU-bound work are exactly what it is for; keeping it in one adapter leaves the executor with one threading model. |
| How a running build reaches the reporter | **Messages through the dispatcher: `dispatch(work, step)`, where `work` is handed a way to tell and `step` takes each `Turn`** | The scheduling thread waits inside `dispatch`, and the dispatcher's own channel is the only way in that needs no second consumer. A target counts as started when a worker says it began, not when it is handed out: forty ready tasks on eight workers would otherwise show forty running. |
| Live output | **One action at a time; the others wait as blocks** | `--stream` shows the first action to begin while none is shown. Outcomes that end meanwhile are logged at once and shown after it, so output never interleaves and parallelism is kept; the outcome is the same with or without the live view, so the stored bytes cannot differ. |
| A dispatcher that loses a task | **An error of the build, `Error::DispatcherFault`; for Load, `Error::LostBuildFiles`** | A build that answers `Ok` with fewer outcomes than targets reports success for work never done. Each fault names what was lost, as data, so each is its own variant. |

## Hashing, files and storage

| Decision | Choice | Because |
|---|---|---|
| Graph library | **Own arenas; petgraph declined** | The graph is built once, never changes, needs three algorithms, and must serialize stably. |
| Hash | **SHA-256** | Compatibility with the Remote Execution API outweighs blake3's speed, and the stat cache makes hashing rare on warm builds anyway. |
| Cache files | **JSON written by the canonical serializer, read back by the canonical reader** | Easy to diff and debug beats compact; airsl's sorted-key JSON is the precedent. The reader checks a key's digest against its parts and a row's outputs against its key, so a damaged file is `Error::CorruptState`, apart from a file that could not be read. |
| Floats in canonical JSON | **Refused, and banned at compile time** | Nothing in the model is a float, and a non-finite float serializes as `null`, which would make a key disagree with the value it came from. See [rule 2](ch11-05-determinism-rules.md#2-all-json-leaves-through-one-canonical-serializer). |
| `Timestamp` representation | **`u64` nanoseconds since the Unix epoch; no date library** | The clock feeds durations as well as timestamps, and `Instant` is banned in `buildl-core`, so a duration is the difference of two timestamps, which seconds could not express. Formatting belongs to whatever displays it. |
| What an output's digest is | **The digest of its `OutputManifest`, for a file and a directory alike** | A row, a log event and a dependent's key each hold one digest per output. One small manifest blob says whether it is a file, an executable file or a tree, so the executable bit is in the digest, and a file never shares a digest with a tree. |
| What a directory output may hold | **Regular files only; anything else is refused, not approximated** | A symbolic link, a special file, an empty directory and a name that is not Unicode cannot be put back exactly, so the action fails as `OutputRefused`. A stamp file is the way to handle such a tree. |
| The action cache on disk | **A snapshot, `cache.json`, and a journal, `cache.journal`, folded once per build** | A row must be on disk when `record` returns, and rewriting the whole file per row is quadratic: seconds for a thousand rows, where one appended line takes microseconds. Rows reach `cache.json` only sorted, so finishing order never shows. |
| Syncing to disk | **None, for any file** | A rename already covers the process dying, which is what [crash safety](ch11-02-crash-safety.md) promises. A sync would only help if the machine itself died, whose remedy is to remove `.buildl/` and `out/`, at some thirty times the cost of the write. |
| Protecting the cas | **Read-only blobs, an executable twin, and a link only where writing through it is refused** | An action that opens a dependency's output for writing would otherwise write into the store. The adapter checks once per store that writing to a read-only file is refused, and copies everywhere when it is not. A tool that changes the mode first can still damage a blob. |
| A second buildl in one workspace | **Refused, by a lock on `.buildl/lock` held for one `plan`, `build`, `test`, `verify`, `clean`, `gc` or `plugins fetch`** | Two builds at tier 1 write the same paths under `out/`. The lock is the operating system's, so it is freed when its holder dies, and `StateLock::acquire` lets it go on every failing path. `check`, `graph`, `query`, `doctor` and `log` take none. |
| Which stat-cache entries are kept | **Those of files modified more than 2 s before the run began, keyed by path** | An edit the stat cache cannot see leaves the modification time and size equal, so it falls inside the time's granule, and 2 s is the coarsest met. The price: a file edited in the two seconds before a run is hashed again next time. |
| When `graph.json` is written | **By every build, right after Resolve, always the whole graph** | The graph on disk then matches the rows and blobs beside it, and is the same bytes for `build` and `build <target>`. `plan`, `graph` and `query` record nothing. |

## Labels and declarations

| Decision | Choice | Because |
|---|---|---|
| Label syntax | **Absolute only, with a separate resolver** | A `Label` is `//dir:name` and nothing else, so nothing downstream asks whether one still needs resolving. `Label::resolve` is the one place the three forms a build file may write, `//dir:name`, `:sibling` and a bare `name`, become one. |
| Directory-only labels | **Refused, not read as `//dir:dir`** | One target with two spellings breaks the round trip from text to label and back, and admits two cache keys for the same target. |
| A rule's description | **Expanded with the command; display text outside the key** | It reaches a `ReadyAction` as its own type, `ExpandedDescription`, because an expanded list of paths has no length limit while a written `Description` is held to 1024 bytes. Nothing in Plan reads it, so editing only a `desc` leaves every target cached. |
| Whose code made a declaration | **`Origin` on every declaration and node, shown and checked, never in a key** | The same declaration has one key whoever wrote it, so moving a target from a build file into a plugin rebuilds nothing. A plugin's authority is checked at Plan instead, and every error naming a target prints the chain. |

## Planning and running actions

| Decision | Choice | Because |
|---|---|---|
| The execution settings and the key | **`workers`, `timeout` and `strategy` are not part of the key** | `workers` is scheduling. `timeout` decides whether an action finishes, not what it makes, and a timed-out action is never cached. `strategy` decides what an action can reach beyond its inputs, not what they are; keying on it would rebuild everything on each move up the ladder. A tool that records its directory or reads the clock can still give other bytes under `bare` than `sandbox`. |
| The execution settings as types | **`Execution { workers: Workers, timeout: Timeout, strategy: Isolation }`** | Each grammar is checked once. The manifest's `0` never leaves `Workers`: a reader matches `PerCore` or `Fixed`, or asks `Workers::on(cores)`. `Timeout` is whole milliseconds, never zero. `Isolation` is ordered like the ladder, so "tier 2 or above" is `>= Isolation::Sandbox`. |
| A program named through a build setting | **Found at Plan, under the settings in force** | A setting's value is known before anything runs, so the program gets the ceiling check, the lookup and a fingerprint like one written out. Plan also expands every command, so the last check before a build is one uniform check over every expanded command. |
| Who judges what an action left | **`collect` reports what it `Found`; `buildl-core` decides** | Input and output stay in the port, and every decision can be tested on plain values. Only a file's executable bit is kept. |
| A build of named targets | **The graph is narrowed between Resolve and Plan** | Plan asks for every input of every node it is given, so narrowing later would fail a build on a missing input of an unrelated target. Output places are still checked over the whole graph. Narrowing takes the run's `Purpose`: a test run that names a target that is not a test fails with `Error::NotATest`. |
| A command a signal ended | **Its own class, `Killed { signal }`, an action failure** | A process a signal ended has no exit status for `ActionFailure` to hold. A timeout is told by the worker's own flag, never by the signal. |
| Stopping an action | **The direct child only; no process group at tier 1** | A child in its own group stops getting the terminal's Ctrl-C, and std can neither signal a group nor forward a signal to one. What an action started is not stopped with it. |
| Signals to buildl itself | **No handler** | std has no safe way to install one, and the workspace forbids `unsafe`. Actions stay in `buildl`'s process group, so Ctrl-C ends both; storage survives a kill at any point, and an interrupted build prints no summary. |
| Finding a program | **One search path, read once, used by the tool resolver and both strategies through `exec/run.rs`; no trait between them** | The file run must be the file fingerprinted. A search path can be built from any list of directories, so tests need no seam. |

## The sandbox

| Decision | Choice | Because |
|---|---|---|
| Putting inputs in the sandbox | **Copies, mode kept; no symbolic link, no hard link** | A symbolic link makes a file's real path the workspace's: a script linked in found the workspace's `node_modules`, and the same script copied in did not. A hard link shares the file, so a tool that writes its input overwrote the workspace's copy. A copy is near-instant where the file system can share the bytes (measured on APFS), and correctness does not depend on that. |
| Where the sandbox directory lives | **Outside the workspace, under the temporary directory** | From inside the workspace, tools that search upward find the workspace's `go.mod`, `node_modules` or Cargo workspace. The price is that the path is not under `.buildl/`, so the state lock removes it, and `Scratch::claim` uses an existing one only if it is a real directory, owned by the lock file's owner, mode `0700`, and outside the workspace. |
| The sandbox directory's name | **Stable per target: the first 32 hex digits of the SHA-256 of the workspace root's path, then of the label** | The directory a compile runs in reaches its output: `rustc -g` in two directories made two different files. A random name would make a debug build differ on every run; a name from the label gives one target one path every time, and no two targets share one. |
| Where the pinned variables are written | **In `buildl-core`: `Isolation::pinned`, and one check in Plan** | The names and values are written once, beside the ladder, and the sandbox reads them from there. A target that declares one is refused at Plan as `Error::PinnedEnv`, a pure check. `SOURCE_DATE_EPOCH` is 1980-01-01 because a ZIP file cannot hold an earlier time. |
| What `verify` does with `out/`, and what its sandbox does | **`verify` puts `out/` back from the cache first, and its sandbox leaves output paths alone** | Re-runs go through the dispatcher on several workers, so a re-run that cleared its output paths could remove what a re-run of a dependent was copying. `verify` restores every clean target's outputs first, builds its sandbox with `OutPaths::Untouched`, and never places a re-run's outputs. `build` and `test` keep `OutPaths::Cleared`. |

## Approvals and plugins

| Decision | Choice | Because |
|---|---|---|
| Where approvals are remembered | **Outside the workspace: `<data>/buildl/approved/<workspace id>.json`, `<data>` being `$XDG_DATA_HOME`, else `$HOME/.local/share`** | A repository can ship its own `.buildl/`, so an approval kept inside it would come with the code it approves. direnv keeps its approvals outside the project for the same reason. Only an absolute path counts, and a failure is never an approval. |
| Plugin caps against the ceiling | **The plugin check runs before the ceiling check; a plugin's reach is its manifest, the workspace's cap and the ceiling, intersected** | The manifest says what a plugin needs and the cap what the workspace lets it have. A want refused by either names the plugin, which the workspace-wide `Error::Refused` would not. No cap widens the ceiling. |
| What a plugin instance sees | **A new environment per instance, with one-level copies of the library and module tables** | A plugin that reached the build file's `buildl` could declare as the build file and slip past its cap, and one that wrote into a shared `table` could replace a function the build file calls later. Copies close both. |
| Checking a fetched plugin | **Digest the stored files against `buildl.lock` on every command that makes a source; fetching never repins on its own** | A changed file must not go unnoticed, and a check that trusted file times would miss an edit that keeps them. A difference, or a tag that moved, is `PluginChanged`, never a silent new pin. |

## Discovery targets

| Decision | Choice | Because |
|---|---|---|
| Discovery results | **Read back from the log and the cas for a second load; no result is not an error** | The log and the cas already hold what a target made, so there is no new store, and a command that reads only build files stays lock-free. A damaged log or a missing blob turns `check` and `graph` into "nothing has run"; only `build` and `test`, which have just run the target, treat an unreadable result as the fault it is. |

## The remote cache and remote execution

| Decision | Choice | Because |
|---|---|---|
| What a remote cache is asked | **A Remote Execution API `Action` built by the core, with `buildl`'s action key as its salt** | The server can only be asked in the API's terms, and the action key is not an `Action` digest. The salt carries everything the key holds, so two targets with different keys never share an entry; the core builds the value and the adapter encodes it, so no protobuf reaches `buildl-core`. |
| Who judges a remote answer | **`buildl-core`; the adapter only moves and encodes** | A result written by another machine is trusted for nothing: the exit code, the declared paths, a discovery output's kind, and every blob's length and SHA-256 are checked in the core before a byte reaches the content store. |
| A remote failure | **Lenient by default: warn once, run locally, and turn the remote off for the rest of the command; `--remote-strict` fails the build** | A cache that is down must not stop a build that can run without it, and a flapping one must not be asked again for every target. Strict is for CI, where a silent fall-back to a full local build is what you want to find out about. |
| Uploading from a bare action | **Refused** | At tier 1 an action can read files it did not declare, so its result depends on more than its key, and a shared cache would hand it to every machine. The refusal comes before the approver is asked. |
| The protocol code | **Vendored `.proto` files, committed generated code, and a generator outside the workspace** | Generating at build time would add a license outside the allowed set and a banned dependency, and put the generator's tools into every build. CI regenerates and diffs, so the committed code cannot drift. |
| Where the test server lives | **A `test-server` feature of `buildl`, off by default** | The adapter's tests, the end-to-end tests and the binary's tests all need the same in-memory server, and another crate can reach only a library's public API. The shipped binary does not enable it; `buildl-cli` takes it as a dev-dependency only. |

## Watch mode

| Decision | Choice | Because |
|---|---|---|
| File events | **`notify` 8.2.1, in `buildl` alone; its CC0-1.0 license by a one-crate exception; no debouncer crate** | It speaks FSEvents and inotify, and its minimum Rust version is under ours. `deny.toml` allows CC0-1.0 for `notify` only, so the general list holds only what the graph needs. A 50 ms settling window is small enough to write in the adapter. |
| What starts a build in watch | **A check over paths, never event kinds: a path that is not a declared input or a config file loads the build files, and a build follows only if the graph changed** | On macOS the event kinds are merged and unreliable, so whether a file is new cannot be read from an event. A new file reaches a build only through a load, so the load is the test: it builds when `b.sources()` now matches a file, and ignores a README, an editor's swap file or a comment. Read events are the exception: inotify reports every open, so they are dropped, and only a file closed after a write is kept. |
| Taking the state lock in watch | **Per cycle, inside the build only; the workspace is opened again for every cycle** | An open `Workspace` reads `buildl.toml` once, so a watch that kept one would never see an edit to the ceiling. Opening again makes each cycle exactly the `buildl build` you would have typed, and lets the lock go between cycles, so `plan`, `log`, `graph` and `doctor` work while watch waits. |

## Tests

| Decision | Choice | Because |
|---|---|---|
| Tools in tests (building blocks) | **System tools by argv; no compiled helper, no published kit of fakes** | Cargo gives a test the path of a built binary only in the package that owns it, so a helper would cost the published `buildl` crate a binary or a feature, or the workspace a fifth member. |
| Tools in adapter and end-to-end tests (architecture) | **System tools run by argv; `sh -c` in tests only** | The same reason, from the adapters' side: `sh`, `cp`, `sort`, `true`, `false` and `sleep` are on every machine that builds `buildl`, and `buildl` itself never starts a shell. |

[Testing Layers](ch11-07-testing-layers.md) shows where each kind of test lives.
