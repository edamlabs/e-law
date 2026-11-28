mod common;

use common::{Outcome, Project};

fn flagged(id: &str, snippets: &[(&str, &str)]) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let project = Project::new()?;
    project.config("catalogues = [\"eager\"]\n")?;
    for (name, text) in snippets {
        project.file(&format!("src/p/{name}.py"), text)?;
    }
    let out = project.check(&["--select", id])?;
    let mut names: Vec<String> = out
        .stdout
        .lines()
        .filter(|line| line.contains(&format!(": {id} ")))
        .filter_map(|line| line.strip_prefix("src/p/")?.split(".py").next().map(str::to_owned))
        .collect();
    names.sort_unstable();
    Ok(names)
}

fn verdict(id: &str, bad: &[(&str, &str)], good: &[(&str, &str)]) -> Outcome {
    let all: Vec<(&str, &str)> = bad.iter().chain(good).copied().collect();
    let mut want: Vec<&str> = bad.iter().map(|(name, _)| *name).collect();
    want.sort_unstable();
    assert_eq!(flagged(id, &all)?, want);
    Ok(())
}

#[test]
fn eager_no_pop_front_flags_a_list_attribute_and_spares_the_deque_and_the_other_indexes() -> Outcome {
    verdict(
        "metron.no-pop-front",
        &[("attr", "class A:\n    def f(self):\n        return self.q.pop(0)\n")],
        &[
            ("deque", "x = dq.popleft()\n"),
            ("last", "x = a.pop(-1)\n"),
            ("keyed", "x = d.pop(0, None)\n"),
        ],
    )
}

#[test]
fn eager_bounded_queue_flags_every_unbounded_constructor_and_spares_the_bounded() -> Outcome {
    verdict(
        "kairos.bounded-queue",
        &[
            ("sync", "import queue\nq = queue.Queue()\n"),
            ("prio", "from asyncio import PriorityQueue\nq = PriorityQueue()\n"),
            ("lifo", "import asyncio\nq = asyncio.LifoQueue()\n"),
            ("subscript", "import asyncio\nq = asyncio.Queue[int]()\n"),
            (
                "bare_subscript",
                "from asyncio import Queue\nq: Queue[Item] = Queue[Item]()\n",
            ),
            ("zero", "import asyncio\nq = asyncio.Queue(maxsize=0)\n"),
            ("zero_positional", "import asyncio\nq = asyncio.Queue(0)\n"),
            ("negative", "import asyncio\nq = asyncio.Queue(maxsize=-1)\n"),
            (
                "negative_spaced",
                "import asyncio\nq = asyncio.PriorityQueue( maxsize = - 5 )\n",
            ),
            ("sync_lifo", "import queue\nq = queue.LifoQueue(0)\n"),
            ("subscript_zero", "import asyncio\nq = asyncio.Queue[int](maxsize=0)\n"),
        ],
        &[
            ("positional", "import asyncio\nq = asyncio.Queue(100)\n"),
            (
                "subscript_bounded",
                "import asyncio\nq = asyncio.Queue[int](maxsize=100)\n",
            ),
            ("name", "import asyncio\nq = asyncio.Queue(maxsize=n)\n"),
            ("name_positional", "import asyncio\nq = asyncio.Queue(size)\n"),
            ("ten", "import asyncio\nq = asyncio.Queue(10)\n"),
            ("keyword", "import asyncio\nq = asyncio.Queue(maxsize=100)\n"),
            ("simple", "import queue\nq = queue.SimpleQueue()\n"),
            ("subclass", "class Q(asyncio.Queue):\n    pass\n"),
        ],
    )
}

#[test]
fn eager_no_nested_wait_for_flags_a_wait_for_inside_a_wait_for_argument_only() -> Outcome {
    verdict(
        "kairos.no-nested-wait-for",
        &[(
            "gathered",
            "async def f():\n    await asyncio.wait_for(asyncio.gather(asyncio.wait_for(a(), 1), b()), 5)\n",
        )],
        &[
            (
                "siblings",
                "async def f():\n    await asyncio.gather(asyncio.wait_for(a(), 1), asyncio.wait_for(b(), 2))\n",
            ),
            (
                "timeouts",
                "async def f():\n    async with asyncio.timeout(5):\n        async with asyncio.timeout(1):\n            await x()\n",
            ),
        ],
    )
}

