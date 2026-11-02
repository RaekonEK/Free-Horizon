//! Writes Rust from the Java that `java.rs` read: classes become structs and
//! `impl` blocks, methods become functions, statements and expressions are
//! carried over, and every construct the writer cannot carry is listed in the
//! report instead of being dropped silently.
//!
//! The output is a starting point for a native Horizon mod, not a finished
//! one: Java's Minecraft API (`Level`, `ItemStack`, `@Inject`...) has no
//! one-to-one Rust name, so calls into it are marked and listed.
use crate::java::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct Stats {
    pub classes: usize,
    pub methods: usize,
    pub fields: usize,
    pub statements: usize,
    pub annotations: usize,
}

#[derive(Debug, Default, Clone)]
pub struct Translated {
    pub text: String,
    pub issues: Vec<String>,
    pub stats: Stats,
}

/// Java's `camelCase` as Rust's `snake_case`.
pub fn snake(name: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() {
            let prev_lower =
                i > 0 && (chars[i - 1].is_ascii_lowercase() || chars[i - 1].is_ascii_digit());
            let next_lower = i + 1 < chars.len() && chars[i + 1].is_ascii_lowercase();
            if i > 0 && (prev_lower || next_lower) {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else if c.is_ascii_alphanumeric() || *c == '_' {
            out.push(*c);
        } else {
            out.push('_');
        }
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    let out = out.trim_matches('_').to_owned();
    if is_rust_keyword(&out) {
        format!("{out}_")
    } else {
        out
    }
}

/// A Java primitive or standard type as the Rust type that holds it.
fn rust_type(java: &str, known: &BTreeSet<String>, issues: &mut Vec<String>) -> String {
    let ty = java.trim();
    let (base, args) = match ty.split_once('<') {
        Some((b, rest)) => {
            let inner = rest.trim_end_matches('>');
            (b.trim(), Some(inner))
        }
        None => (ty, None),
    };
    let array = base.ends_with("[]");
    let base = base.trim_end_matches("[]").trim();
    let simple = base.rsplit('.').next().unwrap_or(base);
    let primitives = [
        ("byte", "i8"),
        ("short", "i16"),
        ("int", "i32"),
        ("long", "i64"),
        ("float", "f32"),
        ("double", "f64"),
        ("boolean", "bool"),
        ("char", "char"),
        ("void", "()"),
        ("String", "String"),
        ("Integer", "i32"),
        ("Long", "i64"),
        ("Float", "f32"),
        ("Double", "f64"),
        ("Boolean", "bool"),
        ("Character", "char"),
        ("Byte", "i8"),
        ("Short", "i16"),
        ("Object", "serde_json::Value"),
        ("var", "_"),
    ];
    if let Some((_, rust)) = primitives.iter().find(|(j, _)| *j == simple) {
        if simple == "Object" {
            issues.push("`Object` has no Rust type: written as `serde_json::Value`".into());
        }
        let mut out = (*rust).to_owned();
        if array {
            out = format!("Vec<{out}>");
        }
        return out;
    }
    let generic = |name: &str| match name {
        "List" | "ArrayList" | "LinkedList" | "Collection" | "Iterable" | "Queue" | "Deque" => {
            Some(("Vec", true))
        }
        "Set" | "HashSet" | "LinkedHashSet" | "TreeSet" => Some(("BTreeSet", false)),
        "Map" | "HashMap" | "LinkedHashMap" | "TreeMap" => Some(("BTreeMap", false)),
        "Optional" => Some(("Option", false)),
        "Supplier" | "Function" | "Consumer" | "Predicate" | "Runnable" => Some(("fn", false)),
        _ => None,
    };
    if let Some((rust, _mutable)) = generic(simple) {
        if rust == "fn" {
            issues.push(format!(
                "`{simple}` as a Rust function type: check its signature"
            ));
            return "fn()".to_owned();
        }
        let inner = args
            .map(|a| {
                a.split(',')
                    .map(|t| rust_type(t, known, issues))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_else(|| "_".to_owned());
        let mut out = format!("{rust}<{inner}>");
        if array {
            out = format!("Vec<{out}>");
        }
        return out;
    }
    if simple == "?" || simple.starts_with('?') {
        issues.push(format!("wildcard type `{ty}`: written as `_`"));
        return "_".to_owned();
    }
    let name = sanitize(simple);
    let mut out = if known.contains(simple) {
        name
    } else {
        issues.push(format!(
            "type `{simple}` is Java's or another mod's: written as `serde_json::Value`"
        ));
        "serde_json::Value".to_owned()
    };
    if array {
        out = format!("Vec<{out}>");
    }
    out
}

/// A Java identifier as a Rust one (keywords and `$` are made safe).
fn sanitize(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        return "unnamed".into();
    }
    if is_rust_keyword(&out) {
        out.push('_');
    }
    if out
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        out.insert(0, '_');
    }
    out
}

fn is_rust_keyword(name: &str) -> bool {
    const KEYWORDS: [&str; 51] = [
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
        "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
        "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do",
        "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
    ];
    KEYWORDS.contains(&name)
}

/// Each method name's return type, when every class agrees on it.
pub type Returns = BTreeMap<String, String>;

/// Collects method-name to return-type over every class read, skipping names
/// that differ between classes.
pub fn collect_returns<'a>(units: impl Iterator<Item = &'a CompilationUnit>) -> Returns {
    fn walk(ty: &TypeDecl, seen: &mut BTreeMap<String, Option<String>>) {
        for m in &ty.methods {
            let ret = m.ret.trim();
            if m.name == "new" || ret.is_empty() || ret == "void" {
                continue;
            }
            match seen.get(&m.name) {
                None => {
                    seen.insert(m.name.clone(), Some(ret.to_owned()));
                }
                Some(Some(prev)) if prev != ret => {
                    seen.insert(m.name.clone(), None);
                }
                _ => {}
            }
        }
        for n in &ty.nested {
            walk(n, seen);
        }
    }
    let mut seen: BTreeMap<String, Option<String>> = BTreeMap::new();
    for unit in units {
        for ty in &unit.types {
            walk(ty, &mut seen);
        }
    }
    seen.into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect()
}

/// Which argument counts each Java method name is declared with, over every
/// class read. Java overloads by arity; Rust cannot, so a name with more than
/// one arity becomes `name_N`.
pub type Arities = BTreeMap<String, BTreeSet<usize>>;

pub fn collect_arities<'a>(units: impl Iterator<Item = &'a CompilationUnit>) -> Arities {
    fn walk(ty: &TypeDecl, out: &mut Arities) {
        for m in &ty.methods {
            if m.name != "new" {
                out.entry(m.name.clone())
                    .or_default()
                    .insert(m.params.len());
            }
        }
        for n in &ty.nested {
            walk(n, out);
        }
    }
    let mut out = Arities::new();
    for unit in units {
        for ty in &unit.types {
            walk(ty, &mut out);
        }
    }
    out
}

