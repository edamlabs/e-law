"""Where the `law` binary is, looked for in the order ruff looks for its own."""

import sys
import sysconfig
from pathlib import Path
from typing import Final

EXECUTABLE: Final[str] = f"law{sysconfig.get_config_var('EXE') or ''}"
PACKAGE: Final[Path] = Path(__file__).parent
INSTALLED: Final[str] = "lib/python*/site-packages/e_law"
SCRIPTS: Final[str] = "scripts"
BASE: Final[str] = "base"
USER: Final = "user"
BIN: Final[str] = "bin"
PREFIX_DEPTH: Final[int] = 3


class LawNotFoundError(FileNotFoundError):
    """The `law` binary is in none of the places it is looked for."""

    def __init__(self, places: tuple[Path, ...]) -> None:
        """The error naming every place tried."""
        super().__init__(f"{EXECUTABLE} is in none of: {', '.join(map(str, places))}")
        self.places = places


def law() -> Path:
    """The `law` binary, in the first of the places ruff looks for its own."""
    places = tuple(
        dict.fromkeys(
            Path(place)
            for place in (
                sysconfig.get_path(SCRIPTS),
                sysconfig.get_path(SCRIPTS, vars={BASE: sys.base_prefix}),
                str(PACKAGE.parents[PREFIX_DEPTH] / BIN)
                if PACKAGE.match(INSTALLED)
                else None,
                str(PACKAGE.parent / BIN),
                sysconfig.get_path(
                    SCRIPTS, scheme=sysconfig.get_preferred_scheme(USER)
                ),
            )
            if place
        ),
    )
    if (
        found := next(
            (path for place in places if (path := place / EXECUTABLE).is_file()), None
        )
    ) is None:
        raise LawNotFoundError(places)
    return found
