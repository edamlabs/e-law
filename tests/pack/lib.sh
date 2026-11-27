set -euo pipefail

PACK_HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO=$(cd "$PACK_HERE/../.." && pwd)
MATURIN="maturin@1.16.0"
SCRUBBED="/usr/bin:/bin"
RULES=(--select symbolon.no-jump-finally --select natures.layer-order)

WORK=$(mktemp -d)
cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

version() {
    sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' "$REPO/Cargo.toml"
}

ok() { printf 'ok %s\n' "$1"; }
die() { printf 'FAIL %s: %s\n' "$NAME" "$1" >&2; exit 1; }

wheel() {
    local dist=${PACK_DIST:-$WORK/dist}
    if ! compgen -G "$dist/*.whl" >/dev/null; then
        (cd "$REPO" && uvx "$MATURIN" build --release --out "$dist" >&2)
    fi
    local found=("$dist"/*.whl)
    [[ ${#found[@]} -eq 1 ]] || die "expected one wheel in $dist, found ${#found[@]}"
    printf '%s\n' "${found[0]}"
}

venv_with() {
    local dir=$1 whl=$2
    uv venv --quiet "$dir"
    uv pip install --quiet --python "$dir/bin/python" "$whl"
}

fixture() {
    local dir=$1 catalogue=${2:-eager}
    mkdir -p "$dir/src/pkg/core" "$dir/src/pkg/ui" "$dir/src/pkg/low"
    : >"$dir/src/pkg/__init__.py"
    : >"$dir/src/pkg/core/__init__.py"
    : >"$dir/src/pkg/ui/__init__.py"
    : >"$dir/src/pkg/low/__init__.py"
    printf 'roots = ["src"]\ncatalogues = ["%s"]\n' "$catalogue" >"$dir/law.toml"
    printf 'z = 1\n' >"$dir/src/pkg/ui/view.py"
    printf 'y = 1\n' >"$dir/src/pkg/low/base.py"
    printf 'from pkg.ui import view\n\n\ndef run() -> int:\n    try:\n        return 1\n    finally:\n        return 2\n' \
        >"$dir/src/pkg/core/model.py"
    cat >"$dir/tach.toml" <<'TOML'
source_roots = ["src"]
layers = ["ui", "core", "low"]

[[modules]]
path = "pkg.ui"
layer = "ui"
depends_on = ["pkg.core"]

[[modules]]
path = "pkg.core"
layer = "core"
depends_on = ["pkg.low"]

[[modules]]
path = "pkg.low"
layer = "low"
depends_on = []
TOML
}

expect_both() {
    local out=$1 code=$2
    [[ $code -eq 1 ]] || die "exit $code, wanted 1: $out"
    grep -q 'symbolon.no-jump-finally' <<<"$out" || die "no ruff finding: $out"
    grep -q 'natures.layer-order' <<<"$out" || die "no tach finding: $out"
}

run_scrubbed() {
    local law=$1 project=$2
    shift 2
    (cd "$project" && env -i PATH="$SCRUBBED" "$law" "$@") 2>&1
}
