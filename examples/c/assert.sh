#!/bin/sh
# The C example's own checks. ../check.sh runs them in a fresh copy, after a build and a
# test; BUILDL names the buildl binary.

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

"${BUILDL}" build

expect "out/hello prints its line" \
    '^\[c example\] hello from buildl: util_add(2, 3) = 5, blob = 42$' "$(./out/hello)"

# A comment added to banner.h changes the bytes of the one source that includes it, so
# its object is the one target to run. It is rebuilt to the same bytes, so the link and
# everything after it stay cached (early cutoff).
echo '/* edited */' >> include/banner.h
planned="$("${BUILDL}" plan 2>&1)" || fail "plan after the header edit exits 0" "${planned}"
expect "the object of src/main.c is the one target to run" "^run    //:hello.src.main.c.o: " "${planned}"
expect "nothing else runs" "^1 to run, " "${planned}"

# The build proper is the last summary: the discovery targets that read the headers run
# first, in a summary of their own.
built="$("${BUILDL}" build 2>&1)" || fail "build after the header edit exits 0" "${built}"
summary="$(printf '%s\n' "${built}" | grep -e ' cached$' | tail -n 1)"
expect "one target built, the rest cached" "^1 built, [0-9][0-9]* cached$" "${summary}"
expect "the edit changes nothing in the output" \
    '^\[c example\] hello from buildl: util_add(2, 3) = 5, blob = 42$' "$(./out/hello)"

tested="$("${BUILDL}" test 2>&1)" || fail "the test passes" "${tested}"
expect "the test passes" "^tests: 1 passed" "${tested}"
