//! A Java reader for HexaGon: tokens, a syntax tree, and a parser for the
//! subset of Java that Minecraft mods are written in (classes, fields,
//! methods, the usual statements and expressions, annotations).
//!
//! It never runs or resolves Java. It reads it, so the Rust writer
//! (`rust_gen`) can say what it translated and what it could not.

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Num(String),
    Str(String),
    Chr(char),
    Sym(String),
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub line: u32,
}

/// Splits Java source into tokens, dropping comments and whitespace.
pub fn lex(src: &str) -> Result<Vec<Token>, String> {
    let b: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut line = 1u32;
    // Java 1.6 text blocks and normal strings both end in `"`.
    while i < b.len() {
        let c = b[i];
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '/' && i + 1 < b.len() && b[i + 1] == '/' {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < b.len() && b[i + 1] == '*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                if b[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i = (i + 2).min(b.len());
            continue;
        }
        if c == '"' {
            let start = line;
            i += 1;
            let mut s = String::new();
            while i < b.len() && b[i] != '"' {
                if b[i] == '\\' && i + 1 < b.len() {
                    let e = b[i + 1];
                    s.push(match e {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        '"' => '"',
                        '\'' => '\'',
                        '\\' => '\\',
                        '0' => '\0',
                        other => other,
                    });
                    i += 2;
                    continue;
                }
                if b[i] == '\n' {
                    line += 1;
                }
                s.push(b[i]);
                i += 1;
            }
            i += 1;
            out.push(Token {
                tok: Tok::Str(s),
                line: start,
            });
            continue;
        }
        if c == '\'' {
            i += 1;
            let mut ch = ' ';
            if i < b.len() {
                if b[i] == '\\' && i + 1 < b.len() {
                    ch = match b[i + 1] {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        '\'' => '\'',
                        '\\' => '\\',
                        other => other,
                    };
                    i += 2;
                } else {
                    ch = b[i];
                    i += 1;
                }
            }
            if i < b.len() && b[i] == '\'' {
                i += 1;
            }
            out.push(Token {
                tok: Tok::Chr(ch),
                line,
            });
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && i + 1 < b.len() && b[i + 1].is_ascii_digit()) {
            let start = line;
            let mut s = String::new();
            let hex = c == '0' && i + 1 < b.len() && (b[i + 1] == 'x' || b[i + 1] == 'X');
            if hex {
                s.push(b[i]);
                s.push(b[i + 1]);
                i += 2;
                while i < b.len() && (b[i].is_ascii_hexdigit() || b[i] == '_') {
                    s.push(b[i]);
                    i += 1;
                }
            } else {
                while i < b.len()
                    && (b[i].is_ascii_digit()
                        || b[i] == '.'
                        || b[i] == '_'
                        || b[i] == 'e'
                        || b[i] == 'E')
                {
                    // A leading sign after e/E belongs to the number.
                    if (b[i] == 'e' || b[i] == 'E')
                        && i + 1 < b.len()
                        && (b[i + 1] == '-' || b[i + 1] == '+')
                    {
                        s.push(b[i]);
                        s.push(b[i + 1]);
                        i += 2;
                        continue;
                    }
                    s.push(b[i]);
                    i += 1;
                }
            }
            while i < b.len() && matches!(b[i], 'f' | 'F' | 'd' | 'D' | 'l' | 'L') {
                s.push(b[i]);
                i += 1;
            }
            out.push(Token {
                tok: Tok::Num(s),
                line: start,
            });
            continue;
        }
        if c.is_alphabetic() || c == '_' || c == '$' {
            let start = line;
            let mut s = String::new();
            while i < b.len() && (b[i].is_alphanumeric() || b[i] == '_' || b[i] == '$') {
                s.push(b[i]);
                i += 1;
            }
            out.push(Token {
                tok: Tok::Ident(s),
                line: start,
            });
            continue;
        }
        // The longest operator first (`>>>=` before `>>>` before `>>`).
        const OPS: [&str; 24] = [
            ">>>=", "<<=", ">>=", "...", ">>>", "->", "::", "++", "--", "==", "!=", "<=", ">=",
            "&&", "||", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<",
        ];
        let mut matched = None;
        for op in OPS {
            let op: Vec<char> = op.chars().collect();
            if i + op.len() <= b.len() && b[i..i + op.len()] == op[..] {
                matched = Some(op.iter().collect::<String>());
                break;
            }
        }
        if let Some(op) = matched {
            i += op.chars().count();
            out.push(Token {
                tok: Tok::Sym(op),
                line,
            });
            continue;
        }
        out.push(Token {
            tok: Tok::Sym(c.to_string()),
            line,
        });
        i += 1;
    }
    out.push(Token {
        tok: Tok::Eof,
        line,
    });
    Ok(out)
}

#[derive(Debug, Clone, Default)]
pub struct Annotation {
    pub name: String,
    pub args: String,
}

