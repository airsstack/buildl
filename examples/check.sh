#!/bin/sh
# check.sh <example-dir>
#
# Runs the checks every example shares, then the example's own assert.sh, on a
# fresh copy of the example in a temporary directory, so the checkout stays clean.
# The buildl binary is $BUILDL when set, else the first `buildl` on PATH. When the
# example holds a prepare.sh, it runs first, inside the copy, with $BUILDL_REPO set to
# this repository's root. Each step prints its name; the first that fails stops the
# check with its output.

set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: $0 <example-dir>" >&2
    exit 2
fi

example="$(cd "$1" && pwd)"
name="$(basename "${example}")"
BUILDL="${BUILDL:-buildl}"
# A relative path would stop naming the binary once the check moves into the copy.
case "${BUILDL}" in
    */*) BUILDL="$(cd "$(dirname "${BUILDL}")" && pwd)/$(basename "${BUILDL}")" ;;
esac
export BUILDL
# The repository root, for a prepare.sh that needs something from this checkout.
BUILDL_REPO="$(cd "$(dirname "$0")/.." && pwd)"
export BUILDL_REPO

work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT
trap 'exit 130' INT TERM
cp -R "${example}" "${work}/${name}"
cd "${work}/${name}"
# What building the example in place left behind is not part of a fresh copy.
rm -rf out .buildl

log="${work}/step.log"

# step <name> <command>...: runs the command, keeping its output, and stops the
# check with that output when it fails.
step() {
    label="$1"
    shift
    echo "${name}: ${label}"
    if ! "$@" > "${log}" 2>&1; then
        echo "${name}: FAILED at ${label}" >&2
        cat "${log}" >&2
        exit 1
    fi
}

# all_clean: every target `plan --json` reports has the verdict Clean. An example
# declaring only tests and discovery targets has none to report.
all_clean() {
    "${BUILDL}" plan --json | python3 -c '
import json, sys
targets = json.load(sys.stdin)["targets"]
dirty = sorted(label for label, row in targets.items() if row["verdict"] != "Clean")
for label in dirty:
    print(label, targets[label]["verdict"], targets[label]["reason"])
sys.exit(1 if dirty else 0)
'
}

# none_mismatched: `verify` exits 0 and reports no mismatch.
none_mismatched() {
    report="$("${BUILDL}" verify 2>&1)" || { printf '%s\n' "${report}"; return 1; }
    printf '%s\n' "${report}"
    printf '%s\n' "${report}" | grep -q " 0 mismatched"
}

if [ -f prepare.sh ]; then
    step "prepare.sh" sh prepare.sh
fi
step "check" "${BUILDL}" check
step "build" "${BUILDL}" build
step "test" "${BUILDL}" test
step "plan --json: every target clean" all_clean
step "verify: 0 mismatched" none_mismatched
step "assert.sh" sh assert.sh

echo "${name}: ok"
