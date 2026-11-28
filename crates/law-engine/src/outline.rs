use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use law_model::Span;

use crate::source::{Source, Syntax};

const CLASS: &str = "class_definition";
const FUNCTION: &str = "function_definition";
const DECORATED: &str = "decorated_definition";
const DECORATOR: &str = "decorator";
const IMPORT: &str = "import_statement";
const FROM: &str = "import_from_statement";
const ALIASED: &str = "aliased_import";
const DOTTED: &str = "dotted_name";
const RELATIVE: &str = "relative_import";
const WILDCARD: &str = "wildcard_import";
const ASSIGNMENT: &str = "assignment";
const STATEMENT: &str = "expression_statement";
const BLOCKS: [&str; 8] = [
    "block",
    "if_statement",
    "elif_clause",
    "else_clause",
    "try_statement",
    "except_clause",
    "except_group_clause",
    "finally_clause",
];
const TARGETS: [&str; 4] = ["pattern_list", "tuple_pattern", "list_pattern", "list_splat_pattern"];
const TYPE: &str = "type";
const LEFT: &str = "left";
const GETATTR: &str = "__getattr__";
const IDENTIFIER: &str = "identifier";
const ATTRIBUTE: &str = "attribute";
const SUBSCRIPT: &str = "subscript";
const CALL: &str = "call";
const KEYWORD: &str = "keyword_argument";
const COMMENT: &str = "comment";
const NAME: &str = "name";
const ALIAS: &str = "alias";
const MODULE: &str = "module_name";
const BODY: &str = "body";
const BASES: &str = "superclasses";
const DEFINITION: &str = "definition";
const OBJECT: &str = "object";
const VALUE: &str = "value";
const CALLED: &str = "function";
const RIGHT: &str = "right";
const ARGUMENTS: &str = "arguments";
const DOT: &str = ".";
const BLOCK: &str = "block";
const IF: &str = "if_statement";
const CONDITION: &str = "condition";
const CONSEQUENCE: &str = "consequence";
const TYPE_CHECKING: &str = "TYPE_CHECKING";
pub(crate) const ABSTRACTMETHOD: &str = "abstractmethod";
const ABSTRACT: [&str; 4] = [
    ABSTRACTMETHOD,
    "abstractproperty",
    "abstractclassmethod",
    "abstractstaticmethod",
];

/// A method as its class body writes it: its name, where the name sits, the names of its decorators.
pub(crate) struct Method {
    pub(crate) name: String,
    pub(crate) span: Span,
    pub(crate) decorators: Vec<String>,
}

/// A name a class body binds: by an assignment — to a name, or to a name among a tuple's targets — or by an
/// annotation alone, which gives it no value; at the body's level or inside an `if` or a `try` block of it.
pub(crate) struct Binding {
    pub(crate) name: String,
    pub(crate) span: Span,
    pub(crate) valued: bool,
}

/// An import statement as its module writes it, wherever it sits: the module it names — a relative one with its
/// leading dots —, the names a `from` import takes, each perhaps a submodule, where it is written, and whether it sits
/// under `if TYPE_CHECKING:`. `import a, b` is two.
pub(crate) struct Import {
    pub(crate) module: String,
    pub(crate) names: Vec<String>,
    pub(crate) span: Span,
    pub(crate) typing: bool,
}

/// A base as a class's bases list writes it: a dotted name, or an expression no name stands for, by its first line.
pub(crate) enum Base {
    Name(String),
    Expression(String),
}

/// A class as its module writes it: its name and where it sits, its bases, its methods in order, the names its body
/// binds.
pub(crate) struct Class {
    pub(crate) name: String,
    pub(crate) span: Span,
    pub(crate) bases: Vec<Base>,
    pub(crate) methods: Vec<Method>,
    pub(crate) bindings: Vec<Binding>,
}

