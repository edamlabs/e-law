#!/usr/bin/env bash
NAME=binary-missing
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

whl=$(wheel)
venv_with "$WORK/venv" "$whl"
rm "$WORK/venv/bin/law"
mkdir "$WORK/home"
set +e
err=$(env HOME="$WORK/home" "$WORK/venv/bin/python" -m e_law --version 2>&1 >/dev/null)
code=$?
out=$(env HOME="$WORK/home" "$WORK/venv/bin/python" -m e_law --version 2>/dev/null)
set -e
[[ $code -eq 1 ]] || die "exit $code, wanted 1: $err"
[[ -z $out ]] || die "something on stdout: $out"
[[ $(wc -l <<<"$err") -eq 1 ]] || die "not one line on stderr: $err"
grep -q '^e-law: law is in none of: ' <<<"$err" || die "not the one-line message: $err"
grep -q 'Traceback' <<<"$err" && die "a traceback leaked: $err"
grep -q 'LawNotFoundError' <<<"$err" && die "the typed error leaked: $err"
grep -qF "$WORK/venv/bin" <<<"$err" || die "the message does not name the venv's bin: $err"
uv pip install --quiet --no-deps --python "$WORK/venv/bin/python" --target "$WORK/target" "$whl"
[[ -x $WORK/target/bin/law ]] || die "pip --target put no binary in target/bin"
found=$(env HOME="$WORK/home" PYTHONPATH="$WORK/target" "$WORK/venv/bin/python" -m e_law --version 2>&1) \
    || die "the --target lookup (package dir's bin) no longer finds the binary: $found"
grep -q '^law ' <<<"$found" || die "unexpected --version from a --target install: $found"
printf 'stderr (%s lines), exit %s:\n%s\n' "$(wc -l <<<"$err")" "$code" "$err"
ok "$NAME"
