use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, PoisonError, RwLock};

use ast_grep_language::{LanguageExt, SupportLang};

use crate::bank::Bank;
use crate::census::Unresolved;
use crate::engine::Sink;
use crate::environment::Environment;
use crate::outline::{ABSTRACTMETHOD, Base, Class, Method, Outline};

const DOT: &str = ".";
const PYTHON: &str = "py";
const STUB: &str = "pyi";
const PACKAGE: &str = "__init__";
const COMPILED: [&str; 2] = [".so", ".pyd"];
const OBJECT: &str = "object";
const BUILTINS: &str = "builtins";
const SRC: &str = "src";

type Key = (PathBuf, String);
type Module = Result<Arc<Outline>, Unresolved>;
type Cell = Arc<OnceLock<Module>>;
type Found = Result<Target, Missing>;

/// A class a class inherits from, and the outline of the module that writes it.
#[derive(Clone)]
pub(crate) struct Ancestor {
    pub(crate) outline: Arc<Outline>,
    pub(crate) class: Arc<Class>,
}

/// A base that did not resolve, and why: named as the module of the class whose base it is writes it; and the part
/// of the line that did resolve, nearest first, each once.
#[derive(Clone)]
pub(crate) struct Missing {
    pub(crate) name: String,
    pub(crate) why: Unresolved,
    pub(crate) resolved: Arc<[Ancestor]>,
}

/// The line of a class: every class it inherits from, nearest first, each once — a module's or the bank's; and whether
/// a class of the bank other than `object` stands in it.
#[derive(Default)]
pub(crate) struct Lineage {
    pub(crate) ancestors: Arc<[Ancestor]>,
    pub(crate) builtin: bool,
}

/// What a base names: a class a module writes; a class of the bank — a bare name no import binds, or a class imported
/// from a module no file holds; a function of the bank's factories, standing for the bank's bases it names; or
/// `object`, the root, which adds nothing.
enum Target {
    Class(Arc<Outline>, String),
    Banked(String),
    Factory(&'static [String]),
    Root,
}

/// The line of a class, or the base where it broke.
pub(crate) type Line = Result<Arc<Lineage>, Missing>;

/// The resolver of bases for one run: a base is a class of the same module, of a module an absolute import names
/// under a source root and then under a foreign root (relative imports resolved inside foreign packages, a stub read
/// where the source does not bind the name), or a class of the bank; each module read and parsed once, each class's
/// line of ancestors walked once. A class of the bank stands in a line as a module's would, its methods its entry's.
#[derive(Default)]
pub(crate) struct Ancestry {
    root: PathBuf,
    roots: Vec<PathBuf>,
    foreign: Vec<PathBuf>,
    modules: RwLock<HashMap<String, Cell>>,
    stubs: RwLock<HashMap<String, Cell>>,
    lines: RwLock<HashMap<Key, Line>>,
    project: AtomicUsize,
    read: AtomicUsize,
}

impl Missing {
    fn new(name: &str, why: Unresolved) -> Self {
        Self {
            name: name.to_owned(),
            why,
            resolved: Arc::default(),
        }
    }

    fn after(self, own: Ancestor) -> Self {
        let resolved = std::iter::once(own).chain(self.resolved.iter().cloned());
        Self {
            resolved: Ancestry::merged(false, resolved).ancestors.clone(),
            ..self
        }
    }
}

impl Ancestry {
    fn candidates(base: &Path, stub: bool) -> impl Iterator<Item = (PathBuf, bool)> {
        let sources = [PYTHON, STUB]
            .into_iter()
            .filter(move |extension| !stub || *extension == STUB);
        sources.flat_map(move |extension| {
            [
                (base.with_extension(extension), false),
                (base.join(PACKAGE).with_extension(extension), true),
            ]
        })
    }

    fn compiled(base: &Path) -> bool {
        let (Some(dir), Some(stem)) = (base.parent(), base.file_name()) else {
            return false;
        };
        let stem = format!("{}{DOT}", stem.to_string_lossy());
        fs::read_dir(dir).into_iter().flatten().flatten().any(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with(&stem) && COMPILED.iter().any(|suffix| name.ends_with(suffix))
        })
    }

