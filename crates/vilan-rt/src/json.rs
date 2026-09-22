//! `vilan-rt::json` — the host JSON value `std::json` is written against
//! (tracker F18 slice 2; `proposal/native-apps.md` §12).
//!
//! # What this is a twin OF
//!
//! `vilan/std/src/json.vl` declares `external struct JsonValue` and reaches the
//! host through exactly nine seams: `JSON.parse` (trusting), a guarded parse
//! (the `TryParseJson` intrinsic), the four walkers `field`/`tag`/`elements`/
//! `is_null`, the normalized `kind`, `Object.hasOwn`, and the three coercions
//! `String`/`Boolean`/`Number`. Those nine ARE the contract — every item here
//! exists because one of them does, and the comment at each one names it and
//! says what JavaScript does at that seam.
//!
//! # Why by hand
//!
//! Order 37's R8 and Order 39's R1: `vilan-rt` takes no crates.io
//! dependencies. JSON is a 400-line grammar with no ambiguity, so it fits
//! inside that rule the way HTTP/1.1 did; SQLite does not, which is why
//! `std::db` natively is the separate `vilan-rt-sqlite` crate.
//!
//! # The `undefined` arm is not decoration
//!
//! On the JS backend a `JsonValue` is *any* JS value, and `value.field("absent")`
//! is `undefined` — which is neither `null` nor any of the five JSON shapes.
//! `json.vl` says so at `JsonKind` ("a `JsonValue` is whatever the host handed
//! over, so `value.field("absent")` … has a kind outside this set") and the
//! derived decoders lean on it: `has_field` is the check that keeps a decoder
//! out of that case, and `kind()` answering `"undefined"` is what makes a
//! missing field a decode ERROR instead of a coerced `NaN`. So the value type
//! here carries [`JsonValue::Undefined`] as a seventh arm rather than folding it
//! into `Null`, which would silently turn every missing field into a present
//! `null`.

use std::rc::Rc;

use crate::{Js, Json, Str, js_number, json_string, str_new};

/// The largest nesting depth [`parse`] and [`try_parse`] descend before
/// answering "not valid JSON".
///
/// A frame off a socket is attacker-supplied input and the parser is recursive,
/// so an unbounded descent is a stack overflow a stranger can ask for. V8 has
/// the same bound and reports it the same way — `JSON.parse` throws a
/// `RangeError` on a document nested past its stack — so a document this
/// refuses is one node refuses too; the exact number differs, and node's is
/// larger. Nothing a program can write reaches it: the deepest document std
/// itself produces is a wire frame, which is four levels.
const MAX_DEPTH: usize = 512;

/// A parsed JSON value — the host value `std::json`'s `external struct
/// JsonValue` names.
///
/// `Rc` on the two containers for the reason [`Str`] is one: a vilan value is
/// copied by rule 1 wherever it is read into a call or an aggregate, and a
/// `JsonValue` is read once per decoded field.
#[derive(Clone, Debug)]
pub enum JsonValue {
    /// Not a JSON shape at all: the result of reading a field an object does
    /// not have. See this module's header.
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Str),
    Array(Rc<Vec<JsonValue>>),
    /// Insertion-ordered, because JavaScript objects are: `Object.keys` answers
    /// string keys in insertion order, and `__json_tag` reads `Object.keys(v)[0]`.
    Object(Rc<Vec<(Str, JsonValue)>>),
}

impl JsonValue {
    /// `value[name]` — the `JsonField` intrinsic.
    ///
    /// A key an object does not carry is `undefined`, and so is a field read of
    /// anything that is not an object: `(5).x` and `"s".x` are both `undefined`
    /// in JavaScript, and neither is a shape a decoder is allowed to be in
    /// without having asked `has_field` first.
    pub fn field(&self, name: &str) -> JsonValue {
        match self {
            JsonValue::Object(entries) => entries
                .iter()
                .find(|(key, _)| &**key == name)
                .map(|(_, value)| value.clone())
                .unwrap_or(JsonValue::Undefined),
            _ => JsonValue::Undefined,
        }
    }

