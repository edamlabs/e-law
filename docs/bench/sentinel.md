# sentinel bench — `law` against the gates it replaces

Date 2026-11-27. Harness, raw samples and `run.sh`: `.ergon/scratch/sentinel/bench/` (`raw/*.json` hold every timing and the exact command lines; `raw/*.md` the tables below). Replaces the SANITY numbers of `.ergon/reports/plans/sentinel.md` §1.1, §5, §5b, §14, §14b; none copied.

## Subject and environment

| item | value |
|---|---|
| e-law tree | HEAD `6bb9d74`, uncommitted: `git status --short \| wc -l` = 18 at build time; `cargo build --release -p law-cli`, rustc 1.94.0 |
| `law` binary | sha256 `77e936dd47bf1d75b2695b8d0ab8045bb0f49877f67a33cb0058801f780fdc09`, 0.1.0, links ast-grep-core 0.51.3 |
| corpus | a private Python service of the author's (commit `ce4b2501`, clean), temp copies of `src tests gates` (+ `harness` data for the semantic arm, `pyproject.toml`, `tach.toml`, `.gitignore`), `.venv` symlinked: **587 `.py` files, 67 209 lines**; the original never written |
| copies | `py/` original pragmas (Python arms); `law/` pragmas respelled `<ns>.<id>` in 36 files + `law.exceptions.toml` (37 licences, `reach = "pragma"`), the parity catalogue; `law-eager/` = `py/` + `catalogues = ["eager"]` |
| machine | i9-14900K, 32 logical CPUs, governor `powersave`, kernel 7.0.0-44-generic, 192 GiB |
| load | **not idle**: load average 3-5 before arms A-E, 6.7-10 around F-G (a desktop session and a JVM at ~90% of one core were running; not mine, not stopped). Interleaving spreads it over both arms; it widens the intervals and is not removed |
| tools | python 3.14.3 (venv and harness); ast-grep 0.51.1 (venv, used by the Python gate) and 0.51.3 (`uv tool`); ruff 0.17.6, ty 0.0.112, tach 0.35.3 (`uv tool`, first on PATH; the venv holds ruff 0.17.3, ty 0.0.94, not used); uv 0.12.15; graphify 0.9.102 |
| method | no `hyperfine`: stdlib harness (`lib.py`): `Popen` + `os.wait4` (wall by `perf_counter`; CPU and peak RSS from rusage), stdout discarded, every arm of a bench run in each round with the order reversed on alternate rounds (A B / B A), 3 warm-ups (E: 2), n = 30 rounds, extended by 10 while any arm > 20 ms had CV > 5%, capped at 60 (B, F: 40). Every bench run reached its cap: law's CV is 10-14% on ~50 ms runs. Ratio = median(num)/median(den), paired bootstrap (10 000 resamples of rounds, seed 7), 95%. Exit codes asserted before timing. |
| threshold | pre-registered: a ratio whose interval excludes 1.00 is a difference; no practical threshold beyond it |

## A. The syntax gate

Hypothesis: `law check` (ported catalogue) is faster than `gates.engine check`. Work: the same 36 ast-grep rules + 1 tree rule over `src tests gates`; both exit 0 with no finding (asserted: Python prints nothing, law "All checks passed."). Work differs in: the Python gate runs ast-grep 0.51.1 and the `tree` rule in Python, law links 0.51.3 and also resolves the pragmas against the licences; law scans from respelled-pragma files (comment text only differs).

Commands (cwd the copy): `uv run --no-sync python -m gates.engine check` · `law check` · same Python gate with ast-grep 0.51.3 first on PATH (venv python direct) · floors: `uv run --no-sync python -c "import gates.engine"`, `.venv/bin/python -c "import gates.engine"`, `.venv/bin/python -c pass`, `law --version`.