    fn located<'a>(
        module: &str,
        stub: bool,
        roots: impl IntoIterator<Item = (&'a Path, bool)>,
        exists: impl Fn(&Path) -> bool,
    ) -> Option<(PathBuf, bool, bool)> {
        let relative: PathBuf = module.split(DOT).collect();
        roots.into_iter().find_map(|(root, foreign)| {
            Self::candidates(&root.join(&relative), stub)
                .find(|(path, _)| exists(path))
                .map(|(path, package)| (path, package, foreign))
        })
    }

    fn load(&self, module: &str, stub: bool) -> Module {
        let relative: PathBuf = module.split(DOT).collect();
        let roots = || {
            self.roots
                .iter()
                .map(|root| (root.as_path(), false))
                .chain(self.foreign.iter().map(|root| (root.as_path(), true)))
        };
        let found = Self::located(module, stub, roots(), |path| self.root.join(path).is_file());
        let Some((path, package, foreign)) = found else {
            let compiled = roots().any(|(root, _)| Self::compiled(&self.root.join(root).join(&relative)));
            return Err(if compiled {
                Unresolved::CompiledModule
            } else {
                Unresolved::NoSuchModule
            });
        };
        let text = fs::read_to_string(self.root.join(&path)).map_err(|_| Unresolved::NoSuchModule)?;
        let tree = SupportLang::Python.ast_grep(&text);
        let parent = || module.rsplit_once(DOT).map_or("", |(parent, _)| parent).to_owned();
        let outline = Outline {
            module: Some(module.to_owned()),
            foreign: foreign.then(|| if package { module.to_owned() } else { parent() }),
            ..Outline::new(&path, &tree.root())
        };
        let counter = if foreign { &self.read } else { &self.project };
        counter.fetch_add(1, Ordering::Relaxed);
        Ok(Arc::new(outline))
    }

    fn module(&self, module: &str, stub: bool) -> Module {
        let modules = if stub { &self.stubs } else { &self.modules };
        let known = modules
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(module)
            .cloned();
        let cell = known.unwrap_or_else(|| {
            let mut modules = modules.write().unwrap_or_else(PoisonError::into_inner);
            Arc::clone(modules.entry(module.to_owned()).or_default())
        });
        cell.get_or_init(|| self.load(module, stub)).clone()
    }

    fn absolute(outline: &Outline, bound: &str) -> Result<String, Missing> {
        if !bound.starts_with(DOT) {
            return Ok(bound.to_owned());
        }
        let package = outline
            .foreign
            .as_deref()
            .ok_or_else(|| Missing::new(bound, Unresolved::RelativeImport))?;
        Self::anchored(package, bound).ok_or_else(|| Missing::new(bound, Unresolved::NoSuchModule))
    }

    /// A dotted name as a module of a package writes it, made absolute: its leading dots climb from the package; none
    /// when they climb above the top.
    pub(crate) fn anchored(package: &str, bound: &str) -> Option<String> {
        let rest = bound.trim_start_matches(DOT);
        let dots = bound.len() - rest.len();
        if dots == 0 {
            return Some(bound.to_owned());
        }
        let parts: Vec<&str> = package.split(DOT).filter(|part| !part.is_empty()).collect();
        let kept = parts.len().checked_sub(dots - 1)?;
        Some(
            parts[..kept]
                .iter()
                .copied()
                .chain((!rest.is_empty()).then_some(rest))
                .collect::<Vec<&str>>()
                .join(DOT),
        )
    }

    /// The project module a dotted name resolves to, among the paths that exist: under a source root, in order, then
    /// under the project root itself, as the interpreter run from the root finds a top-level package; never a stub.
    pub(crate) fn project(&self, module: &str, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
        let roots = self
            .roots
            .iter()
            .map(|root| (root.as_path(), false))
            .chain(std::iter::once((Path::new(""), false)));
        Self::located(module, false, roots, |path| {
            path.extension().is_some_and(|extension| extension == PYTHON) && exists(path)
        })
        .map(|(path, _, _)| path)
    }

    /// The dotted name of a project module at a path from the project root, by the first source root that holds it; a
    /// package's `__init__` named as the package; none outside every source root.
    pub(crate) fn named(&self, path: &Path) -> Option<String> {
        let relative = self.roots.iter().find_map(|root| path.strip_prefix(root).ok())?;
        let stem = relative.with_extension("");
        let parts: Vec<&str> = stem.iter().filter_map(|part| part.to_str()).collect();
        let parts = parts.strip_suffix(&[PACKAGE]).unwrap_or(&parts);
        Some(parts.join(DOT))
    }

