#!/usr/bin/env bash
NAME=venv-scrubbed-path
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

whl=$(wheel)
venv_with "$WORK/venv" "$whl"
for tool in ruff ty tach prek law; do
    [[ ! -e /usr/bin/$tool && ! -e /bin/$tool ]] || die "$tool is on the scrubbed PATH: the proof would be vacuous"
done
fixture "$WORK/project"
set +e
out=$(run_scrubbed "$WORK/venv/bin/law" "$WORK/project" check "${RULES[@]}")
code=$?
set -e
expect_both "$out" "$code"
json=$(cd "$WORK/project" && env -i PATH="$SCRUBBED" "$WORK/venv/bin/law" check "${RULES[@]}" --output-format json || true)
python3 -I -c '
import json, sys
names = sorted(t["name"] for t in json.loads(sys.stdin.read())["run"]["tools"])
assert names == ["ruff", "tach"], names
' <<<"$json" || die "the report's tools are not ruff and tach"
ok "$NAME"
