#!/bin/sh
# The remote example's checks, run against the server compose.yaml starts (Linux x86_64).
# Unlike the other examples it is not run by ../check.sh: it needs that server. Each
# step works in a fresh copy of this directory, as a separate checkout would.
#
#   A  build --remote-execute    the server runs the target          1 remote
#      test --remote-execute     the server runs the test            tests: 1 passed (1 remote)
#   B  build                     a fresh copy fetches what A ran     1 fetched
#   C  edit, --remote-upload     runs here and sends the result      1 built
#   D  same edit, build          a fresh copy fetches what C sent    1 fetched
#
# BUILDL names the buildl binary.

set -eu
BUILDL="${BUILDL:-buildl}"
case "${BUILDL}" in
    */*) BUILDL="$(cd "$(dirname "${BUILDL}")" && pwd)/$(basename "${BUILDL}")" ;;
esac
here="$(cd "$(dirname "$0")" && pwd)"

work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT
trap 'exit 130' INT TERM

# fail <description> <text>: stops, naming the check and showing the text.
fail() {
    echo "assert: $1" >&2
    printf '%s\n' "$2" >&2
    exit 1
}

# A line no earlier run wrote, so the server's cache cannot already hold these actions.
stamp="# run $(date +%s)-$$"

# copy <name>: a fresh copy of the example, with this run's stamp, named <name>.
copy() {
    cp -R "${here}" "${work}/$1"
    rm -rf "${work}/$1/out" "${work}/$1/.buildl"
    echo "${stamp}" >> "${work}/$1/app/greet.py"
}

# step <copy> <expected summary> <description> <flag>...: builds in the copy and checks
# the line that closes the build. --remote-strict makes a failed remote call fail the build.
step() {
    dir="$1" expected="$2" description="$3"
    shift 3
    echo "remote: ${dir}: ${description}"
    built="$(cd "${work}/${dir}" && "${BUILDL}" build --remote-strict "$@" 2>&1)" \
        || fail "${dir}: build exits 0" "${built}"
    [ "$(printf '%s\n' "${built}" | tail -n 1)" = "${expected}" ] \
        || fail "${dir}: ${description}: expected '${expected}'" "${built}"
    printed="$(python3 "${work}/${dir}/out/app.pyz")"
    [ "${printed}" = "hello from python" ] || fail "${dir}: out/app.pyz prints its greeting" "${printed}"
}

copy a
step a "1 remote" "the server runs the target" --remote-execute
echo "remote: a: the server runs the test"
tested="$(cd "${work}/a" && "${BUILDL}" test --remote-strict --remote-execute 2>&1)" \
    || fail "a: test exits 0" "${tested}"
[ "$(printf '%s\n' "${tested}" | tail -n 1)" = "tests: 1 passed (1 remote)" ] \
    || fail "a: the server runs the test: expected 'tests: 1 passed (1 remote)'" "${tested}"

copy b
step b "1 fetched" "a fresh copy fetches what the server ran"

copy c
echo "# an edit" >> "${work}/c/app/greet.py"
step c "1 built" "an edited copy builds here and uploads" --remote-upload
copy d
echo "# an edit" >> "${work}/d/app/greet.py"
step d "1 fetched" "a fresh copy with the same edit fetches what was uploaded"

echo "remote: ok"
