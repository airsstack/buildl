#!/bin/sh
# The C++ example's own checks. ../check.sh runs them in a fresh copy, after a build and
# a test; BUILDL names the buildl binary.

set -eu
BUILDL="${BUILDL:-buildl}"

# fail <description> <text>: stops, naming the check and showing the text.
fail() {
    echo "assert: $1" >&2
    printf '%s\n' "$2" >&2
    exit 1
}

"${BUILDL}" build

ran="$(./out/hello)"
want="== greet ==
hello, buildl"
[ "${ran}" = "${want}" ] || fail "out/hello prints the banner and the greeting" "${ran}"

# The shared library's file name depends on the system it was built on.
case "$(uname -s)" in
    Darwin) lib=out/libgreet.dylib ;;
    *) lib=out/libgreet.so ;;
esac
[ -f "${lib}" ] || fail "the shared library is ${lib}" "$(ls out)"

# A comment added to banner.h changes the bytes of the one source that includes it, so its
# object is the one target to run. It is rebuilt to the same bytes, so the link and
# everything after it stay cached (early cutoff).
printf '// a comment\n' >> include/banner.h
planned="$("${BUILDL}" plan 2>&1)" || fail "plan after the header edit exits 0" "${planned}"
printf '%s\n' "${planned}" | grep -q "^run    //:hello.src.main.cc.o: " \
    || fail "the object of src/main.cc is the one target to run" "${planned}"
printf '%s\n' "${planned}" | grep -q "^1 to run, " \
    || fail "nothing else runs" "${planned}"

# The build proper is the last summary: the discovery targets that read the headers run
# first, in a summary of their own.
rebuilt="$("${BUILDL}" build 2>&1)" || fail "build after editing banner.h exits 0" "${rebuilt}"
summary="$(printf '%s\n' "${rebuilt}" | tail -n 1)"
[ "${summary}" = "1 built, 5 cached" ] \
    || fail "editing banner.h builds the object of src/main.cc and nothing else" "${rebuilt}"

# The test binary loads the shared library from its own folder.
tested="$("${BUILDL}" test 2>&1)" || fail "the test passes with the library loaded" "${tested}"
printf '%s\n' "${tested}" | grep -q "^tests: 1 passed" \
    || fail "the test passes with the library loaded" "${tested}"