| arm | n | median ms | IQR | min | CV | CPU ms | peak RSS MiB |
|---|---|---|---|---|---|---|---|
| py_check | 60 | 127.5 | 8.2 | 114.6 | 4.6% | 550 | 32 |
| law_check | 60 | 52.4 | 6.9 | 44.5 | 10.5% | 681 | 46 |
| py_check, ast-grep 0.51.3 | 60 | 118.8 | 9.2 | 109.8 | 5.1% | 540 | 32 |
| floor: uv run + import gates.engine | 60 | 66.0 | 6.5 | 59.6 | 6.8% | 66 | 26 |
| floor: python + import gates.engine | 60 | 57.7 | 6.0 | 50.6 | 7.7% | 58 | 26 |
| floor: python -c pass | 60 | 10.9 | 3.2 | 8.1 | 16.0% | 11 | 19 |
| floor: law --version | 60 | 0.9 | 0.2 | 0.6 | 13.6% | 1 | 19 |

| ratio | median | 95% CI |
|---|---|---|
| py_check / law_check | 2.43 | [2.37, 2.50] |
| py_check(0.51.3) / law_check | 2.27 | [2.20, 2.32] |
| interpreter + imports (no work) / py_check | 0.45 (python), 0.52 (uv run) | [0.44, 0.46], [0.50, 0.53] |

Verdict: **confirmed** — law is faster by 2.4x [2.37, 2.50] wall. Imports of the gate alone are 45% of the Python wall time (57.7 of 127.5 ms; 66 ms through `uv run`). Law spends more CPU (681 vs 550 ms) and 14 MiB more memory: it is faster because it is parallel (see D), not cheaper.
Threats: load; ast-grep version is not the driver (0.51.3 Python: 2.27x); law's CV 10% at 50 ms; the Python arm is whatever `uv run` costs (included, as invoked by the Makefile).

## B. Genealogy — NOT identical work

Baseline `gates.engine semantic` (imports the program: `PYTHONPATH=<copy>/src`, `CHAT_PROVIDER=openai` set by me, else the settings refuse to load) judges **10 of the 816 methods** that reach its `gate_written` test (861 non-dunder methods in 640 classes; 45 dropped as foreign; 799 of 816 carry a `<@beartype…>` `co_filename`, hence "not written in the class's file"; `raw/coverage_py.json`). Challenger `law check --select ost.private-lineage --select ost.privates-first` (eager catalogue) judges statically **638 classes, 0 skipped**, 269 on a builtin base (`-v`); it does not count methods. Findings: Python 0, rc 0; law 3 `ost.private-lineage` (the three `_prepare_*` of `RestoredAGUIAgent`, §14b) + 159 `law.pragma` (the unrespelled `# ast-grep-ignore` pragmas of the copy; `--ignore law.pragma` leaves the time unchanged), rc 1. The sentinel report's 319/3174 and 593/15 do not reproduce: the counting differs (mine counts methods at the `gate_written` call; law's tree has moved) and was not reconciled.

| arm | n | median ms | IQR | min | CV | CPU ms | RSS MiB |
|---|---|---|---|---|---|---|---|
| python semantic | 40 | 2329.4 | 78.1 | 2255.7 | 2.3% | 2330 | 264 |
| law (2 members rules) | 40 | 90.7 | 8.0 | 83.3 | 7.6% | 741 | 53 |

Verdict: the times are reported, no ratio headline: two different amounts of work. The Python gate returned a clean verdict on a tree it judged 1.2% of at method level; law judged every class and found the three true violations.
Threats: coverage units differ (methods vs classes); law's "judged" includes classes on opaque builtin bases whose inherited publics are unknown (a possible false-positive source, not measured here).

## C. ast-grep in process vs the binary

Hypothesis: in process is not slower than the subprocess. Work: the same 36 rules over `src tests gates`; the ast-grep argv is the one `gates.engine` builds, captured (`raw/astgrep_argv.json`: `scan --inline-rules … --json=stream --include-metadata src tests gates`); law with `--select` of the 36 pattern ids (the tree rule excluded), plus exception handling. 0 findings in all three.

