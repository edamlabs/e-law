#!/usr/bin/env bash
NAME=wheel-content
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

whl=$(wheel)
want=$(version)
listing=$(python3 -I - "$whl" <<'PY'
import sys, zipfile
z = zipfile.ZipFile(sys.argv[1])
print("\n".join(z.namelist()))
print("----")
print(z.read(next(n for n in z.namelist() if n.endswith("METADATA"))).decode())
PY
)
files=${listing%%----*}
meta=${listing#*----}

grep -qE '\.data/scripts/law$' <<<"$files" || die "no law script in: $files"
grep -qx 'e_law/__main__.py' <<<"$files" || die "no e_law/__main__.py"
grep -qx 'e_law/find.py' <<<"$files" || die "no e_law/find.py"
! grep -q '__init__.py' <<<"$files" || die "an __init__.py is in the wheel"
for dep in 'ruff>=0.17.6' 'ty>=0.0.112' 'tach>=0.35.3' 'prek>=0.5.4'; do
    grep -qx "Requires-Dist: $dep" <<<"$meta" || die "METADATA lacks Requires-Dist: $dep"
done
grep -qx 'Requires-Python: >=3.12' <<<"$meta" || die "METADATA Requires-Python"
grep -qx "Version: $want" <<<"$meta" || die "METADATA version is not $want"
printf 'size wheel %s bytes\n' "$(stat -c %s "$whl")"
ok "$NAME"
