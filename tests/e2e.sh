#!/usr/bin/env bash
# Run a built crabx against the live crates.io, GitHub, and Sigstore with no Rust toolchain,
# then again with the network blocked, from its cache. e2e.yml runs it on every target.
#   tests/e2e.sh CRABX TARGET
# The cache must not hold the pinned versions yet. Blocking the network takes sudo on Linux
# and an administrator on Windows, where it adds a firewall rule for CRABX until exit.
set -euo pipefail

fail() {
    echo "e2e: $*" >&2
    exit 1
}

[ $# -eq 2 ] || fail "usage: $0 CRABX TARGET"
[ -f "$1" ] || fail "no crabx at $1"
crabx=$(cd "$(dirname "$1")" && pwd -P)/$(basename "$1")
target=$2

# Without a toolchain, a regression that compiles instead of downloading fails here.
path=
IFS=: read -ra dirs <<<"$PATH"
for dir in "${dirs[@]}"; do
    for tool in cargo rustup rustc; do
        if [ -e "$dir/$tool" ] || [ -e "$dir/$tool.exe" ]; then
            # Dropping a system directory, such as a distro cargo's /usr/bin, takes the script's own tools.
            if [ -e "$dir/uname" ] || [ -e "$dir/uname.exe" ]; then fail "$dir holds both $tool and system tools; run where the toolchain has its own directory"; fi
            continue 2
        fi
    done
    path=${path:+$path:}$dir
done
export PATH=$path
unset CARGO
for tool in cargo rustup rustc; do
    if command -v "$tool" >/dev/null; then fail "$tool is still on PATH"; fi
done

online() { "$@"; }
block() { :; }
unblock() { :; }
case $(uname -s) in
    Linux)
        cache=${XDG_CACHE_HOME:-$HOME/.cache}
        xdg=$cache
        # sudo resets the environment, so pass through what crabx reads.
        offline() { sudo unshare --net --setuid "$(id -u)" --setgid "$(id -g)" -- env HOME="$HOME" XDG_CACHE_HOME="$xdg" PATH="$PATH" "$@"; }
        ;;
    Darwin)
        cache=$HOME/Library/Caches
        offline() { sandbox-exec -p '(version 1)(allow default)(deny network*)' "$@"; }
        ;;
    MINGW* | MSYS*)
        cache=$(cygpath -u "$LOCALAPPDATA")
        # The rule names crabx's path, so the programs crabx starts keep their network; none needs it.
        block() {
            trap unblock EXIT
            powershell -NoProfile -Command "New-NetFirewallRule -DisplayName crabx-e2e -Direction Outbound -Action Block -Program '$(cygpath -w "$crabx")' | Out-Null"
        }
        unblock() {
            trap - EXIT
            powershell -NoProfile -Command "Remove-NetFirewallRule -DisplayName crabx-e2e"
        }
        offline() { "$@"; }
        ;;
    *) fail "no network block for $(uname -s)" ;;
esac
cache=$cache/crabx/v1/$target

# pinned NETWORK SPEC MODE SOURCE [OPTIONS...]: run SPEC's --version through crabx and check its
# output, which may add a build note, and the trust source its cache entry records.
pinned() {
    local network=$1 spec=$2 mode=$3 source=$4
    shift 4
    local name=${spec%@*} version=${spec#*@}
    local entry=$cache/$mode/$name/$version out
    if [ "$network" = online ] && [ -e "$entry" ]; then fail "$entry is already cached"; fi
    echo "e2e: $network crabx $* $spec"
    out=$("$network" "$crabx" "$@" "$spec" --version | tr -d '\r')
    case $out in "$name $version" | "$name $version "*) ;; *) fail "$spec printed '$out'" ;; esac
    grep -qF "\"source\":\"$source" "$entry/record.json" || fail "$spec recorded $(cat "$entry/record.json")"
}

# Pins keep a third party's new release from changing the result.
cases() {
    # GitHub's release attestation and the author's own build provenance, under both trust roots.
    pinned "$1" fnox@1.50.0 author "GitHub attestations (2) sha256:"
    # crabx's pipeline, the signer every crabx-built crate carries.
    pinned "$1" crabx@0.1.0 author "GitHub attestations (1) sha256:"
    pinned "$1" hyperfine@2.0.0 author "author release (no published attestation)\""
    # Its author attaches no binaries; quickinstall builds it for every target crabx runs on.
    pinned "$1" cargo-expand@1.0.126 quickinstall quickinstall --allow-quickinstall
}

# A failure log needs the API quota crabx had: the token's, and the one this address shares
# with other tenants, which crabx spends without a token.
quota() { curl -fsS --max-time 30 "$@" https://api.github.com/rate_limit | tr -d ' \n' | grep -o '"core":{[^}]*}'; }
echo "e2e: unauthenticated GitHub API quota: $(quota)"
token=${GH_TOKEN:-${GITHUB_TOKEN:-}}
if [ -n "$token" ]; then echo "e2e: token's GitHub API quota: $(quota -H @- <<<"Authorization: Bearer $token")"; fi
cases online

# refused SPEC EXPECT [VAR=VALUE...]: crabx refuses SPEC, naming what to do instead, and installs nothing.
refused() {
    local spec=$1 expect=$2 error
    shift 2
    echo "e2e: $* crabx $spec refuses"
    if error=$(env "$@" "$crabx" "$spec" --version 2>&1); then fail "$spec ran: $error"; fi
    case $error in *"$expect"*) ;; *) fail "$spec failed with: $error" ;; esac
    if [ -e "$cache/author/${spec%@*}/${spec#*@}" ]; then fail "$spec left a cache entry"; fi
}
# Only quickinstall serves it, and the quickinstall entry cached above must not satisfy the default policy.
refused cargo-expand@1.0.126 "no usable prebuilt release for cargo-expand@1.0.126 on $target; use --build to compile explicitly"
# A token GitHub rejects fails by name instead of being resent for every candidate URL.
refused bat@0.26.1 "GitHub rejected the GH_TOKEN or GITHUB_TOKEN token; replace or unset it" GH_TOKEN=ghp_e2e_rejected_token

block
# An uncached version fails offline but installs once the network is back, so the block, not
# some other failure, is what stopped it, while the cached cases succeed under the same block.
if error=$(offline "$crabx" bat@0.26.1 --version 2>&1); then fail "the network block let crabx install bat@0.26.1"; fi
echo "e2e: offline crabx bat@0.26.1 fails: $error"
cases offline
unblock
pinned online bat@0.26.1 author "author release (no published attestation)\""
echo "e2e: every case passed on $target"