#[derive(Debug, Clone, Default)]
pub struct Field {
    pub name: String,
    pub ty: String,
    pub init: Option<Expr>,
    pub modifiers: Vec<String>,
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone, Default)]
pub struct Method {
    pub name: String,
    pub ret: String,
    pub params: Vec<(String, String)>,
    pub body: Option<Vec<Stmt>>,
    pub modifiers: Vec<String>,
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone, Default)]
pub struct TypeDecl {
    pub name: String,
    /// class, interface, enum, record, annotation.
    pub kind: String,
    pub modifiers: Vec<String>,
    pub annotations: Vec<Annotation>,
    pub extends: Vec<String>,
    pub implements: Vec<String>,
    pub fields: Vec<Field>,
    pub methods: Vec<Method>,
    pub nested: Vec<TypeDecl>,
}

#[derive(Debug, Clone, Default)]
pub struct CompilationUnit {
    pub package: String,
    pub imports: Vec<String>,
    pub types: Vec<TypeDecl>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Lit {
    Int(String),
    Float(String),
    Str(String),
    Char(char),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Ident(String),
    This,
    Lit(Lit),
    Field(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Call(Option<Box<Expr>>, String, Vec<Expr>),
    New(String, Vec<Expr>),
    NewArray(String, Vec<Expr>),
    Unary(String, Box<Expr>),
    Postfix(String, Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
    Assign(String, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Cast(String, Box<Expr>),
    Array(Vec<Expr>),
    Lambda(Vec<(String, String)>, Box<Expr>),
    MethodRef(Box<Expr>, String),
    ClassLit(String),
    /// `expr instanceof Type name` (the name is Java 16 pattern binding).
    InstanceOf(Box<Expr>, String, Option<String>),
    Block(Vec<Stmt>),
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Local(String, String, Option<Expr>),
    Expr(Expr),
    Block(Vec<Stmt>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    While(Expr, Box<Stmt>),
    DoWhile(Box<Stmt>, Expr),
    For(Option<Box<Stmt>>, Option<Expr>, Option<Expr>, Box<Stmt>),
    ForEach(String, String, Expr, Box<Stmt>),
    Return(Option<Expr>),
    Break,
    Continue,
    Throw(Expr),
    Switch(Expr, Vec<(Option<Expr>, Vec<Stmt>)>),
    Try(
        Vec<Stmt>,
        Vec<(String, String, Vec<Stmt>)>,
        Option<Vec<Stmt>>,
    ),
    Synchronized(Expr, Box<Stmt>),
    Empty,
}

pub struct Parser {
    toks: Vec<Token>,
    at: usize,
    /// What was skipped and why, for the report.
    pub problems: Vec<String>,
}

impl Parser {
    pub fn new(toks: Vec<Token>) -> Self {
        Self {
            toks,
            at: 0,
            problems: Vec::new(),
        }
    }

    fn peek(&self) -> &Tok {
        &self.toks[self.at].tok
    }

    fn line(&self) -> u32 {
        self.toks[self.at].line
    }

    fn bump(&mut self) -> Tok {
        let t = self.toks[self.at].tok.clone();
        if self.at + 1 < self.toks.len() {
            self.at += 1;
        }
        t
    }

    fn sym(&mut self, s: &str) -> bool {
        if matches!(self.peek(), Tok::Sym(x) if x == s) {
            self.bump();
            return true;
        }
        false
    }

    fn ident(&mut self, s: &str) -> bool {
        if matches!(self.peek(), Tok::Ident(x) if x == s) {
            self.bump();
            return true;
        }
        false
    }

    fn expect_sym(&mut self, s: &str) -> Result<(), String> {
        if self.sym(s) {
            Ok(())
        } else {
            Err(format!(
                "line {}: expected `{s}`, found {:?}",
                self.line(),
                self.peek()
            ))
        }
    }

    fn expect_ident(&mut self) -> Result<String, String> {
        match self.peek().clone() {
            Tok::Ident(s) => {
                self.bump();
                Ok(s)
            }
            other => Err(format!(
                "line {}: expected a name, found {other:?}",
                self.line()
            )),
        }
    }

    fn skip_balanced(&mut self, open: &str, close: &str) -> String {
        // Consumes `open ... close` (or up to Eof) and returns the text between.
        let mut depth = 0usize;
        let mut text = String::new();
        while !matches!(self.peek(), Tok::Eof) {
            let t = self.bump();
            let s = match &t {
                Tok::Sym(s) => s.clone(),
                Tok::Ident(s) => s.clone(),
                Tok::Str(s) => format!("\"{s}\""),
                Tok::Chr(c) => format!("'{c}'"),
                Tok::Num(n) => n.clone(),
                Tok::Eof => break,
            };
            if s == open {
                depth += 1;
                if depth == 1 {
                    continue;
                }
            }
            if s == close {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    break;
                }
            }
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&s);
        }
        text
    }

    /// `@Name` or `@Name(...)`, one at a time.
    fn annotation(&mut self) -> Option<Annotation> {
        if !self.sym("@") {
            return None;
        }
        let mut name = self.expect_ident().ok()?;
        while self.sym(".") {
            if let Ok(more) = self.expect_ident() {
                name.push('.');
                name.push_str(&more);
            }
        }
        let args = if matches!(self.peek(), Tok::Sym(s) if s == "(") {
            self.skip_balanced("(", ")")
        } else {
            String::new()
        };
        Some(Annotation { name, args })
    }

    fn modifiers(&mut self) -> (Vec<String>, Vec<Annotation>) {
        let mut mods = Vec::new();
        let mut annotations = Vec::new();
        loop {
            match self.peek() {
                Tok::Sym(s) if s == "@" => {
                    if let Some(a) = self.annotation() {
                        annotations.push(a);
                    } else {
                        break;
                    }
                }
                Tok::Ident(s)
                    if matches!(
                        s.as_str(),
                        "public"
                            | "private"
                            | "protected"
                            | "static"
                            | "final"
                            | "abstract"
                            | "native"
                            | "synchronized"
                            | "transient"
                            | "volatile"
                            | "default"
                            | "strictfp"
                            | "sealed"
                            | "non-sealed"
                    ) =>
                {
                    mods.push(s.clone());
                    self.bump();
                }
                _ => break,
            }
        }
        (mods, annotations)
    }

    /// A type, as its Java text: `java.util.List<Item>`, `int[]`, `var`.
    fn ty(&mut self) -> Result<String, String> {
        let mut out = String::new();
        if self.sym("?") {
            out.push('?');
            if self.ident("extends") || self.ident("super") {
                out.push(' ');
                out.push_str(&self.ty()?);
            }
            return Ok(out);
        }
        out.push_str(&self.expect_ident()?);
        while self.sym(".") {
            out.push('.');
            out.push_str(&self.expect_ident()?);
        }
        if self.sym("<") {
            out.push('<');
            let mut first = true;
            while !matches!(self.peek(), Tok::Sym(s) if s == ">")
                && !matches!(self.peek(), Tok::Eof)
            {
                if !first {
                    if !self.sym(",") {
                        break;
                    }
                    out.push(',');
                }
                first = false;
                out.push_str(&self.ty()?);
            }
            self.sym(">");
            out.push('>');
        }
        while self.sym("[") {
            self.sym("]");
            out.push_str("[]");
        }
        if self.sym("...") {
            out.push_str("...");
        }
        Ok(out)
    }

    pub fn unit(&mut self) -> CompilationUnit {
        let mut unit = CompilationUnit::default();
        // Package and imports may repeat (one per file), at the top only.
        loop {
            if self.ident("package") {
                let mut name = self.expect_ident().unwrap_or_default();
                while self.sym(".") {
                    name.push('.');
                    name.push_str(&self.expect_ident().unwrap_or_default());
                }
                let _ = self.expect_sym(";");
                unit.package = name;
                continue;
            }
            if self.ident("import") {
                let mut name = String::new();
                if self.ident("static") {
                    name.push_str("static ");
                }
                while !matches!(self.peek(), Tok::Sym(s) if s == ";")
                    && !matches!(self.peek(), Tok::Eof)
                {
                    match self.bump() {
                        Tok::Ident(s) => name.push_str(&s),
                        Tok::Sym(s) => name.push_str(&s),
                        Tok::Num(n) => name.push_str(&n),
                        _ => {}
                    }
                }
                self.sym(";");
                unit.imports.push(name);
                continue;
            }
            if self.sym(";") {
                continue;
            }
            break;
        }
        while !matches!(self.peek(), Tok::Eof) {
            match self.type_decl() {
                Ok(t) => unit.types.extend(t),
                Err(e) => {
                    self.problems.push(e);
                    // Skip to the next `;` or the end of a balanced body.
                    let mut depth = 0i32;
                    while !matches!(self.peek(), Tok::Eof) {
                        match self.peek() {
                            Tok::Sym(s) if s == "{" => depth += 1,
                            Tok::Sym(s) if s == "}" => {
                                depth -= 1;
                                if depth <= 0 {
                                    self.bump();
                                    break;
                                }
                            }
                            Tok::Sym(s) if s == ";" && depth == 0 => {
                                self.bump();
                                break;
                            }
                            _ => {}
                        }
                        if depth > 0 && self.at + 1 >= self.toks.len() {
                            break;
                        }
                        let before = self.at;
                        self.bump();
                        if self.at == before {
                            break;
                        }
                    }
                }
            }
        }
        unit
    }

    fn type_decl(&mut self) -> Result<Vec<TypeDecl>, String> {
        let (mods, annotations) = self.modifiers();
        let kind = match self.peek().clone() {
            Tok::Ident(k)
                if matches!(
                    k.as_str(),
                    "class" | "interface" | "enum" | "record" | "@interface"
                ) =>
            {
                self.bump();
                k
            }
            Tok::Sym(s) if s == "@" => {
                self.bump();
                self.ident("interface");
                "@interface".to_owned()
            }
            other => {
                return Err(format!(
                    "line {}: expected a type, found {other:?}",
                    self.line()
                ));
            }
        };
        let name = self.expect_ident()?;
        let mut td = TypeDecl {
            name,
            kind: kind.clone(),
            modifiers: mods,
            annotations,
            ..Default::default()
        };
        if self.ident("extends") {
            loop {
                td.extends.push(self.ty()?);
                if !self.sym(",") {
                    break;
                }
            }
        }
        if self.ident("implements") {
            loop {
                td.implements.push(self.ty()?);
                if !self.sym(",") {
                    break;
                }
            }
        }
        // A record's components.
        if kind == "record" && matches!(self.peek(), Tok::Sym(s) if s == "(") {
            let params = self.params()?;
            for (ty, name) in params {
                td.fields.push(Field {
                    name,
                    ty,
                    ..Default::default()
                });
            }
        }
        self.expect_sym("{")?;
        // An enum's constants, which are not members.
        if kind == "enum" {
            while !matches!(self.peek(), Tok::Sym(s) if s == "}" || s == ";")
                && !matches!(self.peek(), Tok::Eof)
            {
                if matches!(self.peek(), Tok::Sym(s) if s == "(") {
                    self.skip_balanced("(", ")");
                    continue;
                }
                self.bump();
            }
            self.sym(";");
        }
        while !matches!(self.peek(), Tok::Sym(s) if s == "}") && !matches!(self.peek(), Tok::Eof) {
            if self.sym(";") {
                continue;
            }
            let before = self.at;
            match self.member(&mut td) {
                Ok(()) => {}
                Err(e) => {
                    self.problems.push(format!("{}.{}: {e}", td.name, "?"));
                    self.recover_member();
                }
            }
            if self.at == before {
                self.bump();
            }
        }
        self.sym("}");
        Ok(vec![td])
    }

    fn recover_member(&mut self) {
        let mut depth = 0i32;
        while !matches!(self.peek(), Tok::Eof) {
            match self.peek() {
                Tok::Sym(s) if s == "{" => depth += 1,
                Tok::Sym(s) if s == "}" => {
                    if depth == 0 {
                        return;
                    }
                    depth -= 1;
                    if depth == 0 {
                        self.bump();
                        return;
                    }
                }
                Tok::Sym(s) if s == ";" && depth == 0 => {
                    self.bump();
                    return;
                }
                _ => {}
            }
            self.bump();
        }
    }

    fn params(&mut self) -> Result<Vec<(String, String)>, String> {
        self.expect_sym("(")?;
        let mut out = Vec::new();
        while !matches!(self.peek(), Tok::Sym(s) if s == ")") && !matches!(self.peek(), Tok::Eof) {
            let (mods, _ann) = self.modifiers();
            let _ = mods;
            let ty = self.ty()?;
            let name = match self.peek().clone() {
                Tok::Ident(n) => {
                    self.bump();
                    n
                }
                _ => "arg".to_owned(),
            };
            // A trailing `[]` after the name (`int args[]`).
            let mut ty = ty;
            while self.sym("[") {
                self.sym("]");
                ty.push_str("[]");
            }
            out.push((ty, name));
            if !self.sym(",") {
                break;
            }
        }
        self.expect_sym(")")?;
        Ok(out)
    }

    fn member(&mut self, td: &mut TypeDecl) -> Result<(), String> {
        let (mods, annotations) = self.modifiers();
        // A nested type.
        if matches!(self.peek(), Tok::Ident(k) if matches!(k.as_str(), "class" | "interface" | "enum" | "record"))
            || matches!(self.peek(), Tok::Sym(s) if s == "@")
        {
            let mut nested = self.type_decl()?;
            td.nested.append(&mut nested);
            return Ok(());
        }
        // An initializer block: `static { ... }` or `{ ... }`.
        if matches!(self.peek(), Tok::Sym(s) if s == "{") {
            let body = self.block()?;
            let m = Method {
                name: if mods.iter().any(|m| m == "static") {
                    "static_init".into()
                } else {
                    "init".into()
                },
                ret: String::new(),
                params: Vec::new(),
                body: Some(body),
                modifiers: mods,
                annotations,
            };
            td.methods.push(m);
            return Ok(());
        }
        // A constructor: `Name(...)`.
        if matches!(self.peek(), Tok::Ident(n) if *n == td.name)
            && matches!(self.toks.get(self.at + 1).map(|t| &t.tok), Some(Tok::Sym(s)) if s == "(")
        {
            let name = self.bump();
            let Tok::Ident(name) = name else {
                unreachable!()
            };
            let params = self.params()?;
            while self.sym("[") {
                self.sym("]");
            }
            let body = if matches!(self.peek(), Tok::Sym(s) if s == "{") {
                Some(self.block()?)
            } else {
                self.expect_sym(";")?;
                None
            };
            td.methods.push(Method {
                name: "new".into(),
                ret: name,
                params,
                body,
                modifiers: mods,
                annotations,
            });
            return Ok(());
        }
        let ty = self.ty()?;
        let name = self.expect_ident()?;
        // A method.
        if matches!(self.peek(), Tok::Sym(s) if s == "(") {
            let params = self.params()?;
            while self.sym("[") {
                self.sym("]");
            }
            if self.ident("throws") {
                loop {
                    let _ = self.ty()?;
                    if !self.sym(",") {
                        break;
                    }
                }
            }
            let body = if matches!(self.peek(), Tok::Sym(s) if s == "{") {
                Some(self.block()?)
            } else {
                self.expect_sym(";")?;
                None
            };
            td.methods.push(Method {
                name,
                ret: ty,
                params,
                body,
                modifiers: mods,
                annotations,
            });
            return Ok(());
        }
        // One or more fields: `int a = 1, b = 2;`.
        let mut current = name;
        loop {
            while self.sym("[") {
                self.sym("]");
            }
            let init = if self.sym("=") {
                Some(self.expr()?)
            } else {
                None
            };
            td.fields.push(Field {
                name: current.clone(),
                ty: ty.clone(),
                init,
                modifiers: mods.clone(),
                annotations: annotations.clone(),
            });
            if self.sym(",") {
                current = self.expect_ident()?;
                continue;
            }
            break;
        }
        self.expect_sym(";")?;
        Ok(())
    }

    fn block(&mut self) -> Result<Vec<Stmt>, String> {
        self.expect_sym("{")?;
        let mut out = Vec::new();
        while !matches!(self.peek(), Tok::Sym(s) if s == "}") && !matches!(self.peek(), Tok::Eof) {
            let before = self.at;
            match self.stmt() {
                Ok(s) => out.push(s),
                Err(e) => {
                    self.problems.push(e);
                    self.recover_stmt();
                }
            }
            if self.at == before {
                self.bump();
            }
        }
        self.sym("}");
        Ok(out)
    }

    fn recover_stmt(&mut self) {
        let mut depth = 0i32;
        while !matches!(self.peek(), Tok::Eof) {
            match self.peek() {
                Tok::Sym(s) if s == "{" => depth += 1,
                Tok::Sym(s) if s == "}" => {
                    if depth == 0 {
                        return;
                    }
                    depth -= 1;
                }
                Tok::Sym(s) if s == ";" && depth == 0 => {
                    self.bump();
                    return;
                }
                _ => {}
            }
            self.bump();
        }
    }

    fn stmt(&mut self) -> Result<Stmt, String> {
        if matches!(self.peek(), Tok::Sym(s) if s == "{") {
            return Ok(Stmt::Block(self.block()?));
        }
        if self.sym(";") {
            return Ok(Stmt::Empty);
        }
        if self.ident("if") {
            self.expect_sym("(")?;
            let cond = self.expr()?;
            self.expect_sym(")")?;
            let then = Box::new(self.stmt()?);
            let otherwise = if self.ident("else") {
                Some(Box::new(self.stmt()?))
            } else {
                None
            };
            return Ok(Stmt::If(cond, then, otherwise));
        }
        if self.ident("while") {
            self.expect_sym("(")?;
            let cond = self.expr()?;
            self.expect_sym(")")?;
            return Ok(Stmt::While(cond, Box::new(self.stmt()?)));
        }
        if self.ident("do") {
            let body = Box::new(self.stmt()?);
            self.ident("while");
            self.expect_sym("(")?;
            let cond = self.expr()?;
            self.expect_sym(")")?;
            self.sym(";");
            return Ok(Stmt::DoWhile(body, cond));
        }
        if self.ident("for") {
            self.expect_sym("(")?;
            // `for (Type name : expr)`.
            let save = self.at;
            let foreach = (|| -> Result<Option<Stmt>, String> {
                let (mods, _) = self.modifiers();
                let _ = mods;
                if matches!(self.peek(), Tok::Sym(s) if s == ";") {
                    return Ok(None);
                }
                let ty = self.ty()?;
                let name = self.expect_ident()?;
                if !self.sym(":") {
                    return Err(String::new());
                }
                let iter = self.expr()?;
                self.expect_sym(")")?;
                let body = Box::new(self.stmt()?);
                Ok(Some(Stmt::ForEach(ty, name, iter, body)))
            })();
            match foreach {
                Ok(Some(s)) => return Ok(s),
                Ok(None) => {
                    self.at = save;
                    return self.classic_for();
                }
                Err(e) if e.is_empty() => {
                    self.at = save;
                    return self.classic_for();
                }
                Err(e) => return Err(e),
            }
        }
        if self.ident("return") {
            if self.sym(";") {
                return Ok(Stmt::Return(None));
            }
            let e = self.expr()?;
            self.expect_sym(";")?;
            return Ok(Stmt::Return(Some(e)));
        }
        if self.ident("break") {
            let _ = self.expect_ident();
            self.expect_sym(";")?;
            return Ok(Stmt::Break);
        }
        if self.ident("continue") {
            self.expect_sym(";")?;
            return Ok(Stmt::Continue);
        }
        if self.ident("throw") {
            let e = self.expr()?;
            self.expect_sym(";")?;
            return Ok(Stmt::Throw(e));
        }
        if self.ident("switch") {
            self.expect_sym("(")?;
            let subject = self.expr()?;
            self.expect_sym(")")?;
            self.expect_sym("{")?;
            let mut arms: Vec<(Option<Expr>, Vec<Stmt>)> = Vec::new();
            let mut current: Option<Option<Expr>> = None;
            while !matches!(self.peek(), Tok::Sym(s) if s == "}")
                && !matches!(self.peek(), Tok::Eof)
            {
                if self.ident("case") {
                    let e = self.expr()?;
                    self.expect_sym(":")?;
                    let _ = self.sym("->");
                    if let Some(prev) = current.take() {
                        arms.push((prev, Vec::new()));
                    }
                    current = Some(Some(e));
                    continue;
                }
                if self.ident("default") {
                    self.expect_sym(":")?;
                    let _ = self.sym("->");
                    if let Some(prev) = current.take() {
                        arms.push((prev, Vec::new()));
                    }
                    current = Some(None);
                    continue;
                }
                let s = self.stmt()?;
                match current.as_mut() {
                    Some(arm)
                        if arms
                            .last()
                            .map(|a| a.0.is_none() && a.1.is_empty())
                            .unwrap_or(false) =>
                    {
                        let _ = arm;
                        arms.push((None, vec![s]));
                        current = Some(None);
                    }
                    None => arms.push((None, vec![s])),
                    Some(_) => {}
                }
            }
            self.sym("}");
            return Ok(Stmt::Switch(subject, arms));
        }
        if self.ident("try") {
            let body = self.block()?;
            let mut catches = Vec::new();
            while self.ident("catch") {
                self.expect_sym("(")?;
                let (mods, _) = self.modifiers();
                let _ = mods;
                let mut ty = self.ty()?;
                while self.sym("|") {
                    if let Ok(t) = self.ty() {
                        ty = t;
                    }
                }
                let name = self.expect_ident()?;
                self.expect_sym(")")?;
                catches.push((ty, name, self.block()?));
            }
            let finally = if self.ident("finally") {
                Some(self.block()?)
            } else {
                None
            };
            return Ok(Stmt::Try(body, catches, finally));
        }
        if self.ident("synchronized") {
            self.expect_sym("(")?;
            let lock = self.expr()?;
            self.expect_sym(")")?;
            return Ok(Stmt::Synchronized(lock, Box::new(self.stmt()?)));
        }
        if self.ident("assert") {
            let e = self.expr()?;
            if self.sym(":") {
                let _ = self.expr()?;
            }
            self.expect_sym(";")?;
            return Ok(Stmt::Expr(e));
        }
        // A label, then its statement.
        if let Tok::Ident(_) = self.peek()
            && matches!(self.toks.get(self.at + 1).map(|t| &t.tok), Some(Tok::Sym(s)) if s == ":")
        {
            self.bump();
            self.bump();
            return self.stmt();
        }
        // A local declaration, else an expression statement.
        let save = self.at;
        if let Ok(Some(s)) = self.local_decl() {
            return Ok(s);
        }
        self.at = save;
        let e = self.expr()?;
        self.expect_sym(";")?;
        Ok(Stmt::Expr(e))
    }

    fn local_decl(&mut self) -> Result<Option<Stmt>, String> {
        let (mods, _) = self.modifiers();
        let ty = self.ty()?;
        if !matches!(self.peek(), Tok::Ident(_)) {
            return Ok(None);
        }
        let name = self.expect_ident()?;
        // A declaration list (`int a = 1, b;`): the first is the statement, the
        // rest become their own declarations.
        let init = if self.sym("=") {
            Some(self.expr()?)
        } else {
            None
        };
        if self.sym(",") {
            // Give up on the list form and keep the first.
            while !matches!(self.peek(), Tok::Sym(s) if s == ";")
                && !matches!(self.peek(), Tok::Eof)
            {
                self.bump();
            }
        }
        self.expect_sym(";")?;
        let _ = mods;
        Ok(Some(Stmt::Local(ty, name, init)))
    }

    fn classic_for(&mut self) -> Result<Stmt, String> {
        let init = if self.sym(";") {
            None
        } else {
            let save = self.at;
            match self.local_decl()? {
                Some(s) => Some(Box::new(s)),
                None => {
                    self.at = save;
                    let e = self.expr()?;
                    self.expect_sym(";")?;
                    Some(Box::new(Stmt::Expr(e)))
                }
            }
        };
        let cond = if matches!(self.peek(), Tok::Sym(s) if s == ";") {
            None
        } else {
            Some(self.expr()?)
        };
        self.expect_sym(";")?;
        let update = if matches!(self.peek(), Tok::Sym(s) if s == ")") {
            None
        } else {
            Some(self.expr()?)
        };
        self.expect_sym(")")?;
        let body = Box::new(self.stmt()?);
        Ok(Stmt::For(init, cond, update, body))
    }

    pub fn expr(&mut self) -> Result<Expr, String> {
        self.assignment()
    }

    fn assignment(&mut self) -> Result<Expr, String> {
        let left = self.ternary()?;
        const OPS: [&str; 11] = [
            "=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=",
        ];
        for op in OPS {
            if self.sym(op) {
                let right = self.assignment()?;
                return Ok(Expr::Assign(op.to_owned(), Box::new(left), Box::new(right)));
            }
        }
        Ok(left)
    }

    fn ternary(&mut self) -> Result<Expr, String> {
        let cond = self.binary(0)?;
        if self.sym("?") {
            let a = self.assignment()?;
            self.expect_sym(":")?;
            let b = self.assignment()?;
            return Ok(Expr::Ternary(Box::new(cond), Box::new(a), Box::new(b)));
        }
        Ok(cond)
    }

    const LEVELS: [&'static [&'static str]; 10] = [
        &["||"],
        &["&&"],
        &["|"],
        &["^"],
        &["&"],
        &["==", "!="],
        &["<", ">", "<=", ">=", "instanceof"],
        &["<<", ">>", ">>>"],
        &["+", "-"],
        &["*", "/", "%"],
    ];

    fn binary(&mut self, level: usize) -> Result<Expr, String> {
        if level >= Self::LEVELS.len() {
            return self.unary();
        }
        let mut left = self.binary(level + 1)?;
        loop {
            let op = match self.peek() {
                Tok::Sym(s) if Self::LEVELS[level].contains(&s.as_str()) => s.clone(),
                Tok::Ident(s) if s == "instanceof" => s.clone(),
                _ => break,
            };
            self.bump();
            if op == "instanceof" {
                let ty = self.ty()?;
                // Java 16 pattern binding: `x instanceof Player player`.
                let name = match self.peek().clone() {
                    Tok::Ident(n) => {
                        self.bump();
                        Some(n)
                    }
                    _ => None,
                };
                left = Expr::InstanceOf(Box::new(left), ty, name);
                continue;
            }
            let right = self.binary(level + 1)?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.sym("!")
            || self.sym("~")
            || self.sym("-")
            || self.sym("+")
            || self.sym("++")
            || self.sym("--")
        {
            let op = match &self.toks[self.at - 1].tok {
                Tok::Sym(s) => s.clone(),
                _ => unreachable!(),
            };
            let e = self.unary()?;
            return Ok(Expr::Unary(op, Box::new(e)));
        }
        // A cast: `(Type) expr`, only when the parentheses hold a type.
        if matches!(self.peek(), Tok::Sym(s) if s == "(") {
            let save = self.at;
            self.bump();
            let cast = (|| -> Option<String> {
                let ty = self.ty().ok()?;
                if !matches!(self.peek(), Tok::Sym(s) if s == ")") {
                    return None;
                }
                if !looks_like_type(&ty) {
                    return None;
                }
                self.bump();
                Some(ty)
            })();
            match cast {
                Some(ty) => {
                    let e = self.unary()?;
                    return Ok(Expr::Cast(ty, Box::new(e)));
                }
                None => self.at = save,
            }
            // A parenthesised lambda: `(a, b) -> ...`.
            if let Some(lambda) = self.try_lambda() {
                return Ok(lambda);
            }
        }
        // `name -> ...`
        if let Tok::Ident(_) = self.peek()
            && matches!(self.toks.get(self.at + 1).map(|t| &t.tok), Some(Tok::Sym(s)) if s == "->")
        {
            let name = self.expect_ident()?;
            self.sym("->");
            let body = if matches!(self.peek(), Tok::Sym(s) if s == "{") {
                Expr::Block(self.block()?)
            } else {
                self.expr()?
            };
            return Ok(Expr::Lambda(vec![(String::new(), name)], Box::new(body)));
        }
        self.postfix()
    }

    fn try_lambda(&mut self) -> Option<Expr> {
        let save = self.at;
        if !self.sym("(") {
            return None;
        }
        let mut params = Vec::new();
        loop {
            if matches!(self.peek(), Tok::Sym(s) if s == ")") {
                break;
            }
            // `Type name` or just `name`.
            let mut parts = Vec::new();
            while !matches!(self.peek(), Tok::Sym(s) if s == "," || s == ")")
                && !matches!(self.peek(), Tok::Eof)
            {
                parts.push(self.bump());
            }
            let name = match parts.last() {
                Some(Tok::Ident(n)) => n.clone(),
                _ => {
                    self.at = save;
                    return None;
                }
            };
            let ty = if parts.len() > 1 {
                parts[..parts.len() - 1]
                    .iter()
                    .filter_map(|t| match t {
                        Tok::Ident(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                String::new()
            };
            params.push((ty, name));
            if !self.sym(",") {
                break;
            }
        }
        if !self.sym(")") || !self.sym("->") {
            self.at = save;
            return None;
        }
        let body = if matches!(self.peek(), Tok::Sym(s) if s == "{") {
            match self.block() {
                Ok(b) => Expr::Block(b),
                Err(_) => {
                    self.at = save;
                    return None;
                }
            }
        } else {
            match self.expr() {
                Ok(e) => e,
                Err(_) => {
                    self.at = save;
                    return None;
                }
            }
        };
        Some(Expr::Lambda(params, Box::new(body)))
    }

    fn postfix(&mut self) -> Result<Expr, String> {
        let mut e = self.primary()?;
        loop {
            if self.sym(".") {
                // A method reference: `Type::method` is `::`, and `.` always
                // names a field or method.
                if self.sym("<") {
                    // Explicit generic call: `.<T>method`.
                    let _ = self.skip_balanced("<", ">");
                }
                let name = match self.peek().clone() {
                    Tok::Ident(n) => {
                        self.bump();
                        n
                    }
                    Tok::Sym(s) if s == "class" => {
                        self.bump();
                        if let Expr::Ident(t) = &e {
                            e = Expr::ClassLit(t.clone());
                        }
                        continue;
                    }
                    other => {
                        return Err(format!(
                            "line {}: expected a member, found {other:?}",
                            self.line()
                        ));
                    }
                };
                if matches!(self.peek(), Tok::Sym(s) if s == "(") {
                    let args = self.args()?;
                    e = Expr::Call(Some(Box::new(e)), name, args);
                } else {
                    e = Expr::Field(Box::new(e), name);
                }
                continue;
            }
            if self.sym("::") {
                let name = self.expect_ident().unwrap_or_default();
                e = Expr::MethodRef(Box::new(e), name);
                continue;
            }
            if self.sym("[") {
                let idx = self.expr()?;
                self.expect_sym("]")?;
                e = Expr::Index(Box::new(e), Box::new(idx));
                continue;
            }
            if self.sym("++") {
                e = Expr::Postfix("++".into(), Box::new(e));
                continue;
            }
            if self.sym("--") {
                e = Expr::Postfix("--".into(), Box::new(e));
                continue;
            }
            break;
        }
        Ok(e)
    }

    fn args(&mut self) -> Result<Vec<Expr>, String> {
        self.expect_sym("(")?;
        let mut out = Vec::new();
        while !matches!(self.peek(), Tok::Sym(s) if s == ")") && !matches!(self.peek(), Tok::Eof) {
            out.push(self.expr()?);
            if !self.sym(",") {
                break;
            }
        }
        self.expect_sym(")")?;
        Ok(out)
    }

    fn primary(&mut self) -> Result<Expr, String> {
        match self.peek().clone() {
            Tok::Num(n) => {
                self.bump();
                if n.contains('.')
                    || n.ends_with('f')
                    || n.ends_with('F')
                    || n.ends_with('d')
                    || n.ends_with('D')
                {
                    Ok(Expr::Lit(Lit::Float(n)))
                } else {
                    Ok(Expr::Lit(Lit::Int(n)))
                }
            }
            Tok::Str(s) => {
                self.bump();
                Ok(Expr::Lit(Lit::Str(s)))
            }
            Tok::Chr(c) => {
                self.bump();
                Ok(Expr::Lit(Lit::Char(c)))
            }
            Tok::Ident(s) => {
                self.bump();
                match s.as_str() {
                    "true" => return Ok(Expr::Lit(Lit::Bool(true))),
                    "false" => return Ok(Expr::Lit(Lit::Bool(false))),
                    "null" => return Ok(Expr::Lit(Lit::Null)),
                    "this" => return Ok(Expr::This),
                    "super" => return Ok(Expr::Ident("super".into())),
                    "new" => {
                        // `new Type(...)` or `new Type[]{...}`.
                        let ty = self.ty()?;
                        if matches!(self.peek(), Tok::Sym(s) if s == "{") {
                            let b = self.array_init()?;
                            return Ok(Expr::NewArray(ty, b));
                        }
                        let args = self.args()?;
                        // An anonymous class body after `new`: skip it.
                        if matches!(self.peek(), Tok::Sym(s) if s == "{") {
                            let _ = self.skip_balanced("{", "}");
                        }
                        return Ok(Expr::New(ty, args));
                    }
                    _ => {}
                }
                if matches!(self.peek(), Tok::Sym(s) if s == "(") {
                    let args = self.args()?;
                    return Ok(Expr::Call(None, s, args));
                }
                Ok(Expr::Ident(s))
            }
            Tok::Sym(s) if s == "(" => {
                self.bump();
                let e = self.expr()?;
                self.expect_sym(")")?;
                Ok(e)
            }
            Tok::Sym(s) if s == "{" => {
                let items = self.array_init()?;
                Ok(Expr::Array(items))
            }
            other => Err(format!("line {}: unexpected {other:?}", self.line())),
        }
    }

    fn array_init(&mut self) -> Result<Vec<Expr>, String> {
        self.expect_sym("{")?;
        let mut out = Vec::new();
        while !matches!(self.peek(), Tok::Sym(s) if s == "}") && !matches!(self.peek(), Tok::Eof) {
            out.push(self.expr()?);
            if !self.sym(",") {
                break;
            }
        }
        self.expect_sym("}")?;
        Ok(out)
    }
}

/// Whether a parsed type text is a type (for telling a cast from a group).
fn looks_like_type(ty: &str) -> bool {
    let first = ty.split('<').next().unwrap_or(ty).trim();
    const PRIMS: [&str; 8] = [
        "byte", "short", "char", "int", "long", "float", "double", "boolean",
    ];
    if PRIMS.contains(&first) {
        return true;
    }
    // A capitalised or dotted name, not an expression.
    first
        .chars()
        .next()
        .map(|c| c.is_ascii_uppercase())
        .unwrap_or(false)
        && first
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '_' || c == '$')
}
