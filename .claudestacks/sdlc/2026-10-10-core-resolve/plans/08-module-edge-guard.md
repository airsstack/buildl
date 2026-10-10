---
status: approved
created: 2026-10-10
depends-on: [07]
---

# Module Edge Guard Implementation Plan

**Goal:** The gate fails when `buildl-core` gains a module-to-module import that the golden file does not list.

**Architecture:** A `cargo make` task in the shape of `guard-crate-edges`: it lists which top-level module of `buildl-core` imports which, by reading the text `crate::<module>` on lines that are not comments, and diffs the list against `crates/expected-module-edges.txt`. A new edge, such as one phase importing another, turns the gate red until someone edits the golden file. Two spellings cross modules without that text, so the task refuses them outright: a grouped `crate::{…}`, and `super::` climbing to the crate root (twice from a nested file, once from a module's own `mod.rs`). The task hangs off `clippy`, like the other two guards, so `cargo make dod` runs it.

**Tech Stack:** `cargo-make`, POSIX shell, TOML. No new dependency.

**Content authority:** spec §7 (the module-edge guard); roadmap follow-up #11.

**Checkpoints:** review once, when task 1 is done.

---

## Context an implementer needs

What plan `07` leaves that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| the final set of `buildl-core` modules and their imports, including `pipeline -> resolve` | `crates/buildl-core/src/` | what the golden file records |
| `guard-crate-edges` and `guard-core-purity`, dependencies of the `clippy` task | `Makefile.toml:60`, `:121`, `:146` | the shape this task copies, and the list it joins |

Facts that bite here:

- `export LC_ALL=C` fixes `sort`'s order, so the list is the same on macOS and Linux.
- The pipeline that lists a module's edges ends in `grep -v`, which exits `1` when a module has
  no edge to print; `|| true` keeps that from ending the script.
- The task was run on macOS. CI's Ubuntu job is the first run on Linux; it uses only `grep`,
  `sed`, `sort`, `basename`, `mktemp` and `diff`, as `guard-crate-edges` does.
- One spelling stays outside the guard: a top-level `super::` in a single-file module.
  `error.rs` is the only such module, and it is not a phase.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every command output below was captured by running that command on exactly the state the step describes.

## File structure

```
Makefile.toml                       — [modify] guard-module-edges, added to clippy's dependencies
crates/expected-module-edges.txt    — [create] the golden list of module edges
```

### Task 1 — Guard `buildl-core`'s module edges

**Files:**
- Modify `Makefile.toml`
- Create `crates/expected-module-edges.txt`

**Steps:**

1. Create `crates/expected-module-edges.txt` as an empty file (`: > crates/expected-module-edges.txt`), so the first run shows every edge as missing from it.

2. Add the task to `Makefile.toml`. Insert:

   ```toml
   [tasks.guard-module-edges]
   category = "Gate"
   description = "Assert buildl-core's module-to-module imports against crates/expected-module-edges.txt"
   # The compiler lets any module of a crate import any other, so nothing stops one
   # pipeline phase from importing another — the rule buildl-core's crate doc states
   # first. A committed list of the imports that exist turns a new one into a diff
   # someone has to approve.
   #
   # The listing reads the text `crate::<module>` on lines that are not comments. Two
   # spellings cross modules without that text, so both are refused outright: a
   # grouped `crate::{...}`, and `super::` climbing to the crate root — twice from a
   # nested file, once from a module's own mod.rs.
   script_runner = "@shell"
   script = '''
   src="crates/buildl-core/src"
   expected="crates/expected-module-edges.txt"
   actual="$(mktemp)"
   trap 'rm -f "${actual}"' EXIT
   export LC_ALL=C

   if grep -rnE 'crate::\{|super::super' "${src}"; then
       echo "guard: the imports above cross modules in a spelling the edge listing cannot read" >&2
       echo "       write each one as crate::<module>::<item>" >&2
       exit 1
   fi

   if grep -n 'super::' "${src}"/*/mod.rs; then
       echo "guard: super:: in a module's mod.rs names the crate root" >&2
       echo "       write each one as crate::<module>::<item>" >&2
       exit 1
   fi

   for entry in "${src}"/*; do
       module="$(basename "${entry}" .rs)"
       if [ "${module}" = "lib" ]; then
           continue
       fi
       grep -rhv '^[[:space:]]*//' "${entry}" \
           | grep -oE 'crate::[a-z_]+' \
           | sort -u \
           | sed "s|^crate::|${module} -> |" \
           | grep -v "^${module} -> ${module}$" >> "${actual}" || true
   done

   if ! diff -u "${expected}" "${actual}"; then
       echo "guard: buildl-core's module edges do not match ${expected}" >&2
       echo "       remove the import, or update the golden file if the edge is intended" >&2
       exit 1
   fi
   '''
   ```

   immediately before:

   ```toml
   [tasks.guard-core-purity]
   ```

3. Run it and confirm it fails, listing the sixteen edges the tree has:

   ```
   $ cargo make guard-module-edges
   +error -> types
   +json -> error
   +load -> error
   +load -> ports
   +load -> types
   +pipeline -> error
   +pipeline -> load
   +pipeline -> ports
   +pipeline -> resolve
   +pipeline -> types
   +ports -> error
   +ports -> types
   +resolve -> error
   +resolve -> types
   +types -> error
   +types -> json
   guard: buildl-core's module edges do not match crates/expected-module-edges.txt
          remove the import, or update the golden file if the edge is intended
   ```

4. Record those edges. Replace the content of `crates/expected-module-edges.txt` with:

   ```
   error -> types
   json -> error
   load -> error
   load -> ports
   load -> types
   pipeline -> error
   pipeline -> load
   pipeline -> ports
   pipeline -> resolve
   pipeline -> types
   ports -> error
   ports -> types
   resolve -> error
   resolve -> types
   types -> error
   types -> json
   ```

5. Run it again and confirm it passes:

   ```
   $ cargo make guard-module-edges
   [cargo-make] INFO - Running Task: guard-module-edges
   [cargo-make] INFO - Build Done in … seconds.
   ```

6. Hang the task off `clippy`, in `Makefile.toml`. Replace:

   ```toml
   dependencies = ["guard-core-purity", "guard-crate-edges"]
   ```

   with:

   ```toml
   dependencies = ["guard-core-purity", "guard-crate-edges", "guard-module-edges"]
   ```

7. Prove the guard catches one phase importing another. Create a throwaway file `crates/buildl-core/src/resolve/zz_probe.rs`:

   ```rust
   use crate::load::load;
   ```

8. Run the lint task and confirm the guard stops it:

   ```
   $ cargo make clippy
   +resolve -> load
   guard: buildl-core's module edges do not match crates/expected-module-edges.txt
          remove the import, or update the golden file if the edge is intended
   ```

9. Delete `crates/buildl-core/src/resolve/zz_probe.rs`.

10. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   ```

   Expected: both exit `0`. `cargo fmt --all -- --check` prints nothing, and `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`

11. Commit:

   ```
   $ git add Makefile.toml crates/expected-module-edges.txt
   $ git commit -m "ci(repo): guard buildl-core's module edges"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- `crates/expected-module-edges.txt` holds `pipeline -> resolve`, and holds neither `load -> resolve` nor
  `resolve -> load`.
- With `use crate::load::load;` in a file under `crates/buildl-core/src/resolve/`,
  `cargo make clippy` exits non-zero (step 8 above); the file is gone afterwards:
  `git status --short crates/buildl-core` prints nothing before the commit.
