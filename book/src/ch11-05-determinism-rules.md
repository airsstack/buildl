# Determinism Rules

One principle sits under everything on this page: **every difference in behaviour between two
builds must be traceable to a difference in a declared input.** The action key is the list of what
is declared. The sandbox makes sure nothing outside that list can matter. `verify` checks what is
left over.

The trouble with non-determinism is that it never fails loudly. It shows up as a cache that never
hits, or a file that differs on another machine for no reason anyone can see. So `buildl` treats
it as a bug, and has rules to keep it out at three levels: in its own Rust code, in the build
files, and in the actions.

## The engineering rules

These are the house rules for `buildl`'s own code. They can be checked in review, and some are
checked by the gate on every change. Code comments cite them by number, so the numbers do not
change.

### 1. No `HashMap` iteration reaches any output

Anything that is written to a file, shown to a person or hashed into a key iterates a `BTreeMap`
or a sorted `Vec`. A `HashMap` is fine as a lookup table; its order is never allowed to leak.

### 2. All JSON leaves through one canonical serializer

Object keys are sorted at every level, and floating-point numbers are refused. `graph.json`,
`cache.json`, `log.jsonl` and the bytes hashed into an `ActionKey` all go through it, so "the key
of X" and "the file of X" can never disagree.

The float ban is enforced twice, because neither half is complete alone. The serializer walks the
value after `serde_json` has turned it into a tree, so it catches every finite float a
dependency's `Serialize` might slip in. It cannot catch a non-finite one: `serde_json` has already
turned `NaN` and the infinities into `null`, which looks exactly like a real `null`. The real
guarantee is the ban on `f32` and `f64` in `crates/buildl-core/clippy.toml`, asserted by
`cargo make guard-core-purity`. The walk is the second line, not the first.

JSON comes back in through one place too. `canonical::from_slice` reads the value through its
type's own `Deserialize`, writes it again, and compares the bytes. So it accepts exactly what the
serializer writes, and nothing else: a missing or unknown field, keys out of order or written
twice, extra whitespace, or an escape the serializer would not use are all refused. The build log
is read the same way, a line at a time, and a line the reader refuses is skipped and counted as
damaged rather than failing the whole file.

### 3. Parallelism is never observable

Build files load in parallel, but the results are put back in order and merged sorted. Files are
hashed in parallel, but each result goes into its own slot. The order in which actions finish shows
up only in the log's timestamps, never in any output, cache file or key.

### 4. The wall clock is quarantined

The current time is read in exactly two places, the timestamps of log events and the durations
worked out from them, and only through the `Clock` port, whose one real adapter lives in `buildl`.
Two other times exist, and neither reaches an output as a reading of the clock. The `exec` adapter
reads the monotonic clock for an action's deadline, a value that reaches no key, file or log. The
stat cache's "began" is a file's modification time: the one the file system gave the state lock.

The crate boundary does not enforce this, since `std::time` is open to every crate.
`crates/buildl-core/clippy.toml` bans `Instant`, `SystemTime` and `SystemTimeError`, and
`cargo make guard-core-purity` runs that ban inside `cargo make dod`. It is lint configuration,
not a grep and not the compiler, that keeps the rule true.

### 5. Double-run checks are tests, not doctrine

`buildl check` evaluates the build files twice and compares the two sorted lists of declarations
by value (with a discovery target, the two loads that read its result). The test suite builds each
of two fixture workspaces twice, in two directories, once with four workers and once with one, and
asserts that `out/`, `graph.json`, `cache.json` and the cas are byte for byte the same
(`two_clean_builds_in_two_directories_are_byte_identical`, in `crates/buildl/tests/e2e`).
`log.jsonl` and `statcache.json` are left out of that comparison: the first holds clock readings
and finishing order, the second modification times.

## Build files: deterministic by construction

A build file runs on airsl, and airsl's `confined` preset is already close. Two changes close the
rest.

**What could vary is taken away.** The declaration engine uses airsl's smallest language surface,
which has no `os` at all, so `os.time` and `os.clock` are gone. The host modules are `json`, `path`,
`regex`, `hash` and `glob`, and `time`, `proc`, `env` and `stdio` are left out. The smallest surface
still loads `math` whole, so `buildl` removes `math.random` and `math.randomseed` itself. It also
removes `airsstack.path.absolute`, which reads the working directory, and `print`, because standard
output belongs to the command line. Last, it replaces `tostring` and `string.format`: rendering a
table or a function shows its address, which changes between runs, so rendering one with no
`__tostring` is an error, and so is `%p`. [The Toolbox](ch03-03-toolbox.md) shows what is left.

