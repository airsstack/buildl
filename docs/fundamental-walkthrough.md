# buildl Against the Fundamentals — a Visual Walkthrough

A companion study document to _Build System Fundamentals_ and _Task Runner Fundamentals_ [1][2]. It maps the `buildl` design and architecture specs onto the theory, diagram by diagram, with concrete scenarios traced end to end. Each section pairs one fundamental concept with the exact buildl mechanism that implements it — and the one place where the mapping breaks.

---

## 1. Where buildl Sits in the à la Carte Taxonomy

_Build Systems à la Carte_ decomposes every build system into a **scheduler** (what order) and a **rebuilder** (run at all?) [3]. buildl occupies a precise cell in that grid.

```mermaid
%% buildl placed in the scheduler x rebuilder grid
quadrantChart
    title Scheduler strategy vs rebuilder strategy
    x-axis Dirty bit --> Constructive traces
    y-axis Topological --> Suspending
    quadrant-1 Dynamic and cached
    quadrant-2 Dynamic graphs
    quadrant-3 Classic static
    quadrant-4 Static and cached
    Make: [0.15, 0.15]
    Ninja: [0.45, 0.2]
    Bazel: [0.8, 0.55]
    Buck2: [0.85, 0.9]
    Nix: [0.9, 0.85]
    buildl: [0.8, 0.15]
```

buildl is a **topological scheduler with constructive traces**: the graph is fully known after Resolve, walked bottom-up, and every action's output is stored in a content-addressed store keyed by its action key. That is the same rebuilder as Bazel, paired with the simpler scheduler of Make and Ninja — a deliberate combination, because buildl's declaration phase produces a complete static graph, so the more complex suspending/restarting schedulers buy nothing. The taxonomy also predicts buildl's one gap (§6): a purely topological scheduler cannot handle dependencies discovered _during_ execution.

The dimension the grid cannot show is buildl's actual signature: **authority is derived from the graph, not assumed from the host**. No surveyed tool has that axis.

---

## 2. The Anatomy Diagram: Scheduler + Rebuilder Made Literal

The fundamentals describe a build system as a pipeline: request → scheduler → rebuilder → execute-or-skip [1][3]. buildl's five phases are that pipeline with the phase boundaries turned into serializable values.

```mermaid
%% buildl's five phases mapped to the a la carte anatomy
flowchart TD
    subgraph THEORY["Theory — a la carte anatomy"]
        REQ["Request"] --> SCH["Scheduler<br/>walk graph in dep order"]
        SCH --> REB{"Rebuilder<br/>inputs unchanged?"}
        REB -->|yes| SKIP["Skip — reuse output"]
        REB -->|no| RUN["Execute stale work"]
    end
    subgraph BUILDL["Practice — buildl phases"]
        L["Load<br/>airsl evaluates build.lua"] --> R["Resolve<br/>assemble the DAG"]
        R --> P["Plan<br/>action keys vs cache"]
        P --> X["Execute<br/>parallel workers"]
        X --> REC["Record<br/>cas row + log"]
    end
    SCH -.->|"is"| R
    REB -.->|"is"| P
    RUN -.->|"is"| X
```

The mapping is exact: Resolve builds what the scheduler walks, Plan _is_ the rebuilder (the action-key comparison), Execute runs only the dirty set. What buildl adds beyond the theory is that each arrow between phases is a value (`Vec<Declaration>` → `TargetGraph` → `Plan` → `Vec<ActionOutcome>`), which is why `buildl graph` and `buildl plan` exist for free — each CLI command is the pipeline truncated at one handoff.

---

## 3. The Action Key: One Concrete Trace

The rebuilder's entire intelligence is the action key. The fundamentals describe it abstractly — "hash of inputs + command + environment + toolchain" [1]. Here is one concrete trace through a Go service target.

```lua
-- services/api/build.lua
local srcs = { b.sources("cmd/**/*.go"), b.sources("internal/**/*.go"),
               "go.mod", "go.sum" }
b.target("server", { inputs = srcs, run = { "go", "build", "-o", "$out", "./cmd/server" } })
```

**Scenario:** you edit `internal/auth/login.go` and run `buildl build server`.