/// What a module declares, read off its tree: its module-level classes, imports (the first binding of a name wins,
/// a relative one kept with its leading dots), star imports, the names it binds otherwise, whether it exports lazily
/// through a module `__getattr__`, the functions among those names, the names bound to a call on a dotted first
/// argument (`Mapping = _alias(collections.abc.Mapping, 2)` → `Mapping`: `_alias`, `collections.abc.Mapping`); how
/// many module-level function definitions it writes, a name written twice (an overload) counted twice; every
/// class it holds, nested ones included, in order; every import statement, nested ones included, in order; and, read
/// from a foreign root, the package its relative imports resolve against; read through an import, the dotted name of
/// the module.
#[derive(Default)]
pub(crate) struct Outline {
    pub(crate) path: PathBuf,
    pub(crate) module: Option<String>,
    pub(crate) classes: BTreeMap<String, Arc<Class>>,
    pub(crate) imports: BTreeMap<String, String>,
    pub(crate) stars: Vec<String>,
    pub(crate) values: BTreeSet<String>,
    pub(crate) functions: BTreeSet<String>,
    pub(crate) defined: usize,
    pub(crate) calls: BTreeMap<String, (String, String)>,
    pub(crate) lazy: bool,
    pub(crate) every: Vec<Arc<Class>>,
    pub(crate) statements: Vec<Import>,
    pub(crate) foreign: Option<String>,
}

impl Import {
    fn guarded(node: &Syntax<'_>) -> bool {
        let Some(test) = node.parent().filter(|test| test.kind() == IF) else {
            return false;
        };
        let checking = test
            .field(CONDITION)
            .and_then(|condition| Outline::dotted(&condition))
            .is_some_and(|name| name.rsplit(DOT).next() == Some(TYPE_CHECKING));
        checking
            && test
                .field(CONSEQUENCE)
                .is_some_and(|consequence| consequence.range() == node.range())
    }

    fn typing(node: &Syntax<'_>) -> bool {
        node.ancestors()
            .any(|above| above.kind() == BLOCK && Self::guarded(&above))
    }

    fn name(node: &Syntax<'_>) -> Option<String> {
        match &*node.kind() {
            ALIASED => Outline::dotted(&node.field(NAME)?),
            _ => Outline::dotted(node),
        }
    }

    fn new(node: &Syntax<'_>) -> Vec<Self> {
        let span = Source::span(node);
        let typing = Self::typing(node);
        let names = node.field_children(NAME).filter_map(|name| Self::name(&name));
        match &*node.kind() {
            FROM => Outline::origin(node)
                .map(|module| Self {
                    module,
                    names: names.collect(),
                    span: span.clone(),
                    typing,
                })
                .into_iter()
                .collect(),
            _ => names
                .map(|module| Self {
                    module,
                    names: Vec::new(),
                    span: span.clone(),
                    typing,
                })
                .collect(),
        }
    }
}

impl Method {
    fn called(node: &Syntax<'_>) -> Option<String> {
        match &*node.kind() {
            IDENTIFIER => Some(node.text().into_owned()),
            ATTRIBUTE => node.field(ATTRIBUTE).map(|name| name.text().into_owned()),
            CALL => node.field(CALLED).and_then(|called| Self::called(&called)),
            _ => None,
        }
    }

    fn new(node: &Syntax<'_>) -> Option<Self> {
        let (definition, decorators) = match &*node.kind() {
            DECORATED => (
                node.field(DEFINITION)?,
                node.named_children()
                    .filter(|child| child.kind() == DECORATOR)
                    .filter_map(|decorator| decorator.named_children().next())
                    .filter_map(|called| Self::called(&called))
                    .collect(),
            ),
            _ => (node.clone(), Vec::new()),
        };
        let name = (definition.kind() == FUNCTION)
            .then(|| definition.field(NAME))
            .flatten()?;
        Some(Self {
            name: name.text().into_owned(),
            span: Source::span(&name),
            decorators,
        })
    }