/// The whole compilation unit as Rust source, plus what it could not carry.
pub fn translate(
    unit: &CompilationUnit,
    file: &str,
    known: &BTreeSet<String>,
    returns: &Returns,
    arities: &Arities,
) -> Translated {
    let mut out = Translated::default();
    out.text.push_str(&format!(
        "// Translated from {file} by HexaGon.\n\
         // Java is never run here: this is a starting point for a native Horizon mod,\n\
         // and every `todo!(\"hexagon: ...\")` marks a construct that needs a human decision.\n\
         #![allow(dead_code, unused_variables, unused_mut, unused_parens, unreachable_code, non_snake_case)]\n\
         \n\
         // The mod's other classes live in the crate's other modules.\n\
         use crate::*;\n\n"
    ));
    if !unit.package.is_empty() {
        out.text.push_str(&format!("// package {}\n", unit.package));
    }
    let mut seen = BTreeSet::new();
    for ty in &unit.types {
        emit_type(ty, &mut out, known, returns, arities, 0, &mut seen);
    }
    out
}

fn emit_type(
    ty: &TypeDecl,
    out: &mut Translated,
    known: &BTreeSet<String>,
    returns: &Returns,
    arities: &Arities,
    depth: usize,
    seen: &mut BTreeSet<String>,
) {
    let pad = "    ".repeat(depth);
    out.stats.classes += 1;
    for a in &ty.annotations {
        out.stats.annotations += 1;
        out.issues.push(format!(
            "class {}: annotation @{} becomes a TODO",
            ty.name, a.name
        ));
    }
    match ty.kind.as_str() {
        "interface" | "@interface" => {
            out.text.push_str(&format!(
                "{pad}// interface {} (Java contracts have no Rust body here)\n",
                ty.name
            ));
            for m in &ty.methods {
                out.issues.push(format!(
                    "interface {}: method `{}` needs a Rust trait",
                    ty.name, m.name
                ));
            }
            return;
        }
        "enum" => {
            out.text.push_str(&format!("{pad}pub enum {} {{\n{pad}    // TODO(hexagon): the constants were not read\n{pad}}}\n\n", sanitize(&ty.name)));
            out.issues.push(format!(
                "enum {}: its constants are not translated; fill the variants",
                ty.name
            ));
            return;
        }
        _ => {}
    }
    let name = sanitize(&ty.name);
    if !seen.insert(name.clone()) {
        out.issues.push(format!(
            "two classes named {}: the second is not written",
            ty.name
        ));
        return;
    }
    let instance: Vec<&Field> = ty
        .fields
        .iter()
        .filter(|f| !f.modifiers.iter().any(|m| m == "static"))
        .collect();
    out.stats.fields += ty.fields.len();
    if depth == 0 {
        out.text.push_str(&format!("{pad}// {}\n", ty.name));
    }
    out.text.push_str(&format!(
        "{pad}#[derive(Debug, Clone, Default)]\n{pad}pub struct {name} {{\n"
    ));
    for f in &instance {
        let rt = rust_type(&f.ty, known, &mut out.issues);
        out.text
            .push_str(&format!("{pad}    pub {}: {rt},\n", snake(&f.name)));
    }
    out.text
        .push_str(&format!("{pad}}}\n\n{pad}impl {name} {{\n"));
    for f in ty
        .fields
        .iter()
        .filter(|f| f.modifiers.iter().any(|m| m == "static"))
    {
        out.issues.push(format!(
            "{}: static field `{}` needs a Rust `static`/`OnceLock` decision",
            ty.name, f.name
        ));
    }
    // Java overloading: the same name with different argument counts. Rust has
    // no overloading, so a name shared by several arities is written `name_N`.
    let mut constructor: Option<&Method> = None;
    for m in &ty.methods {
        if m.name == "new" {
            constructor = Some(m);
            continue;
        }
        emit_method(ty, m, out, known, returns, arities, depth + 1);
    }
    emit_constructor(
        ty,
        constructor,
        &instance,
        out,
        known,
        returns,
        arities,
        depth + 1,
    );
    out.text.push_str(&format!("{pad}}}\n\n"));
    for nested in &ty.nested {
        emit_type(nested, out, known, returns, arities, depth + 1, seen);
    }
}

fn emit_constructor(
    ty: &TypeDecl,
    ctor: Option<&Method>,
    instance: &[&Field],
    out: &mut Translated,
    known: &BTreeSet<String>,
    returns: &Returns,
    arities: &BTreeMap<String, BTreeSet<usize>>,
    depth: usize,
) {
    let pad = "    ".repeat(depth);
    let mut tr = Translator {
        issues: std::mem::take(&mut out.issues),
        known,
        returns,
        arities,
        depth: depth + 1,
        statements: 0,
        vars: std::collections::BTreeMap::new(),
        fields: ty
            .fields
            .iter()
            .filter(|f| !f.modifiers.iter().any(|m| m == "static"))
            .map(|f| (f.name.clone(), f.ty.clone()))
            .collect(),
        static_fields: ty
            .fields
            .iter()
            .filter(|f| f.modifiers.iter().any(|m| m == "static"))
            .map(|f| f.name.clone())
            .collect(),
        class: ty.name.clone(),
        methods: ty.methods.iter().map(|m| m.name.clone()).collect(),
        ret_type: "Self".to_owned(),
        static_method: false,
        this_name: "this".to_owned(),
    };
    let (params, body) = match ctor {
        Some(c) => (
            c.params
                .iter()
                .map(|(t, n)| {
                    tr.vars.insert(n.clone(), t.clone());
                    format!("{}: {}", snake(n), rust_type(t, known, &mut tr.issues))
                })
                .collect::<Vec<_>>(),
            c.body.as_deref(),
        ),
        None => (Vec::new(), None),
    };
    out.text.push_str(&format!(
        "{pad}pub fn new({}) -> Self {{\n",
        params.join(", ")
    ));
    // Field initializers, then the constructor's own body.
    let mut assigned: BTreeSet<String> = BTreeSet::new();
    if let Some(body) = body {
        for s in body {
            if let Stmt::Expr(Expr::Assign(_, lhs, _)) = s
                && let Expr::Field(base, field) = lhs.as_ref()
                && matches!(base.as_ref(), Expr::This)
            {
                assigned.insert(field.clone());
            }
        }
    }
    out.text
        .push_str(&format!("{pad}    let mut this = Self {{\n"));
    for f in instance {
        let rt = rust_type(&f.ty, known, &mut tr.issues);
        let init = match (&f.init, assigned.contains(&f.name)) {
            (_, true) => "Default::default()".to_owned(),
            (Some(e), _) => tr.expr(e),
            (None, _) => {
                let plain = rt.starts_with("Vec")
                    || rt.starts_with("BTree")
                    || rt.starts_with("Option")
                    || rt == "String"
                    || rt == "bool"
                    || rt.starts_with('i')
                    || rt.starts_with('u')
                    || rt.starts_with('f');
                if !plain {
                    tr.issues.push(format!(
                        "field {} has no initializer: `Default` is assumed",
                        f.name
                    ));
                }
                "Default::default()".to_owned()
            }
        };
        out.text
            .push_str(&format!("{pad}        {}: {init},\n", snake(&f.name)));
    }
    out.text.push_str(&format!("{pad}    }};\n"));
    if let Some(body) = body {
        for s in body {
            if let Stmt::Expr(Expr::Call(None, name, _)) = s
                && (name == "super" || name == "this")
            {
                tr.issues.push(format!(
                    "constructor of {} calls `{name}(...)`: a Rust `new` has no parent call",
                    ty.name
                ));
                continue;
            }
            tr.stmt(s, &mut out.text, pad.as_str(), "    ");
        }
    }
    out.text.push_str(&format!("{pad}    this\n{pad}}}\n"));
    out.stats.statements += tr.statements;
    out.issues = tr.issues;
}

