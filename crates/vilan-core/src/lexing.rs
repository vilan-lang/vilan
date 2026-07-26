//! The handwritten lexer (H6 S1, `proposal/frontend.md` §2).
//!
//! A dependency-free, single-pass scan over `&str` producing `Vec<(Token, Span)>`
//! byte-identical — spans included — to the chumsky lexer in `lexer.rs`, which
//! stays in-tree as the oracle for the whole H6 arc (deleted at S5). Nothing in
//! the pipeline calls this yet; it is exercised only by the differential and unit
//! tests. At S5 this module replaces `lexer.rs` and [`tokenize`] takes over from
//! `lexer()`.
//!
//! The behaviour reproduced here (keyword classification, longest-match operators,
//! `<`/`>` always `Ctrl`, `=>` an `Op`, numeric-literal shape, in-lexer i-string
//! desugaring, linear trivia skipping, and the exact span each token carries) is a
//! faithful copy of the chumsky lexer, quirks included — the differential is the
//! referee, not any judgement about what a token or span *should* be. Ugly-but-
//! reproduced behaviours are recorded for the S4/S5 error-quality pass, not fixed.

use crate::span::{Span, Spanned};
use crate::token::Token;

/// A lexing error: the byte offset and the character the lexer could not turn into
/// a token. S1 records these but does not yet reproduce chumsky's error *messages*
/// (that is S4's concern); the differential compares token streams, not errors.
///
/// One error is recorded per un-lexable character. The chumsky lexer coalesces a
/// run of consecutive un-lexable characters into a single diagnostic — a
/// difference in error *count*, not in the token stream, deferred to S4.
///
/// `rule` upgrades the generic "found X expected a token" to a curated statement
/// of the language rule the character broke (diagnostics-standard.md B6 — the
/// prohibition explains itself and names the sanctioned spelling). The parser
/// renders it as [`crate::parsing::ParseErrorReason::Rule`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexError {
    pub position: usize,
    pub character: char,
    pub rule: Option<&'static str>,
}

/// Lex `source` into its token stream (with spans) and any lexing errors. The
/// token stream is byte-identical to `lexer().parse(source)` for every source the
/// H6 differential covers.
pub fn tokenize(source: &str) -> (Vec<Spanned<Token<'_>>>, Vec<LexError>) {
    let mut lexer = Lexer::new(source);
    lexer.run();
    (lexer.tokens, lexer.errors)
}

fn span(start: usize, end: usize) -> Span {
    (start..end).into()
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_operator_byte(byte: u8) -> bool {
    // The operator charset, exactly `one_of("-:!*/+=|&^?%")` in `lexer.rs`.
    matches!(
        byte,
        b'-' | b':' | b'!' | b'*' | b'/' | b'+' | b'=' | b'|' | b'&' | b'^' | b'?' | b'%'
    )
}

fn is_control_byte(byte: u8) -> bool {
    // `one_of("()[]{}<>;,.")` — `<`/`>` are control tokens (the parser reassembles
    // span-adjacent pairs into shifts), and `.` splits `?.`/`..` apart.
    matches!(
        byte,
        b'(' | b')' | b'[' | b']' | b'{' | b'}' | b'<' | b'>' | b';' | b',' | b'.'
    )
}

/// The control charset *inside* an interpolation hole: the top-level set minus the
/// braces, which delimit the hole (`one_of("()[]<>;,.")` in `lexer.rs`).
fn is_hole_control_byte(byte: u8) -> bool {
    is_control_byte(byte) && byte != b'{' && byte != b'}'
}

struct Lexer<'src> {
    source: &'src str,
    bytes: &'src [u8],
    position: usize,
    tokens: Vec<Spanned<Token<'src>>>,
    errors: Vec<LexError>,
}