    /// The dotted name of a project module at a path from the project root itself, a package's `__init__` named as
    /// the package.
    pub(crate) fn spelled(path: &Path) -> String {
        let stem = path.with_extension("");
        let parts: Vec<&str> = stem.iter().filter_map(|part| part.to_str()).collect();
        parts.strip_suffix(&[PACKAGE]).unwrap_or(&parts).join(DOT)
    }

    fn imported(&self, outline: &Outline, bound: &str, rest: Option<&str>, seen: &mut HashSet<String>) -> Found {
        let bound = Self::absolute(outline, bound)?;
        let next = rest.map_or_else(|| bound.clone(), |rest| format!("{bound}{DOT}{rest}"));
        match self.find(&next, seen) {
            Err(missing)
                if missing.name == next
                    && matches!(missing.why, Unresolved::NoSuchModule | Unresolved::CompiledModule)
                    && Bank::get().classes.contains_key(&next) =>
            {
                Ok(Target::Banked(next))
            }
            found => found,
        }
    }

    fn starred(&self, outline: &Outline, name: &str, seen: &mut HashSet<String>) -> Option<Target> {
        outline.stars.iter().find_map(|star| {
            let module = Self::absolute(outline, star).ok()?;
            self.find(&format!("{module}{DOT}{name}"), seen).ok()
        })
    }

    fn wrapped(outline: &Outline, dotted: &str, name: &str) -> Option<String> {
        let (module, _) = dotted.rsplit_once(DOT)?;
        let (callee, first) = outline.calls.get(name)?;
        Bank::get()
            .wrappers
            .contains(&format!("{module}{DOT}{callee}"))
            .then(|| first.clone())
    }

    fn within(&self, outline: &Arc<Outline>, dotted: &str, name: &str, seen: &mut HashSet<String>) -> Found {
        if outline.classes.contains_key(name) {
            return Ok(Target::Class(Arc::clone(outline), name.to_owned()));
        }
        if let Some(bound) = outline.imports.get(name) {
            return self.imported(outline, bound, None, seen);
        }
        if let Some(bases) = Bank::get()
            .factories
            .get(dotted)
            .filter(|_| outline.functions.contains(name))
        {
            return Ok(Target::Factory(bases));
        }
        if let Some(first) = Self::wrapped(outline, dotted, name) {
            return self.aimed(outline, &first, seen);
        }
        if let Some(found) = self.starred(outline, name, seen) {
            return Ok(found);
        }
        let why = match (outline.values.contains(name), outline.lazy, outline.stars.is_empty()) {
            (true, _, _) => Unresolved::DynamicBase,
            (false, true, _) => Unresolved::LazyExport,
            (false, false, false) => Unresolved::StarImport,
            (false, false, true) => Unresolved::NoSuchName,
        };
        Err(Missing::new(dotted, why))
    }

    fn find(&self, dotted: &str, seen: &mut HashSet<String>) -> Found {
        if !seen.insert(dotted.to_owned()) {
            return Err(Missing::new(dotted, Unresolved::Cycle));
        }
        let (module, name) = dotted
            .rsplit_once(DOT)
            .ok_or_else(|| Missing::new(dotted, Unresolved::DynamicBase))?;
        let module = Bank::get().module(module);
        let outline = self.module(module, false).map_err(|why| Missing::new(dotted, why))?;
        self.within(&outline, dotted, name, seen)
            .or_else(|missing| match self.module(module, true) {
                Ok(stub) if stub.path != outline.path => self.within(&stub, dotted, name, seen).map_err(|_| missing),
                _ => Err(missing),
            })
    }

    fn bare(dotted: &str) -> Found {
        let key = format!("{BUILTINS}{DOT}{dotted}");
        match (dotted == OBJECT, Bank::get().classes.contains_key(&key)) {
            (true, _) => Ok(Target::Root),
            (false, true) => Ok(Target::Banked(key)),
            (false, false) => Err(Missing::new(dotted, Unresolved::NoSuchName)),
        }
    }

    fn aimed(&self, outline: &Arc<Outline>, dotted: &str, seen: &mut HashSet<String>) -> Found {
        let (first, rest) = dotted
            .split_once(DOT)
            .map_or((dotted, None), |(first, rest)| (first, Some(rest)));
        if rest.is_none() && outline.classes.contains_key(first) {
            return Ok(Target::Class(Arc::clone(outline), first.to_owned()));
        }
        if let Some(bound) = outline.imports.get(first) {
            return self.imported(outline, bound, rest, seen);
        }
        if let Some(found) = rest.is_none().then(|| self.starred(outline, first, seen)).flatten() {
            return Ok(found);
        }
        match (
            rest,
            outline.values.contains(first) || outline.classes.contains_key(first),
        ) {
            (None, false) => Self::bare(dotted),
            (_, true) => Err(Missing::new(dotted, Unresolved::DynamicBase)),
            (Some(_), false) => Err(Missing::new(dotted, Unresolved::NoSuchName)),
        }
    }