    /// The externally-tagged discriminator — the `JsonTag` intrinsic, which is
    /// `transformer.rs`'s `__json_tag` helper.
    ///
    /// A bare `"Variant"` is its own tag; a `{"Variant": ..}` object's tag is
    /// its FIRST key. Everything else answers `""` (A116), a tag no variant can
    /// be spelled with, so a derived decoder's `_` arm reports "unknown variant"
    /// rather than the helper throwing on a document the caller did not choose.
    pub fn tag(&self) -> Str {
        match self {
            JsonValue::Text(text) => Rc::clone(text),
            JsonValue::Object(entries) => match entries.first() {
                Some((key, _)) => Rc::clone(key),
                None => str_new(""),
            },
            _ => str_new(""),
        }
    }

    /// The `JsonElements` intrinsic. On the JS backend a parsed array already
    /// IS a JS array, so the helper is the identity and the type is the whole
    /// of it; here the arm has to build the `Vec` the emitted type names.
    ///
    /// Anything that is not an array answers the empty list, which is what a
    /// `for` over a non-array would do on the other backend after the type
    /// assertion the intrinsic performs.
    pub fn elements(&self) -> Vec<JsonValue> {
        match self {
            JsonValue::Array(items) => items.as_ref().clone(),
            _ => Vec::new(),
        }
    }

    /// `value === null` — the `JsonIsNull` intrinsic, and the `Option::None`
    /// discriminator. `undefined === null` is `false` in JavaScript, so a
    /// MISSING field is not a null one here either.
    pub fn is_null(&self) -> bool {
        matches!(self, JsonValue::Null)
    }

    /// The normalized JSON type — the `JsonKind` intrinsic, which is
    /// `transformer.rs`'s `__json_kind`: `typeof` buckets arrays and `null` as
    /// `"object"`, so both are named explicitly.
    ///
    /// The answer is a `str` because `JsonKind` is a BACKED enum, and a backed
    /// enum IS its backing value on both backends (§12).
    pub fn kind(&self) -> Str {
        str_new(match self {
            JsonValue::Undefined => "undefined",
            JsonValue::Null => "null",
            JsonValue::Bool(_) => "boolean",
            JsonValue::Number(_) => "number",
            JsonValue::Text(_) => "string",
            JsonValue::Array(_) => "array",
            JsonValue::Object(_) => "object",
        })
    }

    /// `Object.hasOwn(value, name)` — the presence test the derived struct
    /// decoder reads. A non-object has no own properties in the sense a decoder
    /// asks about.
    pub fn has_field(&self, name: &str) -> bool {
        match self {
            JsonValue::Object(entries) => entries.iter().any(|(key, _)| &**key == name),
            _ => false,
        }
    }

    /// `String(value)` — `std::json`'s `coerce_str`, reached only past a
    /// `kind() == String` check, so the string arm is the one that carries the
    /// weight. The others are JavaScript's `String()` on the same value, written
    /// out rather than left to a panic, because `String` is a total function
    /// there and a runtime abort would be a divergence a `kind` check is
    /// supposed to have made unreachable.
    pub fn coerce_str(&self) -> Str {
        match self {
            JsonValue::Text(text) => Rc::clone(text),
            JsonValue::Undefined => str_new("undefined"),
            JsonValue::Null => str_new("null"),
            JsonValue::Bool(value) => str_new(if *value { "true" } else { "false" }),
            JsonValue::Number(value) => str_new(&js_number(*value)),
            // `String([1,2])` is `"1,2"` and `String({})` is `"[object Object]"`.
            JsonValue::Array(items) => {
                let parts: Vec<String> = items
                    .iter()
                    .map(|item| match item {
                        JsonValue::Null | JsonValue::Undefined => String::new(),
                        other => other.coerce_str().to_string(),
                    })
                    .collect();
                str_new(&parts.join(","))
            }
            JsonValue::Object(_) => str_new("[object Object]"),
        }
    }

    /// `Boolean(value)` — `std::json`'s `coerce_bool`. JavaScript's falsy set is
    /// `undefined`, `null`, `false`, `0`/`NaN` and `""`; every object and every
    /// array, empty ones included, is truthy.
    pub fn coerce_bool(&self) -> bool {
        match self {
            JsonValue::Undefined | JsonValue::Null => false,
            JsonValue::Bool(value) => *value,
            JsonValue::Number(value) => *value != 0.0 && !value.is_nan(),
            JsonValue::Text(text) => !text.is_empty(),
            JsonValue::Array(_) | JsonValue::Object(_) => true,
        }
    }

