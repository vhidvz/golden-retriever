#!/bin/sh
# Optionally sign SHA256SUMS with a detached GPG signature.
#   BUDDY_SIGNING_KEY=KEYID scripts/sign-release.sh [DIR]
#   BUDDY_SIGNING_PRIVATE_KEY="$(cat key.asc)" imports a key first.
# No key configured is not an error: the release stays unsigned-checksum-only.
set -eu
dir=${1:-dist/release}
[ -f "$dir/SHA256SUMS" ] || { echo "no SHA256SUMS in $dir" >&2; exit 1; }
command -v gpg >/dev/null 2>&1 || { echo "gpg is required for signing" >&2; exit 1; }
if [ -n "${BUDDY_SIGNING_PRIVATE_KEY:-}" ]; then
    printf '%s' "$BUDDY_SIGNING_PRIVATE_KEY" | gpg --batch --import >/dev/null 2>&1
fi
if [ -z "${BUDDY_SIGNING_KEY:-}" ]; then
    echo "No BUDDY_SIGNING_KEY is configured; leaving SHA256SUMS unsigned."
    exit 0
fi
sign() {
    if [ -n "${BUDDY_SIGNING_PASSPHRASE:-}" ]; then
        printf '%s' "$BUDDY_SIGNING_PASSPHRASE" | gpg --batch --yes --pinentry-mode loopback --passphrase-fd 0 "$@"
    else
        gpg --batch --yes "$@"
    fi
}
sign --local-user "$BUDDY_SIGNING_KEY" --armor --detach-sign --output "$dir/SHA256SUMS.asc" "$dir/SHA256SUMS"
gpg --batch --verify "$dir/SHA256SUMS.asc" "$dir/SHA256SUMS" >/dev/null
echo "signed $dir/SHA256SUMS.asc"