    fn target(&self, outline: &Arc<Outline>, base: &Base) -> Found {
        match base {
            Base::Name(dotted) => self.aimed(outline, dotted, &mut HashSet::new()),
            Base::Expression(text) => Err(Missing::new(text, Unresolved::DynamicBase)),
        }
    }

    /// A base's dotted name as the module writing it spells it, its first part read through the module's imports.
    pub(crate) fn written(outline: &Outline, base: &Base) -> String {
        let (Base::Name(dotted) | Base::Expression(dotted)) = base;
        if matches!(base, Base::Expression(_)) {
            return dotted.clone();
        }
        let (first, rest) = dotted
            .split_once(DOT)
            .map_or((dotted.as_str(), None), |(first, rest)| (first, Some(rest)));
        match (outline.imports.get(first), rest) {
            (Some(bound), Some(rest)) => format!("{bound}{DOT}{rest}"),
            (Some(bound), None) => bound.clone(),
            (None, _) => dotted.clone(),
        }
    }

    fn banked(&self, key: &str, path: &mut HashSet<Key>) -> Line {
        let Some((module, name)) = key.rsplit_once(DOT) else {
            return Err(Missing::new(key, Unresolved::NoSuchName));
        };
        let Some(entry) = Bank::get().classes.get(key) else {
            return self.resolved(self.find(key, &mut HashSet::new())?, path);
        };
        let cached = (PathBuf::from(module), name.to_owned());
        if let Some(known) = self.lines.read().unwrap_or_else(PoisonError::into_inner).get(&cached) {
            return known.clone();
        }
        let outline = Arc::new(Outline {
            path: cached.0.clone(),
            module: Some(module.to_owned()),
            foreign: Some(module.to_owned()),
            ..Outline::default()
        });
        let class = Arc::new(Class {
            name: name.to_owned(),
            span: Sink::origin(),
            bases: Vec::new(),
            bindings: Vec::new(),
            methods: entry
                .methods
                .iter()
                .map(|method| Method {
                    name: method.clone(),
                    span: Sink::origin(),
                    decorators: entry
                        .pending
                        .contains(method)
                        .then(|| ABSTRACTMETHOD.to_owned())
                        .into_iter()
                        .collect(),
                })
                .collect(),
        });
        let bases = entry
            .bases
            .iter()
            .map(
                |base| match base.strip_prefix(BUILTINS).and_then(|rest| rest.strip_prefix(DOT)) {
                    Some(OBJECT) => Ok(Arc::new(Lineage::default())),
                    _ => self.banked(base, path),
                },
            )
            .collect::<Result<Vec<_>, Missing>>();
        let own = Ancestor { outline, class };
        let line = bases.map(|bases| Self::merged(true, std::iter::once(own).chain(Self::flat(&bases))));
        self.lines
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(cached, line.clone());
        line
    }

    fn resolved(&self, target: Target, path: &mut HashSet<Key>) -> Line {
        match target {
            Target::Class(outline, name) => self.line(&outline, &name, path),
            Target::Banked(key) => self.banked(&key, path),
            Target::Factory(bases) => {
                let bases = bases
                    .iter()
                    .map(|base| self.banked(base, path))
                    .collect::<Result<Vec<_>, Missing>>()?;
                Ok(Self::merged(true, Self::flat(&bases)))
            }
            Target::Root => Ok(Arc::new(Lineage::default())),
        }
    }

    fn flat(lines: &[Arc<Lineage>]) -> impl Iterator<Item = Ancestor> {
        lines.iter().flat_map(|line| line.ancestors.iter().cloned())
    }

    fn merged(builtin: bool, lines: impl IntoIterator<Item = Ancestor>) -> Arc<Lineage> {
        let mut seen = HashSet::new();
        Arc::new(Lineage {
            ancestors: lines
                .into_iter()
                .filter(|ancestor| seen.insert((ancestor.outline.path.clone(), ancestor.class.name.clone())))
                .collect(),
            builtin,
        })
    }

