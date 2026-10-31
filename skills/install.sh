#!/bin/sh
# Install the buildl and airsl binaries and the buildl agent skills.
#
#   sh install.sh --claude              install the skills as a Claude Code plugin
#   sh install.sh --dir ~/.codex/skills copy the buildl and buildl-migrate skills into that folder
#
# The buildl and airsl binaries come from each project's latest release through its install.sh;
# from a clone, or with a --ref other than main, buildl is built with cargo instead.
#
# Options: --ref R (git branch of the skills and, when it is not main, of the buildl built from
# it; default main), --dry-run (print, run nothing).
set -eu

repo="airsstack/buildl"
ref="main"
mode=""
dir=""
dry_run=0
tmp=""

usage() {
    cat >&2 <<'EOF'
usage: sh install.sh (--claude | --dir PATH) [--ref BRANCH] [--dry-run]

  --claude     install the skills as a Claude Code plugin
  --dir PATH   copy the buildl and buildl-migrate skills into PATH
  --ref BRANCH the git branch of the skills and, when it is not main, of the buildl built
               from it (default: main)
  --dry-run    print every command and run none
EOF
    exit 2
}

while [ $# -gt 0 ]; do
    case "$1" in
        --claude)
            [ -z "${mode}" ] || usage
            mode="claude"
            ;;
        --dir)
            if [ -n "${mode}" ] || [ $# -lt 2 ] || [ -z "$2" ]; then
                usage
            fi
            mode="dir"
            dir="$2"
            shift
            ;;
        --ref)
            [ $# -ge 2 ] || usage
            ref="$2"
            shift
            ;;
        --dry-run)
            dry_run=1
            ;;
        *)
            usage
            ;;
    esac
    shift
done
[ -n "${mode}" ] || usage

cleanup() {
    [ -z "${tmp}" ] || rm -rf "${tmp}"
}
trap cleanup EXIT
trap 'exit 130' INT TERM HUP

# Print a word so that a space or a quote in it stays visible, as a shell would take it back.
quote() {
    case "$1" in
        "" | *[!A-Za-z0-9_./:@=+,%-]*)
            printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"
            ;;
        *)
            printf '%s' "$1"
            ;;
    esac
}

run() {
    printf '+'
    for word in "$@"; do
        printf ' '
        quote "${word}"
    done
    printf '\n'
    if [ "${dry_run}" -eq 0 ]; then
        "$@"
    fi
}

have() {
    command -v "$1" >/dev/null 2>&1
}

# Resolved symlinks, so two spellings of one folder compare equal.
real_dir() {
    (cd "$1" && pwd -P)
}

skills="buildl buildl-migrate"
installed=""

if [ "${mode}" = "claude" ] && ! have claude; then
    echo "install Claude Code first" >&2
    exit 1
fi

# A clone: this script sits in a folder named skills whose parent holds the marketplace manifest.
clone=""
if [ "${mode}" = "dir" ]; then
    here="$(real_dir "$(dirname "$0")")"
    if [ "$(basename "${here}")" = "skills" ] && [ -f "${here}/../.claude-plugin/marketplace.json" ]; then
        clone="$(real_dir "${here}/..")"
    fi
fi

# How each binary is installed: skip (already on PATH), cargo (built from source) or pointer
# (the install.sh of its latest release).
if have buildl; then
    buildl_route="skip"
elif [ -n "${clone}" ] || [ "${ref}" != "main" ]; then
    buildl_route="cargo"
else
    buildl_route="pointer"
fi
if have airsl; then
    airsl_route="skip"
else
    airsl_route="pointer"
fi

if [ "${buildl_route}" = "cargo" ] && ! have cargo; then
    echo "install Rust from https://rustup.rs" >&2
    exit 1
fi
if { [ "${buildl_route}" = "pointer" ] || [ "${airsl_route}" = "pointer" ]; } && ! have curl; then
    echo "install curl: it is needed to fetch the buildl and airsl installers" >&2
    exit 1
