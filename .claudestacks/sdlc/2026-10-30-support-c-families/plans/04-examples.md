---
status: approved
created: 2026-10-30
depends-on: [01, 03]
built-on: f917a9ac0d2d
---

# C and C++ Examples Implementation Plan

**Goal:** `examples/c` and `examples/cpp` build, test and verify through the `c` plugin fetched by
git from a subfolder, on Linux and macOS in CI, as spec §8 defines.

**Architecture:** each example is a workspace whose `buildl.toml` names the plugin as a user would
(`github`, `rev = "plugin-c-v0.1.0"`, `dir = "plugins/c"`). `examples/check.sh` gains one hook: an
example's `prepare.sh`, run on the copy before the shared steps, makes a local git repository from
this checkout's `plugins/c`, tags it, points the copy's `buildl.toml` at it and fetches. The examples
join the existing `cargo make examples` loop and CI job unchanged.

**Tech Stack:** POSIX `sh`, `git`, the `buildl` binary, gcc/clang on the runner images.

---

## Files that change

```
examples/check.sh              — [modify] export BUILDL_REPO; run prepare.sh when present, before the shared steps
examples/README.md             — [modify] a line on prepare.sh; rows for c and cpp
examples/c/buildl.toml         — [create] ceiling cc, ar, sh; env.read PATH; [plugins.c] github/rev/dir and cap
examples/c/build.lua           — [create] c.library, c.binary, c.test, a config.h target, the .S discovery and object
examples/c/include/util.h, include/banner.h, include/blob.h, lib/util.c, src/main.c, src/blob.S, test/util_test.c — [create] sources
examples/c/prepare.sh          — [create] local plugin repository, tag, rewrite, fetch
examples/c/assert.sh           — [create] output, header-edit, test checks
examples/c/README.md           — [create] what it shows and how to run it
examples/cpp/buildl.toml       — [create] ceiling cc, c++, sh; env.read PATH; [plugins.c]
examples/cpp/build.lua         — [create] the outline of spec §8
examples/cpp/include/greet.h, include/banner.h, lib/greet.cc, src/main.cc, test/greet_test.cc — [create] sources
examples/cpp/prepare.sh        — [create] as examples/c's
examples/cpp/assert.sh         — [create] output, header-edit, shared-library, test checks
examples/cpp/README.md         — [create]
```

## Facts this plan relies on

| Fact | Source |
|---|---|
| `check.sh` copies the example into `${work}` (:25-31), then runs `check`, `build`, `test`, every-target-clean, `verify`, `assert.sh` (:68-73) through `step` (:37-46) | spec P31; examples/check.sh read at commit f0f427d |
| `cargo make examples` runs `check.sh` on every `examples/*/` with an `assert.sh`, except `remote` (Makefile.toml:369-383); CI's examples job runs it on `ubuntu-latest` and `macos-latest` (ci.yml:69-109) | spec P32 |
| a commit in a fresh repository is made with `-c user.name=… -c user.email=…` | spec P37 |
| the images carry gcc, clang and git | spec P58 |
| a generated header is reached through a dependency and `-Iout`; `-MM` names it `out/config.h` | spec P23 |
| a data-only `.S` assembles and links on both systems | spec P44, P57 |
| a comment-only edit to a header recompiles the objects that include it and leaves the link cached (early cutoff) | spec P1 (`util.h` comment → `2 built, 1 cached`) |
| shared libraries load beside the binary with the `c.rpath` values; macOS needs `c.soname` | spec P48, P50, P55 |
| the version probe is off by default, so every-target-clean holds | spec P21, §5.2 |
| `plan --json` and test labels are absolute (`//:name`) | examples/go-discovery/assert.sh |
| the plugin API and helpers | spec §5, §6; plan 03 |
| `dir` and the fetch from a subfolder | spec §2; plan 01 |

## Order of work

### Task 1 — `prepare.sh` in `check.sh`

**Files:**
- Modify `examples/check.sh` — before the copy, set and export `BUILDL_REPO` to the repository root (the parent of the folder holding `check.sh`); after `rm -rf out .buildl` (:31), when the copy holds `prepare.sh`, run it as a step named `prepare.sh` through `step`; update the header comment
- Modify `examples/README.md` — a sentence in *Checking an example* (:18-40) on `prepare.sh` and `BUILDL_REPO`

**Change:** an example can prepare its copy before the shared checks.

**Proof:** `cargo make examples` passes with the existing examples unchanged (none has `prepare.sh`); Tasks 2 and 3 exercise the hook.

**Done when:** `cargo make examples` passes, `cargo make dod` and `cargo deny check` are green.
Commit `feat(repo): let an example prepare its copy`.

### Task 2 — `examples/c`

**Files:** the `examples/c/` files listed above.