fn emit_method(
    ty: &TypeDecl,
    m: &Method,
    out: &mut Translated,
    known: &BTreeSet<String>,
    returns: &Returns,
    arities: &BTreeMap<String, BTreeSet<usize>>,
    depth: usize,
) {
    let pad = "    ".repeat(depth);
    out.stats.methods += 1;
    let is_static = m.modifiers.iter().any(|x| x == "static");
    let abstract_only = m.body.is_none();
    for a in &m.annotations {
        out.stats.annotations += 1;
        let name = a.name.rsplit('.').next().unwrap_or(&a.name);
        if matches!(
            name,
            "Inject"
                | "Redirect"
                | "ModifyVariable"
                | "ModifyArg"
                | "Overwrite"
                | "Accessor"
                | "Invoker"
        ) {
            out.issues.push(format!(
                "{}: Java mixin @{}({}) is a TODO: it needs a [[injection]] in horizon-mixin.toml",
                ty.name, name, a.args
            ));
        } else {
            out.issues.push(format!(
                "{}: annotation @{} on `{}` is a TODO",
                ty.name, a.name, m.name
            ));
        }
    }
    let mut vars = std::collections::BTreeMap::new();
    for (t, n) in &m.params {
        vars.insert(n.clone(), t.clone());
    }
    let fields: std::collections::BTreeMap<String, String> = ty
        .fields
        .iter()
        .filter(|f| !f.modifiers.iter().any(|m| m == "static"))
        .map(|f| (f.name.clone(), f.ty.clone()))
        .collect();
    let static_fields: BTreeSet<String> = ty
        .fields
        .iter()
        .filter(|f| f.modifiers.iter().any(|m| m == "static"))
        .map(|f| f.name.clone())
        .collect();
    let ret_type = rust_type(&m.ret, known, &mut out.issues);
    let mut tr = Translator {
        issues: std::mem::take(&mut out.issues),
        known,
        returns,
        depth: depth + 1,
        statements: 0,
        vars,
        fields,
        static_fields,
        class: ty.name.clone(),
        methods: ty.methods.iter().map(|m| m.name.clone()).collect(),
        ret_type: ret_type.clone(),
        arities,
        static_method: is_static,
        this_name: "self".to_owned(),
    };
    let params = m
        .params
        .iter()
        .map(|(t, n)| format!("{}: {}", snake(n), rust_type(t, known, &mut tr.issues)))
        .collect::<Vec<_>>();
    let ret = if ret_type == "()" {
        String::new()
    } else {
        format!(" -> {ret_type}")
    };
    let takes_self = if is_static {
        String::new()
    } else if body_mutates_self(m) {
        "&mut self".to_owned()
    } else {
        "&self".to_owned()
    };
    let mut all_params = Vec::new();
    if !takes_self.is_empty() {
        all_params.push(takes_self);
    }
    all_params.extend(params);
    let fn_name = tr.method_name(&m.name, m.params.len());
    out.text.push_str(&format!(
        "{pad}pub fn {fn_name}({}){ret} {{\n",
        all_params.join(", ")
    ));
    // Java's pattern variables are bound by the type test; here they start as
    // an explicit `todo!()` so the body still compiles and the site is listed.
    if let Some(body) = &m.body {
        let mut patterns = BTreeSet::new();
        collect_patterns(body, &mut patterns);
        for name in patterns {
            if m.params.iter().any(|(_, p)| p == &name) {
                continue;
            }
            out.text.push_str(&format!(
                "{pad}    let mut {} = todo!(\"hexagon: pattern variable {name}\");\n",
                snake(&name)
            ));
        }
    }
    match &m.body {
        Some(body) => {
            for s in body {
                tr.stmt(s, &mut out.text, pad.as_str(), "    ");
            }
            if !ret.is_empty() {
                // Java may fall off the end only to throw; make that explicit.
                out.text.push_str(&format!(
                    "{pad}    todo!(\"TODO(hexagon): Java returns nothing on some paths\")\n"
                ));
            }
        }
        None => {
            if abstract_only {
                out.text.push_str(&format!(
                    "{pad}    todo!(\"TODO(hexagon): abstract `{}` has no body\")\n",
                    m.name
                ));
                tr.issues.push(format!(
                    "{}::{} is abstract: its body must be written",
                    ty.name, m.name
                ));
            }
        }
    }
    out.text.push_str(&format!("{pad}}}\n"));
    out.stats.statements += tr.statements;
    out.issues = tr.issues;
}

/// Whether a method writes one of its own fields (or a field of `this`).
fn body_mutates_self(m: &Method) -> bool {
    fn scan(stmts: &[Stmt]) -> bool {
        stmts.iter().any(|s| match s {
            Stmt::Expr(e) | Stmt::Local(_, _, Some(e)) | Stmt::Return(Some(e)) | Stmt::Throw(e) => {
                expr_writes(e)
            }
            Stmt::Block(b) => scan(b),
            Stmt::If(_, a, b) => {
                scan(std::slice::from_ref(a))
                    || b.as_deref()
                        .map(|b| scan(std::slice::from_ref(b)))
                        .unwrap_or(false)
            }
            Stmt::While(_, b)
            | Stmt::DoWhile(b, _)
            | Stmt::ForEach(_, _, _, b)
            | Stmt::Synchronized(_, b) => scan(std::slice::from_ref(b)),
            Stmt::For(i, _, _, b) => {
                i.as_deref()
                    .map(|i| scan(std::slice::from_ref(i)))
                    .unwrap_or(false)
                    || scan(std::slice::from_ref(b))
            }
            Stmt::Switch(_, arms) => arms.iter().any(|(_, s)| scan(s)),
            Stmt::Try(b, c, f) => {
                scan(b)
                    || c.iter().any(|(_, _, b)| scan(b))
                    || f.as_deref().map(scan).unwrap_or(false)
            }
            _ => false,
        })
    }
    fn expr_writes(e: &Expr) -> bool {
        match e {
            Expr::Assign(_, lhs, rhs) => {
                matches!(lhs.as_ref(), Expr::Field(b, _) if matches!(b.as_ref(), Expr::This))
                    || expr_writes(rhs)
            }
            Expr::Postfix(_, e) | Expr::Unary(_, e) => expr_writes(e),
            Expr::Call(recv, _, args) => {
                recv.as_deref().map(expr_writes).unwrap_or(false) || args.iter().any(expr_writes)
            }
            Expr::Binary(_, a, b) => expr_writes(a) || expr_writes(b),
            Expr::Ternary(a, b, c) => expr_writes(a) || expr_writes(b) || expr_writes(c),
            Expr::Field(b, _)
            | Expr::Cast(_, b)
            | Expr::Index(b, _)
            | Expr::InstanceOf(b, _, _) => expr_writes(b),
            Expr::Lambda(_, b) => expr_writes(b),
            Expr::Block(b) => scan(b),
            _ => false,
        }
    }
    m.body.as_deref().map(scan).unwrap_or(false)
}

