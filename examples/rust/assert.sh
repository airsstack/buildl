#!/bin/sh
# The Rust starter's own checks. ../check.sh runs them in a fresh copy, after a
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

expect "out/hello/bin/hello prints its greeting" "^hello from rust$" "$(./out/hello/bin/hello)"

echo "// an edit" >> src/main.rs
expect "plan names the edited input" "input src/main.rs changed" "$("${BUILDL}" plan 2>&1)"

"${BUILDL}" build
expect "the first test after the edit runs" "^tests: 1 passed$" "$("${BUILDL}" test 2>&1)"
expect "the second test is served from the cache" "^tests: 1 passed (1 cached)$" "$("${BUILDL}" test 2>&1)"
shown="$("${BUILDL}" log //:unit 2>&1)" || fail "log exits 0" "${shown}"
expect "log shows what cargo test printed" "^test result: ok" "${shown}"
