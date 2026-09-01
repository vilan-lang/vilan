//! A declaration-only TypeScript (`.d.ts`) parser.
//!
//! `.d.ts` is a small, highly regular fragment of TypeScript: declarations and
//! type expressions, no statements, no function bodies, no control flow. That
//! is the whole grammar this module implements — enough for
//! [`crate::bindgen`] to see the shapes it maps, and nothing more.
//!
//! Two properties matter more than completeness:
//!
//! - **It never gives up on a file.** Anything this parser cannot read is
//!   captured verbatim as a [`Declaration::Unsupported`] / [`TsType::Unsupported`]
//!   carrying the construct's name and its raw source text, and parsing
//!   continues at the next declaration. bindgen's central invariant — an
//!   unmappable construct never disappears silently — starts here: a parse
//!   failure is data, not an abort.
//! - **It is syntax only.** No type resolution, no cross-file imports, no
//!   declaration merging. `proposal/bindgen.md` §2 scopes bindgen to shapes a
//!   parser can see, and §5 puts the semantic constructs out of v1 entirely.
//!
//! Deliberately NOT a general TypeScript parser: there are no expressions here
//! beyond literal types, because a `.d.ts` has none.

use std::fmt::Write as _;

// --- Tokens ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// An identifier or keyword. `.d.ts` keywords are all contextual, so they
    /// are lexed as identifiers and matched by text at the parser.
    Identifier(String),
    /// A string literal, with its quotes removed and escapes left as written
    /// (bindgen only ever compares or re-emits these, never evaluates them).
    String(String),
    /// A numeric literal, verbatim.
    Number(String),
    /// A template literal, verbatim including backticks — only ever a
    /// template-literal *type*, which v1 does not map.
    Template(String),
    /// Punctuation. Only `=>` and `...` are multi-character: keeping `>` a
    /// single token is what makes nested generics (`Foo<Bar<T>>`) parse without
    /// any token-splitting hack.
    Punctuation(&'static str),
    EndOfFile,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    /// Byte range in the source, for slicing raw text back out.
    pub start: usize,
    pub end: usize,
}

/// The multi-character punctuation the type grammar needs. Everything else is
/// lexed one character at a time.
const MULTI_CHARACTER_PUNCTUATION: [&str; 2] = ["...", "=>"];

const SINGLE_CHARACTER_PUNCTUATION: [&str; 18] = [
    "{", "}", "(", ")", "[", "]", "<", ">", ",", ";", ":", "?", "=", "|", "&", ".", "+", "-",
];

fn is_identifier_start(character: char) -> bool {
    character.is_alphabetic() || character == '_' || character == '$' || character == '#'
}

fn is_identifier_continue(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '$'
}