    /// `Number(value)` — the twelve `coerce_*` bindings, which differ only in
    /// the vilan type they label the result with. `Number("")` is `0`,
    /// `Number(null)` is `0`, `Number(undefined)` is `NaN`, and `Number("12")`
    /// is `12` (JavaScript coerces a numeric string).
    pub fn coerce_number(&self) -> f64 {
        match self {
            JsonValue::Undefined => f64::NAN,
            JsonValue::Null => 0.0,
            JsonValue::Bool(value) => {
                if *value {
                    1.0
                } else {
                    0.0
                }
            }
            JsonValue::Number(value) => *value,
            JsonValue::Text(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    0.0
                } else {
                    trimmed.parse::<f64>().unwrap_or(f64::NAN)
                }
            }
            JsonValue::Array(items) if items.is_empty() => 0.0,
            JsonValue::Array(items) if items.len() == 1 => items[0].coerce_number(),
            JsonValue::Array(_) | JsonValue::Object(_) => f64::NAN,
        }
    }

    /// An object from named entries — what [`crate::http::Request::headers`]
    /// builds, and the only constructor outside the parser.
    pub fn object(entries: Vec<(Str, JsonValue)>) -> JsonValue {
        JsonValue::Object(Rc::new(entries))
    }
}

/// `JSON.parse(text)` — TRUSTING, exactly as `std::json`'s `parse_json_value`
/// documents it: malformed text is a host exception, which natively is the
/// same abort every other host throw takes.
pub fn parse(text: &str) -> JsonValue {
    match try_parse(text) {
        Some(value) => value,
        None => crate::panic_with(&format!(
            "SyntaxError: Unexpected token in JSON at position 0 (parsing {:?})",
            truncate_for_message(text)
        )),
    }
}

/// The `TryParseJson` intrinsic — `JSON.parse` guarded: `None` for malformed
/// text instead of a host exception.
///
/// This is the entry point a wire frame goes through, and `json.vl` states the
/// rule it keeps: "a wire frame is attacker-supplied input, so the codec must
/// refuse it as a decode error, never a crash".
pub fn try_parse(text: &str) -> Option<JsonValue> {
    let bytes: Vec<char> = text.chars().collect();
    let mut parser = Parser {
        input: &bytes,
        at: 0,
    };
    parser.skip_whitespace();
    let value = parser.value(0)?;
    parser.skip_whitespace();
    if parser.at != parser.input.len() {
        return None;
    }
    Some(value)
}

fn truncate_for_message(text: &str) -> String {
    let head: String = text.chars().take(32).collect();
    if head.chars().count() < text.chars().count() {
        format!("{head}…")
    } else {
        head
    }
}

