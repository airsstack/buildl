#!/bin/sh
# The cargo-buildl example's own checks. ../check.sh runs them in a fresh copy, after a build and a
# test; BUILDL names the buildl binary. The cargo-buildl binary is $CARGO_BUILDL when set, else
# the one beside $BUILDL: both land in the same target directory.

set -eu
BUILDL="${BUILDL:-buildl}"

if [ -z "${CARGO_BUILDL:-}" ]; then
    case "${BUILDL}" in
        */*) CARGO_BUILDL="$(dirname "${BUILDL}")/cargo-buildl" ;;
        *) CARGO_BUILDL="$(dirname "$(command -v "${BUILDL}")")/cargo-buildl" ;;
    esac
fi
if [ ! -x "${CARGO_BUILDL}" ]; then
    echo "assert: no cargo-buildl at ${CARGO_BUILDL}; build it with cargo build -p buildl-cargo, or set CARGO_BUILDL" >&2
    exit 1
fi

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

# cargo_buildl <args>...: runs cargo-buildl as cargo does for `cargo buildl <args>`: the
# subcommand name comes first. Output is standard error and output together; the exit status is
# kept in $status.
cargo_buildl() {
    status=0
    output="$("${CARGO_BUILDL}" buildl "$@" 2>&1)" || status=$?
}

# The binary and the static library are copied out; the rlib stays inside cargo.
expect "out/app prints its line" '^\[cargo-buildl\] hello, buildl!$' "$(./out/app)"
[ -f out/libffi.a ] || fail "out/libffi.a is the static library" "$(ls out)"
leftover=""
for entry in out/*; do
    case "${entry}" in
        out/app | out/libffi.a) ;;
        *) leftover="${leftover}${entry}
" ;;
    esac
done
[ -z "${leftover}" ] || fail "out/ holds the binary and the static library only" "${leftover}"

cargo_buildl sync --check
[ "${status}" -eq 0 ] || fail "sync --check exits 0 on a fresh project" "${output}"

# A second binary, found by cargo without a line in any Cargo.toml: build.lua falls behind.
mkdir app/src/bin
cat > app/src/bin/extra.rs <<'EOF'
fn main() {
    println!("[cargo-buildl] extra");
}
EOF
before_tail="$(sed -n '/^-- cargo-buildl: end generated/,$p' build.lua)"

cargo_buildl sync --check
[ "${status}" -eq 5 ] || fail "sync --check exits 5 once a binary is added (got ${status})" "${output}"
expect "sync --check says build.lua is out of date" \
    '^sync: build.lua is out of date with Cargo.toml; run cargo buildl sync$' "${output}"

cargo_buildl sync
[ "${status}" -eq 0 ] || fail "sync exits 0" "${output}"
expect "sync says it updated build.lua" '^sync: build.lua updated$' "${output}"
expect "build.lua declares the new binary" \
    '^rust.binary("extra", { args = rust.package("app") })$' "$(cat build.lua)"
expect "build.lua keeps the line below the generated section" \
    '^-- Your own targets go below\. ' "$(cat build.lua)"
after_tail="$(sed -n '/^-- cargo-buildl: end generated/,$p' build.lua)"
[ "${before_tail}" = "${after_tail}" ] || fail "sync leaves everything from the end marker on alone" "${after_tail}"

cargo_buildl sync --check
[ "${status}" -eq 0 ] || fail "sync --check exits 0 after sync" "${output}"

planned="$("${BUILDL}" plan //:extra 2>&1)" || fail "plan exits 0 with the new binary" "${planned}"
expect "plan runs //:extra" "^run    //:extra: " "${planned}"

# Every other command goes to buildl's own command line.
cargo_buildl check
[ "${status}" -eq 0 ] || fail "cargo buildl check exits 0" "${output}"
