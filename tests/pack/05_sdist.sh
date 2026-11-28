#!/usr/bin/env bash
NAME=sdist
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

export CARGO_TARGET_DIR="$WORK/target"
want="law $(version)"

build_from() {
    local source=$1 label=$2
    mkdir -p "$WORK/$label/sdist" "$WORK/$label/unpack" "$WORK/$label/dist"
    (cd "$source" && uvx "$MATURIN" sdist --out "$WORK/$label/sdist" >&2)
    local archives=("$WORK/$label/sdist"/*.tar.gz)
    [[ ${#archives[@]} -eq 1 ]] || die "$label: expected one sdist"
    printf 'size sdist(%s) %s bytes\n' "$label" "$(stat -c %s "${archives[0]}")"
    tar -xzf "${archives[0]}" -C "$WORK/$label/unpack"
    local roots=("$WORK/$label/unpack"/*/)
    (cd "${roots[0]}" && uvx "$MATURIN" build --release --out "$WORK/$label/dist" >&2)
}

build_from "$REPO" with-include
whls=("$WORK/with-include/dist"/*.whl)
[[ ${#whls[@]} -eq 1 ]] || die "no wheel from the sdist"
venv_with "$WORK/venv" "${whls[0]}"
[[ $("$WORK/venv/bin/law" --version) == "$want" ]] || die "law --version from the sdist's wheel"
ok "$NAME build-from-sdist"

mkdir "$WORK/copy"
tar -C "$REPO" --exclude=./target --exclude=./.git --exclude=./.ergon --exclude=./.serena --exclude=./dist --exclude=./tests -cf - . |
    tar -C "$WORK/copy" -xf -
grep -v '^include = ' "$REPO/pyproject.toml" >"$WORK/copy/pyproject.toml"
! grep -q '^include = ' "$WORK/copy/pyproject.toml" || die "include line still present in the copy"
set +e
build_from "$WORK/copy" without-include 2>"$WORK/without.err"
code=$?
set -e
if [[ $code -eq 0 ]]; then
    printf 'info include-line: NOT needed, the wheel builds from an sdist without it\n'
else
    printf 'info include-line: NEEDED, without it the build from the sdist fails (exit %s):\n' "$code"
    grep -E 'error|Error|No such file|not found' "$WORK/without.err" | head -n 8 || true
fi
ok "$NAME include-question-answered"