| arm | n | median ms | IQR | min | CV | CPU ms | RSS MiB (wait4; `/usr/bin/time -v` agrees: 32.6 / 32.4 / 47.9) |
|---|---|---|---|---|---|---|---|
| ast-grep 0.51.1 (venv) | 60 | 60.0 | 5.7 | 54.2 | 8.0% | 488 | 32 |
| ast-grep 0.51.3 | 60 | 61.7 | 8.7 | 54.0 | 12.6% | 502 | 32 |
| law, 36 pattern rules | 60 | 53.6 | 8.1 | 45.7 | 11.5% | 719 | 46 |

| ratio | median | 95% CI |
|---|---|---|
| ast-grep 0.51.1 / law | 1.12 | [1.09, 1.14] |
| ast-grep 0.51.3 / law | 1.15 | [1.12, 1.21] |
| 0.51.1 / 0.51.3 | 0.97 | [0.92, 1.00] |

Verdict: **confirmed** (not slower: 12-15% faster). Cost: 47% more CPU time and 14 MiB more peak RSS than the binary. Versions 0.51.1 vs 0.51.3 are indistinguishable at this n.
Threats: the binary pays JSON serialisation of matches (none here: zero findings) and law reads the exceptions registry; wall gap is within load noise on the lower bound.

## D. Scaling

Files: explicit path arguments, one nested random sample (seed 7) of 1, 10, 100 files, then the whole tree. **Not identical work**: `gates.engine check [paths]` scans the whole tree with ast-grep and only then filters its violations to the paths (`gate_within`); law reads only the files named. The Python times do not move (131 / 135 / 133 / 131 ms) for that reason; the ratios below are not speed-ups of equal work.

| files | py median ms | law median ms | law CPU ms | py / law (95% CI) |
|---|---|---|---|---|
| 1 | 131.2 | 18.2 | 21 | 7.19 [6.92, 7.68] |
| 10 | 134.7 | 19.3 | 35 | 6.98 [6.62, 7.42] |
| 100 | 132.7 | 29.2 | 162 | 4.54 [4.35, 4.86] |
| all (587) | 131.4 | 55.0 | 713 | 2.39 [2.30, 2.56] |

law, 1 file → whole tree: 3.02x [2.78, 3.26]; 18 ms for one file is process start + catalogue load (`--version` alone: 0.9 ms; the split between them is not measured).

Threads, law on the whole tree, `RAYON_NUM_THREADS`:

| threads | 1 | 2 | 4 | 8 | all (unset) |
|---|---|---|---|---|---|
| median ms | 382.8 | 204.7 | 118.9 | 78.6 | 53.0 |
| vs unset (CI) | 7.23 [6.84, 7.39] | 3.87 [3.65, 3.96] | 2.24 [2.13, 2.30] | 1.48 [1.40, 1.53] | 1 |

Both arms, the same pinning (`taskset`, one logical CPU per physical core in the order 0,2,…,16,17,…; "24" = 24 physical cores, not 32 threads; identical work, whole tree):

| cores | 1 | 2 | 4 | 8 | 24 |
|---|---|---|---|---|---|
| py median ms | 401.1 | 239.8 | 160.1 | 123.2 | 131.4 |
| law median ms | 378.0 | 198.4 | 113.4 | 69.5 | 57.6 |
| py / law | 1.06 [1.06, 1.07] | 1.21 [1.20, 1.22] | 1.41 [1.36, 1.46] | 1.77 [1.74, 1.82] | 2.28 [2.25, 2.34] |

Verdict: law scales with cores (7.2x from 1 thread to all, sub-linear: ~14x ideal over 32 threads); on one core the two are within 6% [1.06, 1.07]; the gap of A grows with cores because the Python side stops improving at 8 cores (its floor is the interpreter, ~58 ms). The Python arm has no thread knob; pinning is the only common one.
Threats: pinning by `taskset` leaves ast-grep and rayon to size their pools from the affinity mask (assumed, not verified in the binaries); P/E core mix at 17+; CV 14% for law at all-cores.

## E. Cold vs warm

