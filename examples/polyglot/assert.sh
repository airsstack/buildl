#!/bin/sh
# The polyglot example's own checks. ../check.sh runs them in a fresh copy, after
# a build and a test; BUILDL names the buildl binary.

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

expect "the Go service prints the schema" "^go service 1.0: hello from the schema$" "$(./out/go/service)"
expect "the Rust CLI prints the schema" "^rust cli 1.0: hello from the schema$" "$(./out/rust/cli)"
expect "the Python tool prints the schema" "^python tool 1.0: hello from the schema$" "$(python3 out/python/tool.pyz)"

# An edit to the comment changes the generator's input, so it runs again; it writes the
# same bytes, so the three programs downstream are served from the cache.
sed -e 's/ignores this line\./ignores this line. Edited./' schema/schema.json > schema.json.new
mv schema.json.new schema/schema.json
built="$("${BUILDL}" build 2>&1)" || fail "build after the comment edit exits 0" "${built}"
expect "only the generator runs after the comment edit" "^1 built, 3 cached$" "${built}"

dirty="$("${BUILDL}" plan --json | python3 -c '
import json, sys
targets = json.load(sys.stdin)["targets"]
for label in sorted(targets):
    if targets[label]["verdict"] != "Clean":
        print(label, targets[label]["verdict"])
')"
[ -z "${dirty}" ] || fail "every target is clean after the build" "${dirty}"

# One target writes the Go, Rust and Python sources, so what depends on it spans all three
# (the Rust test compiles the same sources, so it is listed too).
reached="$("${BUILDL}" query 'rdeps(//schema:gen)' 2>&1)" || fail "query exits 0" "${reached}"
[ "${reached}" = "//go:service
//python:tool.pyz
//rust:cli
//rust:cli_test" ] || fail "rdeps(//schema:gen) names the program of each language and the Rust test" "${reached}"

# An edit to the Rust source changes the inputs of the binary and of its test, so `plan`, asked
# for the two (it leaves tests out otherwise), names both as the targets to run.
sed -e 's/rust cli/rust cli (edited)/' rust/src/main.rs > main.rs.new
mv main.rs.new rust/src/main.rs
stale="$("${BUILDL}" plan --json //rust:cli //rust:cli_test | python3 -c '
import json, sys
targets = json.load(sys.stdin)["targets"]
for label in sorted(targets):
    if targets[label]["verdict"] != "Clean":
        print(label)
')"
[ "${stale}" = "//rust:cli
//rust:cli_test" ] || fail "an edit to rust/src/main.rs runs //rust:cli and //rust:cli_test" "${stale}"

tested="$("${BUILDL}" test 2>&1)" || fail "test after the Rust edit exits 0" "${tested}"