impl<'src> Lexer<'src> {
    fn new(source: &'src str) -> Self {
        Lexer {
            source,
            bytes: source.as_bytes(),
            position: 0,
            tokens: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn run(&mut self) {
        loop {
            self.skip_trivia();
            if self.position >= self.bytes.len() {
                break;
            }
            self.lex_one();
        }
    }

    /// Skip a maximal run of trivia — whitespace and `//` line comments,
    /// interleaved — leaving `position` on the next token (or at end). Linear in
    /// the run's length (the pinned property: a quadratic trivia loop once made a
    /// blanked macro world take seconds).
    fn skip_trivia(&mut self) {
        loop {
            self.skip_whitespace();
            if self.bytes[self.position..].starts_with(b"//") {
                self.position += 2;
                // A comment runs to (not including) the next newline; the newline
                // is left for the whitespace pass. `\n` is ASCII, so scanning bytes
                // never lands inside a multi-byte character.
                while self.position < self.bytes.len() && self.bytes[self.position] != b'\n' {
                    self.position += 1;
                }
                continue;
            }
            break;
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(character) = self.current_char() {
            if character.is_whitespace() {
                self.position += character.len_utf8();
            } else {
                break;
            }
        }
    }

    /// The character at `position` (which is always a UTF-8 boundary), or `None` at
    /// end of input.
    fn current_char(&self) -> Option<char> {
        self.source[self.position..].chars().next()
    }

    /// Lex one lexeme at `position`, which is neither trivia nor end of input.
    fn lex_one(&mut self) {
        let start = self.position;
        let first = self.bytes[start];
        if first == b'i' && self.bytes.get(start + 1) == Some(&b'"') {
            if self.bytes[start + 1..].starts_with(b"\"\"\"") {
                self.lex_interpolated_multiline();
            } else {
                self.lex_interpolated();
            }
        } else if first == b'"' {
            match self.read_string(start) {
                Some((token, end)) => self.push(token, start, end),
                // An unterminated string: skip the opening quote and retry, exactly
                // as chumsky's skip-then-retry recovery does (`"unterminated` lexes
                // the tail as identifiers).
                None => self.skip_illegal(),
            }
        } else if first.is_ascii_digit() {
            let (token, end) = self.read_number(start);
            self.push(token, start, end);
        } else if is_ident_start(first) {
            let (token, end) = self.read_identifier(start);
            self.push(token, start, end);
        } else if is_operator_byte(first) {
            let (token, end) = self.read_operator(start, true);
            self.push(token, start, end);
        } else if is_control_byte(first) {
            self.push(Token::Ctrl(first as char), start, start + 1);
        } else {
            self.skip_illegal();
        }
    }

    fn push(&mut self, token: Token<'src>, start: usize, end: usize) {
        self.tokens.push((token, span(start, end)));
        self.position = end;
    }

    /// Record an un-lexable character and step over it (one whole char), leaving the
    /// rest of the stream to be lexed — chumsky's `skip_then_retry_until` recovery.
    fn skip_illegal(&mut self) {
        let character = self.current_char().expect("skip_illegal called at end");
        self.errors.push(LexError {
            position: self.position,
            character,
            rule: None,
        });
        self.position += character.len_utf8();
    }

    // --- Lexeme readers (pure: they compute a token and its end, never mutate) ---

    /// A numeric literal: a hex integer (`0x…`) or a decimal with an optional
    /// fraction, each with an optional identifier-shaped type suffix. `start` is a
    /// digit.
    fn read_number(&self, start: usize) -> (Token<'src>, usize) {
        // Hex (`0x` + at least one hex digit) is tried first, so `0xFF` is not read
        // as `0` with suffix `xFF`. `0X` (capital) and `0x` with no hex digit fall
        // through to the decimal path (`0` with an identifier suffix).
        if self.bytes[start] == b'0'
            && self.bytes.get(start + 1) == Some(&b'x')
            && self
                .bytes
                .get(start + 2)
                .is_some_and(|byte| byte.is_ascii_hexdigit())
        {
            let mut position = start + 2;
            while self
                .bytes
                .get(position)
                .is_some_and(|byte| byte.is_ascii_hexdigit())
            {
                position += 1;
            }
            let whole = &self.source[start..position];
            let (suffix, end) = self.read_optional_suffix(position);
            return (Token::Number(whole, None, suffix), end);
        }

        // Decimal integer part: a lone `0`, or `[1-9]` followed by any digits (the
        // no-leading-zero rule of `text::int`, so `007` is three `0`,`0`,`7`).
        let mut position = if self.bytes[start] == b'0' {
            start + 1
        } else {
            let mut position = start;
            while self.bytes.get(position).is_some_and(u8::is_ascii_digit) {
                position += 1;
            }
            position
        };
        let whole = &self.source[start..position];

        // A fraction (`.` then at least one digit). A `.` not followed by a digit
        // is left as a control token (`1.` is `1` then `.`).
        let mut fraction = None;
        if self.bytes.get(position) == Some(&b'.')
            && self.bytes.get(position + 1).is_some_and(u8::is_ascii_digit)
        {
            let fraction_start = position + 1;
            position += 1;
            while self.bytes.get(position).is_some_and(u8::is_ascii_digit) {
                position += 1;
            }
            fraction = Some(&self.source[fraction_start..position]);
        }

        let (suffix, end) = self.read_optional_suffix(position);
        (Token::Number(whole, fraction, suffix), end)
    }

    /// An optional identifier-shaped type suffix (`u32`, `f`, `n`, `_000`, …)
    /// starting at `position`. Returns the suffix slice (or `None`) and the new end.
    /// The suffix is a raw identifier slice — never keyword-classified — so `1if` is
    /// `Number("1", None, Some("if"))`, matching `text::ascii::ident().or_not()`.
    fn read_optional_suffix(&self, position: usize) -> (Option<&'src str>, usize) {
        if self
            .bytes
            .get(position)
            .is_some_and(|&byte| is_ident_start(byte))
        {
            let end = self.identifier_end(position);
            (Some(&self.source[position..end]), end)
        } else {
            (None, position)
        }
    }

    fn identifier_end(&self, start: usize) -> usize {
        let mut position = start + 1;
        while self
            .bytes
            .get(position)
            .is_some_and(|&byte| is_ident_continue(byte))
        {
            position += 1;
        }
        position
    }

    /// An identifier or keyword. `start` is an identifier-start byte.
    fn read_identifier(&self, start: usize) -> (Token<'src>, usize) {
        let end = self.identifier_end(start);
        let text = &self.source[start..end];
        let token = match text {
            "async" => Token::Async,
            "await" => Token::Await,
            "const" => Token::Const,
            "else" => Token::Else,
            "enum" => Token::Enum,
            "export" => Token::Export,
            "external" => Token::External,
            "false" => Token::Bool(false),
            "for" => Token::For,
            "fun" => Token::Fun,
            "if" => Token::If,
            "impl" => Token::Impl,
            "import" => Token::Import,
            "in" => Token::In,
            "is" => Token::Is,
            "jump" => Token::Jump,
            "let" => Token::Let,
            "macro" => Token::Macro,
            "match" => Token::Match,
            "mod" => Token::Mod,
            "mut" => Token::Mut,
            "null" => Token::Null,
            "own" => Token::Own,
            "borrows" => Token::Borrows,
            "ret" => Token::Ret,
            "resource" => Token::Resource,
            "struct" => Token::Struct,
            "trait" => Token::Trait,
            "type" => Token::Type,
            "true" => Token::Bool(true),
            "use" => Token::Use,
            "with" => Token::With,
            _ => Token::Ident(text),
        };
        (token, end)
    }

    /// An operator. `start` is an operator-charset byte. `allow_arrow` selects `=>`
    /// as its own token (`Token::Op("=>")`); inside an interpolation hole `=>` is
    /// not a token, so `=` and `>` split (`>` is a hole control character).
    ///
    /// Longest-match: the two-character operators win over their one-character
    /// prefixes, so `!*v` stays `!`,`*`,`v` and `&&&` is `&&`,`&`.
    fn read_operator(&self, start: usize, allow_arrow: bool) -> (Token<'src>, usize) {
        let first = self.bytes[start];
        let second = self.bytes.get(start + 1).copied();
        let two_characters = match (first, second) {
            (b'=', Some(b'>')) => allow_arrow,
            (b'!', Some(b'=')) => true,
            (b'%', Some(b'=')) => true,
            (b'&', Some(b'&')) => true,
            (b'*', Some(b'=')) => true,
            (b'+', Some(b'=')) => true,
            (b'-', Some(b'=')) => true,
            (b'/', Some(b'=')) => true,
            (b':', Some(b':')) => true,
            (b'=', Some(b'=')) => true,
            (b'|', Some(b'|')) => true,
            _ => false,
        };
        let end = if two_characters { start + 2 } else { start + 1 };
        (Token::Op(&self.source[start..end]), end)
    }

    /// A string literal. `start` is `"`. Returns the token and its end, or `None`
    /// if the string is unterminated. A triple-quoted `"""…"""` is tried first (it
    /// is raw and runs to the first `"""`); otherwise a `"…"` string whose body is
    /// kept raw (escapes are interpreted at code generation).
    fn read_string(&self, start: usize) -> Option<(Token<'src>, usize)> {
        if self.bytes[start..].starts_with(b"\"\"\"") {
            let content_start = start + 3;
            let closing = self.source[content_start..].find("\"\"\"")?;
            let content_end = content_start + closing;
            let content = &self.source[content_start..content_end];
            return Some((Token::MultilineString(content), content_end + 3));
        }

        let content_start = start + 1;
        let mut position = content_start;
        loop {
            match self.bytes.get(position) {
                None => return None,
                Some(b'"') => break,
                Some(b'\\') => {
                    // A backslash escapes the next character (so `\"` does not close
                    // the string). The escaped character may be multi-byte.
                    let escaped = self.source[position + 1..].chars().next()?;
                    position += 1 + escaped.len_utf8();
                }
                Some(_) => {
                    let character = self.source[position..]
                        .chars()
                        .next()
                        .expect("byte present implies a character");
                    position += character.len_utf8();
                }
            }
        }
        let content = &self.source[content_start..position];
        Some((Token::String(content), position + 1))
    }

    // --- Interpolated strings ------------------------------------------------

    /// Desugar `i"…{expr}…"` in place into the token sequence for a parenthesised
    /// string concatenation, e.g. `i"a{x}b"` becomes
    /// `( "" + "a" + ( x ) + "b" )`. Every *wrapper* token (the outer parens, the
    /// seed `""`, the `+`s, and the literal fragments) carries the whole i-string's
    /// span; the hole's tokens carry their own spans and its parens carry the
    /// `{…}` span. `position` is the leading `i`.
    fn lex_interpolated(&mut self) {
        let istring_start = self.position;
        self.position += 2; // `i` and the opening `"`

        // Scan the body into parts first: the wrapper tokens need the closing
        // quote's position, which is only known once the body is consumed.
        let mut parts: Vec<IStringPart<'src>> = Vec::new();
        let close = loop {
            match self.bytes.get(self.position) {
                None => break None, // unterminated
                Some(b'"') => break Some(self.position),
                Some(b'{') => parts.push(IStringPart::Hole(self.lex_hole())),
                Some(b'\\') => match self.bytes.get(self.position + 1) {
                    // `\{` / `\}` collapse to the brace itself (the slice is the
                    // brace character only).
                    Some(b'{') | Some(b'}') => {
                        let brace = &self.source[self.position + 1..self.position + 2];
                        parts.push(IStringPart::Text(brace));
                        self.position += 2;
                    }
                    // Any other escape is kept raw as a `\X` fragment (interpreted
                    // at code generation, like a plain string).
                    Some(_) => {
                        let escaped = self.source[self.position + 1..]
                            .chars()
                            .next()
                            .expect("byte present implies a character");
                        let mut end = self.position + 1 + escaped.len_utf8();
                        // A CRLF is ONE line terminator (windows-support.md §2), so
                        // an escape whose escaped character is the CR must take the
                        // whole pair. This is one of exactly TWO scanners that end a
                        // fragment on a character COUNT rather than a delimiter (the
                        // other is `lex_multiline_escape`, the i-triple form's twin
                        // of this branch), so these two are the only places a pair
                        // could split across two `String` tokens — where the
                        // per-token normalization that builds the value can no
                        // longer see it, and the CR would survive into a value its
                        // LF twin does not have. A third count-based scanner must
                        // take the pair the same way. (A plain `"…"` is one
                        // contiguous token, so it cannot split.)
                        if escaped == '\r' && self.bytes.get(end) == Some(&b'\n') {
                            end += 1;
                        }
                        parts.push(IStringPart::Text(&self.source[self.position..end]));
                        self.position = end;
                    }
                    None => break None,
                },
                // A bare, unmatched `}` makes the i-string malformed (a clean source
                // never reaches here); record it and stop the body scan.
                Some(b'}') => {
                    self.skip_illegal();
                    break None;
                }
                Some(_) => {
                    let text_start = self.position;
                    while let Some(&byte) = self.bytes.get(self.position) {
                        if matches!(byte, b'{' | b'}' | b'"' | b'\\') {
                            break;
                        }
                        let character = self.source[self.position..]
                            .chars()
                            .next()
                            .expect("byte present implies a character");
                        self.position += character.len_utf8();
                    }
                    parts.push(IStringPart::Text(&self.source[text_start..self.position]));
                }
            }
        };

        let Some(close) = close else {
            // Unterminated: best-effort. A clean source never gets here; chumsky
            // discards its whole output in this case (a recovery pathology recorded
            // for S4). We keep what we scanned and record the error.
            self.errors.push(LexError {
                position: istring_start,
                character: 'i',
                rule: None,
            });
            return;
        };

        self.emit_interpolated(parts, span(istring_start, close + 1));
        self.position = close + 1;
    }

    /// Push the token sequence for an interpolated string's `parts`: the outer
    /// parens, the seed `""`, and a `+` before every part — all carrying `whole`,
    /// the literal's own span.
    fn emit_interpolated(&mut self, parts: Vec<IStringPart<'src>>, whole: Span) {
        self.tokens.push((Token::Ctrl('('), whole));
        self.tokens.push((Token::String(""), whole));
        for part in parts {
            self.tokens.push((Token::Op("+"), whole));
            match part {
                IStringPart::Text(text) => self.tokens.push((Token::String(text), whole)),
                IStringPart::Hole(hole_tokens) => self.tokens.extend(hole_tokens),
            }
        }
        self.tokens.push((Token::Ctrl(')'), whole));
    }

    /// Desugar `i"""…"""` (backlog H7) into the same parenthesised concatenation
    /// `lex_interpolated` produces. `position` is the leading `i`.
    ///
    /// Two rules meet here, in this order:
    ///
    /// 1. **Trimming happens first, on the raw text.** The literal's inner text is
    ///    laid out by [`crate::util::multiline_layout`] — the same rule, the same
    ///    code, as a plain `"""` — with holes and `\{` / `\}` counting as ordinary
    ///    characters of that text. So a hole never disturbs its line's indent
    ///    accounting: the closing delimiter's indentation is stripped from the
    ///    start of every content line whether that line goes on to open with a
    ///    hole, with text, or with an escape. (A hole may span lines; its
    ///    continuation lines carry the prefix like any other, and stripping is a
    ///    no-op inside the hole, where whitespace is trivia.) A literal whose
    ///    shape breaks the rule degrades to its plain twin so the analyzer reports
    ///    the precise error.
    /// 2. **Fragmenting happens second, on the trimmed text.** Exactly two escapes
    ///    exist: `\{` and `\}` for a literal brace. Everything else is raw — a
    ///    backslash before any other character is a literal backslash and that
    ///    character, exactly as in a plain `"""`, with no `\n` / `\t` processing.
    ///
    /// Rule 2 is delivered by the fragments themselves: a literal fragment is a
    /// slice of the source, and `transformer::unescape_string` reads it at code
    /// generation, so a fragment must never CONTAIN a backslash. Every backslash
    /// is emitted as [`RAW_BACKSLASH`] instead, which unescapes back to one.
    fn lex_interpolated_multiline(&mut self) {
        let istring_start = self.position;
        let content_start = istring_start + 4; // `i` and the opening `"""`
        let Some(closing) = self.source[content_start..].find("\"\"\"") else {
            // Unterminated. There is no resynchronisation point inside an
            // unclosed multi-line literal (its body may hold anything), so the
            // rest of the input belongs to the string: record the error and stop.
            // The parser still gets every token lexed before it.
            self.errors.push(LexError {
                position: istring_start,
                character: 'i',
                rule: None,
            });
            self.position = self.bytes.len();
            return;
        };
        let raw_end = content_start + closing;
        let raw = &self.source[content_start..raw_end];
        let whole = span(istring_start, raw_end + 3);

        let layout = match crate::util::multiline_layout(raw) {
            Ok(layout) => layout,
            Err(_) => {
                // A malformed shape degrades to its plain twin: the identical
                // `"""…"""` text as a multiline-string token, whose already
                // shipped validation in the analyzer reports the exact offender.
                // The token keeps the WHOLE literal's span (the `i` included), so
                // the formatter still recovers it verbatim and the analyzer's
                // error base — measured back from the span's end — still lands on
                // the raw text.
                self.tokens.push((Token::MultilineString(raw), whole));
                self.position = raw_end + 3;
                return;
            }
        };

        let content_start_offset = content_start + layout.content.start;
        let mut content_end = content_start + layout.content.end;
        // The last content line's trailing `\r` belongs to the line terminator the
        // trimming removes (`trim_multiline_string` strips it per line). Dropping
        // it here keeps a CRLF file's value byte-identical to its LF twin — the
        // rest of the pairs stay inside a fragment, where `unescape_string`
        // normalizes them.
        if self.source[content_start_offset..content_end].ends_with('\r') {
            content_end -= 1;
        }

        let mut parts: Vec<IStringPart<'src>> = Vec::new();
        let mut at_line_start = true;
        self.position = content_start_offset;
        while self.position < content_end {
            if at_line_start {
                at_line_start = false;
                self.skip_multiline_indentation(layout.prefix, content_end);
                continue;
            }
            match self.bytes[self.position] {
                b'{' => parts.push(IStringPart::Hole(self.lex_hole())),
                // A bare `}` is malformed, exactly as in `i"…"`: `\}` is one of the
                // two escapes that exist, which is only meaningful if an unescaped
                // `}` is not already a literal one — and the shape it catches is a
                // hole whose `}` was forgotten. Unlike `i"…"` the literal is not
                // abandoned (a multi-line body swallows far too much source for
                // that): the offender is reported and read as text.
                b'}' => {
                    self.errors.push(LexError {
                        position: self.position,
                        character: '}',
                        rule: Some(UNESCAPED_BRACE),
                    });
                    parts.push(IStringPart::Text(
                        &self.source[self.position..self.position + 1],
                    ));
                    self.position += 1;
                }
                b'\\' => self.lex_multiline_escape(&mut parts, content_end, &mut at_line_start),
                _ => {
                    let text_start = self.position;
                    while self.position < content_end {
                        let byte = self.bytes[self.position];
                        if matches!(byte, b'{' | b'}' | b'\\') {
                            break;
                        }
                        let character = self.source[self.position..]
                            .chars()
                            .next()
                            .expect("byte present implies a character");
                        self.position += character.len_utf8();
                        if byte == b'\n' {
                            // The fragment ends WITH its line terminator (a `\r\n`
                            // pair stays contiguous inside it); the next line's
                            // indentation is skipped, not emitted.
                            at_line_start = true;
                            break;
                        }
                    }
                    parts.push(IStringPart::Text(&self.source[text_start..self.position]));
                }
            }
        }

        self.emit_interpolated(parts, whole);
        self.position = raw_end + 3;
    }

    /// At the start of a content line: step over the indentation prefix
    /// [`crate::util::multiline_layout`] validated. A whitespace-only line may
    /// fall short of the prefix — it contributes nothing, so the whole of it goes.
    fn skip_multiline_indentation(&mut self, prefix: &str, content_end: usize) {
        let rest = &self.source[self.position..content_end];
        if rest.starts_with(prefix) {
            self.position += prefix.len();
        } else {
            self.position = match rest.find('\n') {
                Some(offset) => self.position + offset,
                None => content_end,
            };
        }
    }

    /// One `\`-led sequence in an `i"""…"""` body. `\{` / `\}` collapse to the
    /// brace; every other backslash is literal, and so is the character after it.
    fn lex_multiline_escape(
        &mut self,
        parts: &mut Vec<IStringPart<'src>>,
        content_end: usize,
        at_line_start: &mut bool,
    ) {
        let escaped = self.source[self.position + 1..content_end].chars().next();
        match escaped {
            Some('{') | Some('}') => {
                parts.push(IStringPart::Text(
                    &self.source[self.position + 1..self.position + 2],
                ));
                self.position += 2;
            }
            // A lone backslash at the very end of the content (the line terminator
            // the trimming removed is not its to escape).
            None => {
                parts.push(IStringPart::Text(RAW_BACKSLASH));
                self.position += 1;
            }
            Some(character) => {
                parts.push(IStringPart::Text(RAW_BACKSLASH));
                let mut end = self.position + 1 + character.len_utf8();
                // A CRLF is ONE line terminator (windows-support.md §2): an escape
                // whose escaped character is the CR must take the whole pair, or
                // the pair splits across two fragments — where the per-fragment
                // normalization that builds the value can no longer see it, and
                // the CR survives into a value its LF twin does not have. This is
                // the second of exactly two count-based fragment scanners (the
                // other is `lex_interpolated`'s escape branch — the single-quoted
                // twin of this one); both must take the pair, and so must any
                // third that ever joins them.
                if character == '\r' && end < content_end && self.bytes[end] == b'\n' {
                    end += 1;
                }
                if character == '\\' {
                    parts.push(IStringPart::Text(RAW_BACKSLASH));
                } else {
                    parts.push(IStringPart::Text(&self.source[self.position + 1..end]));
                }
                if self.source[self.position + 1..end].ends_with('\n') {
                    *at_line_start = true;
                }
                self.position = end;
            }
        }
    }

    /// Lex one interpolation hole `{…}` into its parenthesised token list. The
    /// hole's parens carry the `{…}` span; the inner tokens carry their own. Hole
    /// tokens differ from top-level ones: no `=>` arrow, braces are not control
    /// characters (they delimit the hole), no comments, and no nested i-string
    /// desugaring (a `i"…"` in a hole is an `i` identifier then a string).
    /// `position` is the opening `{`.
    fn lex_hole(&mut self) -> Vec<Spanned<Token<'src>>> {
        let brace_open = self.position;
        self.position += 1; // `{`
        let mut inner = Vec::new();
        let brace_close = loop {
            self.skip_whitespace();
            match self.bytes.get(self.position) {
                None => break self.position, // unterminated; best-effort
                Some(b'}') => break self.position,
                Some(_) => match self.lex_hole_token() {
                    Some(token) => inner.push(token),
                    // A construct no hole token matches (a nested `{`, an illegal
                    // char) makes the hole malformed; stop (clean sources never do).
                    None => break self.position,
                },
            }
        };
        let hole_span = span(brace_open, brace_close + 1);
        self.position = (brace_close + 1).min(self.bytes.len());

        let mut wrapped = Vec::with_capacity(inner.len() + 2);
        wrapped.push((Token::Ctrl('('), hole_span));
        wrapped.extend(inner);
        wrapped.push((Token::Ctrl(')'), hole_span));
        wrapped
    }

    /// Lex one token inside an interpolation hole, or `None` if the current
    /// character starts no hole token. Whitespace is already skipped.
    fn lex_hole_token(&mut self) -> Option<Spanned<Token<'src>>> {
        let start = self.position;
        let first = self.bytes[start];
        let (token, end) = if first == b'"' {
            // An unterminated string inside a hole cannot be recovered locally; the
            // hole is malformed. A clean source never gets here.
            self.read_string(start)?
        } else if first.is_ascii_digit() {
            self.read_number(start)
        } else if is_ident_start(first) {
            self.read_identifier(start)
        } else if is_operator_byte(first) {
            self.read_operator(start, false)
        } else if is_hole_control_byte(first) {
            (Token::Ctrl(first as char), start + 1)
        } else {
            return None;
        };
        self.position = end;
        Some((token, span(start, end)))
    }
}

enum IStringPart<'src> {
    Text(&'src str),
    Hole(Vec<Spanned<Token<'src>>>),
}

/// The rule an unescaped `}` in an interpolated string breaks. Curated
/// (diagnostics-standard.md B6): the braces are the hole's, and the sanctioned
/// spelling of a literal one is named.
const UNESCAPED_BRACE: &str = "a literal `}` inside an interpolated string is written `\\}` — an unescaped \
     brace belongs to a `{expr}` hole";

/// One literal backslash, as an `i"""…"""` fragment must spell it. A fragment is
/// read back by `transformer::unescape_string`, so a backslash that must survive
/// as itself cannot be a slice of the source — it is this two-character escape
/// instead. (`&'static str` coerces to any `&'src str`, exactly as the seed `""`
/// fragment does.)
const RAW_BACKSLASH: &str = "\\\\";

#[cfg(test)]
mod tests {
    use super::*;

    /// Tokens only, asserting a clean lex (no errors) — mirrors `lexer.rs`'s helper
    /// so the two lexers' pins read alike.
    fn lex(source: &str) -> Vec<Token<'_>> {
        let (tokens, errors) = tokenize(source);
        assert!(errors.is_empty(), "lex errors: {errors:?}");
        tokens.into_iter().map(|(token, _span)| token).collect()
    }

    /// `(token, start, end)` triples, for span-inclusive pins.
    fn lex_spanned(source: &str) -> Vec<(Token<'_>, usize, usize)> {
        let (tokens, errors) = tokenize(source);
        assert!(errors.is_empty(), "lex errors: {errors:?}");
        tokens
            .into_iter()
            .map(|(token, span)| {
                let range = span.into_range();
                (token, range.start, range.end)
            })
            .collect()
    }

    // --- Trivia (the carried-over `lexer.rs` pins) ---------------------------

    #[test]
    fn trivia_only_files_lex_to_empty_streams() {
        assert!(lex("").is_empty());
        assert!(lex("   \n\t \n").is_empty());
        assert!(lex("// just a comment").is_empty());
        assert!(lex("  // padded comment\n   ").is_empty());
        assert!(lex("// one\n// two\n").is_empty());
    }

    #[test]
    fn leading_trivia_interleavings_reach_the_first_token() {
        let expected = vec![Token::Fun, Token::Ident("main")];
        assert_eq!(lex("fun main"), expected);
        assert_eq!(lex("   \n\n  fun main"), expected);
        assert_eq!(lex("// header\nfun main"), expected);
        assert_eq!(lex("  \n// a\n  // b\n\n  fun main"), expected);
    }

    #[test]
    fn trivia_between_and_after_tokens() {
        assert_eq!(
            lex("fun // trailing comment\n   main   // eof comment"),
            vec![Token::Fun, Token::Ident("main")]
        );
        assert_eq!(lex("main // no newline"), vec![Token::Ident("main")]);
    }

    // A blanked macro world is dominated by huge whitespace runs; the trivia skip
    // must stay linear (a quadratic loop once took seconds per world).
    #[test]
    fn huge_whitespace_runs_lex_in_linear_time() {
        let mut source = String::new();
        for _ in 0..20_000 {
            source.push_str("                                                \n");
        }
        source.push_str("fun main");
        let start = std::time::Instant::now();
        assert_eq!(lex(&source), vec![Token::Fun, Token::Ident("main")]);
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "lexing a ~1MB whitespace prefix took {:?} — the trivia loop has gone quadratic",
            start.elapsed()
        );
    }

