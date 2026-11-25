#!/usr/bin/env bash
# Install crabx from npm, then run `crabx install` and `crabx install CRATE` from npm's crabx, as
# `npx @kixel/crabx install` does, and run crabx and the crate by name from a new shell that lacks
# npm's directories. e2e.yml runs it on the packages release.py packs from a commit's build, and
# release.yml on the published version.
#   tests/install.sh CRATE@VERSION NPM-INSTALL-ARGUMENT...
# It installs into a new CRABX_INSTALL_DIR, which on Windows crabx adds to the user's Path.
set -euo pipefail

fail() {
    echo "install: $*" >&2
    exit 1
}

[ $# -ge 2 ] || fail "usage: $0 CRATE@VERSION NPM-INSTALL-ARGUMENT..."
spec=$1
shift
name=${spec%@*}
version=${spec#*@}
temp=$(mktemp -d)
npm install --prefix "$temp/npm" --no-audit --no-fund "$@"
npm_crabx=$temp/npm/node_modules/.bin/crabx

case $(uname -s) in
    MINGW* | MSYS*)
        CRABX_INSTALL_DIR=$(cygpath -w "$temp/bin")
        installed=$CRABX_INSTALL_DIR\\crabx.exe
        # A new terminal's Path: the machine's, then the user's, which crabx install extends.
        fresh() { powershell -NoProfile -Command "\$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User'); $1"; }
        where='(Get-Command crabx).Source'
        ;;
    *)
        CRABX_INSTALL_DIR=$temp/bin
        installed=$CRABX_INSTALL_DIR/crabx
        # On Unix the user adds the directory to PATH, as crabx install says to.
        fresh() { PATH="$CRABX_INSTALL_DIR:/usr/bin:/bin" bash -c "$1"; }
        where='command -v crabx'
        ;;
esac
export CRABX_INSTALL_DIR

echo "install: crabx install"
"$npm_crabx" install
found=$(fresh "$where" | tr -d '\r')
[ "$found" = "$installed" ] || fail "a new shell runs crabx from '$found', not $installed"
echo "install: crabx install $spec"
"$npm_crabx" install "$spec"
out=$(fresh "$name --version" | tr -d '\r')
case $out in "$name $version" | "$name $version "*) ;; *) fail "a new shell's $name --version printed '$out'" ;; esac
echo "install: a new shell runs crabx and $name from $CRABX_INSTALL_DIR"
