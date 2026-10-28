#!/bin/sh
# The plugin example's own checks. ../check.sh runs them in a fresh copy, after a
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

expect "out/hello prints its line" "^hello from a plugin$" "$(./out/hello)"
expect "out/bye prints its line" "^bye from a plugin$" "$(./out/bye)"

# What the plugin's targets want is shown with the plugin that declared it.
declared="declared by plugin gobin@0.1.0 for //:"
shown="$("${BUILDL}" doctor 2>&1)" || fail "doctor exits 0" "${shown}"
expect "doctor attributes go to the plugin" "${declared}" "${shown}"
shown="$("${BUILDL}" plan 2>&1)" || fail "plan exits 0" "${shown}"
expect "plan attributes go to the plugin" "${declared}" "${shown}"

origins="$("${BUILDL}" graph --json | python3 -c '
import json, sys
targets = json.load(sys.stdin)["targets"]
for label in sorted(targets):
    origin = targets[label]["origin"]
    plugins = origin.get("Plugins", []) if isinstance(origin, dict) else []
    print(label, " ".join(p["name"] for p in plugins) or origin)
')"
[ "${origins}" = "//:bye gobin
//:hello gobin" ] || fail "graph gives both targets the plugin's origin" "${origins}"
