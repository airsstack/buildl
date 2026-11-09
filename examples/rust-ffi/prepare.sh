#!/bin/sh
# Makes the `rust` and `c` plugins of this checkout fetchable under the tags buildl.toml
# names, then fetches them. ../check.sh runs this in a fresh copy of the example, with
# BUILDL_REPO set to the repository root and BUILDL naming the buildl binary.
#
# The tags exist once the plugins are released, and CI runs the plugins as this checkout
# has them anyway. So the checkout's plugins/rust and plugins/c go into one temporary git
# repository, tagged plugin-rust-v0.2.0 and plugin-c-v0.2.0, and the copy's buildl.toml
# names that repository instead of GitHub.

set -eu
BUILDL="${BUILDL:-buildl}"

if [ -z "${BUILDL_REPO:-}" ]; then
    echo "prepare: BUILDL_REPO is not set" >&2
    exit 1
fi

repo="$(mktemp -d)"
# The store keeps its own copy of what `plugins fetch` pulls, so the repository can go.
trap 'rm -rf "${repo}"' EXIT

git init --quiet "${repo}"
mkdir -p "${repo}/plugins"
cp -R "${BUILDL_REPO}/plugins/rust" "${repo}/plugins/rust"
cp -R "${BUILDL_REPO}/plugins/c" "${repo}/plugins/c"
git -C "${repo}" add plugins
git -C "${repo}" -c user.name=buildl -c user.email=buildl@example.invalid \
    -c commit.gpgsign=false commit --quiet -m "the rust and c plugins"
for tag in plugin-rust-v0.2.0 plugin-c-v0.2.0; do
    git -C "${repo}" -c user.name=buildl -c user.email=buildl@example.invalid \
        -c tag.gpgsign=false tag "${tag}"
done

# Point both plugins at the local repository instead of the published one.
line='github      = "airsstack/buildl"'
grep -qxF "${line}" buildl.toml || {
    echo "prepare: buildl.toml has no line ${line}" >&2
    exit 1
}
sed "s|^github      = \"airsstack/buildl\"\$|git         = \"file://${repo}\"|" buildl.toml > buildl.toml.new
mv buildl.toml.new buildl.toml

"${BUILDL}" plugins fetch