    /// Whether the method is left abstract: a decorator of `abc`'s abstract family, by its last segment, marks it.
    pub(crate) fn abstracted(&self) -> bool {
        self.decorators
            .iter()
            .any(|decorator| ABSTRACT.contains(&decorator.as_str()))
    }
}

impl Binding {
    fn names(target: &Syntax<'_>) -> Vec<(String, Span)> {
        match &*target.kind() {
            IDENTIFIER => vec![(target.text().into_owned(), Source::span(target))],
            kind if TARGETS.contains(&kind) => target.named_children().flat_map(|part| Self::names(&part)).collect(),
            _ => Vec::new(),
        }
    }

    fn statements<'a>(node: &Syntax<'a>) -> Vec<Syntax<'a>> {
        node.named_children()
            .flat_map(|child| match &*child.kind() {
                STATEMENT => vec![child],
                kind if BLOCKS.contains(&kind) => Self::statements(&child),
                _ => Vec::new(),
            })
            .collect()
    }

    fn new(node: &Syntax<'_>) -> Vec<Self> {
        let Some(assignment) = node.named_children().find(|child| child.kind() == ASSIGNMENT) else {
            return Vec::new();
        };
        let valued = assignment.field(RIGHT).is_some() || assignment.field(TYPE).is_none();
        assignment
            .field(LEFT)
            .map(|left| Self::names(&left))
            .unwrap_or_default()
            .into_iter()
            .map(|(name, span)| Self { name, span, valued })
            .collect()
    }
}