struct Translator<'a> {
    issues: Vec<String>,
    known: &'a BTreeSet<String>,
    depth: usize,
    statements: usize,
    /// Each method name's Java return type, when the classes agree.
    returns: &'a Returns,
    /// Each local's and parameter's Java type, by name.
    vars: std::collections::BTreeMap<String, String>,
    /// The class's fields' Java types (instance fields only).
    fields: std::collections::BTreeMap<String, String>,
    /// The class's static fields, which have no Rust member here.
    static_fields: BTreeSet<String>,
    /// The class being written.
    class: String,
    /// The class's method names, for Java's unqualified calls to its own.
    methods: std::collections::BTreeSet<String>,
    /// The Rust type the method returns (empty for `()`).
    ret_type: String,
    /// Which argument counts each Java method name is declared with.
    arities: &'a BTreeMap<String, BTreeSet<usize>>,
    /// Whether the method being written is static (no `self`).
    static_method: bool,
    /// `self` in a method, `this` in a constructor's `new`.
    this_name: String,
}

impl Translator<'_> {
    fn pad(&self, extra: usize) -> String {
        "    ".repeat(self.depth + extra)
    }

    /// A method's Rust name: `name` or, when Java overloads it, `name_N`.
    fn method_name(&self, name: &str, argc: usize) -> String {
        if let Some(set) = self.arities.get(name)
            && set.len() > 1
            && set.contains(&argc)
        {
            return format!("{}_{}", snake(name), argc);
        }
        snake(name)
    }

    /// Whether a Java type is one the game provides (no Rust name here).
    fn api_type(&self, ty: &str) -> bool {
        let simple = ty
            .trim()
            .trim_end_matches("[]")
            .split('<')
            .next()
            .unwrap_or(ty)
            .rsplit('.')
            .next()
            .unwrap_or(ty)
            .trim();
        const OWN: [&str; 28] = [
            "byte",
            "short",
            "int",
            "long",
            "float",
            "double",
            "boolean",
            "char",
            "void",
            "String",
            "Integer",
            "Long",
            "Float",
            "Double",
            "Boolean",
            "Character",
            "Byte",
            "Short",
            "Object",
            "var",
            "List",
            "ArrayList",
            "LinkedList",
            "Collection",
            "Iterable",
            "Set",
            "HashSet",
            "Link",
        ];
        if OWN.contains(&simple) {
            return false;
        }
        !self.known.contains(simple)
    }

    /// The Java type of an expression, when it is a variable, a field or a
    /// class name.
    fn expr_type(&self, e: &Expr) -> Option<String> {
        match e {
            Expr::Ident(n) => self
                .vars
                .get(n)
                .cloned()
                .or_else(|| self.fields.get(n).cloned())
                .or_else(|| self.known.contains(n).then(|| n.clone())),
            Expr::This => Some(self.class.clone()),
            Expr::Field(base, name) => {
                if matches!(base.as_ref(), Expr::This) {
                    self.fields.get(name).cloned()
                } else {
                    self.expr_type(base).and(None)
                }
            }
            Expr::Cast(ty, _) => Some(ty.clone()),
            Expr::Call(_, name, _) => self.returns.get(name).cloned(),
            _ => None,
        }
    }

    /// A Java API call with no Rust name yet, as a `todo!()` that keeps the
    /// generated crate compiling and the site visible.
    fn api_todo(&mut self, what: String) -> String {
        self.issues
            .push(format!("Java API left for a human: {what}"));
        // `todo!` formats its message, so braces from nested `format!` texts
        // must be escaped.
        let safe = what
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('{', "{{")
            .replace('}', "}}");
        format!("todo!(\"hexagon: {safe}\")")
    }

    fn stmt(&mut self, s: &Stmt, out: &mut String, _pad: &str, unit: &str) {
        self.statements += 1;
        let _ = unit;
        let p = self.pad(0);
        match s {
            Stmt::Empty => out.push_str(&format!("{p};\n")),
            Stmt::Expr(e) => {
                let text = self.expr(e);
                match e {
                    Expr::Postfix(op, target) if op == "++" || op == "--" => {
                        let op = if op == "++" { "+=" } else { "-=" };
                        out.push_str(&format!("{p}{} {op} 1;\n", self.expr(target)));
                    }
                    Expr::Unary(op, target) if op == "++" || op == "--" => {
                        let op = if op == "++" { "+=" } else { "-=" };
                        out.push_str(&format!("{p}{} {op} 1;\n", self.expr(target)));
                    }
                    _ => out.push_str(&format!("{p}{text};\n")),
                }
            }
            Stmt::Local(ty, name, init) => {
                self.vars.insert(name.clone(), ty.clone());
                let api = self.api_type(ty);
                let rt = rust_type(ty, self.known, &mut self.issues);
                let init = match init {
                    Some(e) => format!(" = {}", self.expr(e)),
                    None => String::new(),
                };
                // Java locals are written again later more often than not; the
                // generated file allows the unused `mut`. An API type has no
                // Rust name, so the binding's type is inferred (from a
                // `todo!()`), keeping the crate compiling.
                if api && init.is_empty() {
                    out.push_str(&format!(
                        "{p}let mut {} = todo!(\"hexagon: {ty}\");\n",
                        snake(name)
                    ));
                } else if api {
                    out.push_str(&format!("{p}let mut {}{init};\n", snake(name)));
                } else {
                    out.push_str(&format!("{p}let mut {}: {rt}{init};\n", snake(name)));
                }
            }
            Stmt::Return(e) => match e {
                Some(e) => {
                    let mut text = self.expr(e);
                    // Java widens an int literal to a float return; Rust needs the cast.
                    if matches!(e, Expr::Lit(Lit::Int(_)))
                        && (self.ret_type == "f32" || self.ret_type == "f64")
                    {
                        text = format!("{text} as {}", self.ret_type);
                    }
                    // Java's `null` against a Rust-carried JSON value.
                    if text == "None" && self.ret_type.starts_with("serde_json") {
                        text = "serde_json::Value::Null".to_owned();
                    }
                    out.push_str(&format!("{p}return {text};\n"));
                }
                None => out.push_str(&format!("{p}return;\n")),
            },
            Stmt::Break => out.push_str(&format!("{p}break;\n")),
            Stmt::Continue => out.push_str(&format!("{p}continue;\n")),
            Stmt::Throw(e) => {
                self.issues
                    .push("a Java `throw` became a Rust `panic!`".into());
                let text = self.expr(e);
                out.push_str(&format!("{p}panic!(\"{{:?}}\", {text});\n"));
            }
            Stmt::Block(b) => {
                out.push_str(&format!("{p}{{\n"));
                self.depth += 1;
                for s in b {
                    self.stmt(s, out, "", "    ");
                }
                self.depth -= 1;
                out.push_str(&format!("{p}}}\n"));
            }
            Stmt::If(cond, then, else_) => {
                let cond = self.expr(cond);
                out.push_str(&format!("{p}if {cond} {{\n"));
                self.depth += 1;
                self.stmt(then, out, "", "    ");
                self.depth -= 1;
                if let Some(e) = else_ {
                    out.push_str(&format!("{p}}} else {{\n"));
                    self.depth += 1;
                    self.stmt(e, out, "", "    ");
                    self.depth -= 1;
                }
                out.push_str(&format!("{p}}}\n"));
            }
            Stmt::While(cond, body) => {
                let cond = self.expr(cond);
                out.push_str(&format!("{p}while {cond} {{\n"));
                self.depth += 1;
                self.stmt(body, out, "", "    ");
                self.depth -= 1;
                out.push_str(&format!("{p}}}\n"));
            }
            Stmt::DoWhile(body, cond) => {
                out.push_str(&format!("{p}loop {{\n"));
                self.depth += 1;
                self.stmt(body, out, "", "    ");
                let cond = self.expr(cond);
                out.push_str(&format!("{p}    if !{cond} {{ break; }}\n"));
                self.depth -= 1;
                out.push_str(&format!("{p}}}\n"));
            }
            Stmt::For(init, cond, update, body) => {
                out.push_str(&format!("{p}{{\n"));
                self.depth += 1;
                if let Some(i) = init {
                    self.stmt(i, out, "", "    ");
                }
                let cond = cond
                    .as_ref()
                    .map(|c| self.expr(c))
                    .unwrap_or_else(|| "true".into());
                out.push_str(&format!("{}loop {{\n", self.pad(0)));
                self.depth += 1;
                out.push_str(&format!("{}if !{cond} {{ break; }}\n", self.pad(0)));
                self.stmt(body, out, "", "    ");
                if let Some(u) = update {
                    let text = self.expr(u);
                    match u {
                        Expr::Postfix(op, t) if op == "++" || op == "--" => {
                            let op = if op == "++" { "+=" } else { "-=" };
                            out.push_str(&format!("{}{} {op} 1;\n", self.pad(0), self.expr(t)));
                        }
                        Expr::Unary(op, t) if op == "++" || op == "--" => {
                            let op = if op == "++" { "+=" } else { "-=" };
                            out.push_str(&format!("{}{} {op} 1;\n", self.pad(0), self.expr(t)));
                        }
                        _ => out.push_str(&format!("{}{text};\n", self.pad(0))),
                    }
                }
                self.depth -= 1;
                out.push_str(&format!("{}}}\n", self.pad(0)));
                self.depth -= 1;
                out.push_str(&format!("{p}}}\n"));
            }
            Stmt::ForEach(ty, name, iter, body) => {
                let rt = rust_type(ty, self.known, &mut self.issues);
                let iter = self.expr(iter);
                out.push_str(&format!("{p}for {} in {iter} {{\n", snake(name)));
                let _ = rt;
                self.depth += 1;
                self.stmt(body, out, "", "    ");
                self.depth -= 1;
                out.push_str(&format!("{p}}}\n"));
            }
            Stmt::Switch(subject, arms) => {
                self.issues
                    .push("a Java `switch` became a `match`: check each arm".into());
                let subject = self.expr(subject);
                out.push_str(&format!("{p}match {subject} {{\n"));
                for (case, body) in arms {
                    let pattern = match case {
                        Some(e) => self.expr(e),
                        None => "_".to_owned(),
                    };
                    out.push_str(&format!("{p}    {pattern} => {{\n"));
                    self.depth += 1;
                    self.depth += 1;
                    for s in body {
                        self.stmt(s, out, "", "    ");
                    }
                    self.depth -= 2;
                    out.push_str(&format!("{p}    }}\n"));
                }
                out.push_str(&format!("{p}}}\n"));
            }
            Stmt::Try(body, catches, finally) => {
                self.issues.push(
                    "a Java `try`/`catch` became a plain block: Rust has no exceptions here".into(),
                );
                out.push_str(&format!("{p}{{\n"));
                self.depth += 1;
                for s in body {
                    self.stmt(s, out, "", "    ");
                }
                for (ty, name, block) in catches {
                    out.push_str(&format!("{}// catch ({ty} {name})\n", self.pad(0)));
                    for s in block {
                        self.stmt(s, out, "", "    ");
                    }
                }
                if let Some(f) = finally {
                    out.push_str(&format!("{}// finally\n", self.pad(0)));
                    for s in f {
                        self.stmt(s, out, "", "    ");
                    }
                }
                self.depth -= 1;
                out.push_str(&format!("{p}}}\n"));
            }
            Stmt::Synchronized(lock, body) => {
                self.issues
                    .push("a Java `synchronized` block has no Rust equivalent here".into());
                let lock = self.expr(lock);
                out.push_str(&format!("{p}// synchronized on {lock}\n"));
                self.stmt(body, out, "", "    ");
            }
        }
    }

    fn expr(&mut self, e: &Expr) -> String {
        match e {
            Expr::Ident(n) => {
                if n == "super" {
                    self.issues.push("`super` cannot be carried over".into());
                    return "todo!(\"super\")".into();
                }
                // A parameter or local, then one of the class's own fields.
                if self.vars.contains_key(n) {
                    return snake(n);
                }
                if self.fields.contains_key(n) {
                    return format!("{}.{}", self.this_name, snake(n));
                }
                if self.static_fields.contains(n) {
                    self.issues
                        .push(format!("static field `{n}` has no Rust storage here"));
                    return self.api_todo(format!("static field {n}"));
                }
                ident_name(n)
            }
            Expr::This => self.this_name.clone(),
            Expr::Lit(l) => literal(l),
            Expr::Field(base, name) => {
                let base_text = self.expr(base);
                if is_todo_text(&base_text) {
                    // A Java API receiver already left as a `todo!()`: keep it.
                    return base_text;
                }
                if base_text == "System.out" || base_text == "System.err" {
                    return format!("{base_text}.{name}");
                }
                if let Some(t) = self.expr_type(base)
                    && self.api_type(&t)
                {
                    return self.api_todo(format!("{t}.{name}"));
                }
                if base_text
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_uppercase())
                    .unwrap_or(false)
                {
                    // `ItemStack.empty` / `Constants.commonConfig` names a static
                    // member of a Java class: there is no Rust one here.
                    return self.api_todo(format!("static {base_text}.{name}"));
                }
                format!("{base_text}.{}", snake(name))
            }
            Expr::Index(base, idx) => format!("{}[{}]", self.expr(base), self.expr(idx)),
            Expr::Call(recv, name, args) => self.call(recv.as_deref(), name, args),
            Expr::New(ty, args) => {
                let simple = ty
                    .split('<')
                    .next()
                    .unwrap_or(ty)
                    .rsplit('.')
                    .next()
                    .unwrap_or(ty)
                    .trim();
                let args: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
                let joined = args.join(", ");
                // Java's collections and strings have Rust counterparts.
                match simple {
                    "ArrayList" | "LinkedList" | "Vector" => return "Vec::new()".into(),
                    "HashSet" | "LinkedHashSet" | "TreeSet" => return "BTreeSet::new()".into(),
                    "HashMap" | "LinkedHashMap" | "TreeMap" => return "BTreeMap::new()".into(),
                    "StringBuilder" | "StringBuffer" => return "String::new()".into(),
                    "String" => {
                        return if args.is_empty() {
                            "String::new()".into()
                        } else {
                            format!("({joined}).to_string()")
                        };
                    }
                    _ => {}
                }
                let ty = sanitize(simple);
                if self.known.contains(simple) {
                    format!("{ty}::new({joined})")
                } else {
                    self.api_todo(format!("new {simple}({joined})"))
                }
            }
            Expr::NewArray(_ty, items) => {
                let items: Vec<String> = items.iter().map(|a| self.expr(a)).collect();
                // `new int[n]` has one item, the size; `new int[]{1, 2}` has the values.
                if items.len() == 1 && items[0].parse::<i64>().is_ok() {
                    return format!("vec![Default::default(); {} as usize]", items[0]);
                }
                format!("vec![{}]", items.join(", "))
            }
            Expr::Unary(op, e) => {
                let inner = self.expr(e);
                match op.as_str() {
                    "++" | "--" => format!("{inner} /* {op} */"),
                    "!" => format!("!{inner}"),
                    _ => format!("{op}{inner}"),
                }
            }
            Expr::Postfix(op, e) => format!("{} /* {op} */", self.expr(e)),
            Expr::Binary(op, a, b) => {
                if op == "instanceof" {
                    let raw = match b.as_ref() {
                        Expr::Ident(t) => t.clone(),
                        _ => String::new(),
                    };
                    let simple = raw
                        .split('<')
                        .next()
                        .unwrap_or(&raw)
                        .rsplit('.')
                        .next()
                        .unwrap_or(&raw)
                        .to_string();
                    let subject = self.expr(a);
                    if self.known.contains(&simple) {
                        self.issues
                            .push("`instanceof` became a Rust `matches!` pattern".into());
                        return format!("matches!({subject}, {} {{ .. }})", sanitize(&simple));
                    }
                    self.issues.push(format!(
                        "`instanceof {raw}` checks a Java game type: left for a human"
                    ));
                    return self.api_todo(format!("instanceof {raw}"));
                }
                // `x != null` / `x == null` on an `Option`.
                if matches!(b.as_ref(), Expr::Lit(Lit::Null)) && (op == "==" || op == "!=") {
                    if let Some(t) = self.expr_type(a)
                        && self.api_type(&t)
                    {
                        return self.api_todo(format!("null check on {t}"));
                    }
                    let method = if op == "==" { "is_none" } else { "is_some" };
                    return format!("{}.{method}()", self.expr(a));
                }
                if matches!(a.as_ref(), Expr::Lit(Lit::Null)) && (op == "==" || op == "!=") {
                    if let Some(t) = self.expr_type(b)
                        && self.api_type(&t)
                    {
                        return self.api_todo(format!("null check on {t}"));
                    }
                    let method = if op == "==" { "is_none" } else { "is_some" };
                    return format!("{}.{method}()", self.expr(b));
                }
                if op == "+" && (is_string(a) || is_string(b)) {
                    return format!("format!(\"{{}}{{}}\", {}, {})", self.expr(a), self.expr(b));
                }
                let a_text = self.expr(a);
                let b_text = self.expr(b);
                // An API side already became a `todo!()`: comparing it would
                // only add a type error.
                if is_todo_text(&a_text) {
                    return a_text;
                }
                if is_todo_text(&b_text) {
                    return b_text;
                }
                format!("{a_text} {op} {b_text}")
            }
            Expr::Assign(op, l, r) => {
                let lhs = self.expr(l);
                if is_todo_text(&lhs) {
                    return lhs;
                }
                let mut rhs = self.expr(r);
                // A Java `int` from `length()`/`size()`: Rust counts in `usize`.
                if op == "="
                    && (rhs.contains("len()") || rhs.contains("count()"))
                    && !rhs.contains(" as ")
                {
                    let java = self.expr_type(l).unwrap_or_default();
                    if java == "int" || java == "long" {
                        rhs = format!(
                            "({rhs}) as {}",
                            rust_type(&java, self.known, &mut Vec::new())
                        );
                    }
                }
                format!("{lhs} {op} {rhs}")
            }
            Expr::Ternary(c, a, b) => {
                format!(
                    "if {} {{ {} }} else {{ {} }}",
                    self.expr(c),
                    self.expr(a),
                    self.expr(b)
                )
            }
            Expr::InstanceOf(subject, ty, name) => {
                let subject = self.expr(subject);
                let simple = ty
                    .split('<')
                    .next()
                    .unwrap_or(ty)
                    .rsplit('.')
                    .next()
                    .unwrap_or(ty)
                    .trim()
                    .to_owned();
                if let Some(bound) = name {
                    self.issues.push(format!(
                        "pattern variable `{bound}` of `{ty}` is declared as a `todo!()` at the top of the method"
                    ));
                }
                if self.known.contains(&simple) {
                    format!("matches!({subject}, {} {{ .. }})", sanitize(&simple))
                } else {
                    self.issues.push(format!(
                        "`instanceof {ty}` checks a Java game type: left for a human"
                    ));
                    self.api_todo(format!("instanceof {ty}"))
                }
            }
            Expr::Cast(ty, e) => {
                let inner = self.expr(e);
                if is_numeric(ty) {
                    format!("{inner} as {}", rust_type(ty, self.known, &mut self.issues))
                } else {
                    self.issues
                        .push(format!("cast to `{ty}` was dropped: check the Rust types"));
                    inner
                }
            }
            Expr::Array(items) => {
                let items: Vec<String> = items.iter().map(|a| self.expr(a)).collect();
                if items.is_empty() {
                    "vec![]".into()
                } else {
                    format!("vec![{}]", items.join(", "))
                }
            }
            Expr::Lambda(params, body) => {
                let names: Vec<String> = params.iter().map(|(_, n)| sanitize(n)).collect();
                let body = match body.as_ref() {
                    Expr::Block(b) => {
                        let mut text = String::new();
                        self.depth += 1;
                        for s in b {
                            self.stmt(s, &mut text, "", "    ");
                        }
                        self.depth -= 1;
                        format!("{{\n{text}{}}}", self.pad(0))
                    }
                    other => self.expr(other),
                };
                format!("|{}| {body}", names.join(", "))
            }
            Expr::MethodRef(base, name) => {
                let base = self.expr(base);
                format!("{base}::{}", snake(name))
            }
            Expr::ClassLit(ty) => {
                self.issues
                    .push("a class literal became its Rust type name".into());
                sanitize(ty.rsplit('.').next().unwrap_or(ty))
            }
            Expr::Block(b) => {
                let mut text = String::new();
                self.depth += 1;
                for s in b {
                    self.stmt(s, &mut text, "", "    ");
                }
                self.depth -= 1;
                format!("{{\n{text}{}}}", self.pad(0))
            }
        }
    }

    fn call(&mut self, recv: Option<&Expr>, name: &str, args: &[Expr]) -> String {
        let args_text: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
        let joined = args_text.join(", ");
        // Render the receiver once: rendering it twice nested calls grow the
        // report exponentially.
        let receiver = recv.map(|r| self.expr(r));
        if let Some(r_text) = &receiver
            && is_todo_text(r_text)
        {
            return r_text.clone();
        }
        // A receiver's own type, when it is a simple name.
        let recv_name = match recv {
            Some(Expr::Ident(n)) => Some(n.as_str()),
            Some(Expr::Field(b, f)) if matches!(b.as_ref(), Expr::Ident(n) if n == "System") => {
                return match f.as_str() {
                    "out" if name == "println" => format!("println!(\"{{}}\", {joined})"),
                    "out" if name == "print" => format!("print!(\"{{}}\", {joined})"),
                    "err" if name == "println" => format!("eprintln!(\"{{}}\", {joined})"),
                    "err" if name == "print" => format!("eprint!(\"{{}}\", {joined})"),
                    _ => format!("System.{f}.{name}({joined})"),
                };
            }
            _ => None,
        };
        if let Some(r) = recv_name {
            match (r, name) {
                ("Math", "max") => {
                    return format!(
                        "({}).max({joined})",
                        args_text.first().cloned().unwrap_or_default()
                    );
                }
                ("Math", "min") => {
                    return format!(
                        "({}).min({joined})",
                        args_text.first().cloned().unwrap_or_default()
                    );
                }
                ("Math", "abs") => return format!("({}).abs()", args_text.join(", ")),
                ("Math", "sqrt") => return format!("({}).sqrt()", args_text.join(", ")),
                ("Math", "floor") => return format!("({}).floor()", args_text.join(", ")),
                ("Math", "ceil") => return format!("({}).ceil()", args_text.join(", ")),
                ("Math", "round") => return format!("({}).round()", args_text.join(", ")),
                ("Math", "pow") => {
                    return format!(
                        "({}).powf({})",
                        args_text.first().cloned().unwrap_or_default(),
                        args_text.get(1).cloned().unwrap_or_default()
                    );
                }
                ("Objects", "requireNonNull") => return args_text.join(", "),
                ("Objects", "equals") => {
                    return format!(
                        "{} == {}",
                        args_text.first().cloned().unwrap_or_default(),
                        args_text.get(1).cloned().unwrap_or_default()
                    );
                }
                ("String", "format") if !args.is_empty() => {
                    let fmt = java_format_to_rust(&args_text[0]);
                    let rest = args_text[1..].join(", ");
                    return if rest.is_empty() {
                        format!("format!(\"{fmt}\")")
                    } else {
                        format!("format!(\"{fmt}\", {rest})")
                    };
                }
                ("String", "valueOf") => return format!("({}).to_string()", joined),
                _ => {}
            }
        }
        // A Java API receiver the mod does not own: keep the site visible.
        if let Some(r) = recv {
            if let Some(t) = self.expr_type(r)
                && self.api_type(&t)
            {
                return self.api_todo(format!("{t}.{name}({joined})"));
            }
            if let Expr::Ident(n) = r
                && n.chars()
                    .next()
                    .map(|c| c.is_ascii_uppercase())
                    .unwrap_or(false)
                && !self.known.contains(n)
            {
                return self.api_todo(format!("{n}.{name}({joined})"));
            }
        } else if self.methods.contains(name) {
            // Java calls its own methods without `self.`.
            let rust_name = self.method_name(name, args.len());
            return if self.static_method {
                format!("Self::{rust_name}({joined})")
            } else {
                format!("self.{rust_name}({joined})")
            };
        }
        if let Some(r) = &receiver {
            // A class's static method: `ItemStack.empty()` is `ItemStack::empty()`.
            if r.chars()
                .next()
                .map(|c| c.is_ascii_uppercase())
                .unwrap_or(false)
                && !r.contains('.')
            {
                let rust_name = self.method_name(name, args.len());
                return format!("{r}::{rust_name}({joined})");
            }
        }
        // Java's `String` methods with a Rust equivalent.
        if let Some(r) = recv {
            let t = self.expr_type(r).unwrap_or_default();
            if t.trim_end_matches("[]") == "String" || t == "StringBuilder" || t == "StringBuffer" {
                let r_text = receiver.clone().unwrap_or_default();
                match name {
                    "split" => {
                        return format!(
                            "{r_text}.split({joined}).map(|s| s.to_string()).collect::<Vec<_>>()"
                        );
                    }
                    "toCharArray" => return format!("{r_text}.chars().collect::<Vec<char>>()"),
                    "length" => return format!("{r_text}.chars().count()"),
                    "charAt" => {
                        return format!(
                            "{r_text}.chars().nth(({joined}) as usize).unwrap_or('\\0')"
                        );
                    }
                    "substring" if args.len() == 1 => {
                        return format!("{}[({}) as usize..].to_string()", r_text, args_text[0]);
                    }
                    "substring" if args.len() == 2 => {
                        return format!(
                            "{}[({}) as usize..({}) as usize].to_string()",
                            r_text, args_text[0], args_text[1]
                        );
                    }
                    "toLowerCase" => return format!("{r_text}.to_lowercase()"),
                    "toUpperCase" => return format!("{r_text}.to_uppercase()"),
                    "indexOf" => {
                        return format!("{r_text}.find({joined}).map(|i| i as i32).unwrap_or(-1)");
                    }
                    "lastIndexOf" => {
                        return format!("{r_text}.rfind({joined}).map(|i| i as i32).unwrap_or(-1)");
                    }
                    "startsWith" => return format!("{r_text}.starts_with({joined})"),
                    "endsWith" => return format!("{r_text}.ends_with({joined})"),
                    "replace" if args.len() == 2 => {
                        return format!("{r_text}.replace({}, {})", args_text[0], args_text[1]);
                    }
                    "concat" => return format!("format!(\"{{}}{{}}\", {r_text}, {joined})"),
                    "equalsIgnoreCase" => {
                        return format!("{r_text}.eq_ignore_ascii_case({joined})");
                    }
                    "trim" => return format!("{r_text}.trim().to_string()"),
                    _ => {}
                }
            }
        }
        // Java's `Optional` is Rust's `Option`.
        if let Some(r) = recv
            && let Some(t) = self.expr_type(r)
            && (t.trim_start().starts_with("Optional") || t.trim_start().starts_with("Option<"))
        {
            let r_text = receiver.clone().unwrap_or_default();
            return match name {
                "isPresent" => format!("{r_text}.is_some()"),
                "isEmpty" => format!("{r_text}.is_none()"),
                "get" => format!("{r_text}.unwrap()"),
                "orElse" => format!("{r_text}.unwrap_or({joined})"),
                "orElseGet" => format!("{r_text}.unwrap_or_else({joined})"),
                "orElseThrow" => format!("{r_text}.expect(\"empty Optional\")"),
                _ => format!("{r_text}.{}({joined})", snake(name)),
            };
        }
        let own_receiver = recv
            .and_then(|r| self.expr_type(r))
            .map(|t| {
                let simple = t
                    .trim()
                    .trim_end_matches("[]")
                    .split('<')
                    .next()
                    .unwrap_or(&t)
                    .rsplit('.')
                    .next()
                    .unwrap_or(&t)
                    .to_owned();
                self.known.contains(&simple)
            })
            .unwrap_or(false);
        let rust_name = if own_receiver {
            // A method of one of the mod's own classes: keep overloads apart.
            self.method_name(name, args.len())
        } else {
            match name {
                "add" | "push" => "push".to_owned(),
                "size" => "len".to_owned(),
                "isEmpty" => "is_empty".to_owned(),
                "get" if recv.is_some() => "get".to_owned(),
                "put" => "insert".to_owned(),
                "containsKey" => "contains_key".to_owned(),
                "containsValue" => "contains_value".to_owned(),
                "remove" => "remove".to_owned(),
                "equals" => "eq".to_owned(),
                "toString" => "to_string".to_owned(),
                "hashCode" => "hash_code".to_owned(),
                other => snake(other),
            }
        };
        match receiver {
            Some(r) => format!("{r}.{rust_name}({joined})"),
            None => {
                // A plain call to another class's static method: `Class.method`.
                if self.known.contains(name) {
                    format!("{}::{}({joined})", sanitize(name), snake(name))
                } else {
                    // Inherited or static-imported: no Rust function of that name.
                    self.api_todo(format!("{name}({joined})"))
                }
            }
        }
    }
}

