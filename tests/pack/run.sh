#!/usr/bin/env bash
set -euo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PACK_DIST=$(mktemp -d)
export PACK_DIST
NAME=run
source "$HERE/lib.sh"
trap 'rm -rf "$PACK_DIST" "$WORK"' EXIT
wheel >/dev/null
for proof in "$HERE"/[0-9][0-9]_*.sh; do
    bash "$proof"
done
ok pack