    fn bases(
        &self,
        outline: &Arc<Outline>,
        class: &Class,
        path: &mut HashSet<Key>,
    ) -> Result<Vec<Arc<Lineage>>, Missing> {
        let lines: Vec<Line> = class
            .bases
            .iter()
            .map(|base| {
                let target = self.target(outline, base).map_err(|missing| Missing {
                    name: Self::written(outline, base),
                    ..missing
                })?;
                self.resolved(target, path)
            })
            .collect();
        let Some(first) = lines.iter().find_map(|line| line.as_ref().err()).cloned() else {
            return Ok(lines.into_iter().flatten().collect());
        };
        let resolved = lines.iter().flat_map(|line| {
            line.as_ref()
                .map_or_else(|missing| &*missing.resolved, |line| &*line.ancestors)
                .iter()
                .cloned()
        });
        Err(Missing {
            resolved: Self::merged(false, resolved).ancestors.clone(),
            ..first
        })
    }

    fn joined(&self, outline: &Arc<Outline>, class: &Class, path: &mut HashSet<Key>, own: Ancestor) -> Line {
        let bases = self
            .bases(outline, class, path)
            .map_err(|missing| missing.after(own.clone()))?;
        let builtin = bases.iter().any(|line| line.builtin);
        Ok(Self::merged(builtin, std::iter::once(own).chain(Self::flat(&bases))))
    }

    fn line(&self, outline: &Arc<Outline>, name: &str, path: &mut HashSet<Key>) -> Line {
        let key = (outline.path.clone(), name.to_owned());
        if let Some(known) = self.lines.read().unwrap_or_else(PoisonError::into_inner).get(&key) {
            return known.clone();
        }
        let class = outline
            .classes
            .get(name)
            .ok_or_else(|| Missing::new(name, Unresolved::NoSuchName))?;
        if !path.insert(key.clone()) {
            return Err(Missing::new(name, Unresolved::Cycle));
        }
        let own = Ancestor {
            outline: Arc::clone(outline),
            class: Arc::clone(class),
        };
        let line = self.joined(outline, class, path, own);
        path.remove(&key);
        self.lines
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, line.clone());
        line
    }

    /// The resolver of a project's root, its source roots from that root, and the foreign roots of its environment.
    /// A project that holds a `src` directory no root names resolves under it too, ahead of the project root — ruff's
    /// default `src = [".", "src"]`: a `src` layout needs no configuration.
    pub(crate) fn new(root: &Path, roots: &[PathBuf], environment: &Environment) -> Self {
        let configured: Vec<PathBuf> = roots
            .iter()
            .map(|root| {
                root.components()
                    .filter(|part| !matches!(part, Component::CurDir))
                    .collect()
            })
            .collect();
        let src = PathBuf::from(SRC);
        let implicit = !root.as_os_str().is_empty() && !configured.contains(&src) && root.join(&src).is_dir();
        let top = configured.iter().position(|root| root.as_os_str().is_empty());
        let mut roots = configured;
        if implicit {
            roots.insert(top.unwrap_or(roots.len()), src);
        }
        Self {
            root: root.to_owned(),
            roots,
            foreign: environment.roots.clone(),
            ..Self::default()
        }
    }

    /// Every module, line and stub forgotten: the next class resolved in a world of its own.
    pub(crate) fn clear(&self) {
        self.modules.write().unwrap_or_else(PoisonError::into_inner).clear();
        self.stubs.write().unwrap_or_else(PoisonError::into_inner).clear();
        self.lines.write().unwrap_or_else(PoisonError::into_inner).clear();
    }

    /// The modules read so far: from the project's roots, and from foreign ones.
    pub(crate) fn read(&self) -> (usize, usize) {
        (self.project.load(Ordering::Relaxed), self.read.load(Ordering::Relaxed))
    }

    /// The line of a class of a module: every class it inherits from, nearest first, each once, and whether a class
    /// of the bank stands in it; otherwise the root of the break and why: the base that did not resolve, named as the
    /// module of the class whose base it is writes it — the class's own, or an ancestor's — and the part of the line
    /// that did resolve.
    pub(crate) fn of(&self, outline: &Arc<Outline>, class: &Class) -> Line {
        let bases = self.bases(outline, class, &mut HashSet::new())?;
        Ok(Self::merged(bases.iter().any(|line| line.builtin), Self::flat(&bases)))
    }
}