/// A name as Rust spells it: a class keeps its name, a variable or field is
/// snake_case, and an ALL_CAPS constant stays as it is.
fn ident_name(name: &str) -> String {
    let all_caps = name.len() > 1
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    if all_caps {
        return name.to_owned();
    }
    let first = name.chars().next().unwrap_or('_');
    if first.is_ascii_uppercase() {
        sanitize(name)
    } else {
        snake(name)
    }
}

fn is_todo_text(text: &str) -> bool {
    text.trim_start().starts_with("todo!(")
}

/// The names Java 16 pattern bindings introduce anywhere in a method.
fn collect_patterns(stmts: &[Stmt], out: &mut BTreeSet<String>) {
    fn expr(e: &Expr, out: &mut BTreeSet<String>) {
        match e {
            Expr::InstanceOf(b, _, name) => {
                if let Some(n) = name {
                    out.insert(n.clone());
                }
                expr(b, out);
            }
            Expr::Assign(_, a, b) | Expr::Binary(_, a, b) => {
                expr(a, out);
                expr(b, out);
            }
            Expr::Ternary(a, b, c) => {
                expr(a, out);
                expr(b, out);
                expr(c, out);
            }
            Expr::Unary(_, a) | Expr::Postfix(_, a) | Expr::Cast(_, a) => expr(a, out),
            Expr::Field(a, _) => expr(a, out),
            Expr::Index(a, b) => {
                expr(a, out);
                expr(b, out);
            }
            Expr::Call(recv, _, args) => {
                if let Some(r) = recv {
                    expr(r, out);
                }
                for a in args {
                    expr(a, out);
                }
            }
            Expr::New(_, args) | Expr::NewArray(_, args) | Expr::Array(args) => {
                for a in args {
                    expr(a, out);
                }
            }
            Expr::Lambda(_, b) | Expr::MethodRef(b, _) => expr(b, out),
            Expr::Block(b) => collect_patterns(b, out),
            _ => {}
        }
    }
    for s in stmts {
        match s {
            Stmt::Local(_, _, Some(e)) | Stmt::Expr(e) | Stmt::Return(Some(e)) | Stmt::Throw(e) => {
                expr(e, out)
            }
            Stmt::Block(b) => collect_patterns(b, out),
            Stmt::If(c, a, b) => {
                expr(c, out);
                collect_patterns(std::slice::from_ref(a), out);
                if let Some(b) = b {
                    collect_patterns(std::slice::from_ref(b), out);
                }
            }
            Stmt::While(c, b) | Stmt::DoWhile(b, c) | Stmt::Synchronized(c, b) => {
                expr(c, out);
                collect_patterns(std::slice::from_ref(b), out);
            }
            Stmt::For(i, c, u, b) => {
                if let Some(i) = i {
                    collect_patterns(std::slice::from_ref(i), out);
                }
                if let Some(c) = c {
                    expr(c, out);
                }
                if let Some(u) = u {
                    expr(u, out);
                }
                collect_patterns(std::slice::from_ref(b), out);
            }
            Stmt::ForEach(_, _, e, b) => {
                expr(e, out);
                collect_patterns(std::slice::from_ref(b), out);
            }
            Stmt::Switch(e, arms) => {
                expr(e, out);
                for (case, body) in arms {
                    if let Some(c) = case {
                        expr(c, out);
                    }
                    collect_patterns(body, out);
                }
            }
            Stmt::Try(b, catches, f) => {
                collect_patterns(b, out);
                for (_, _, b) in catches {
                    collect_patterns(b, out);
                }
                if let Some(f) = f {
                    collect_patterns(f, out);
                }
            }
            _ => {}
        }
    }
}

