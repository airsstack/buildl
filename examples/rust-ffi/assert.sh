#!/bin/sh
# The rust-ffi example's own checks. ../check.sh runs them in a fresh copy, after a build and
# a test; BUILDL names the buildl binary.

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

# refuse <description> <pattern> <text>: fails when a line of the text matches.
refuse() {
    if printf '%s\n' "$3" | grep -q -e "$2"; then
        fail "$1" "$3"
    fi
}

"${BUILDL}" build

expect "C calls the Rust static library" \
    '^\[rust-ffi\] static: mathlib_add(2, 3) = 5, mathlib_mul(6, 7) = 42$' "$(./out/c/app_static)"
# The binary finds out/rust/libgreeter.* through its rpath, from any working directory.
expect "C calls the Rust shared library" \
    '^\[rust-ffi\] shared: hello from a Rust shared library$' "$(cd out && ./c/app_shared)"

# A C header is no input of the Rust targets: they sit in their own folder.
echo '/* edited */' >> c/include/greeter.h
planned="$("${BUILDL}" plan 2>&1)" || fail "plan after the header edit exits 0" "${planned}"
expect "the C object that includes the header runs" "^run    //c:app_shared.src.shared_main.c.o: " "${planned}"
refuse "no Rust target runs" "^run    //rust:" "${planned}"
"${BUILDL}" build > /dev/null

# A Rust source edit reruns cargo; the C side waits on what the library becomes.
echo '// edited' >> rust/mathlib/src/lib.rs
planned="$("${BUILDL}" plan 2>&1)" || fail "plan after the Rust edit exits 0" "${planned}"
expect "the static library runs" "^run    //rust:mathlib: " "${planned}"

tested="$("${BUILDL}" test 2>&1)" || fail "the tests pass" "${tested}"
expect "the Rust and the C test pass" "^tests: 2 passed" "${tested}"