    // --- Operators (longest match, the `<`/`>`-are-control split) ------------

    #[test]
    fn multi_character_operators_win_over_their_prefixes() {
        for operator in ["!=", "%=", "&&", "*=", "+=", "-=", "/=", "::", "==", "||"] {
            assert_eq!(
                lex(operator),
                vec![Token::Op(operator)],
                "lexing {operator:?}"
            );
        }
        assert_eq!(lex("=>"), vec![Token::Op("=>")]);
        // The boundary holds mid-stream: `x-=-y` is `x`,`-=`,`-`,`y`.
        assert_eq!(
            lex("x-=-y"),
            vec![
                Token::Ident("x"),
                Token::Op("-="),
                Token::Op("-"),
                Token::Ident("y"),
            ]
        );
        // A run of an operator character munches longest-first, not maximally.
        assert_eq!(lex("&&&"), vec![Token::Op("&&"), Token::Op("&")]);
        assert_eq!(lex(":::"), vec![Token::Op("::"), Token::Op(":")]);
    }

    #[test]
    fn adjacent_prefix_operators_do_not_fuse() {
        assert_eq!(
            lex("!*v"),
            vec![Token::Op("!"), Token::Op("*"), Token::Ident("v")]
        );
        assert_eq!(
            lex("!!b"),
            vec![Token::Op("!"), Token::Op("!"), Token::Ident("b")]
        );
        assert_eq!(
            lex("-*v"),
            vec![Token::Op("-"), Token::Op("*"), Token::Ident("v")]
        );
    }

