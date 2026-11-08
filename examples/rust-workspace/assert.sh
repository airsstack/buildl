#!/bin/sh
# The rust-workspace example's own checks. ../check.sh runs them in a fresh copy, after a build
# and a test; BUILDL names the buildl binary.

set -eu
BUILDL="${BUILDL:-buildl}"

# fail <description> <text>: stops, naming the check and showing the text.
fail() {
    echo "assert: $1" >&2
    printf '%s\n' "$2" >&2
    exit 1
}

# expect <description> <pattern> <text>: fails unless a line of the text matches.
expect() {
    printf '%s\n' "$3" | grep -q -e "$2" || fail "$1" "$3"
}

# The binary went through the proc-macro (answer = 42) and the rlib with `shout` on.
expect "out/cli prints its line" '^\[rust-workspace\] HELLO, BUILDL! answer = 42$' "$(./out/cli)"

# Nothing but the binary is copied out: the rlib and the proc-macro stay inside cargo.
leftover="$(ls out | grep -v -e '^cli$' || true)"
[ -z "${leftover}" ] || fail "out/ holds only the binary" "${leftover}"

# An edit to the rlib is an input of every target of the folder, so all three run.
echo "// an edit" >> greeting/src/lib.rs
planned="$("${BUILDL}" plan //:cli //:unit //:greeting_shout 2>&1)" ||
    fail "plan after the edit exits 0" "${planned}"
expect "plan runs //:cli" "^run    //:cli: " "${planned}"
expect "plan runs //:unit" "^run    //:unit: " "${planned}"
expect "plan runs //:greeting_shout" "^run    //:greeting_shout: " "${planned}"

tested="$("${BUILDL}" test 2>&1)" || fail "the tests pass" "${tested}"
expect "both tests pass" "^tests: 2 passed" "${tested}"
shown="$("${BUILDL}" log //:unit 2>&1)" || fail "log exits 0" "${shown}"
expect "cargo tested the proc-macro's expansion" "the_macro_expands_to_a_function ... ok" "${shown}"
shown="$("${BUILDL}" log //:greeting_shout 2>&1)" || fail "log exits 0" "${shown}"
expect "cargo tested the rlib with shout on" "greets_by_name ... ok" "${shown}"