```mermaid
%% One edit traced through the rebuilder
flowchart TD
    EDIT["Edit internal/auth/login.go"] --> STAT["Stat cache check<br/>mtime changed — rehash the file"]
    STAT --> H["New content hash<br/>for login.go"]
    H --> KEY["Action key recomputed<br/>argv + input hashes + env values<br/>+ tool fingerprint + os/arch"]
    KEY --> CMP{"Equals cached key<br/>for //services/api:server?"}
    CMP -->|no| DIRTY["Dirty — reason recorded:<br/>input login.go hash changed"]
    DIRTY --> EXEC["Worker runs go build<br/>outputs to temp dir"]
    EXEC --> CAS["Hash output into cas<br/>then write cache row"]
    CAS --> CUT{"Output binary<br/>byte-identical to before?"}
    CUT -->|yes| CLEAN["Early cutoff —<br/>downstream stays clean"]
    CUT -->|no| PROP["Downstream keys change —<br/>docker image rebuilds"]
```

Every box corresponds to a named mechanism in the specs: the stat cache (design §8.3), the key components ledger (design §12.4), the cas-first crash-safety invariant (design §8.5), and early cutoff falling out of hashing outputs into the cas. The `plan` command's "why dirty" explanation is nothing more than the diff of the key components — which is why the architecture keeps `KeyComponents` alongside the digest instead of discarding them.

**Three follow-on scenarios, same target:**

|You do this|buildl does this|Because|
|---|---|---|
|Edit a comment in `login.go`, `go build` output is byte-identical|Runs `go build` once, then everything downstream is skipped|Early cutoff: output hash unchanged → dependent keys unchanged|
|Upgrade the Go toolchain, touch nothing|Rebuilds `server` and everything downstream|Tool fingerprint is a key component; a new `go` binary is a new key|
|Run the same build on a colleague's Mac|Full rebuild, no cache sharing|`os/arch` is in every key — a Mac and a Linux machine must never share entries|

---

## 4. Use Case: Untrusted Clone — the Differentiator in Action

The fundamentals treat hermeticity as a property of _execution_ [4]. buildl extends the same instinct to _evaluation_: a cloned repo's build files can be evaluated before any trust decision, because evaluation grants nothing. This is the scenario no tool in either fundamentals document can run safely.

**Scenario:** you clone a contributor's fork of your monorepo. Their PR adds a build file. You run `buildl plan`.

```mermaid
%% Untrusted clone — grant negotiation sequence
sequenceDiagram
    participant Dev as Developer
    participant Host as buildl host — Rust
    participant Lua as airsl engine — confined
    participant User as Approver prompt

    Dev->>Host: buildl plan
    Host->>Host: parse buildl.toml — the ceiling
    Host->>Lua: evaluate build files<br/>grant: fs read on workspace only
    Note over Lua: contributor's file calls<br/>b.target with run curl evil.sh
    Lua-->>Host: staging list — declarations only<br/>nothing executed
    Host->>Host: Resolve DAG, derive wanted set<br/>wanted includes curl
    Host->>Host: intersect with ceiling<br/>curl not in proc.run
    Host-->>Dev: REFUSED at plan time<br/>wanted curl, ceiling grants cc ld go docker<br/>declared by lib/build.lua line 14
    Note over Dev: nothing ran — the malicious<br/>step never had authority
```

Two properties of the fundamentals combine here in a way neither document anticipates: the **restricted configuration language** lesson ("the less expressive the language, the more the tool can reason about it" [1]) and **hermeticity** — but applied at declaration time. Bazel's Starlark cannot do I/O, but Bazel's `BUILD` evaluation still happens with the tool's ambient authority; buildl's evaluation happens with a single explicit grant. The wanted set is derived truth, not a manifest's promise, so the refusal message can name both sides precisely.

---

## 5. Use Case: the Polyglot Monorepo Ripple

The canonical build-system use case — cross-stack dependencies no single-language tool can see. This is design §9.5 traced through two different edits.

```mermaid
%% Polyglot monorepo — edit ripple analysis
graph TD
    PROTO["api.proto<br/>source of truth"] --> PG["proto go codegen<br/>protoc"]
    PROTO --> PT["proto ts codegen<br/>protoc"]
    PG --> API["services api server<br/>go build"]
    PT --> WEB["web bundle<br/>bun build"]
    API --> AIMG["deploy api_image<br/>docker build — iidfile"]
    WEB --> WIMG["deploy web_image<br/>docker build — iidfile"]
    AIMG --> STACK["deploy stack"]
    WIMG --> STACK
```