    #[test]
    fn angle_brackets_are_control_tokens_and_split() {
        // `<`/`>` are always `Ctrl` — the parser reassembles adjacent pairs into
        // shifts. `<=`/`>=` split into a control and an `=` operator; `?.`/`..`
        // split on the `.` control character.
        assert_eq!(lex("<"), vec![Token::Ctrl('<')]);
        assert_eq!(lex("<<"), vec![Token::Ctrl('<'), Token::Ctrl('<')]);
        assert_eq!(lex(">>"), vec![Token::Ctrl('>'), Token::Ctrl('>')]);
        assert_eq!(lex("<="), vec![Token::Ctrl('<'), Token::Op("=")]);
        assert_eq!(lex(">="), vec![Token::Ctrl('>'), Token::Op("=")]);
        assert_eq!(lex("?."), vec![Token::Op("?"), Token::Ctrl('.')]);
        assert_eq!(lex(".."), vec![Token::Ctrl('.'), Token::Ctrl('.')]);
    }

    // --- Keywords and identifiers -------------------------------------------

    #[test]
    fn keywords_classify_and_identifiers_do_not() {
        let keywords = [
            ("async", Token::Async),
            ("await", Token::Await),
            ("const", Token::Const),
            ("else", Token::Else),
            ("enum", Token::Enum),
            ("export", Token::Export),
            ("external", Token::External),
            ("false", Token::Bool(false)),
            ("for", Token::For),
            ("fun", Token::Fun),
            ("if", Token::If),
            ("impl", Token::Impl),
            ("import", Token::Import),
            ("in", Token::In),
            ("is", Token::Is),
            ("jump", Token::Jump),
            ("let", Token::Let),
            ("macro", Token::Macro),
            ("match", Token::Match),
            ("mod", Token::Mod),
            ("mut", Token::Mut),
            ("null", Token::Null),
            ("own", Token::Own),
            ("borrows", Token::Borrows),
            ("ret", Token::Ret),
            ("resource", Token::Resource),
            ("struct", Token::Struct),
            ("trait", Token::Trait),
            ("type", Token::Type),
            ("true", Token::Bool(true)),
            ("use", Token::Use),
            ("with", Token::With),
        ];
        for (text, token) in keywords {
            assert_eq!(lex(text), vec![token], "keyword {text:?}");
        }
        // A keyword with trailing identifier characters is an identifier.
        assert_eq!(lex("asyncx"), vec![Token::Ident("asyncx")]);
        assert_eq!(lex("await123"), vec![Token::Ident("await123")]);
        assert_eq!(lex("_foo"), vec![Token::Ident("_foo")]);
        assert_eq!(lex("_"), vec![Token::Ident("_")]);
    }