fn literal(l: &Lit) -> String {
    match l {
        Lit::Int(n) => n.trim_end_matches(['l', 'L']).replace('_', ""),
        Lit::Float(n) => {
            let n = n.trim_end_matches(['f', 'F', 'd', 'D']).replace('_', "");
            // `1f` is a float in Java; Rust would read `1` as an integer.
            if n.contains('.') || n.contains('e') || n.contains('E') {
                n
            } else {
                format!("{n}.0")
            }
        }
        Lit::Str(s) => format!("\"{}\"", escape(s)),
        Lit::Char(c) => format!("'{}'", escape_char(*c)),
        Lit::Bool(b) => b.to_string(),
        Lit::Null => "None".into(),
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
}

fn escape_char(c: char) -> String {
    match c {
        '\n' => "\\n".into(),
        '\t' => "\\t".into(),
        '\r' => "\\r".into(),
        '\'' => "\\'".into(),
        '\\' => "\\\\".into(),
        c => c.to_string(),
    }
}

fn is_string(e: &Expr) -> bool {
    matches!(e, Expr::Lit(Lit::Str(_)))
        || matches!(e, Expr::Call(Some(r), name, _) if name == "toString" && matches!(r.as_ref(), Expr::This))
}

fn is_numeric(ty: &str) -> bool {
    matches!(
        ty.trim(),
        "byte" | "short" | "int" | "long" | "float" | "double" | "char"
    )
}

/// Java's `%s`/`%d` placeholders as Rust's `{}`.
fn java_format_to_rust(fmt: &str) -> String {
    let mut out = String::new();
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('%') => {
                out.push('%');
                chars.next();
            }
            Some(_) => {
                // Skip flags, width, precision, then the conversion.
                while let Some(n) = chars.peek() {
                    if n.is_ascii_digit()
                        || matches!(n, '-' | '+' | ' ' | '0' | ',' | '.' | '(' | '#')
                    {
                        chars.next();
                    } else {
                        break;
                    }
                }
                if let Some(kind) = chars.next() {
                    match kind {
                        's' | 'S' => out.push_str("{}"),
                        'd' | 'o' | 'x' | 'X' => out.push_str("{}"),
                        'f' | 'e' | 'E' | 'g' | 'G' => out.push_str("{}"),
                        'b' | 'B' => out.push_str("{}"),
                        'c' | 'C' => out.push_str("{}"),
                        'n' => out.push('\n'),
                        other => {
                            out.push('%');
                            out.push(other);
                        }
                    }
                }
            }
            None => out.push('%'),
        }
    }
    escape(&out)
}