**Edit 1 — change `api.proto`.** Both codegen keys change → both columns dirty → Go and Bun columns rebuild _in parallel_ (no path between them), then the two docker images, then the stack. One edit, correct order, maximal parallelism, and if the regenerated Go code happens to be byte-identical (a comment-only proto change), early cutoff stops the ripple at `PG`.

**Edit 2 — change `web/src/App.ts`.** Only `WEB`'s key changes. The entire Go column is cache-clean — **`go` is never spawned**. The build's spawn list is itself evidence of the graph's correctness.

**Edit 3 — a diff lands in CI.** `buildl query "rdeps(//proto:go)"` answers "which targets and tests does this diff touch" — the reverse edges the executor already maintains, exposed as a query. For a large repo this transforms CI cost more than remote execution does, which is why the sequencing plan (design §13) puts `query` second.

---

## 6. The Gap, Visualized: the Depfile Problem

The one fundamental buildl does not yet cover: **dynamic input discovery** — Ninja's depfiles, where the compiler reports the headers it actually read [5]. The design doc's own §5 example demonstrates the hole.

```lua
-- design.md section 5 — per-file C compilation
b.target(b.path.stem(src) .. ".o", { rule = "cc", inputs = { src } })
```

```mermaid
%% The missing edge — declared graph vs true graph
graph TD
    subgraph DECLARED["Graph buildl sees — from declarations"]
        MC1["main.c"] --> MO1["main.o"]
        UC1["util.c"] --> UO1["util.o"]
        MO1 --> APP1["app"]
        UO1 --> APP1
    end
    subgraph TRUE["True dependency graph"]
        MC2["main.c"] --> MO2["main.o"]
        UC2["util.c"] --> UO2["util.o"]
        UH["util.h<br/>NOT DECLARED"] -.->|"missing edge"| MO2
        UH -.->|"missing edge"| UO2
        MO2 --> APP2["app"]
        UO2 --> APP2
    end
```

**Scenario:** edit `util.h`, run `buildl build app`. No declared input changed, so every action key matches the cache — **buildl reports all clean and serves a stale binary**. The behavior depends on the isolation tier:

```mermaid
%% What happens per isolation tier when util.h is undeclared
flowchart TD
    EDIT["Edit util.h — undeclared input"] --> T{"Execution tier?"}
    T -->|"Tier 1 — Bare"| STALE["SILENT STALENESS<br/>compile reads util.h from workspace<br/>key never sees it — wrong cache hits forever"]
    T -->|"Tier 2 — Input sandbox"| LOUD["LOUD FAILURE<br/>util.h absent from exec dir<br/>first compile fails: file not found"]
    STALE --> BAD["Worst outcome —<br/>a shared remote cache<br/>would amplify it"]
    LOUD --> OK["Correct but unusable —<br/>fine-grained C is effectively<br/>unsupported without depfiles"]
```

Tier 2 converts the correctness bug into a loud failure — the sandbox doing exactly its job [4] — but that only proves fine-grained C/C++ cannot be expressed, not that it works. The worked examples in design §9 avoid the problem by wrapping whole self-tracking toolchains (`go build ./...`, `cargo build`) with full source globs, which is sound. The resolution to record before development: either declare fine-grained C compilation out of scope for v1, or add Ninja-style depfile support — the executor parses the compiler's `.d` output, records _observed_ inputs beside the cache row, and folds them into the next run's key [5]. The depfile option is host-side only; the Lua surface never changes.

---

## 7. The Degenerate Case: buildl as Task Runner

The task-runner fundamentals establish that a build system contains a task runner, reachable through escape hatches [2]. buildl's escape hatch is `always = true`, and the verbs-calling-nouns layering from that document collapses into a single graph.

```lua
b.target("push", {
  deps = { "api_image" },
  run = { "docker", "push", "registry.example.com/api" },
  network = true,           -- declared, plan-visible exception
})
b.target("deploy", { deps = { "push" }, run = { "kubectl", "apply", "-f", "k8s/" },
  always = true })          -- a verb: never cached, never skipped
```

