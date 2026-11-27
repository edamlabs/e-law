#!/usr/bin/env bash
NAME=venv-install
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

whl=$(wheel)
want="law $(version)"
venv_with "$WORK/venv" "$whl"
[[ $("$WORK/venv/bin/law" --version) == "$want" ]] || die "law --version"
[[ $("$WORK/venv/bin/python" -m e_law --version) == "$want" ]] || die "python -m e_law --version"
for tool in ruff ty tach prek; do
    [[ -x $WORK/venv/bin/$tool ]] || die "$tool is not in the venv bin"
done
ok "$NAME"
