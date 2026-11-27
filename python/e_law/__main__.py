"""`python -m e_law`: the `law` binary, run in place of the interpreter."""

import os
import sys

from e_law.find import LawNotFoundError, law

if __name__ == "__main__":
    try:
        binary = law()
    except LawNotFoundError as error:
        raise SystemExit(f"e-law: {error}") from None
    os.execv(binary, [binary, *sys.argv[1:]])