impl Class {
    fn new(node: &Syntax<'_>) -> Option<Self> {
        let named = node.field(NAME)?;
        let name = named.text().into_owned();
        let bases = node
            .field(BASES)
            .map(|list| {
                list.named_children()
                    .filter(|base| base.kind() != KEYWORD && base.kind() != COMMENT)
                    .map(|base| {
                        Outline::dotted(&base).map_or_else(
                            || Base::Expression(base.text().lines().next().unwrap_or_default().to_owned()),
                            Base::Name,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let body = node.field(BODY);
        let methods = body
            .iter()
            .flat_map(Syntax::named_children)
            .filter_map(|child| Method::new(&child))
            .collect();
        let bindings = body
            .iter()
            .flat_map(Binding::statements)
            .flat_map(|statement| Binding::new(&statement))
            .collect();
        Some(Self {
            name,
            span: Source::span(&named),
            bases,
            methods,
            bindings,
        })
    }
}

impl Outline {
    fn dotted(node: &Syntax<'_>) -> Option<String> {
        match &*node.kind() {
            IDENTIFIER => Some(node.text().into_owned()),
            ATTRIBUTE => {
                let object = Self::dotted(&node.field(OBJECT)?)?;
                Some(format!("{object}{DOT}{}", node.field(ATTRIBUTE)?.text()))
            }
            SUBSCRIPT => Self::dotted(&node.field(VALUE)?),
            DOTTED => {
                let parts: Vec<String> = node.named_children().map(|part| part.text().into_owned()).collect();
                Some(parts.join(DOT))
            }
            _ => None,
        }
    }

    fn top(node: &Syntax<'_>) -> bool {
        node.ancestors()
            .all(|above| !matches!(&*above.kind(), CLASS | FUNCTION))
    }

    fn joined(module: &str, name: &str) -> String {
        if module.ends_with(DOT) {
            format!("{module}{name}")
        } else {
            format!("{module}{DOT}{name}")
        }
    }

    fn bound(node: &Syntax<'_>, module: Option<&str>) -> Option<(String, String)> {
        match &*node.kind() {
            DOTTED => Self::dotted(node).and_then(|name| match module {
                Some(module) => Some((name.clone(), Self::joined(module, &name))),
                None => name.split(DOT).next().map(|first| (first.to_owned(), first.to_owned())),
            }),
            ALIASED => {
                let name = Self::dotted(&node.field(NAME)?)?;
                let full = module.map_or_else(|| name.clone(), |module| Self::joined(module, &name));
                Some((node.field(ALIAS)?.text().into_owned(), full))
            }
            _ => None,
        }
    }

    fn origin(node: &Syntax<'_>) -> Option<String> {
        let module = node.field(MODULE)?;
        match &*module.kind() {
            DOTTED => Self::dotted(&module),
            RELATIVE => Some(module.text().split_whitespace().collect()),
            _ => None,
        }
    }

    fn star(node: &Syntax<'_>) -> Option<String> {
        (node.kind() == FROM && node.named_children().any(|child| child.kind() == WILDCARD))
            .then(|| Self::origin(node))
            .flatten()
    }

    fn imports(node: &Syntax<'_>) -> Vec<(String, String)> {
        let module = match &*node.kind() {
            FROM => Self::origin(node).map(Some),
            _ => Some(None),
        };
        module
            .map(|module| {
                node.field_children(NAME)
                    .filter_map(|name| Self::bound(&name, module.as_deref()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn value(node: &Syntax<'_>) -> Option<String> {
        let name = match &*node.kind() {
            ASSIGNMENT => node.field(LEFT).filter(|left| left.kind() == IDENTIFIER)?,
            FUNCTION => node.field(NAME)?,
            _ => return None,
        };
        Some(name.text().into_owned())
    }

    fn call(node: &Syntax<'_>) -> Option<(String, (String, String))> {
        let name = node.field(LEFT).filter(|left| left.kind() == IDENTIFIER)?;
        let call = node.field(RIGHT).filter(|right| right.kind() == CALL)?;
        let callee = Self::dotted(&call.field(CALLED)?)?;
        let first = call.field(ARGUMENTS)?.named_children().next()?;
        let first = (first.kind() != KEYWORD).then(|| Self::dotted(&first)).flatten()?;
        Some((name.text().into_owned(), (callee, first)))
    }

    /// The outline of a module at a path, from the project root, off its parse.
    pub(crate) fn new(path: &Path, root: &Syntax<'_>) -> Self {
        let read: Vec<(bool, Arc<Class>)> = root
            .dfs()
            .filter(|node| node.kind() == CLASS)
            .filter_map(|node| Class::new(&node).map(|class| (Self::top(&node), Arc::new(class))))
            .collect();
        let every: Vec<Syntax<'_>> = root
            .dfs()
            .filter(|node| matches!(&*node.kind(), IMPORT | FROM))
            .collect();
        let statements: Vec<&Syntax<'_>> = every.iter().filter(|node| Self::top(node)).collect();
        let mut imports = BTreeMap::new();
        statements
            .iter()
            .flat_map(|node| Self::imports(node))
            .for_each(|(name, full)| {
                imports.entry(name).or_insert(full);
            });
        let bindings: Vec<Syntax<'_>> = root
            .dfs()
            .filter(|node| matches!(&*node.kind(), ASSIGNMENT | FUNCTION) && Self::top(node))
            .collect();
        let values: BTreeSet<String> = bindings.iter().filter_map(Self::value).collect();
        let mut calls = BTreeMap::new();
        bindings
            .iter()
            .filter(|node| node.kind() == ASSIGNMENT)
            .filter_map(Self::call)
            .for_each(|(name, call)| {
                calls.entry(name).or_insert(call);
            });
        Self {
            path: path.to_owned(),
            module: None,
            classes: read
                .iter()
                .filter(|(top, _)| *top)
                .map(|(_, class)| (class.name.clone(), Arc::clone(class)))
                .collect(),
            imports,
            stars: statements.iter().filter_map(|node| Self::star(node)).collect(),
            lazy: values.contains(GETATTR),
            functions: bindings
                .iter()
                .filter(|node| node.kind() == FUNCTION)
                .filter_map(Self::value)
                .collect(),
            defined: bindings.iter().filter(|node| node.kind() == FUNCTION).count(),
            calls,
            values,
            every: read.into_iter().map(|(_, class)| class).collect(),
            statements: every.iter().flat_map(Import::new).collect(),
            foreign: None,
        }
    }
}