**Order is made not to matter.** Lua's `pairs` visits a table in no fixed order. Rather than ban
it, `buildl` makes the graph blind to the order declarations arrive in: targets are keyed by name
and numbered in label order when the graph is resolved. A `pairs` loop that declares targets gives
the same graph whatever order it runs in. The order inside one list is different. `deps`, `inputs`
and `outputs` reach the graph as written, and from there the action key, so a list whose order
changes between two evaluations is a real difference.

And because declaring is cheap, it can be checked. `buildl check` evaluates every build file twice
and compares the two sorted lists. A build file that declares something different the second time
fails with `Error::Nondeterministic`, which names the first place the two runs disagree and what
each run declared there. A `pairs` loop alone does not fail it, because both lists are sorted
before they are compared.

## Actions: guarantee, mitigate, verify

`buildl` cannot make a compiler reproducible. What it can do is pin every input the compiler sees,
and then check what is left. Each property gets the strongest treatment `buildl` can give it:

| Property | Status |
|---|---|
| same declared inputs, same action run | **guaranteed** by the action key |
| an action sees only its declared inputs | **enforced** from tier 2, for the files of the workspace; a file named by an absolute path, or outside the workspace, is still the action's to read |
| time zone, locale and clock differ between machines | **mitigated** from tier 2: the sandbox pins three variables and the modification time of every file it copies in; at tier 1 these are the machine's |
| the network as a hidden input | **removed** at tier 3, which is not built yet |
| the same action gives the same bytes | **up to the tool**: checked by `verify`, not promised |

What the sandbox pins, for every strategy above `bare` (`Isolation::pinned`, in
`crates/buildl-core/src/types/isolation.rs`):

| What | Value | Why |
|---|---|---|
| `LC_ALL` | `C` | sorting, messages and number formats do not follow the machine's locale |
| `SOURCE_DATE_EPOCH` | `315532800`, 1980-01-01T00:00:00Z | the time a tool stamps into what it writes; the earliest a ZIP file can hold, so a tool that packs a ZIP from it never fails |
| `TZ` | `UTC` | local time is the same everywhere |
| modification times | the same 1980-01-01 (`Isolation::BUILD_EPOCH`) | every file and directory copied into the sandbox carries it, so a tool that records file times, such as a ZIP packer, writes the same times on every run and every system |

A target may not set a pinned variable itself: under any strategy above `bare`, declaring one in
`env` fails Plan with `Error::PinnedEnv`, exit code 5. [The Sandbox](ch06-00-sandbox.md) shows all
of this from the outside, and [`verify`](ch05-04-clean-gc-verify.md#buildl-verify) is how what is
left is checked.

## The explicitness ledger

The ledger is the full list of what can make two builds differ, and how each one is made explicit:

| Input | Made explicit by |
|---|---|
| source files | `inputs`, hashed by content through the stat cache ([The Action Key](ch05-01-action-key.md)) |
| what other targets made | `deps`: the digests of their outputs go into the key ([Early Cutoff](ch05-02-early-cutoff.md)) |
| environment variables | only names the target declares, and their values go into the key |
| tools | a fingerprint of the program's file; a program a dependency built enters as that dependency's output |
| operating system and processor | a `platform` in every key, so a Mac and a Linux machine never share entries |
| the network | `network = true` is a declaration in the key, shown by `plan` and the approver; taking the network away from the rest is tier 3's job |
| where an output comes from | `stdout = true` is a field of the target and part of its key: turning it on or off rebuilds the target |
| time, locale, time zone | pinned by the sandbox, as above; a target may not declare them |
| the build file's own logic | reaches a key only through the fields it declares, not through a hash of the file; declaring is sandboxed, has no `os`, and is checked by running it twice |

Two things a workspace sets are left out of the ledger on purpose: the `[execution]` settings
(`workers`, `timeout`, `strategy`), and a rule's `desc`. Neither is part of any key. `workers`,
`timeout` and `desc` change nothing an action produces from its inputs. `strategy` changes none of
the key's parts either, but under `sandbox` the action runs in another directory with the pinned
values, so a tool that records its directory or reads the clock, the locale or the time zone can
give other bytes than under `bare`.

What the ledger leaves out, and what that means, is on
[What buildl Can and Cannot Promise](ch11-09-what-it-promises.md).