/// A recursive-descent reader over ECMA-404, on `char`s rather than bytes so a
/// position is a code point and a `\u` escape can be assembled without slicing
/// into the middle of one.
struct Parser<'a> {
    input: &'a [char],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.input.get(self.at).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.at += 1;
        Some(character)
    }

    fn eat(&mut self, expected: char) -> Option<()> {
        if self.peek() == Some(expected) {
            self.at += 1;
            Some(())
        } else {
            None
        }
    }

    /// JSON's whitespace is exactly these four characters — not Unicode's set.
    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.at += 1;
        }
    }

    fn literal(&mut self, word: &str) -> Option<()> {
        for expected in word.chars() {
            self.eat(expected)?;
        }
        Some(())
    }

    fn value(&mut self, depth: usize) -> Option<JsonValue> {
        if depth > MAX_DEPTH {
            return None;
        }
        match self.peek()? {
            'n' => {
                self.literal("null")?;
                Some(JsonValue::Null)
            }
            't' => {
                self.literal("true")?;
                Some(JsonValue::Bool(true))
            }
            'f' => {
                self.literal("false")?;
                Some(JsonValue::Bool(false))
            }
            '"' => self.string().map(JsonValue::Text),
            '[' => self.array(depth),
            '{' => self.object(depth),
            '-' | '0'..='9' => self.number(),
            _ => None,
        }
    }

    fn array(&mut self, depth: usize) -> Option<JsonValue> {
        self.eat('[')?;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.eat(']').is_some() {
            return Some(JsonValue::Array(Rc::new(items)));
        }
        loop {
            self.skip_whitespace();
            items.push(self.value(depth + 1)?);
            self.skip_whitespace();
            match self.bump()? {
                ',' => continue,
                ']' => return Some(JsonValue::Array(Rc::new(items))),
                _ => return None,
            }
        }
    }

    fn object(&mut self, depth: usize) -> Option<JsonValue> {
        self.eat('{')?;
        let mut entries: Vec<(Str, JsonValue)> = Vec::new();
        self.skip_whitespace();
        if self.eat('}').is_some() {
            return Some(JsonValue::Object(Rc::new(entries)));
        }
        loop {
            self.skip_whitespace();
            let key = self.string()?;
            self.skip_whitespace();
            self.eat(':')?;
            self.skip_whitespace();
            let value = self.value(depth + 1)?;
            // A duplicate key keeps its FIRST position and its LAST value,
            // which is what assigning twice to one property does in JavaScript.
            match entries.iter_mut().find(|(known, _)| *known == key) {
                Some(entry) => entry.1 = value,
                None => entries.push((key, value)),
            }
            self.skip_whitespace();
            match self.bump()? {
                ',' => continue,
                '}' => return Some(JsonValue::Object(Rc::new(entries))),
                _ => return None,
            }
        }
    }

    fn string(&mut self) -> Option<Str> {
        self.eat('"')?;
        let mut out = String::new();
        loop {
            match self.bump()? {
                '"' => return Some(str_new(&out)),
                '\\' => match self.bump()? {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => out.push(self.escape()?),
                    _ => return None,
                },
                // ECMA-404: an unescaped control character is not a string.
                control if control < ' ' => return None,
                other => out.push(other),
            }
        }
    }

    /// One `\uXXXX`, plus the low half of a surrogate PAIR when the first half
    /// is a high surrogate — a vilan `str` is UTF-8 and cannot hold a lone
    /// surrogate, so the pair has to be assembled here or the code point is
    /// lost. A lone surrogate becomes U+FFFD, which is what a UTF-8 encode of
    /// one does everywhere else.
    fn escape(&mut self) -> Option<char> {
        let first = self.hex4()?;
        if (0xD800..0xDC00).contains(&first) {
            let rewind = self.at;
            if self.eat('\\').is_some()
                && self.eat('u').is_some()
                && let Some(second) = self.hex4()
                && (0xDC00..0xE000).contains(&second)
            {
                let combined = 0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00);
                return char::from_u32(combined).or(Some('\u{fffd}'));
            }
            self.at = rewind;
            return Some('\u{fffd}');
        }
        if (0xDC00..0xE000).contains(&first) {
            return Some('\u{fffd}');
        }
        char::from_u32(first)
    }

    fn hex4(&mut self) -> Option<u32> {
        let mut value = 0;
        for _ in 0..4 {
            value = value * 16 + self.bump()?.to_digit(16)?;
        }
        Some(value)
    }

    /// JSON's number grammar: an optional `-`, an integer part with no leading
    /// zero, an optional fraction, an optional exponent. `01`, `+1`, `.5`, `1.`
    /// and `Infinity` are all refused, exactly as `JSON.parse` refuses them.
    fn number(&mut self) -> Option<JsonValue> {
        let start = self.at;
        self.eat('-');
        match self.bump()? {
            '0' => {}
            '1'..='9' => self.digits(),
            _ => return None,
        }
        if self.peek() == Some('.') {
            self.at += 1;
            self.peek()?.to_digit(10)?;
            self.digits();
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            self.at += 1;
            if matches!(self.peek(), Some('+' | '-')) {
                self.at += 1;
            }
            self.peek()?.to_digit(10)?;
            self.digits();
        }
        let text: String = self.input[start..self.at].iter().collect();
        text.parse::<f64>().ok().map(JsonValue::Number)
    }

    fn digits(&mut self) {
        while matches!(self.peek(), Some('0'..='9')) {
            self.at += 1;
        }
    }
}

/// `JSON.stringify(value)` of an already-parsed value.
///
/// `undefined` is the one arm with no JSON rendering: `JSON.stringify(undefined)`
/// answers the value `undefined`, not a string, and the only place a vilan
/// program can reach that is stringifying a field it never checked for. `null`
/// is the stand-in, which is what `JSON.stringify([undefined])` writes for an
/// undefined ELEMENT.
impl Json for JsonValue {
    fn json(&self) -> String {
        match self {
            JsonValue::Undefined | JsonValue::Null => "null".to_string(),
            JsonValue::Bool(value) => value.to_string(),
            JsonValue::Number(value) => value.json(),
            JsonValue::Text(text) => json_string(text),
            JsonValue::Array(items) => {
                let parts: Vec<String> = items.iter().map(Json::json).collect();
                format!("[{}]", parts.join(","))
            }
            JsonValue::Object(entries) => {
                let parts: Vec<String> = entries
                    .iter()
                    .filter(|(_, value)| !matches!(value, JsonValue::Undefined))
                    .map(|(key, value)| format!("{}:{}", json_string(key), value.json()))
                    .collect();
                format!("{{{}}}", parts.join(","))
            }
        }
    }
}