True cold cache is impossible without root. Done instead: `posix_fadvise(DONTNEED)` before each cold run on every file the arm touches (Python: 1786 files = loaded modules, `.pyc`, `.so`, interpreter, `uv`, copy; law: binary + corpus 1029 files), checked with `mincore` on a 251-file probe: resident pages 9560 → 3707 of 10 376 (36% stay resident: shared pages mapped by running processes, other users). Dentry/inode caches and the rest of the page cache are not dropped. Call it **partially cold file cache, not cold**.

| arm | n | median ms | IQR | CV |
|---|---|---|---|---|
| law cold | 60 | 67.9 | 14.5 | 17.8% |
| py cold | 60 | 163.6 | 13.0 | 7.4% |
| law warm | 60 | 53.2 | 7.9 | 13.4% |
| py warm | 60 | 128.1 | 7.6 | 4.1% |

py / law: cold 2.41 [2.22, 2.53], warm 2.41 [2.26, 2.48]. cold / warm: py 1.28 [1.25, 1.31], law 1.28 [1.18, 1.37].
Verdict: the A ratio holds under a partly dropped cache (2.41 both); the cold penalty is 1.28x for both. Not measured: a true cold boot, network filesystems, first run after a rebuild.

## F. The whole eager catalogue — one command vs the chain

**Not the same rule sets.** law: 84 rules (38 pattern, 22 ruff, 12 tach, 8 profile, 2 members, 1 ty, 1 tree); exit 1, 403 findings (159 `law.pragma` + 244 from eager rules this corpus does not satisfy, e.g. `symbolon.no-object` 96, `natures.provider-surname` 47). Chain: the corpus's 37 gate rules + genealogy + the project's ruff, ty, tach config; exits: gates check 0, semantic 0, ruff 0, ty 1 (7 diagnostics, warnings about renamed rules), tach check 0, tach check-external 1. The catalogue is not the project's configuration: findings differ and are not compared.

| arm | n | median ms | IQR | CV | CPU ms | RSS MiB |
|---|---|---|---|---|---|---|
| `law check` (eager, 84 rules) | 40 | 485.8 | 19.9 | 2.9% | 5035 | 420 |
| chain, sequential in one `bash -c` | 40 | 2891.3 | 84.2 | 1.9% | 6267 | 439 |
| gates check | 40 | 127.2 | 7.3 | 4.7% | 543 | 32 |
| gates semantic | 40 | 2343.8 | 97.0 | 2.5% | 2345 | 264 |
| ruff check (cache warm) | 40 | 12.0 | 1.5 | 13.5% | 38 | 21 |
| ty check | 40 | 183.5 | 22.3 | 8.7% | 2823 | 438 |
| tach check | 40 | 122.6 | 7.5 | 5.0% | 204 | 53 |
| tach check-external | 40 | 128.1 | 9.8 | 6.0% | 197 | 51 |

chain / law = 5.95 [5.88, 6.08]. Components sum to 2917 ms (medians); 80% of the chain is `semantic`, which B shows is not the same work.

law's own time by engine (`--select` of that engine's ids, each run a full process; they overlap in the full run, so they do not sum to it):

| selection | median ms | CPU ms |
|---|---|---|
| all 84 | 482.5 | 5057 |
| tach (12) | 376.0 | 1050 |
| ty (1) | 231.1 | 3936 |
| members (2) | 91.3 | 822 |
| pattern (38) | 56.0 | 692 |
| ruff (22) | 51.0 (CV 24%) | 857 |
| profile (8) | 44.0 | 548 |
| tree (1) | 36.8 (CV 25%) | 555 |
| `law.pragma` only | 37.0 | 542 |