**Change:** a C workspace that uses every C-side function and helper of the plugin:
- `buildl.toml`: the user tables of spec §4 with `proc.run = ["cc", "ar", "sh"]` in the ceiling and the cap, `env.read = ["PATH"]`
- sources: `include/util.h` (read by `lib/util.c`, `src/main.c`, `test/util_test.c`); `include/banner.h`, read **only** by `src/main.c`; `include/blob.h`, read only by `src/blob.S`, holding the value and a symbol macro adding `_` under `__APPLE__` (Mach-O C symbols carry a leading underscore, ELF ones do not)
- `build.lua`: `:config.h` written by `sh` (`#define GREETING …`); `c.library("util", …)` from `lib/util.c`; the build file's own discovery target over `src/blob.S` (`cc -Iinclude -MM -MT obj src/blob.S`, `stdout = true`), its result read with `c.depfile.parse` into an object target `blob.o`; `c.binary("hello", …)` from `src/main.c` with `deps = { ":util", ":blob.o" }`, `header_deps = { ":config.h" }`, `headers` the three `include/` headers, `cflags` from `c.includes({ "include" })` plus `-Iout` and `c.defines(…)`; `c.test("util_test", …)` with `deps = { ":util" }`
- `prepare.sh`: make a temporary folder with `mktemp -d` and remove it on exit; in it `git init` a repository, copy `$BUILDL_REPO/plugins/c` to `plugins/c`, commit and tag `plugin-c-v0.1.0` with a command-line identity; replace the copy's `github = "airsstack/buildl"` line with `git = "file://<repo>"`, failing if that line is not found; run `"$BUILDL" plugins fetch` (the store keeps its own copy, so the repository may go afterwards)
- `assert.sh`: `out/hello` prints the expected line; after appending a comment to `include/banner.h`, `"$BUILDL" build`'s last summary (the build proper) reports exactly one target built — the object of `src/main.c` — and the rest cached, the link included (early cutoff); `"$BUILDL" test` passes
- `README.md`: what each part shows, the `buildl.toml`, how to run it with the published tag and through `check.sh`

**Proof:** `BUILDL=target/debug/buildl sh examples/check.sh examples/c` prints `c: ok` on macOS; CI runs it on Linux.

**Done when:** that command and `cargo make examples` pass, `cargo make dod` and `cargo deny check` are
green. Commit `feat(repo): add the C example`.

### Task 3 — `examples/cpp`

**Files:** the `examples/cpp/` files listed above.

**Change:** a C++ workspace following spec §8's outline:
- `buildl.toml`: `proc.run = ["cc", "c++", "sh"]` (`cc` for `c.platform`) in the ceiling and the cap, `env.read = ["PATH"]`
- sources: `include/greet.h` (read by `lib/greet.cc`, `src/main.cc`, `test/greet_test.cc`); `include/banner.h`, read **only** by `src/main.cc`
- `build.lua`: `c.platform()`, `c.shared_name("greet", platform)`, `c.shared_library("greet", …)` with `cxxflags` from `c.includes` plus `-fPIC` and `ldflags = c.soname(lib, platform)`, `output = lib`; `c.binary("hello", …)` and `c.test("greet_test", …)` with `ldflags = c.rpath(platform)` and `deps = { ":greet" }`; `headers` both `include/` headers
- `prepare.sh`: as Task 2's
- `assert.sh`: `out/hello` prints the expected line; the shared library is `out/libgreet.so` on Linux and `out/libgreet.dylib` on macOS (`uname -s`); after appending a comment to `include/banner.h`, the build proper reports exactly one target built — the object of `src/main.cc` — and the rest cached; `"$BUILDL" test` passes with the library loaded
- `README.md`: the two passes (spec §5.5), why `c.platform` is `nil` at first, why the test lists the library

**Proof:** `BUILDL=target/debug/buildl sh examples/check.sh examples/cpp` prints `cpp: ok` on macOS; CI runs it on Linux.

**Done when:** that command and `cargo make examples` pass, `cargo make dod` and `cargo deny check` are
green. Commit `feat(repo): add the C++ example`.

### Task 4 — List the examples

**Files:**
- Modify `examples/README.md` — rows for `c` and `cpp` in the table (:8-16): what each shows, needs "a C compiler (gcc or clang), `git`"

**Change:** the examples index names both.

**Proof:** `grep -n '\[`c`\](c/)\|\[`cpp`\](cpp/)' examples/README.md` finds both rows.

**Done when:** that grep finds both, `cargo make examples` passes, `cargo make dod` and `cargo deny
check` are green. Commit `docs(repo): list the C and C++ examples`.

## Risks

- **Linux is proven only in CI**: the probes ran Linux toolchains on aarch64 under `env -i` (spec P52–P57), not the `buildl` binary on x64. The examples job on `ubuntu-latest` is where it is first proven; a failure there is fixed in this plan.
- The header-edit counts rest on the example's shape: one object reads `banner.h`, and a comment leaves its bytes unchanged (no `-g`, spec P51, P57), so the link stays cached. Adding `-g` to the examples would break the count.
- Not done here: no example for the version probe (it would break every-target-clean, spec P21); it is tested in plan 03.

## Proof

`cargo make examples` green locally and in both CI matrix jobs; `cargo make dod` green.