fi

# Fetch an installer and run it with sh. The download is held whole first, so a failed fetch runs
# nothing.
run_pointer() {
    printf '+ curl -fsSL %s, then sh -c on the whole download\n' "$(quote "$1")"
    if [ "${dry_run}" -eq 0 ]; then
        script="$(curl -fsSL "$1")"
        sh -c "${script}"
    fi
}

case "${buildl_route}" in
    skip)
        echo "buildl is already on PATH, skipped"
        ;;
    cargo)
        if [ -n "${clone}" ]; then
            run cargo install --locked --path "${clone}/crates/buildl-cli"
            installed="${installed}  buildl (cargo install, from ${clone})
"
        else
            run cargo install --locked --git "https://github.com/${repo}" --branch "${ref}" buildl-cli
            installed="${installed}  buildl (cargo install, branch ${ref})
"
        fi
        ;;
    pointer)
        run_pointer "https://airsstack.github.io/buildl/install.sh"
        installed="${installed}  buildl (prebuilt, latest release)
"
        ;;
esac

if [ "${airsl_route}" = "skip" ]; then
    echo "airsl is already on PATH, skipped"
else
    run_pointer "https://airsstack.github.io/airsl/install.sh"
    installed="${installed}  airsl (prebuilt, latest release)
"
fi

if [ "${mode}" = "claude" ]; then
    run claude plugin marketplace add "${repo}@${ref}" --sparse .claude-plugin skills
    run claude plugin install buildl@buildl
    installed="${installed}  the buildl and buildl-migrate skills (Claude Code plugin buildl@buildl, user scope)
"
else
    known=1
    if [ -n "${clone}" ]; then
        src="${here}"
    else
        for tool in curl tar; do
            if ! have "${tool}"; then
                echo "install ${tool}: it is needed to fetch the skills" >&2
                exit 1
            fi
        done
        if [ "${dry_run}" -eq 1 ]; then
            run curl -fsSL -o "<temporary folder>/buildl.tar.gz" "https://github.com/${repo}/archive/${ref}.tar.gz"
            run tar -xzf "<temporary folder>/buildl.tar.gz" -C "<temporary folder>/src"
            src="<temporary folder>/src/<archive>/skills"
            known=0
        else
            tmp="$(mktemp -d)"
            mkdir "${tmp}/src"
            run curl -fsSL -o "${tmp}/buildl.tar.gz" "https://github.com/${repo}/archive/${ref}.tar.gz"
            run tar -xzf "${tmp}/buildl.tar.gz" -C "${tmp}/src"
            src=""
            for candidate in "${tmp}"/src/*/skills; do
                if [ -d "${candidate}" ]; then
                    src="${candidate}"
                fi
            done
            if [ -z "${src}" ]; then
                echo "the archive of ${ref} holds no skills folder" >&2
                exit 1
            fi
        fi
    fi

    if [ "${known}" -eq 1 ]; then
        for name in ${skills}; do
            if [ ! -f "${src}/${name}/SKILL.md" ]; then
                echo "${src}/${name}/SKILL.md is missing" >&2
                exit 1
            fi
        done
        if [ -d "${dir}" ] && [ "$(real_dir "${src}")" = "$(real_dir "${dir}")" ]; then
            echo "${dir} is the folder the skills are copied from; pick another --dir" >&2
            exit 1
        fi
    fi

    run mkdir -p "${dir}"
    for name in ${skills}; do
        run rm -rf "${dir}/${name}"
        run cp -R "${src}/${name}" "${dir}/${name}"
        installed="${installed}  skill ${name} in ${dir}
"
    done
fi

if [ "${dry_run}" -eq 1 ]; then
    echo "dry run, nothing was changed; it would install:"
    printf '%s' "${installed}"
elif [ -n "${installed}" ]; then
    echo "installed:"
    printf '%s' "${installed}"
else
    echo "nothing to install"
fi