    // --- Numbers ------------------------------------------------------------

    #[test]
    fn numeric_literals_split_whole_fraction_and_suffix() {
        assert_eq!(lex("0"), vec![Token::Number("0", None, None)]);
        assert_eq!(lex("123"), vec![Token::Number("123", None, None)]);
        assert_eq!(lex("1.5"), vec![Token::Number("1", Some("5"), None)]);
        assert_eq!(lex("0.000"), vec![Token::Number("0", Some("000"), None)]);
        assert_eq!(lex("0u32"), vec![Token::Number("0", None, Some("u32"))]);
        assert_eq!(
            lex("1.5u32"),
            vec![Token::Number("1", Some("5"), Some("u32"))]
        );
        assert_eq!(lex("1_000"), vec![Token::Number("1", None, Some("_000"))]);
        // No leading zeros (`text::int`): `007` is three `0`,`0`,`7`.
        assert_eq!(
            lex("007"),
            vec![
                Token::Number("0", None, None),
                Token::Number("0", None, None),
                Token::Number("7", None, None),
            ]
        );
        // A `.` not followed by a digit stays a control token.
        assert_eq!(
            lex("1.foo"),
            vec![
                Token::Number("1", None, None),
                Token::Ctrl('.'),
                Token::Ident("foo"),
            ]
        );
    }

