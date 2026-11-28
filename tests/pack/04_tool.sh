#!/usr/bin/env bash
NAME=uv-tool-install
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

whl=$(wheel)
export UV_TOOL_DIR="$WORK/tools" UV_TOOL_BIN_DIR="$WORK/bin"
uv tool install --quiet "$whl"
[[ -L $WORK/bin/law ]] || die "the exposed law is not a symlink"
fixture "$WORK/project"
set +e
out=$(run_scrubbed "$WORK/bin/law" "$WORK/project" check "${RULES[@]}")
code=$?
set -e
expect_both "$out" "$code"
ok "$NAME symlink"

mkdir "$WORK/lone"
cp "$(readlink -f "$WORK/bin/law")" "$WORK/lone/law"
set +e
out=$(run_scrubbed "$WORK/lone/law" "$WORK/project" check "${RULES[@]}")
code=$?
set -e
[[ $code -eq 2 ]] || die "lone binary exited $code, wanted 2: $out"
grep -qE 'ruff|tach' <<<"$out" || die "lone binary names no missing tool: $out"
printf 'lone binary said: %s\n' "$(head -n1 <<<"$out")"
ok "$NAME lone-binary-refused"