#[test]
fn eager_no_cover_pragma_flags_the_comment_and_spares_a_string_and_another_pragma() -> Outcome {
    verdict(
        "elenchos.no-cover-pragma",
        &[
            ("listed", "x = 1  # pragma: no cover, other\n"),
            ("nocover", "x = 1  # pragma: nocover\n"),
            ("tight", "x = 1  # pragma:no cover\n"),
            ("spaces", "x = 1  # pragma:   no   cover\n"),
            ("hash", "x = 1  #pragma: no cover\n"),
            ("upper", "x = 1  # PRAGMA: NO COVER\n"),
            ("mixed", "x = 1  # pragma: NO cover\n"),
            ("blank", "x = 1  # pragma no cover\n"),
        ],
        &[
            ("capital", "x = 1  # Pragma: no cover\n"),
            ("cased", "x = 1  # pragma: no Cover\n"),
            ("string", "x = \"# pragma: no cover\"\n"),
            ("branch", "if x:  # pragma: no branch\n    pass\n"),
        ],
    )
}

#[test]
fn eager_no_finalizer_flags_del_and_spares_the_other_deleters() -> Outcome {
    verdict(
        "symbolon.no-finalizer",
        &[
            ("async", "class D:\n    async def __del__(self): ...\n"),
            ("decorated", "class D:\n    @final\n    def __del__(self): ...\n"),
            (
                "in_function",
                "def make():\n    class D:\n        def __del__(self): ...\n",
            ),
        ],
        &[
            ("module", "def __del__(): ...\n"),
            ("module_decorated", "@final\ndef __del__(): ...\n"),
            ("nested", "class D:\n    def run(self):\n        def __del__(): ...\n"),
            ("descriptor", "class D:\n    def __delete__(self, obj): ...\n"),
            ("attribute", "class D:\n    def __delattr__(self, name): ...\n"),
        ],
    )
}

#[test]
fn eager_slots_dataclass_flags_every_form_without_slots_true() -> Outcome {
    verdict(
        "lexis.slots-dataclass",
        &[
            ("bare", "@dataclass\nclass R:\n    a: int\n"),
            ("call", "@dataclass()\nclass R:\n    a: int\n"),
            ("frozen", "@dataclass(frozen=True)\nclass R:\n    a: int\n"),
            ("off", "@dataclass(slots=False)\nclass R:\n    a: int\n"),
            ("variable", "@dataclass(slots=SLOTS)\nclass R:\n    a: int\n"),
            (
                "commented",
                "@dataclass(frozen=True)  # slots=True\nclass R:\n    a: int\n",
            ),
            ("commented_bare", "@dataclass  # slots=True\nclass R:\n    a: int\n"),
            (
                "commented_inside",
                "@dataclass(\n    frozen=True,  # slots=True\n)\nclass R:\n    a: int\n",
            ),
            ("positional", "@dataclass(True)\nclass R:\n    a: int\n"),
        ],
        &[
            (
                "commented_valid",
                "@dataclass(slots=True)  # frozen\nclass R:\n    a: int\n",
            ),
            ("other", "@total_ordering\nclass R:\n    a: int\n"),
            ("module", "@dataclasses.dataclass(slots=True)\nclass R:\n    a: int\n"),
            (
                "lines",
                "@dataclass(\n    frozen=True,\n    slots=True,\n)\nclass R:\n    a: int\n",
            ),
            (
                "stacked",
                "@dataclass(slots=True, frozen=True)\n@final\nclass R:\n    a: int\n",
            ),
            ("hand", "class R:\n    __slots__ = (\"a\",)\n"),
        ],
    )
}

#[test]
fn eager_no_method_cache_flags_a_cached_method_and_spares_functions_and_static_forms() -> Outcome {
    verdict(
        "kairos.no-method-cache",
        &[(
            "unbounded",
            "class A:\n    @functools.lru_cache(maxsize=None)\n    def f(self): ...\n",
        )],
        &[
            ("function", "@cache\ndef f(x): ...\n"),
            ("static", "class A:\n    @staticmethod\n    @cache\n    def f(x): ...\n"),
            (
                "classmethod",
                "class A:\n    @classmethod\n    @cache\n    def f(cls): ...\n",
            ),
            (
                "property",
                "class A:\n    @functools.cached_property\n    def f(self): ...\n",
            ),
            (
                "closure",
                "class A:\n    def m(self):\n        @cache\n        def inner(x): ...\n",
            ),
        ],
    )
}
