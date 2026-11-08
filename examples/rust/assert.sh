#!/bin/sh
# The Rust example's own checks. ../check.sh runs them in a fresh copy, after a
# build and a test; BUILDL names the buildl binary.

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

expect "out/hello prints its greeting" "^hello from rust$" "$(./out/hello)"

# An edit to a source is an input of both targets, so the plan runs the binary and the test;
# plan leaves tests out unless they are named.
echo "// an edit" >> src/main.rs
planned="$("${BUILDL}" plan //:hello //:unit 2>&1)" || fail "plan after the edit exits 0" "${planned}"
expect "plan runs //:hello" "^run    //:hello: " "${planned}"
expect "plan runs //:unit" "^run    //:unit: " "${planned}"

tested="$("${BUILDL}" test 2>&1)" || fail "the test passes" "${tested}"
expect "the test passes" "^tests: 1 passed$" "${tested}"

# Nothing changed since, so the second test is served from the cache, and the log holds what
# cargo test printed when it last ran.
cached="$("${BUILDL}" test 2>&1)" || fail "the second test passes" "${cached}"
expect "the second test is served from the cache" "^tests: 1 passed (1 cached)$" "${cached}"
shown="$("${BUILDL}" log //:unit 2>&1)" || fail "log exits 0" "${shown}"
expect "log shows what cargo test printed" "^test result: ok" "${shown}"