Verdict: one command is 5.95x [5.88, 6.08] faster than the chain it replaces, on non-identical rule sets (law runs 84 catalogue rules, the chain the project's own gates). Where law runs the same external tool: its ty engine 231 ms vs `ty check` 184 ms, its tach engine 376 ms vs `tach check` + `check-external` 123 + 128 ms: **slower than the tools it wraps**; its ruff engine 51 ms vs `ruff check` 12 ms (the 22 rules are a selection; whether law's ruff run uses ruff's cache was not examined).
Threats: load rose to 6-10 during F and its engine runs; ty/tach/ruff versions differ from those the project pins (venv ruff 0.17.3, ty 0.0.94); RSS 420 MiB is ty.

## G. Graph sources

Corpus graph input: the 587 files. Commands: `ruff analyze graph src tests gates` (JSON, file level, dependencies); `law check -v --select ost.private-lineage`; `graphify update .` on a temp copy (3 runs, no bootstrap).

| source | time | nodes | edges | what it is |
|---|---|---|---|---|
| `ruff analyze graph` | 21.9 ms median (n=60, IQR 9.8, CV 31.7%, 292 ms CPU, 37 MiB) | 587 files | 2525 | file → first-party files it imports |
| `law check -v --select ost.private-lineage` | 96.8 ms median (n=60, CV 10.6%) | — | — | a census on stderr: "638 classes judged, 0 skipped; modules read: 22 project, 34 foreign". No graph is emitted; it reads imports on demand, 56 modules. Time is the whole rule run, not graph construction |
| `graphify update` 0.9.102 | 7.22 / 7.52 / 7.35 s, 382 MiB | 10 751 (7573 code, 2521 rationale, 455 document, 202 concept) | 34 715 (`imports_from` 5526, `imports` 3418, `calls` 5814, `uses` 6345, `contains` 4715, …) | symbol level; includes yaml/doc nodes (`gates/rules.yml`); deterministic path, no LLM key set |

`uvx graphify`: not run via uvx — `error: No solution found when resolving tool dependencies … there are no versions of graphify` (attempt 1); the already-installed `graphify` 0.9.102 was used (attempt 2).

Correctness of `ruff analyze graph`, against my own `ast` walk of the imports (resolving `import a.b`, `from a.b import c` to a corpus file): 10 modules of `src` (seeded sample from those with ≥ 1 first-party import: selection bias) — 44 edges by ast, 44 by ruff, **0 differences** (script: `graphcheck.py`). Whole tree: ast 3249 edges, ruff 2525; 730 edges ast-only, 6 ruff-only. The 730: 729 are importers in `tests/` and 1 `gates/engine.py → gates/models.py` — ruff does not resolve `tests.*` and `gates.*` as first party (its `src` setting covers `src` only); in `src/` the two agree. The 6: self-edges where a module named like a third-party package (`providers/adapters/anytomd.py`, `drivers/chat/openai.py`, `database/psycopg.py`, …) imports that package and ruff resolves it to the module itself: false edges. My `ast` resolver is itself unaudited beyond this comparison; "hand-checked" here means a script I wrote and spot-read, not 10 modules read line by line.

What each gives: ruff — file-level, first-party only, no third-party edges, no symbols, no cycle report (cycles: not computed); needs `src` configured for tests/gates; 22 ms. law (members) — class-level resolution of bases including the environment's `.py/.pyi` (34 foreign modules), per demand, no exportable graph. graphify — symbol level (calls, uses, contains, imports), 330x slower than ruff, 4x memory, extra node kinds; its import edges were not compared with the `ast` walk and its third-party handling was not examined.

## What is not measured

- A true cold cache (E: partial, by `fadvise`); first run after a rebuild; any other machine, filesystem or OS; a quiet machine (load 3-10 throughout).
- Equal-work scaling by path set for the Python gate (it filters after scanning; D).
- Method-level coverage of law's genealogy, its false-positive rate on opaque builtin bases, and the 3 findings against the running program here (only their identity with §14b).
- The eager catalogue's findings against the project's own ruff/ty/tach findings (F compares time only).
- Why law's tach and ty engines are slower than the tools (no profile taken); whether law's ruff run uses ruff's cache.
- graphify: import-edge correctness, third-party edges, cycles; cycle detection in ruff's graph; n=3 only, no interval.
- Memory beyond peak RSS; energy; compile time; Windows/macOS.
- `taskset` and `RAYON_NUM_THREADS` effects on ast-grep's own thread pool in the Python arm beyond the observed times.