/// `console.log` of a parsed value — node's object inspection, which is NOT
/// `JSON.stringify`: keys are bare where they are identifiers, strings are
/// single-quoted, and a container is spaced (`{ a: 1 }`, `[ 1, 2 ]`).
impl Js for JsonValue {
    fn js(&self) -> String {
        match self {
            JsonValue::Undefined => "undefined".to_string(),
            JsonValue::Null => "null".to_string(),
            JsonValue::Bool(value) => value.to_string(),
            JsonValue::Number(value) => js_number(*value),
            JsonValue::Text(text) => text.to_string(),
            JsonValue::Array(items) => {
                if items.is_empty() {
                    return "[]".to_string();
                }
                let parts: Vec<String> = items.iter().map(Js::js_nested).collect();
                format!("[ {} ]", parts.join(", "))
            }
            JsonValue::Object(entries) => {
                if entries.is_empty() {
                    return "{}".to_string();
                }
                let parts: Vec<String> = entries
                    .iter()
                    .map(|(key, value)| format!("{}: {}", inspect_key(key), value.js_nested()))
                    .collect();
                format!("{{ {} }}", parts.join(", "))
            }
        }
    }

    fn js_nested(&self) -> String {
        match self {
            JsonValue::Text(text) => format!("'{text}'"),
            other => other.js(),
        }
    }
}

/// Node prints an object key bare when it is a valid identifier and quoted
/// otherwise — `{ a: 1 }` against `{ 'a-b': 1 }`.
fn inspect_key(key: &str) -> String {
    let identifier = !key.is_empty()
        && !key.starts_with(|first: char| first.is_ascii_digit())
        && key.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '$'
        });
    if identifier {
        key.to_string()
    } else {
        format!("'{key}'")
    }
}