    #[test]
    fn hex_literals_keep_the_prefix_and_optional_suffix() {
        assert_eq!(lex("0xFF"), vec![Token::Number("0xFF", None, None)]);
        assert_eq!(lex("0xFFf"), vec![Token::Number("0xFFf", None, None)]);
        assert_eq!(
            lex("0x80000000u32"),
            vec![Token::Number("0x80000000", None, Some("u32"))]
        );
        assert_eq!(
            lex("0xDEADn"),
            vec![Token::Number("0xDEAD", None, Some("n"))]
        );
        // `0X` (capital) and a bare `0x` are not hex: `0` with an identifier suffix.
        assert_eq!(lex("0X10"), vec![Token::Number("0", None, Some("X10"))]);
        assert_eq!(lex("0x"), vec![Token::Number("0", None, Some("x"))]);
        assert_eq!(lex("0xg"), vec![Token::Number("0", None, Some("xg"))]);
    }

    // --- Strings ------------------------------------------------------------

    #[test]
    fn strings_keep_raw_bodies() {
        assert_eq!(lex(r#""hello""#), vec![Token::String("hello")]);
        assert_eq!(lex(r#""""#), vec![Token::String("")]);
        // The body is raw (escapes not yet interpreted): `\"` does not close.
        assert_eq!(
            lex(r#""with \"escaped\" quotes""#),
            vec![Token::String(r#"with \"escaped\" quotes"#)]
        );
        // A `"…"` string may span lines.
        assert_eq!(lex("\"a\nb\""), vec![Token::String("a\nb")]);
        // …including with CRLF endings, which the token keeps RAW like every
        // other body character. The `\r\n`-is-one-line-terminator rule
        // (windows-support.md §2, spec §2) applies where the literal's VALUE is
        // built — `transformer::unescape_string` — not here, so that spans keep
        // addressing the file exactly as it sits on disk.
        assert_eq!(lex("\"a\r\nb\""), vec![Token::String("a\r\nb")]);
        // A triple-quoted string runs to the first `"""` and may hold a lone `"`.
        assert_eq!(
            lex(r#""""with " inner""""#),
            vec![Token::MultilineString(r#"with " inner"#)]
        );
    }

    // --- Interpolated strings (desugaring shape + the span quirk) ------------

    #[test]
    fn interpolated_strings_desugar_to_a_concatenation() {
        assert_eq!(
            lex(r#"i"a{x}b""#),
            vec![
                Token::Ctrl('('),
                Token::String(""),
                Token::Op("+"),
                Token::String("a"),
                Token::Op("+"),
                Token::Ctrl('('),
                Token::Ident("x"),
                Token::Ctrl(')'),
                Token::Op("+"),
                Token::String("b"),
                Token::Ctrl(')'),
            ]
        );
        // Empty i-string, escaped braces, and a keyword inside a hole.
        assert_eq!(
            lex(r#"i"""#),
            vec![Token::Ctrl('('), Token::String(""), Token::Ctrl(')')]
        );
        assert_eq!(
            lex(r#"i"\{x\}""#),
            vec![
                Token::Ctrl('('),
                Token::String(""),
                Token::Op("+"),
                Token::String("{"),
                Token::Op("+"),
                Token::String("x"),
                Token::Op("+"),
                Token::String("}"),
                Token::Ctrl(')'),
            ]
        );
    }

    #[test]
    fn an_escaped_crlf_stays_in_one_interpolated_fragment() {
        // The fragment scanner ends an escape on a character COUNT, so `\` before
        // a CRLF would otherwise end the fragment BETWEEN the CR and the LF —
        // splitting one line terminator across two `String` tokens, where the
        // per-token normalization that builds the value can no longer see the
        // pair (windows-support.md §2). The pair must ride in one fragment.
        assert_eq!(
            lex("i\"a\\\r\nb\""),
            vec![
                Token::Ctrl('('),
                Token::String(""),
                Token::Op("+"),
                Token::String("a"),
                Token::Op("+"),
                Token::String("\\\r\n"),
                Token::Op("+"),
                Token::String("b"),
                Token::Ctrl(')'),
            ]
        );
        // A LONE `\r` after the backslash is not a line terminator, so the escape
        // takes exactly the CR — one character, as before.
        assert_eq!(
            lex("i\"a\\\rb\""),
            vec![
                Token::Ctrl('('),
                Token::String(""),
                Token::Op("+"),
                Token::String("a"),
                Token::Op("+"),
                Token::String("\\\r"),
                Token::Op("+"),
                Token::String("b"),
                Token::Ctrl(')'),
            ]
        );
        // An UNESCAPED CRLF needs nothing: the text run is delimiter-driven, so
        // the pair is already contiguous inside one fragment.
        assert_eq!(
            lex("i\"a\r\nb\""),
            vec![
                Token::Ctrl('('),
                Token::String(""),
                Token::Op("+"),
                Token::String("a\r\nb"),
                Token::Ctrl(')'),
            ]
        );
    }

    #[test]
    fn interpolated_string_spans_reproduce_the_chumsky_quirk() {
        // Every wrapper token carries the WHOLE i-string span (`i` through the byte
        // past the closing quote); the hole's tokens carry their own spans and its
        // parens carry the `{…}` span. Reproduced byte-for-byte from chumsky —
        // recorded for the S4/S5 span-quality pass, not to be "corrected" here.
        //   i " H  e  l  l  o  {  n  a  m  e  }  "
        //   0 1 2  3  4  5  6  7  8  9 10 11 12 13
        assert_eq!(
            lex_spanned(r#"i"Hello{name}""#),
            vec![
                (Token::Ctrl('('), 0, 14),
                (Token::String(""), 0, 14),
                (Token::Op("+"), 0, 14),
                (Token::String("Hello"), 0, 14),
                (Token::Op("+"), 0, 14),
                (Token::Ctrl('('), 7, 13),
                (Token::Ident("name"), 8, 12),
                (Token::Ctrl(')'), 7, 13),
                (Token::Ctrl(')'), 0, 14),
            ]
        );
    }

    #[test]
    fn hole_tokens_differ_from_top_level_tokens() {
        // Inside a hole `=>` is not an arrow (`>` is a control character) and a
        // nested `i"…"` is an identifier `i` then a string — no re-desugaring.
        assert_eq!(
            lex(r#"i"{a => b}""#),
            vec![
                Token::Ctrl('('),
                Token::String(""),
                Token::Op("+"),
                Token::Ctrl('('),
                Token::Ident("a"),
                Token::Op("="),
                Token::Ctrl('>'),
                Token::Ident("b"),
                Token::Ctrl(')'),
                Token::Ctrl(')'),
            ]
        );
        assert_eq!(
            lex(r#"i"{i"x"}""#),
            vec![
                Token::Ctrl('('),
                Token::String(""),
                Token::Op("+"),
                Token::Ctrl('('),
                Token::Ident("i"),
                Token::String("x"),
                Token::Ctrl(')'),
                Token::Ctrl(')'),
            ]
        );
    }

    // --- Interpolated triple-quoted strings (backlog H7) ---------------------

    #[test]
    fn an_interpolated_triple_quoted_string_fragments_the_trimmed_text() {
        // Trimming first, on the RAW text: the opening line's newline and the
        // closing line's indentation are gone, and every content line loses the
        // prefix — whether it opens with text or with a hole. Fragmenting second:
        // a fragment ends WITH its line terminator, so the next line's prefix can
        // be skipped rather than emitted.
        assert_eq!(
            lex("i\"\"\"\n\ta {x}\n\t{y} b\n\t\"\"\""),
            vec![
                Token::Ctrl('('),
                Token::String(""),
                Token::Op("+"),
                Token::String("a "),
                Token::Op("+"),
                Token::Ctrl('('),
                Token::Ident("x"),
                Token::Ctrl(')'),
                Token::Op("+"),
                Token::String("\n"),
                Token::Op("+"),
                Token::Ctrl('('),
                Token::Ident("y"),
                Token::Ctrl(')'),
                Token::Op("+"),
                Token::String(" b"),
                Token::Ctrl(')'),
            ]
        );
        // Zero content lines is the empty string, exactly as `"""` is.
        assert_eq!(
            lex("i\"\"\"\n\t\"\"\""),
            vec![Token::Ctrl('('), Token::String(""), Token::Ctrl(')')]
        );
    }

    #[test]
    fn an_interpolated_triple_quoted_string_has_only_the_two_brace_escapes() {
        // `\{` / `\}` collapse to the brace. Every other backslash is literal —
        // and so is the character after it — so the fragment cannot be a slice of
        // the source (`unescape_string` reads it back): it is `RAW_BACKSLASH`.
        assert_eq!(
            lex("i\"\"\"\n\t\\{a\\} \\n\n\t\"\"\""),
            vec![
                Token::Ctrl('('),
                Token::String(""),
                Token::Op("+"),
                Token::String("{"),
                Token::Op("+"),
                Token::String("a"),
                Token::Op("+"),
                Token::String("}"),
                Token::Op("+"),
                Token::String(" "),
                Token::Op("+"),
                Token::String("\\\\"),
                Token::Op("+"),
                Token::String("n"),
                Token::Ctrl(')'),
            ]
        );
    }

    #[test]
    fn an_escaped_crlf_stays_in_one_interpolated_triple_quoted_fragment() {
        // The same character-COUNT hazard as the single-quoted form: a `\` before
        // a CRLF must take the whole pair, or the terminator splits across two
        // fragments and the CR survives into a value its LF twin does not have.
        assert_eq!(
            lex("i\"\"\"\r\n\ta\\\r\n\tb\r\n\t\"\"\""),
            vec![
                Token::Ctrl('('),
                Token::String(""),
                Token::Op("+"),
                Token::String("a"),
                Token::Op("+"),
                Token::String("\\\\"),
                Token::Op("+"),
                Token::String("\r\n"),
                Token::Op("+"),
                Token::String("b"),
                Token::Ctrl(')'),
            ]
        );
        // The LAST content line's `\r` belongs to the terminator the trimming
        // removes, so a trailing `\` there stays a lone backslash — the LF twin's
        // shape exactly.
        assert_eq!(
            lex("i\"\"\"\r\n\ta\\\r\n\t\"\"\""),
            vec![
                Token::Ctrl('('),
                Token::String(""),
                Token::Op("+"),
                Token::String("a"),
                Token::Op("+"),
                Token::String("\\\\"),
                Token::Ctrl(')'),
            ]
        );
        assert_eq!(
            lex("i\"\"\"\n\ta\\\n\t\"\"\""),
            vec![
                Token::Ctrl('('),
                Token::String(""),
                Token::Op("+"),
                Token::String("a"),
                Token::Op("+"),
                Token::String("\\\\"),
                Token::Ctrl(')'),
            ]
        );
    }

    #[test]
    fn a_malformed_interpolated_triple_quoted_string_degrades_to_its_plain_twin() {
        // The shape rule is one implementation (`util::multiline_layout`), and so
        // is its diagnostic: a literal that breaks the rule becomes the very
        // `"""…"""` token its text would lex as without the `i`, and the
        // analyzer's shipped validation reports the exact offender. The span
        // still covers the WHOLE literal, the `i` included.
        assert_eq!(
            lex_spanned("i\"\"\"oops\n\"\"\""),
            vec![(Token::MultilineString("oops\n"), 0, 12)]
        );
    }

    #[test]
    fn an_unterminated_interpolated_triple_quoted_string_records_one_error() {
        // Mid-edit in an editor. There is no resynchronisation point inside an
        // unclosed multi-line literal, so the rest of the input belongs to the
        // string: the tokens before it survive, one error is recorded, and the
        // scan terminates (a lexer that failed to advance would hang the server).
        let (tokens, errors) = tokenize("fun f() {\n\tlet x = i\"\"\"\n\tmid edit\n");
        let bare: Vec<Token> = tokens.into_iter().map(|(token, _)| token).collect();
        assert_eq!(
            bare,
            vec![
                Token::Fun,
                Token::Ident("f"),
                Token::Ctrl('('),
                Token::Ctrl(')'),
                Token::Ctrl('{'),
                Token::Let,
                Token::Ident("x"),
                Token::Op("="),
            ]
        );
        assert_eq!(
            errors,
            vec![LexError {
                position: 19,
                character: 'i',
                rule: None,
            }]
        );
    }

    // --- Illegal characters (the error value shape is S1's own choice) -------

    #[test]
    fn illegal_characters_are_skipped_and_recorded() {
        // The token stream skips the illegal character and lexes the rest — the
        // shape the downstream `lexer_skips_an_illegal_character` pin relies on.
        let (tokens, errors) = tokenize("x@y");
        let bare: Vec<Token> = tokens.into_iter().map(|(token, _)| token).collect();
        assert_eq!(bare, vec![Token::Ident("x"), Token::Ident("y")]);
        assert_eq!(
            errors,
            vec![LexError {
                position: 1,
                character: '@',
                rule: None,
            }]
        );
    }

    #[test]
    fn illegal_character_position_is_a_byte_offset() {
        // A multi-byte illegal character records its byte offset and the character.
        let (_, errors) = tokenize("x€y");
        assert_eq!(
            errors,
            vec![LexError {
                position: 1,
                character: '€',
                rule: None,
            }]
        );
    }

    #[test]
    fn a_run_of_illegal_characters_records_one_error_each() {
        // S1 records one error per un-lexable character (chumsky coalesces a run
        // into a single diagnostic — a count difference deferred to S4). The token
        // stream is identical either way: the run is skipped.
        let (tokens, errors) = tokenize("a@@b");
        let bare: Vec<Token> = tokens.into_iter().map(|(token, _)| token).collect();
        assert_eq!(bare, vec![Token::Ident("a"), Token::Ident("b")]);
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].position, 1);
        assert_eq!(errors[1].position, 2);
    }
}
