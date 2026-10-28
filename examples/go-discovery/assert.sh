#!/bin/sh
# The discovery example's own checks. ../check.sh runs them in a fresh copy, after
# a build and a test; BUILDL names the buildl binary.

set -eu
BUILDL="${BUILDL:-buildl}"

# fail <description> <text>: stops, naming the check and showing the text.
fail() {
    echo "assert: $1" >&2
    printf '%s\n' "$2" >&2
    exit 1
}

"${BUILDL}" build

# The test targets the graph holds, and the ones expected: one per directory holding
# Go files, its '/' written '.'.
declared="$("${BUILDL}" graph --json | python3 -c '
import json, sys
targets = json.load(sys.stdin)["targets"]
for label in sorted(label for label, row in targets.items() if row["role"] == "Test"):
    print(label)
')"
expected="$(find . -name '*.go' ! -path './out/*' ! -path './.buildl/*' \
    | sed -e 's|^\./||' -e 's|/[^/]*$||' | LC_ALL=C sort -u | sed -e 's|/|.|g' -e 's|^|//:|' | LC_ALL=C sort)"
[ -n "${expected}" ] || fail "the example holds Go packages" "${expected}"
[ "${declared}" = "${expected}" ] \
    || fail "one test target per package" "declared:
${declared}
expected:
${expected}"

count="$(printf '%s\n' "${expected}" | wc -l | tr -d ' ')"
ran="$("${BUILDL}" test 2>&1)" || fail "every package's test passes" "${ran}"
printf '%s\n' "${ran}" | grep -q "^tests: ${count} passed" || fail "every package's test passes" "${ran}"

# The discovery target is outside verify's default scope; named, it is re-run in the
# sandbox and what it printed is compared with what the cache holds.
checked="$("${BUILDL}" verify //:pkgs 2>&1)" || fail "verify //:pkgs exits 0" "${checked}"
printf '%s\n' "${checked}" | grep -q "^1 verified, 0 mismatched" || fail "go list prints the same packages again" "${checked}"
