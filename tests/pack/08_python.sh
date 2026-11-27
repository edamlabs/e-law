#!/usr/bin/env bash
NAME=python-shim-gates
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

(cd "$WORK" && uvx ruff@0.17.6 check --isolated "$REPO/python/")
(cd "$WORK" && uvx ty@0.0.112 check --extra-search-path "$REPO/python" "$REPO/python/")
ok "$NAME"
