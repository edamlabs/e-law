import asyncio
import logging
from typing import Any, Optional, Union

import pytest

from . import sibling

log = logging.getLogger(__name__)
COUNT = 0


class Holder:
    items = []


def first(a, b: Any) -> Optional[int]:
    global COUNT
    import json
    handle = open("x")
    print(a)
    log.info(f"{a}")
    COUNT += 1
    return None


def second(x: Union[int, str]):
    try:
        pass
    except:
        pass
    try:
        pass
    except Exception:
        pass
    try:
        pass
    except ValueError:
        raise RuntimeError("x")
    if x:
        y = 1
    else:
        y = 2
    out = []
    for i in range(3):
        out.append(i)
    try:
        pass
    finally:
        return out


async def third():
    asyncio.create_task(third())
    with pytest.raises(Exception):
        pass
