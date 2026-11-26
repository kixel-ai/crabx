#!/bin/sh
# Install crabx's release binary:
#   curl --proto '=https' --tlsv1.2 -fsSL https://crabx.kixel.me/install.sh | sh
# Crates to install afterwards with `crabx install` follow as arguments (`sh -s -- CRATE...`) or
# come space-separated in CRABX_INSTALL.
# CRABX_VERSION picks a version instead of the newest stable one on crates.io, and
# CRABX_INSTALL_DIR the directory instead of ~/.local/bin.
set -eu

fail() {
    echo "crabx install: $*" >&2
    exit 1
}

# The crates to install: the arguments, then CRABX_INSTALL's words, split without globbing.
if [ -n "${CRABX_INSTALL:-}" ]; then
    set -f
    # shellcheck disable=SC2086
    set -- "$@" $CRABX_INSTALL
    set +f
fi
# crabx install checks each spec itself; this keeps one from becoming an option of crabx install.
for spec in "$@"; do
    case $spec in
        [A-Za-z]*) ;;
        *) fail "invalid crate '$spec'; expected CRATE or CRATE@VERSION" ;;
    esac
    case $spec in
        *[!0-9A-Za-z_.@+-]*) fail "invalid crate '$spec'; expected CRATE or CRATE@VERSION" ;;
    esac
done

target() {
    arch=$(uname -m)
    case $(uname -s) in
        Linux)
            # The release binaries need glibc 2.28 or later; getconf ships with glibc.
            glibc=$(getconf GNU_LIBC_VERSION 2>/dev/null) ||
                fail "crabx's Linux builds need glibc, and this system has none; install it with \`cargo install crabx\`"
            echo "${glibc#glibc }" | awk -F . '{ exit !($1 > 2 || ($1 == 2 && $2 >= 28)) }' ||
                fail "crabx's Linux builds need glibc 2.28 or later, not $glibc; install it with \`cargo install crabx\`"
            os=unknown-linux-gnu
            ;;
        Darwin)
            os=apple-darwin
            # A shell under Rosetta reports x86_64 on Apple silicon.
            if [ "$(sysctl -n hw.optional.arm64 2>/dev/null)" = 1 ]; then
                arch=arm64
            fi
            ;;
        MINGW* | MSYS* | CYGWIN*) fail "on Windows, run install.ps1 from PowerShell" ;;
        *) fail "no prebuilt crabx for $(uname -s); install it with \`cargo install crabx\`" ;;
    esac
    case $arch in
        x86_64 | amd64) echo "x86_64-$os" ;;
        aarch64 | arm64) echo "aarch64-$os" ;;
        *) fail "no prebuilt crabx for $arch; install it with \`cargo install crabx\`" ;;
    esac
}

# The highest version on crates.io that is neither yanked nor a prerelease.
newest() {
    index=$(curl --proto '=https' --tlsv1.2 -fsSL https://index.crates.io/cr/ab/crabx) || fail "cannot read crabx's versions from crates.io"
    # Each line is one version's JSON; its keys may come in any order.
    echo "$index" |
        awk '
			!/"yanked":false/ || !match($0, /"vers":"[^"]*"/) { next }
			{
				vers = substr($0, RSTART + 8, RLENGTH - 9)
				core = vers
				sub(/\+.*/, "", core)
			}
			core ~ /-/ { next }
			{ split(core, p, ".") }
			!found || p[1] + 0 > a || (p[1] + 0 == a && (p[2] + 0 > b || (p[2] + 0 == b && p[3] + 0 > c))) {
				found = 1; a = p[1] + 0; b = p[2] + 0; c = p[3] + 0; best = vers
			}
			END { if (found) print best }'
}

target=$(target)
version=${CRABX_VERSION:-$(newest)}
[ -n "$version" ] || fail "crates.io has no stable crabx release"
# SemVer 2.0.0: no leading zeros in numeric parts, no empty identifiers. The case rejects a
# newline first, which grep would match line by line.
case $version in
    *[!0-9A-Za-z.+-]*) fail "invalid version $version" ;;
esac
num='(0|[1-9][0-9]*)'
pre='(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)'
build='[0-9A-Za-z-]+'
echo "$version" | grep -Eq "^$num\\.$num\\.$num(-$pre(\\.$pre)*)?(\\+$build(\\.$build)*)?\$" ||
    fail "invalid version $version; expected MAJOR.MINOR.PATCH as SemVer 2.0.0 defines it"
# 0.2.0 is the first version with `crabx install`; before it, `crabx install CRATE` would run a crate.
case $version in
    0.0.* | 0.1.*) [ $# -eq 0 ] || fail "crabx $version cannot install crates; use 0.2.0 or later" ;;
esac
dir=${CRABX_INSTALL_DIR:-$HOME/.local/bin}
name=crabx-$target-v$version
url=https://github.com/kixel-ai/crabx/releases/download/crabx-v$version/$name.tgz

temp=$(mktemp -d)
staged=
# Remove what is staged on any exit, including an interrupt, but never the installed binary.
cleanup() {
    rm -rf "$temp"
    [ -z "$staged" ] || rm -f "$staged"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
echo "crabx install: downloading crabx $version for $target" >&2
curl --proto '=https' --tlsv1.2 -fsSL "$url" -o "$temp/crabx.tgz" || fail "cannot download $url; crabx $version's binaries may not be released yet"
tar -xzf "$temp/crabx.tgz" -C "$temp" "$name/crabx"
mkdir -p "$dir"
# Stage in $dir, so the final rename stays on one filesystem and a running crabx keeps its old file.
staged=$(mktemp "$dir/.crabx.XXXXXX")
mv "$temp/$name/crabx" "$staged"
chmod 755 "$staged"
mv "$staged" "$dir/crabx"
staged=
echo "crabx install: installed $dir/crabx" >&2
case ":$PATH:" in
    *":$dir:"*) ;;
    *) echo "crabx install: add $dir to PATH to run crabx" >&2 ;;
esac
# One crate at a time, as `crabx install` takes one, stopping at the first failure.
for spec in "$@"; do
    echo "crabx install: crabx install $spec" >&2
    "$dir/crabx" install "$spec" || fail "crabx install $spec failed"
done
