#!/usr/bin/env bash
NAME=embedded-catalogue
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

whl=$(wheel)
venv_with "$WORK/venv" "$whl"
fixture "$WORK/embedded" eager
fixture "$WORK/disk" "$REPO/catalogues/eager/"

digest() {
    local out
    out=$(cd "$1" && env -i PATH="$WORK/venv/bin:$SCRUBBED" "$WORK/venv/bin/law" check --output-format json || true)
    [[ -n $out ]] || die "no report from $1"
    printf '%s' "$out" | python3 -I -c '
import hashlib, json, sys
report = json.loads(sys.stdin.read())
del report["run"]["date"]
print(hashlib.sha256(json.dumps(report, sort_keys=True).encode()).hexdigest())
'
}

a=$(digest "$WORK/embedded")
b=$(digest "$WORK/disk")
[[ $a == "$b" ]] || die "the embedded and the on-disk catalogue differ: $a vs $b"
ok "$NAME $a"