/// Two parsed values compare by STRUCTURE here, which is not what `===` on two
/// JS objects does — that compares references. Nothing in `std::json` compares
/// two `JsonValue`s; the impl exists because an emitted struct holding one
/// derives `PartialEq`, and structural equality is the only answer that is
/// right for the scalars it will actually be asked about.
impl PartialEq for JsonValue {
    fn eq(&self, other: &JsonValue) -> bool {
        match (self, other) {
            (JsonValue::Undefined, JsonValue::Undefined) => true,
            (JsonValue::Null, JsonValue::Null) => true,
            (JsonValue::Bool(left), JsonValue::Bool(right)) => left == right,
            (JsonValue::Number(left), JsonValue::Number(right)) => left == right,
            (JsonValue::Text(left), JsonValue::Text(right)) => left == right,
            (JsonValue::Array(left), JsonValue::Array(right)) => left == right,
            (JsonValue::Object(left), JsonValue::Object(right)) => left == right,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> JsonValue {
        try_parse(text).expect("valid JSON")
    }

    #[test]
    fn the_six_shapes_parse_and_name_their_kind() {
        for (text, kind) in [
            ("null", "null"),
            ("true", "boolean"),
            ("12", "number"),
            ("\"s\"", "string"),
            ("[]", "array"),
            ("{}", "object"),
        ] {
            assert_eq!(&*parsed(text).kind(), kind, "the kind of {text}");
        }
    }

    #[test]
    fn a_missing_field_is_undefined_and_is_neither_null_nor_present() {
        let value = parsed("{\"a\":null}");
        assert_eq!(&*value.field("b").kind(), "undefined");
        assert!(!value.field("b").is_null(), "undefined === null is false");
        assert!(!value.has_field("b"));
        // The present-but-null field is the case the two must not be folded
        // into: it IS present and it IS null.
        assert!(value.has_field("a"));
        assert!(value.field("a").is_null());
    }

    #[test]
    fn the_number_grammar_is_ecma_404s() {
        assert_eq!(parsed("-1.5e2").coerce_number(), -150.0);
        assert_eq!(parsed("0").coerce_number(), 0.0);
        for refused in ["01", "+1", ".5", "1.", "1e", "Infinity", "NaN", "-"] {
            assert!(try_parse(refused).is_none(), "{refused} is not JSON");
        }
    }

    #[test]
    fn a_string_takes_the_eight_escapes_and_a_surrogate_pair() {
        assert_eq!(&*parsed(r#""a\"b""#).coerce_str(), "a\"b");
        assert_eq!(&*parsed(r#""A""#).coerce_str(), "A");
        assert_eq!(&*parsed(r#""😀""#).coerce_str(), "\u{1f600}");
        assert_eq!(&*parsed(r#""\ud83d""#).coerce_str(), "\u{fffd}");
        assert_eq!(
            &*parsed(r#""\b\f\n\r\t\/""#).coerce_str(),
            "\u{8}\u{c}\n\r\t/"
        );
        // A raw control character is refused, and so is an unknown escape.
        assert!(try_parse("\"a\nb\"").is_none());
        assert!(try_parse(r#""\x41""#).is_none());
    }

    #[test]
    fn trailing_text_and_trailing_commas_are_refused() {
        for refused in ["{} {}", "[1,]", "{\"a\":1,}", "[1 2]", "", "   "] {
            assert!(try_parse(refused).is_none(), "{refused:?} is not JSON");
        }
    }

    #[test]
    fn a_tag_is_the_string_or_the_first_key_and_nothing_else_tags() {
        assert_eq!(&*parsed("\"Login\"").tag(), "Login");
        assert_eq!(&*parsed("{\"Login\":[1]}").tag(), "Login");
        for untagged in ["null", "12", "true", "[1]", "{}"] {
            assert_eq!(&*parsed(untagged).tag(), "", "{untagged} names no variant");
        }
    }

    #[test]
    fn a_duplicate_key_keeps_its_first_position_and_its_last_value() {
        let value = parsed("{\"a\":1,\"b\":2,\"a\":3}");
        assert_eq!(&*value.tag(), "a");
        assert_eq!(value.field("a").coerce_number(), 3.0);
    }

    #[test]
    fn the_three_coercions_are_javascripts() {
        assert_eq!(&*parsed("12").coerce_str(), "12");
        assert_eq!(&*parsed("null").coerce_str(), "null");
        assert_eq!(&*parsed("[1,2]").coerce_str(), "1,2");
        assert_eq!(&*parsed("{}").coerce_str(), "[object Object]");
        assert!(!parsed("0").coerce_bool());
        assert!(!parsed("\"\"").coerce_bool());
        assert!(parsed("{}").coerce_bool());
        assert!(parsed("[]").coerce_bool());
        assert_eq!(parsed("\"12\"").coerce_number(), 12.0);
        assert_eq!(parsed("null").coerce_number(), 0.0);
        assert!(JsonValue::Undefined.coerce_number().is_nan());
    }

    #[test]
    fn a_document_nested_past_the_bound_is_refused_rather_than_overflowing() {
        let deep = format!("{}{}", "[".repeat(MAX_DEPTH + 2), "]".repeat(MAX_DEPTH + 2));
        assert!(try_parse(&deep).is_none(), "past the bound");
        let shallow = format!("{}{}", "[".repeat(MAX_DEPTH - 1), "]".repeat(MAX_DEPTH - 1));
        assert!(try_parse(&shallow).is_some(), "inside the bound");
    }

    #[test]
    fn stringify_round_trips_and_drops_an_undefined_field() {
        assert_eq!(
            parsed("{\"a\":[1,\"b\",null]}").json(),
            "{\"a\":[1,\"b\",null]}"
        );
        let object = JsonValue::object(vec![
            (str_new("a"), JsonValue::Number(1.0)),
            (str_new("b"), JsonValue::Undefined),
        ]);
        assert_eq!(object.json(), "{\"a\":1}");
    }

    #[test]
    fn console_log_is_nodes_inspection_and_not_stringify() {
        assert_eq!(
            parsed("{\"a\":1,\"b-c\":\"x\"}").js(),
            "{ a: 1, 'b-c': 'x' }"
        );
        assert_eq!(parsed("[1,\"x\"]").js(), "[ 1, 'x' ]");
        assert_eq!(parsed("{}").js(), "{}");
        assert_eq!(parsed("[]").js(), "[]");
    }
}