/// Lexes `source` into tokens, discarding whitespace and comments. Anything
/// unrecognized is skipped one character at a time rather than failing — the
/// parser's recovery is the only error path this module has.
pub fn tokenize(source: &str) -> Vec<Token> {
    let bytes: Vec<char> = source.chars().collect();
    // Character index -> byte offset, so spans slice `source` correctly even
    // through the non-ASCII text real `.d.ts` files carry in comments.
    let mut byte_offsets = Vec::with_capacity(bytes.len() + 1);
    let mut offset = 0;
    for character in &bytes {
        byte_offsets.push(offset);
        offset += character.len_utf8();
    }
    byte_offsets.push(offset);

    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let character = bytes[index];
        if character.is_whitespace() {
            index += 1;
            continue;
        }
        // Comments. A `///` triple-slash directive is just a line comment here.
        if character == '/' && bytes.get(index + 1) == Some(&'/') {
            while index < bytes.len() && bytes[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if character == '/' && bytes.get(index + 1) == Some(&'*') {
            index += 2;
            while index < bytes.len()
                && !(bytes[index] == '*' && bytes.get(index + 1) == Some(&'/'))
            {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
            continue;
        }
        let start = index;
        if character == '"' || character == '\'' {
            let quote = character;
            index += 1;
            let value_start = index;
            while index < bytes.len() && bytes[index] != quote {
                if bytes[index] == '\\' {
                    index += 1;
                }
                index += 1;
            }
            let value: String = bytes[value_start..index.min(bytes.len())].iter().collect();
            index = (index + 1).min(bytes.len());
            tokens.push(Token {
                kind: TokenKind::String(value),
                start: byte_offsets[start],
                end: byte_offsets[index],
            });
            continue;
        }
        if character == '`' {
            index += 1;
            let mut depth = 0;
            while index < bytes.len() {
                match bytes[index] {
                    '\\' => index += 1,
                    '$' if bytes.get(index + 1) == Some(&'{') => {
                        depth += 1;
                        index += 1;
                    }
                    '}' if depth > 0 => depth -= 1,
                    '`' if depth == 0 => break,
                    _ => {}
                }
                index += 1;
            }
            index = (index + 1).min(bytes.len());
            let raw: String = bytes[start..index].iter().collect();
            tokens.push(Token {
                kind: TokenKind::Template(raw),
                start: byte_offsets[start],
                end: byte_offsets[index],
            });
            continue;
        }
        if character.is_ascii_digit() {
            while index < bytes.len()
                && (bytes[index].is_alphanumeric() || bytes[index] == '.' || bytes[index] == '_')
            {
                index += 1;
            }
            let raw: String = bytes[start..index].iter().collect();
            tokens.push(Token {
                kind: TokenKind::Number(raw),
                start: byte_offsets[start],
                end: byte_offsets[index],
            });
            continue;
        }
        if is_identifier_start(character) {
            index += 1;
            while index < bytes.len() && is_identifier_continue(bytes[index]) {
                index += 1;
            }
            let raw: String = bytes[start..index].iter().collect();
            tokens.push(Token {
                kind: TokenKind::Identifier(raw),
                start: byte_offsets[start],
                end: byte_offsets[index],
            });
            continue;
        }
        let rest: String = bytes[index..(index + 3).min(bytes.len())].iter().collect();
        if let Some(punctuation) = MULTI_CHARACTER_PUNCTUATION
            .iter()
            .find(|candidate| rest.starts_with(**candidate))
        {
            index += punctuation.chars().count();
            tokens.push(Token {
                kind: TokenKind::Punctuation(punctuation),
                start: byte_offsets[start],
                end: byte_offsets[index],
            });
            continue;
        }
        if let Some(punctuation) = SINGLE_CHARACTER_PUNCTUATION
            .iter()
            .find(|candidate| candidate.starts_with(character))
        {
            index += 1;
            tokens.push(Token {
                kind: TokenKind::Punctuation(punctuation),
                start: byte_offsets[start],
                end: byte_offsets[index],
            });
            continue;
        }
        // Anything else (`*`, `@`, `!`, `/` outside a comment) is not part of
        // the declaration grammar; skip it rather than stopping.
        index += 1;
    }
    tokens.push(Token {
        kind: TokenKind::EndOfFile,
        start: byte_offsets[bytes.len()],
        end: byte_offsets[bytes.len()],
    });
    tokens
}

// --- The declaration AST -----------------------------------------------------

/// A whole `.d.ts` file, as a flat list of declarations in source order.
#[derive(Debug, Default)]
pub struct DeclarationFile {
    pub declarations: Vec<Declaration>,
}

#[derive(Debug)]
pub enum Declaration {
    Interface(InterfaceDeclaration),
    Class(ClassDeclaration),
    Function(Signature),
    TypeAlias(TypeAliasDeclaration),
    /// `declare const/let/var x: T` — a host *value*, not a callable.
    Variable(VariableDeclaration),
    /// `declare namespace N { … }` / `declare module "m" { … }` / `declare
    /// global { … }` — recognized so bindgen can name what it skipped (§5).
    Unsupported(UnsupportedDeclaration),
}

#[derive(Debug)]
pub struct UnsupportedDeclaration {
    /// The construct class, for the TODO comment and the coverage stats.
    pub construct: &'static str,
    /// A name when one was readable (`"Foo"` for `namespace Foo`), else empty.
    pub name: String,
    /// The declaration's first line, verbatim — enough to recognize it.
    pub raw: String,
}

#[derive(Debug)]
pub struct InterfaceDeclaration {
    pub name: String,
    pub generics: Vec<GenericParameter>,
    pub extends: Vec<TsType>,
    pub members: Vec<Member>,
}

#[derive(Debug)]
pub struct ClassDeclaration {
    pub name: String,
    pub generics: Vec<GenericParameter>,
    pub extends: Vec<TsType>,
    pub implements: Vec<TsType>,
    pub members: Vec<Member>,
}

#[derive(Debug)]
pub struct TypeAliasDeclaration {
    pub name: String,
    pub generics: Vec<GenericParameter>,
    pub value: TsType,
}

#[derive(Debug)]
pub struct VariableDeclaration {
    pub name: String,
    pub declared_type: Option<TsType>,
    pub raw: String,
}

#[derive(Debug, Clone)]
pub struct GenericParameter {
    pub name: String,
    pub constraint: Option<TsType>,
    pub default: Option<TsType>,
}

/// One function/method/constructor signature.
#[derive(Debug, Clone)]
pub struct Signature {
    pub name: String,
    pub generics: Vec<GenericParameter>,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TsType>,
    /// The signature's source text, verbatim — what an overload TODO quotes.
    pub raw: String,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub optional: bool,
    pub rest: bool,
    pub declared_type: Option<TsType>,
}

/// A member of an interface, class, or object type.
#[derive(Debug, Clone)]
pub enum Member {
    Property(PropertyMember),
    Method(MethodMember),
    /// `new (…): T` in an interface, or `constructor(…)` in a class.
    Construct(Signature),
    /// A bare `(…): T` call signature on an interface (a callable object).
    Call(Signature),
    Index(IndexMember),
    Unsupported {
        construct: &'static str,
        raw: String,
    },
}

#[derive(Debug, Clone)]
pub struct PropertyMember {
    pub name: String,
    pub optional: bool,
    pub is_static: bool,
    /// `readonly`, or a `get` accessor with no matching `set`.
    pub readable: bool,
    pub writable: bool,
    pub declared_type: Option<TsType>,
    pub raw: String,
}

#[derive(Debug, Clone)]
pub struct MethodMember {
    pub is_static: bool,
    pub optional: bool,
    pub signature: Signature,
}

#[derive(Debug, Clone)]
pub struct IndexMember {
    pub key: IndexKey,
    pub value: TsType,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexKey {
    String,
    Number,
    Other,
}

/// A TypeScript type expression.
#[derive(Debug, Clone)]
pub enum TsType {
    /// A named type with optional arguments: `string`, `Foo`, `Array<T>`,
    /// `Foo.Bar` (the dotted name is kept whole).
    Reference {
        name: String,
        arguments: Vec<TsType>,
    },
    Array(Box<TsType>),
    Tuple(Vec<TsType>),
    Union(Vec<TsType>),
    Intersection(Vec<TsType>),
    Function(Box<Signature>),
    Constructor(Box<Signature>),
    Object(Vec<Member>),
    StringLiteral(String),
    NumberLiteral(String),
    BooleanLiteral(bool),
    /// A construct recognized but deliberately not modeled: conditional and
    /// mapped types, `keyof`, `typeof`, indexed access, template-literal types,
    /// `infer`, `import(…)`. Carries the class name and the raw text so the
    /// emitter can TODO it precisely (§3.11).
    Unsupported {
        construct: &'static str,
        raw: String,
    },
}

impl TsType {
    /// The type's own construct class, for coverage accounting.
    pub fn construct(&self) -> &'static str {
        match self {
            TsType::Reference { .. } => "type reference",
            TsType::Array(_) => "array type",
            TsType::Tuple(_) => "tuple type",
            TsType::Union(_) => "union type",
            TsType::Intersection(_) => "intersection type",
            TsType::Function(_) => "function type",
            TsType::Constructor(_) => "constructor type",
            TsType::Object(_) => "object type",
            TsType::StringLiteral(_) => "string literal type",
            TsType::NumberLiteral(_) => "number literal type",
            TsType::BooleanLiteral(_) => "boolean literal type",
            TsType::Unsupported { construct, .. } => construct,
        }
    }
}

// --- The parser --------------------------------------------------------------

struct Parser<'source> {
    source: &'source str,
    tokens: Vec<Token>,
    position: usize,
}

/// Modifiers that may precede a declaration or a member, and are otherwise
/// uninteresting to bindgen.
const IGNORED_MODIFIERS: [&str; 9] = [
    "export",
    "declare",
    "default",
    "abstract",
    "public",
    "protected",
    "override",
    "async",
    "accessor",
];

pub fn parse(source: &str) -> DeclarationFile {
    let mut parser = Parser {
        source,
        tokens: tokenize(source),
        position: 0,
    };
    let mut file = DeclarationFile::default();
    while !parser.at_end() {
        let before = parser.position;
        if let Some(declaration) = parser.parse_declaration() {
            file.declarations.push(declaration);
        }
        // Never spin: recovery always consumes at least one token.
        if parser.position == before {
            parser.position += 1;
        }
    }
    file
}

impl<'source> Parser<'source> {
    fn at_end(&self) -> bool {
        matches!(self.peek().kind, TokenKind::EndOfFile)
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.position.min(self.tokens.len() - 1)]
    }

    fn peek_at(&self, offset: usize) -> &Token {
        &self.tokens[(self.position + offset).min(self.tokens.len() - 1)]
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.position.min(self.tokens.len() - 1)].clone();
        if self.position < self.tokens.len() - 1 {
            self.position += 1;
        }
        token
    }

    fn is_punctuation(&self, punctuation: &str) -> bool {
        matches!(&self.peek().kind, TokenKind::Punctuation(found) if *found == punctuation)
    }

    fn is_punctuation_at(&self, offset: usize, punctuation: &str) -> bool {
        matches!(&self.peek_at(offset).kind, TokenKind::Punctuation(found) if *found == punctuation)
    }

    fn eat_punctuation(&mut self, punctuation: &str) -> bool {
        if self.is_punctuation(punctuation) {
            self.advance();
            return true;
        }
        false
    }

    fn is_keyword(&self, keyword: &str) -> bool {
        matches!(&self.peek().kind, TokenKind::Identifier(found) if found == keyword)
    }

    fn is_keyword_at(&self, offset: usize, keyword: &str) -> bool {
        matches!(&self.peek_at(offset).kind, TokenKind::Identifier(found) if found == keyword)
    }

    fn eat_keyword(&mut self, keyword: &str) -> bool {
        if self.is_keyword(keyword) {
            self.advance();
            return true;
        }
        false
    }

    fn eat_identifier(&mut self) -> Option<String> {
        match &self.peek().kind {
            TokenKind::Identifier(name) => {
                let name = name.clone();
                self.advance();
                Some(name)
            }
            _ => None,
        }
    }

    /// A member name: an identifier, a string literal (`"content-type"`), or a
    /// numeric literal (`0`).
    fn eat_member_name(&mut self) -> Option<String> {
        match &self.peek().kind {
            TokenKind::Identifier(name) => {
                let name = name.clone();
                self.advance();
                Some(name)
            }
            TokenKind::String(value) | TokenKind::Number(value) => {
                let value = value.clone();
                self.advance();
                Some(value)
            }
            _ => None,
        }
    }

    fn text_between(&self, start_token: usize, end_token: usize) -> String {
        let start = self.tokens[start_token.min(self.tokens.len() - 1)].start;
        let end = self.tokens[end_token.saturating_sub(1).min(self.tokens.len() - 1)].end;
        self.source[start..end.max(start)]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Consumes a balanced `open`…`close` run, assuming `open` is next. Used
    /// only by recovery, which is why it tolerates an unbalanced tail.
    fn skip_balanced(&mut self, open: &str, close: &str) {
        if !self.eat_punctuation(open) {
            return;
        }
        let mut depth = 1;
        while depth > 0 && !self.at_end() {
            if self.is_punctuation(open) {
                depth += 1;
            } else if self.is_punctuation(close) {
                depth -= 1;
            }
            self.advance();
        }
    }

    /// Skips to just past the end of the current declaration: either a `{…}`
    /// body or the next `;`, whichever comes first at depth 0.
    fn recover_past_declaration(&mut self) {
        loop {
            if self.at_end() {
                return;
            }
            if self.is_punctuation("{") {
                self.skip_balanced("{", "}");
                self.eat_punctuation(";");
                return;
            }
            if self.eat_punctuation(";") {
                return;
            }
            // A new declaration keyword at depth 0 means the previous one ended
            // without a terminator (ASI); stop before consuming it.
            if self.starts_declaration() {
                return;
            }
            self.advance();
        }
    }

    fn starts_declaration(&self) -> bool {
        matches!(&self.peek().kind, TokenKind::Identifier(word)
            if matches!(word.as_str(),
                "interface" | "declare" | "export" | "import" | "namespace" | "module"
                    | "class" | "function" | "type" | "enum" | "const" | "var" | "let"))
    }

    // --- Declarations ---------------------------------------------------

    fn parse_declaration(&mut self) -> Option<Declaration> {
        let start_token = self.position;

        // `import …` / `export …` statements that carry no declaration.
        if self.is_keyword("import") && !self.is_punctuation_at(1, "(") {
            self.recover_past_declaration();
            return None;
        }
        if self.is_keyword("export")
            && (self.is_punctuation_at(1, "{")
                || self.is_punctuation_at(1, "*")
                || self.is_punctuation_at(1, "="))
        {
            self.recover_past_declaration();
            return None;
        }

        while matches!(&self.peek().kind, TokenKind::Identifier(word)
            if IGNORED_MODIFIERS.contains(&word.as_str()))
        {
            // `export default class …` still declares; `declare global {` does not.
            self.advance();
        }

        if self.is_keyword("global") && self.is_punctuation_at(1, "{") {
            self.advance();
            self.skip_balanced("{", "}");
            return Some(Declaration::Unsupported(UnsupportedDeclaration {
                construct: "ambient global augmentation",
                name: String::new(),
                raw: "declare global { … }".to_string(),
            }));
        }

        if self.is_keyword("interface") {
            self.advance();
            return self.parse_interface();
        }
        if self.is_keyword("class") {
            self.advance();
            return self.parse_class();
        }
        if self.is_keyword("function") {
            self.advance();
            return self.parse_function_declaration(start_token);
        }
        if self.is_keyword("type") && matches!(self.peek_at(1).kind, TokenKind::Identifier(_)) {
            self.advance();
            return self.parse_type_alias();
        }
        if (self.is_keyword("namespace") || self.is_keyword("module"))
            && !self.is_punctuation_at(1, ":")
        {
            let keyword = if self.is_keyword("namespace") {
                "namespace"
            } else {
                "module"
            };
            self.advance();
            let name = match &self.peek().kind {
                TokenKind::Identifier(name) => name.clone(),
                TokenKind::String(name) => name.clone(),
                _ => String::new(),
            };
            self.recover_past_declaration();
            return Some(Declaration::Unsupported(UnsupportedDeclaration {
                construct: if keyword == "namespace" {
                    "namespace"
                } else {
                    "module declaration"
                },
                name: name.clone(),
                raw: format!("declare {keyword} {name} {{ … }}"),
            }));
        }
        if self.is_keyword("enum") || (self.is_keyword("const") && self.is_keyword_at(1, "enum")) {
            self.eat_keyword("const");
            self.advance();
            let name = self.eat_identifier().unwrap_or_default();
            self.recover_past_declaration();
            return Some(Declaration::Unsupported(UnsupportedDeclaration {
                construct: "TypeScript enum",
                name: name.clone(),
                raw: format!("enum {name} {{ … }}"),
            }));
        }
        if self.is_keyword("const") || self.is_keyword("var") || self.is_keyword("let") {
            self.advance();
            return self.parse_variable(start_token);
        }

        // Something else entirely: name it and move on.
        let raw = {
            let here = self.position;
            self.recover_past_declaration();
            self.text_between(here, self.position)
        };
        if raw.is_empty() {
            return None;
        }
        Some(Declaration::Unsupported(UnsupportedDeclaration {
            construct: "unrecognized declaration",
            name: String::new(),
            raw: first_line(&raw),
        }))
    }

    fn parse_interface(&mut self) -> Option<Declaration> {
        let name = self.eat_identifier()?;
        let generics = self.parse_generic_parameters();
        let mut extends = Vec::new();
        if self.eat_keyword("extends") {
            loop {
                extends.push(self.parse_type());
                if !self.eat_punctuation(",") {
                    break;
                }
            }
        }
        let members = self.parse_member_body();
        Some(Declaration::Interface(InterfaceDeclaration {
            name,
            generics,
            extends,
            members,
        }))
    }

    fn parse_class(&mut self) -> Option<Declaration> {
        let name = self.eat_identifier()?;
        let generics = self.parse_generic_parameters();
        let mut extends = Vec::new();
        let mut implements = Vec::new();
        loop {
            if self.eat_keyword("extends") {
                extends.push(self.parse_type());
            } else if self.eat_keyword("implements") {
                loop {
                    implements.push(self.parse_type());
                    if !self.eat_punctuation(",") {
                        break;
                    }
                }
            } else {
                break;
            }
        }
        let members = self.parse_member_body();
        Some(Declaration::Class(ClassDeclaration {
            name,
            generics,
            extends,
            implements,
            members,
        }))
    }

    fn parse_function_declaration(&mut self, start_token: usize) -> Option<Declaration> {
        let name = self.eat_identifier()?;
        let signature = self.parse_signature_tail(name, start_token);
        self.eat_punctuation(";");
        Some(Declaration::Function(signature))
    }

    fn parse_type_alias(&mut self) -> Option<Declaration> {
        let name = self.eat_identifier()?;
        let generics = self.parse_generic_parameters();
        if !self.eat_punctuation("=") {
            self.recover_past_declaration();
            return None;
        }
        let value = self.parse_type();
        self.eat_punctuation(";");
        Some(Declaration::TypeAlias(TypeAliasDeclaration {
            name,
            generics,
            value,
        }))
    }

    fn parse_variable(&mut self, start_token: usize) -> Option<Declaration> {
        let name = self.eat_identifier()?;
        let declared_type = self.eat_punctuation(":").then(|| self.parse_type());
        let end_token = self.position;
        self.recover_past_declaration();
        Some(Declaration::Variable(VariableDeclaration {
            name,
            declared_type,
            raw: self.text_between(start_token, end_token),
        }))
    }

    // --- Generics ---------------------------------------------------------

    fn parse_generic_parameters(&mut self) -> Vec<GenericParameter> {
        let mut parameters = Vec::new();
        if !self.eat_punctuation("<") {
            return parameters;
        }
        loop {
            if self.is_punctuation(">") || self.at_end() {
                break;
            }
            // `<const T>` / `<in out T>` variance and const modifiers.
            while self.is_keyword("const") || self.is_keyword("in") || self.is_keyword("out") {
                self.advance();
            }
            let Some(name) = self.eat_identifier() else {
                break;
            };
            let constraint = self.eat_keyword("extends").then(|| self.parse_type());
            let default = self.eat_punctuation("=").then(|| self.parse_type());
            parameters.push(GenericParameter {
                name,
                constraint,
                default,
            });
            if !self.eat_punctuation(",") {
                break;
            }
        }
        self.eat_punctuation(">");
        parameters
    }

    fn parse_type_arguments(&mut self) -> Vec<TsType> {
        let mut arguments = Vec::new();
        if !self.eat_punctuation("<") {
            return arguments;
        }
        loop {
            if self.is_punctuation(">") || self.at_end() {
                break;
            }
            arguments.push(self.parse_type());
            if !self.eat_punctuation(",") {
                break;
            }
        }
        self.eat_punctuation(">");
        arguments
    }

    // --- Signatures -------------------------------------------------------

    /// The tail of a signature, from the generic parameters onward. `name` is
    /// already consumed; `start_token` is where the whole signature began, so
    /// `raw` can quote it verbatim for an overload TODO.
    fn parse_signature_tail(&mut self, name: String, start_token: usize) -> Signature {
        let generics = self.parse_generic_parameters();
        let parameters = self.parse_parameters();
        let return_type = self.eat_punctuation(":").then(|| self.parse_return_type());
        Signature {
            name,
            generics,
            parameters,
            return_type,
            raw: self.text_between(start_token, self.position),
        }
    }

    fn parse_parameters(&mut self) -> Vec<Parameter> {
        let mut parameters = Vec::new();
        if !self.eat_punctuation("(") {
            return parameters;
        }
        loop {
            if self.is_punctuation(")") || self.at_end() {
                break;
            }
            let rest = self.eat_punctuation("...");
            // Destructured parameters (`{ a, b }: Options`) have no usable name.
            let name = if self.is_punctuation("{") || self.is_punctuation("[") {
                let open = if self.is_punctuation("{") { "{" } else { "[" };
                let close = if open == "{" { "}" } else { "]" };
                self.skip_balanced(open, close);
                "pattern".to_string()
            } else {
                match self.eat_member_name() {
                    Some(name) => name,
                    None => {
                        // Unreadable parameter: consume to the next `,` or `)`.
                        while !self.at_end()
                            && !self.is_punctuation(",")
                            && !self.is_punctuation(")")
                        {
                            self.advance();
                        }
                        self.eat_punctuation(",");
                        continue;
                    }
                }
            };
            let optional = self.eat_punctuation("?");
            let declared_type = self.eat_punctuation(":").then(|| self.parse_type());
            // Ambient declarations carry no initializers, but tolerate one.
            if self.eat_punctuation("=") {
                while !self.at_end() && !self.is_punctuation(",") && !self.is_punctuation(")") {
                    self.advance();
                }
            }
            // TypeScript's `this` parameter is a typing device, not an argument.
            if name != "this" {
                parameters.push(Parameter {
                    name,
                    optional,
                    rest,
                    declared_type,
                });
            }
            if !self.eat_punctuation(",") {
                break;
            }
        }
        self.eat_punctuation(")");
        parameters
    }

    /// A return type, which may additionally be a type predicate
    /// (`value is Foo`, `asserts value is Foo`) — both of which are `boolean`
    /// and `void` respectively at runtime.
    fn parse_return_type(&mut self) -> TsType {
        if self.is_keyword("asserts") {
            let start = self.position;
            self.advance();
            while !self.at_end() && !self.is_punctuation(";") && !self.is_punctuation(",") {
                self.advance();
            }
            let _ = start;
            return TsType::Reference {
                name: "void".to_string(),
                arguments: Vec::new(),
            };
        }
        let declared = self.parse_type();
        if self.is_keyword("is") {
            self.advance();
            let _ = self.parse_type();
            return TsType::Reference {
                name: "boolean".to_string(),
                arguments: Vec::new(),
            };
        }
        declared
    }

    // --- Members ----------------------------------------------------------

    fn parse_member_body(&mut self) -> Vec<Member> {
        let mut members = Vec::new();
        if !self.eat_punctuation("{") {
            return members;
        }
        loop {
            while self.eat_punctuation(";") || self.eat_punctuation(",") {}
            if self.is_punctuation("}") || self.at_end() {
                break;
            }
            let before = self.position;
            if let Some(member) = self.parse_member() {
                members.push(member);
            }
            if self.position == before {
                self.advance();
            }
        }
        self.eat_punctuation("}");
        members
    }

    fn parse_member(&mut self) -> Option<Member> {
        let start_token = self.position;
        let mut is_static = false;
        let mut readonly = false;
        let mut private = false;
        loop {
            if self.is_keyword("static") {
                is_static = true;
                self.advance();
            } else if self.is_keyword("readonly") {
                readonly = true;
                self.advance();
            } else if self.is_keyword("private") {
                private = true;
                self.advance();
            } else if matches!(&self.peek().kind, TokenKind::Identifier(word)
                if IGNORED_MODIFIERS.contains(&word.as_str()))
                // `declare`/`abstract`/`public` before a member name, but not a
                // member actually CALLED one of those.
                && !self.is_punctuation_at(1, "(")
                && !self.is_punctuation_at(1, ":")
                && !self.is_punctuation_at(1, "?")
                && !self.is_punctuation_at(1, ";")
            {
                self.advance();
            } else {
                break;
            }
        }

        // `new (…): T` — a construct signature (interfaces) or `constructor(…)`.
        if self.is_keyword("new")
            && (self.is_punctuation_at(1, "(") || self.is_punctuation_at(1, "<"))
        {
            self.advance();
            let signature = self.parse_signature_tail("new".to_string(), start_token);
            return Some(Member::Construct(signature));
        }
        if self.is_keyword("constructor") && self.is_punctuation_at(1, "(") {
            self.advance();
            let signature = self.parse_signature_tail("new".to_string(), start_token);
            return Some(Member::Construct(signature));
        }

        // `[key: string]: T` (index signature), `[K in Keys]: T` (mapped), or
        // `[Symbol.iterator]()` (a computed key).
        if self.is_punctuation("[") {
            return Some(self.parse_bracketed_member(start_token));
        }

        // A bare call signature: `(…): T` or `<T>(…): T`.
        if self.is_punctuation("(") || self.is_punctuation("<") {
            let signature = self.parse_signature_tail(String::new(), start_token);
            return Some(Member::Call(signature));
        }

        // `get name(): T` / `set name(value: T)`.
        let accessor = if (self.is_keyword("get") || self.is_keyword("set"))
            && matches!(
                self.peek_at(1).kind,
                TokenKind::Identifier(_) | TokenKind::String(_)
            ) {
            let kind = if self.is_keyword("get") { "get" } else { "set" };
            self.advance();
            Some(kind)
        } else {
            None
        };

        let name = self.eat_member_name()?;
        if private || name.starts_with('#') {
            self.skip_member_tail();
            return Some(Member::Unsupported {
                construct: "private member",
                raw: first_line(&self.text_between(start_token, self.position)),
            });
        }

        if let Some(kind) = accessor {
            let signature = self.parse_signature_tail(name.clone(), start_token);
            self.skip_member_tail();
            let declared_type = if kind == "get" {
                signature.return_type.clone()
            } else {
                signature
                    .parameters
                    .first()
                    .and_then(|parameter| parameter.declared_type.clone())
            };
            return Some(Member::Property(PropertyMember {
                name,
                optional: false,
                is_static,
                readable: kind == "get",
                writable: kind == "set",
                declared_type,
                raw: signature.raw,
            }));
        }

        let optional = self.eat_punctuation("?");
        // `foo!: T` — a definite-assignment assertion; the `!` is lexed away.

        if self.is_punctuation("(") || self.is_punctuation("<") {
            let signature = self.parse_signature_tail(name, start_token);
            self.skip_member_tail();
            return Some(Member::Method(MethodMember {
                is_static,
                optional,
                signature,
            }));
        }

        let declared_type = self.eat_punctuation(":").then(|| self.parse_type());
        let end_token = self.position;
        self.skip_member_tail();
        Some(Member::Property(PropertyMember {
            name,
            optional,
            is_static,
            readable: true,
            writable: !readonly,
            declared_type,
            raw: self.text_between(start_token, end_token),
        }))
    }

    fn parse_bracketed_member(&mut self, start_token: usize) -> Member {
        // Look ahead inside the brackets for the shape.
        let mut offset = 1;
        let mut depth = 1;
        let mut has_colon = false;
        let mut has_in = false;
        while depth > 0 {
            match &self.peek_at(offset).kind {
                TokenKind::EndOfFile => break,
                TokenKind::Punctuation("[") => depth += 1,
                TokenKind::Punctuation("]") => depth -= 1,
                TokenKind::Punctuation(":") if depth == 1 => has_colon = true,
                TokenKind::Identifier(word) if depth == 1 && word == "in" => has_in = true,
                _ => {}
            }
            offset += 1;
        }

        if has_in {
            self.skip_balanced("[", "]");
            self.skip_member_tail();
            return Member::Unsupported {
                construct: "mapped type",
                raw: first_line(&self.text_between(start_token, self.position)),
            };
        }
        if !has_colon {
            // A computed key: `[Symbol.iterator](): Iterator<T>`.
            self.skip_balanced("[", "]");
            self.skip_member_tail();
            return Member::Unsupported {
                construct: "computed member name",
                raw: first_line(&self.text_between(start_token, self.position)),
            };
        }

        self.eat_punctuation("[");
        let _key_name = self.eat_member_name();
        self.eat_punctuation(":");
        let key_type = self.parse_type();
        self.eat_punctuation("]");
        let value = if self.eat_punctuation(":") {
            self.parse_type()
        } else {
            TsType::Reference {
                name: "any".to_string(),
                arguments: Vec::new(),
            }
        };
        let end_token = self.position;
        self.skip_member_tail();
        let key = match &key_type {
            TsType::Reference { name, .. } if name == "string" => IndexKey::String,
            TsType::Reference { name, .. } if name == "number" => IndexKey::Number,
            _ => IndexKey::Other,
        };
        Member::Index(IndexMember {
            key,
            value,
            raw: self.text_between(start_token, end_token),
        })
    }

    /// Eats the separator after a member (`;` or `,`), tolerating its absence
    /// (ASI): real `.d.ts` files use both and sometimes neither.
    fn skip_member_tail(&mut self) {
        while self.eat_punctuation(";") || self.eat_punctuation(",") {}
    }

    // --- Types ------------------------------------------------------------

    pub fn parse_type(&mut self) -> TsType {
        let start_token = self.position;
        let first = self.parse_intersection_type();
        // A conditional type: `A extends B ? C : D`.
        if self.is_keyword("extends") {
            self.advance();
            let _ = self.parse_intersection_type();
            if self.eat_punctuation("?") {
                let _ = self.parse_type();
                self.eat_punctuation(":");
                let _ = self.parse_type();
            }
            return TsType::Unsupported {
                construct: "conditional type",
                raw: first_line(&self.text_between(start_token, self.position)),
            };
        }
        if !self.is_punctuation("|") {
            return first;
        }
        let mut members = vec![first];
        while self.eat_punctuation("|") {
            members.push(self.parse_intersection_type());
        }
        TsType::Union(members)
    }

    fn parse_intersection_type(&mut self) -> TsType {
        // A leading `|` or `&` is legal formatting, not an operand.
        while self.eat_punctuation("|") || self.eat_punctuation("&") {}
        let first = self.parse_postfix_type();
        if !self.is_punctuation("&") {
            return first;
        }
        let mut members = vec![first];
        while self.eat_punctuation("&") {
            members.push(self.parse_postfix_type());
        }
        TsType::Intersection(members)
    }

    fn parse_postfix_type(&mut self) -> TsType {
        let start_token = self.position;
        let mut current = self.parse_primary_type();
        loop {
            if self.is_punctuation("[") {
                if self.is_punctuation_at(1, "]") {
                    self.advance();
                    self.advance();
                    current = TsType::Array(Box::new(current));
                    continue;
                }
                // An indexed access type: `Foo["bar"]`.
                self.skip_balanced("[", "]");
                current = TsType::Unsupported {
                    construct: "indexed access type",
                    raw: first_line(&self.text_between(start_token, self.position)),
                };
                continue;
            }
            break;
        }
        current
    }

    fn parse_primary_type(&mut self) -> TsType {
        let start_token = self.position;

        // Type operators whose operand v1 does not evaluate.
        for (keyword, construct) in [
            ("keyof", "keyof type"),
            ("typeof", "typeof type"),
            ("infer", "infer type"),
            ("unique", "unique symbol"),
        ] {
            if self.is_keyword(keyword) {
                self.advance();
                let _ = self.parse_postfix_type();
                return TsType::Unsupported {
                    construct,
                    raw: first_line(&self.text_between(start_token, self.position)),
                };
            }
        }
        // `readonly T[]` — the modifier is not representable, the element type is.
        if self.is_keyword("readonly") {
            self.advance();
            return self.parse_postfix_type();
        }
        if self.is_keyword("import") && self.is_punctuation_at(1, "(") {
            self.advance();
            self.skip_balanced("(", ")");
            while self.eat_punctuation(".") {
                let _ = self.eat_identifier();
            }
            let _ = self.parse_type_arguments();
            return TsType::Unsupported {
                construct: "import type",
                raw: first_line(&self.text_between(start_token, self.position)),
            };
        }
        if (self.is_keyword("new") || self.is_keyword("abstract"))
            && (self.is_punctuation_at(1, "(") || self.is_keyword_at(1, "new"))
        {
            self.eat_keyword("abstract");
            self.eat_keyword("new");
            let signature = self.parse_signature_tail_arrow(start_token);
            return TsType::Constructor(Box::new(signature));
        }

        match self.peek().kind.clone() {
            TokenKind::String(value) => {
                self.advance();
                TsType::StringLiteral(value)
            }
            TokenKind::Number(value) => {
                self.advance();
                TsType::NumberLiteral(value)
            }
            TokenKind::Template(_) => {
                self.advance();
                TsType::Unsupported {
                    construct: "template literal type",
                    raw: first_line(&self.text_between(start_token, self.position)),
                }
            }
            TokenKind::Punctuation("-") => {
                self.advance();
                match self.peek().kind.clone() {
                    TokenKind::Number(value) => {
                        self.advance();
                        TsType::NumberLiteral(format!("-{value}"))
                    }
                    _ => TsType::Unsupported {
                        construct: "unrecognized type",
                        raw: first_line(&self.text_between(start_token, self.position)),
                    },
                }
            }
            TokenKind::Punctuation("{") => {
                // A MAPPED type (`{ [K in keyof T]: string }`) wears an object
                // type's braces but is a different construct entirely, and
                // reading it as one produces nonsense members. Recognize it
                // whole (§3.11: recognizing is syntactic, resolving is not).
                if self.braces_open_a_mapped_type() {
                    self.skip_balanced("{", "}");
                    return TsType::Unsupported {
                        construct: "mapped type",
                        raw: first_line(&self.text_between(start_token, self.position)),
                    };
                }
                let members = self.parse_member_body();
                TsType::Object(members)
            }
            TokenKind::Punctuation("[") => {
                self.advance();
                let mut elements = Vec::new();
                loop {
                    if self.is_punctuation("]") || self.at_end() {
                        break;
                    }
                    // Named tuple elements: `[start: number, end: number]`.
                    if matches!(self.peek().kind, TokenKind::Identifier(_))
                        && (self.is_punctuation_at(1, ":")
                            || (self.is_punctuation_at(1, "?") && self.is_punctuation_at(2, ":")))
                    {
                        self.advance();
                        self.eat_punctuation("?");
                        self.eat_punctuation(":");
                    }
                    self.eat_punctuation("...");
                    elements.push(self.parse_type());
                    self.eat_punctuation("?");
                    if !self.eat_punctuation(",") {
                        break;
                    }
                }
                self.eat_punctuation("]");
                TsType::Tuple(elements)
            }
            TokenKind::Punctuation("(") => {
                // Either a function type or a parenthesized type. A function
                // type is the only one whose closing `)` is followed by `=>`.
                if self.parenthesized_run_is_function() {
                    let signature = self.parse_signature_tail_arrow(start_token);
                    return TsType::Function(Box::new(signature));
                }
                self.advance();
                let inner = self.parse_type();
                self.eat_punctuation(")");
                inner
            }
            TokenKind::Punctuation("<") => {
                // A generic function type: `<T>(value: T) => T`.
                let signature = self.parse_signature_tail_arrow(start_token);
                TsType::Function(Box::new(signature))
            }
            // `true` / `false` in TYPE position are boolean literal types, not
            // references to types called `true` and `false`.
            TokenKind::Identifier(word) if word == "true" || word == "false" => {
                self.advance();
                TsType::BooleanLiteral(word == "true")
            }
            TokenKind::Identifier(first) => {
                self.advance();
                let mut name = first;
                while self.is_punctuation(".")
                    && matches!(self.peek_at(1).kind, TokenKind::Identifier(_))
                {
                    self.advance();
                    let segment = self.eat_identifier().unwrap_or_default();
                    let _ = write!(name, ".{segment}");
                }
                let arguments = self.parse_type_arguments();
                TsType::Reference { name, arguments }
            }
            _ => {
                self.advance();
                TsType::Unsupported {
                    construct: "unrecognized type",
                    raw: first_line(&self.text_between(start_token, self.position)),
                }
            }
        }
    }

    /// Whether the `{` at the cursor opens a mapped type — its first member is
    /// a `[K in …]` clause rather than a name or an index signature.
    fn braces_open_a_mapped_type(&self) -> bool {
        let mut offset = 1;
        // Skip the `+`/`-` and `readonly` modifiers a mapped type may carry.
        while matches!(
            &self.peek_at(offset).kind,
            TokenKind::Punctuation("+" | "-")
        ) || matches!(&self.peek_at(offset).kind, TokenKind::Identifier(word) if word == "readonly")
        {
            offset += 1;
        }
        if !matches!(self.peek_at(offset).kind, TokenKind::Punctuation("[")) {
            return false;
        }
        let mut depth = 1;
        offset += 1;
        while depth > 0 {
            match &self.peek_at(offset).kind {
                TokenKind::EndOfFile => return false,
                TokenKind::Punctuation("[") => depth += 1,
                TokenKind::Punctuation("]") => depth -= 1,
                TokenKind::Identifier(word) if depth == 1 && word == "in" => return true,
                _ => {}
            }
            offset += 1;
        }
        false
    }

    /// Whether the `(` at the cursor opens a function type's parameter list
    /// (its matching `)` is followed by `=>`) rather than a parenthesized type.
    fn parenthesized_run_is_function(&self) -> bool {
        let mut offset = 1;
        let mut depth = 1;
        while depth > 0 {
            match &self.peek_at(offset).kind {
                TokenKind::EndOfFile => return false,
                TokenKind::Punctuation("(") => depth += 1,
                TokenKind::Punctuation(")") => depth -= 1,
                _ => {}
            }
            offset += 1;
        }
        self.is_punctuation_at(offset, "=>")
    }

    /// A function-type signature: generics, parameters, then `=> ReturnType`.
    fn parse_signature_tail_arrow(&mut self, start_token: usize) -> Signature {
        let generics = self.parse_generic_parameters();
        let parameters = self.parse_parameters();
        let return_type = self.eat_punctuation("=>").then(|| self.parse_return_type());
        Signature {
            name: String::new(),
            generics,
            parameters,
            return_type,
            raw: self.text_between(start_token, self.position),
        }
    }
}

/// The first line of `text`, ellipsized — TODO comments quote a construct, they
/// do not reproduce it.
fn first_line(text: &str) -> String {
    const LIMIT: usize = 120;
    let line = text.lines().next().unwrap_or_default().trim();
    if line.chars().count() <= LIMIT {
        return line.to_string();
    }
    let truncated: String = line.chars().take(LIMIT).collect();
    format!("{truncated}…")
}