```mermaid
%% Nouns and verbs in one graph — the containment made concrete
graph LR
    subgraph NOUNS["Nouns — cached by action key"]
        SRC["go sources"] --> BIN["server binary"]
        BIN --> IMG["api_image<br/>iidfile digest"]
    end
    subgraph VERBS["Verbs — effects on the world"]
        PUSH["push<br/>network true"] --> DEP["deploy<br/>always true"]
    end
    IMG --> PUSH
```

The interesting middle node is `push`: it is a _cached side effect_, and that is correct here — an unchanged image digest means the registry already has these bytes, so skipping the push is the desired behavior, not the "deploy is up to date while production burns" disaster the task-runner document warns about [2]. `deploy` is the true verb and carries `always = true`. The classification test from that document ("ask the tool to do the same thing twice") gives the right answer at every node: nouns no-op, `push` no-ops when the digest matches, `deploy` runs every time.

This is also why the state machine's summary line matters: `1 failed, 2 skipped, 9 cached, 2 built` distinguishes states a pure task runner cannot represent — `Cached` and `Skipped` are the rebuilder's vocabulary.

---

## 8. The Capability Gradient, with buildl Placed

The task-runner document's gradient — names → ordering → caching → hermeticity — with the one-way door marked [2].

```mermaid
%% The gradient with buildl placed at the far end
flowchart LR
    N["Named tasks<br/>npm scripts"] --> O["Ordering<br/>just, Taskfile"]
    O --> C["Trust-based caching<br/>Turborepo, Nx"]
    C --> H["Enforced hermeticity<br/>Bazel, Buck2, Nix"]
    H --> A["Declared authority<br/>buildl"]
    C -.->|"one-way door:<br/>hermeticity needs<br/>execution control"| H
    H -.->|"second door:<br/>evaluation itself<br/>is sandboxed"| A
```

Turborepo's cache is only as correct as your declarations; Bazel enforces the declarations with sandboxing at execution time; buildl adds a step the gradient did not have — the _configuration language itself_ runs inside the enforcement boundary, and the build's total authority is derived from the graph and checked against a ceiling before anything runs. Each step right costs adoption effort; buildl's bet is that starting from declared authority (inherited from airsl) makes the last step cheaper than retrofitting it.

---

## 9. Summary Card

```mermaid
%% Coverage summary
mindmap
  root((buildl vs<br/>fundamentals))
    Covered
      Static DAG from declarations
      Topological scheduler
      Constructive traces — cas
      Early cutoff
      Isolation ladder tiers 1 to 4
      Determinism — verify, double-run
      Tools as inputs — fingerprints
      REAPI-shaped for remote
      Settings fold into the key
      Phony escape hatch — always true
    Gap
      Dynamic inputs — depfiles
      C headers scenario unsound at tier 1
    Deferred by decision
      Configurations — one target one platform
      Incremental Load — lazy loading pre-named
      Remote cache then execution
```

One sentence per branch: everything the two fundamentals documents name as load-bearing exists in the specs as a concrete mechanism; the single pre-development blocker is deciding the depfile question, because it is a correctness property, not a feature; and the deferred items all have pre-named fallbacks, which is what makes them reversible.

---

## References

1. _Build System Fundamentals_ (uploaded study document), synthesizing Mokhov, Mitchell, Peyton Jones et al.
2. _Task Runner Fundamentals_ (uploaded study document).
3. Andrey Mokhov, Neil Mitchell, Simon Peyton Jones. _Build Systems à la Carte_. Microsoft Research / ICFP, 2018. URL: <https://www.microsoft.com/en-us/research/uploads/prod/2018/03/build-systems.pdf>
4. Google / Bazel authors. _Hermeticity_ and _Sandboxing_. bazel.build. URL: <https://bazel.build/basics/hermeticity>
5. Evan Martin et al. _The Ninja build system — manual_ (depfiles, restat). ninja-build.org. URL: <https://ninja-build.org/manual.html>
6. Bazel authors. _remote-apis — An API for caching and execution of actions on a remote system_. GitHub. URL: <https://github.com/bazelbuild/remote-apis>
7. airsstack. _buildl design and architecture specs_ (project documents: design.md, architecture.md), 2026.
