"""The bank of the interpreter's own classes: what the closed list of modules below binds, and every class in their
lines, read off a live interpreter. The list is written, never scanned, so the bank follows the interpreter's version
and not the build it came from. Regenerate: `uv run --no-project --python 3.14 python scripts/builtins.py > crates/law-engine/src/builtins.json`."""

import collections.abc
import importlib
import inspect
import json
import sys
import sysconfig
import typing
from pathlib import Path
from types import ModuleType
from typing import Final

STDLIB: Final[Path] = Path(sysconfig.get_path("stdlib"))
PACKAGE: Final[str] = "__init__"
SOURCE: Final[str] = ".py"
FACTORIES: Final[dict[str, list[str]]] = {
    "typing.NamedTuple": ["builtins.tuple"],
    "typing.TypedDict": [],
    "typing_extensions.NamedTuple": ["builtins.tuple"],
    "typing_extensions.TypedDict": [],
}
WRAPPERS: Final[list[str]] = ["typing._CallableType", "typing._DeprecatedGenericAlias", "typing._TupleType", "typing._alias"]
ANCHORS: Final[tuple[ModuleType, ...]] = (collections.abc, typing)
ABSTRACT: Final[str] = "__isabstractmethod__"
MODULES: Final[tuple[str, ...]] = (
    "_collections",
    "_contextvars",
    "_functools",
    "_io",
    "_thread",
    "_types",
    "_typing",
    "_weakref",
    "abc",
    "builtins",
    "collections.abc",
    "contextlib",
    "dataclasses",
    "enum",
    "itertools",
    "typing",
)
ALIASED: Final[tuple[str, ...]] = ("_frozen_importlib", "_frozen_importlib_external", "collections.abc", "os.path")


def written(module: ModuleType) -> str | None:
    """The dotted name of the stdlib file a module is read from, or None when it has no such file."""
    file = Path(getattr(module, "__file__", None) or "/")
    parts = file.with_suffix("").relative_to(STDLIB).parts if file.is_relative_to(STDLIB) and file.suffix == SOURCE else ()
    return ".".join(parts[:-1] if parts[-1:] == (PACKAGE,) else parts) or None


def entry(cls: type, keys: dict[int, str]) -> dict[str, list[str]]:
    """A class as the bank holds it: its bases, by bank key or else module and name, the methods it writes, and those
    of them it leaves abstract, when any."""
    pending = sorted(name for name, value in vars(cls).items() if getattr(value, ABSTRACT, False) is True)
    methods = {name for name, value in vars(cls).items() if inspect.isroutine(value)}
    return {
        "bases": [keys.get(id(base), f"{base.__module__}.{base.__qualname__}") for base in cls.__bases__],
        "methods": sorted(methods.union(pending)),
    } | ({"abstract": pending} if pending else {})


modules = {name: importlib.import_module(name) for name in MODULES}
bound = sorted(
    (f"{name}.{key}", value)
    for name, module in modules.items()
    for key, value in vars(module).items()
    if isinstance(value, type) and not key.startswith("__")
)
keys = {id(value): key for key, value in reversed(bound)}
ancestors = {f"{base.__module__}.{base.__qualname__}": base for _, value in bound for base in value.__mro__ if id(base) not in keys}
classes = {key: entry(value, keys) for key, value in [*ancestors.items(), *bound]}
aliases = {name: file for name in ALIASED if (file := written(importlib.import_module(name))) not in (None, name)}
head = {
    "aliases": dict(sorted(aliases.items())),
    "factories": FACTORIES,
    "platform": sys.platform,
    "python": sys.version.split()[0],
    "wrappers": WRAPPERS,
}
rows = ",\n".join(f"  {json.dumps(key)}: {json.dumps(classes[key], sort_keys=True)}" for key in sorted(classes))
sys.stdout.write(json.dumps(head, indent=1, sort_keys=True)[:-2] + ',\n "classes": {\n' + rows + "\n }\n}\n")
