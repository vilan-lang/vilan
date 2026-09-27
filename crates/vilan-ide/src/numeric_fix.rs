//! The edits a numeric-width mismatch carries (`proposal/index-type.md` §8.1,
//! §8.3): computed ONCE, here, for the language server's quick fixes and for
//! `vilan check --fix`, so the editor and the command line can never write two
//! different conversions for one diagnostic.
//!
//! Two families of diagnostic carry a mechanical fix:
//!
//! - E218's steer — `… There are no implicit numeric conversions; convert with
//!   `.as_X()`` — on a value typed against a DECLARED type (an argument, an
//!   annotation, a return, a field, a subscript's index). The fix is the
//!   conversion the message names, written after the value its span covers.
//!   The declared type decides the direction, so an index meeting an `i32`
//!   wire position converts to `i32` and the wire width stays.
//! - the binary-operator refusal between `usize` and another integer width
//!   (ledger rows 345/357: "`<` compares two values of the same type, but the
//!   operands are `usize` and `i32`"). Neither side is declared, so the rule is
//!   the migration's: an index met a non-index, and the NON-index operand
//!   converts to `usize`. Between two non-index widths there is no such rule,
//!   and no fix is offered.
//!
//! Where the value a fix would convert to `usize` is a bare local bound by an
//! unsuffixed integer literal in the same function (`mut at = 0;`), the honest
//! edit is the DECLARATION — `mut at: usize = 0;` — because the counter IS an
//! index; converting at each of its uses would leave `.as_usize()` on a value
//! that should never have been anything else. That edit is offered first.
//!
//! Everything else is declined, never guessed at: a span that is not a whole
//! single-line expression on token boundaries (a closure's return mismatch
//! anchors at its closing brace), and any message this module does not know.

use vilan_core::node::Node;
use vilan_core::parsing;
use vilan_core::span::{Span, Spanned};

/// E218's steer, up to the conversion's name (the analyzer's own constant, so
/// the two cannot drift).
use vilan_core::analyzer::NUMERIC_CONVERSION_STEER;

/// What one fix writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NumericEdit {
    /// `.as_<width>()` after the value — the method's name, `as_usize`.
    Convert(String),
    /// `: usize` after the name of a counter bound by an unsuffixed literal —
    /// the counter's name.
    DeclareUsize(String),
}

/// One edit: replace `span` with `replacement`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumericFix {
    pub span: Span,
    pub replacement: String,
    pub edit: NumericEdit,
    /// Whether the mismatch is about an INDEX — one side of it is `usize`.
    /// The bulk action and the migration are about these; a `u8` meeting an
    /// `i32` is fixed by the same edit but is not an index.
    pub index: bool,
}

/// The fixes one diagnostic carries in `source` (the text of the file its
/// `span` indexes), the PREFERRED first: a counter's declaration where there
/// is one, then the conversion. Empty when the diagnostic carries none.
pub fn numeric_fixes(source: &str, span: Span, message: &str) -> Vec<NumericFix> {
    if let Some(method) = steered_conversion(message) {
        let index = method == "as_usize" || message.contains("usize");
        let Some(written) = whole_expression(source, span) else {
            return Vec::new();
        };
        let mut fixes = Vec::new();
        if method == "as_usize"
            && let Some((name, at)) = literal_counter(source, span, written)
        {
            fixes.push(NumericFix {
                span: Span::from(at..at),
                replacement: ": usize".to_string(),
                edit: NumericEdit::DeclareUsize(name),
                index,
            });
        }
        fixes.push(NumericFix {
            span,
            replacement: converted(written, method),
            edit: NumericEdit::Convert(method.to_string()),
            index,
        });
        return fixes;
    }
    let Some((left, right)) = binary_operands(message) else {
        return Vec::new();
    };
    let convert_left = match (left, right) {
        ("usize", other) if INTEGER_WIDTHS.contains(&other) => false,
        (other, "usize") if INTEGER_WIDTHS.contains(&other) => true,
        _ => return Vec::new(),
    };
    let Some(operand) = operand_span(source, span, convert_left) else {
        return Vec::new();
    };
    let Some(written) = whole_expression(source, operand) else {
        return Vec::new();
    };
    let mut fixes = Vec::new();
    if let Some((name, at)) = literal_counter(source, operand, written) {
        fixes.push(NumericFix {
            span: Span::from(at..at),
            replacement: ": usize".to_string(),
            edit: NumericEdit::DeclareUsize(name),
            index: true,
        });
    }
    fixes.push(NumericFix {
        span: operand,
        replacement: converted(written, "as_usize"),
        edit: NumericEdit::Convert("as_usize".to_string()),
        index: true,
    });
    fixes
}

/// Applies each fix's edit to `source`, last first, skipping any that
/// OVERLAPS one already applied (the next round of a fixed-point driver sees
/// it again). DECLARATIONS win the round: when any fix is one, only those are
/// applied, because a counter respelled `usize` retires every conversion its
/// uses would otherwise collect — the caller re-analyzes before writing a
/// single `.as_*()` into the file. Answers the new text and how many edits it
/// carries.
pub fn apply_numeric_fixes(source: &str, mut fixes: Vec<NumericFix>) -> (String, usize) {
    if fixes
        .iter()
        .any(|fix| matches!(fix.edit, NumericEdit::DeclareUsize(_)))
    {
        fixes.retain(|fix| matches!(fix.edit, NumericEdit::DeclareUsize(_)));
    }
    fixes.sort_by_key(|fix| std::cmp::Reverse((fix.span.start, fix.span.end)));
    fixes.dedup_by_key(|fix| (fix.span.start, fix.span.end));
    let mut text = source.to_string();
    let mut floor = usize::MAX;
    let mut applied = 0;
    for fix in fixes {
        if fix.span.end > floor || text.get(fix.span.into_range()).is_none() {
            continue;
        }
        text.replace_range(fix.span.into_range(), &fix.replacement);
        floor = fix.span.start;
        applied += 1;
    }
    (text, applied)
}

/// The conversion E218's steer names, `as_usize` — `None` for any other
/// message, or one whose tail is not a plain method name.
fn steered_conversion(message: &str) -> Option<&str> {
    let at = message.find(NUMERIC_CONVERSION_STEER)?;
    let method = message[at + NUMERIC_CONVERSION_STEER.len()..].strip_suffix("()`")?;
    (method.starts_with("as_")
        && method
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_'))
    .then_some(method)
}

const INTEGER_WIDTHS: &[&str] = &["i8", "u8", "i16", "u16", "i32", "u32", "i53", "u53"];

/// The two operand types a binary-operator refusal names, when it is one.
fn binary_operands(message: &str) -> Option<(&str, &str)> {
    let rest = message.split_once("but the operands are `")?.1;
    let (left, rest) = rest.split_once("` and `")?;
    let (right, _) = rest.split_once('`')?;
    Some((left, right))
}

/// `written` with the conversion after it, parenthesized unless it already
/// takes a method call as it stands.
fn converted(written: &str, method: &str) -> String {
    if is_postfix_operand(written) {
        format!("{written}.{method}()")
    } else {
        format!("({written}).{method}()")
    }
}

/// The text `span` covers, when it is a whole expression a conversion can be
/// written after: non-empty, one line, no surrounding whitespace, neither end
/// cutting a word in two, and parseable on its own. A MULTI-LINE value is
/// declined: `(if … { … } else { … }).as_i32()` parses, and it is the
/// conversion a person should write at the arm or the binding instead.
fn whole_expression(source: &str, span: Span) -> Option<&str> {
    let written = source.get(span.into_range())?;
    if written.is_empty()
        || written.contains('\n')
        || written.trim() != written
        || written.ends_with('.')
        || !on_token_boundaries(source, span)
    {
        return None;
    }
    let probe = format!("fun probe() {{\n\tlet value = ({written});\n}}\n");
    let (parsed, errors) = parsing::parse(&probe);
    (parsed.is_some() && errors.is_empty()).then_some(written)
}

/// Whether `span` starts and ends on token boundaries of `source`. A span
/// re-reported out of a macro world can index GENERATED text and land
/// mid-name in the file it is attributed to (`quo|te(`).
fn on_token_boundaries(source: &str, span: Span) -> bool {
    let bytes = source.as_bytes();
    let word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    let start_ok = span.start == 0
        || !(word(bytes[span.start - 1]) && bytes.get(span.start).copied().is_some_and(word));
    let end_ok = span.end >= bytes.len()
        || !(word(bytes[span.end]) && span.end > 0 && word(bytes[span.end - 1]));
    start_ok && end_ok
}

/// Whether `written` takes a method call as it stands: it starts with a name,
/// and outside its brackets and string literals it is only names, `.` and `?`.
/// A number literal is NOT one (`5.as_u53()` would lex the `5.` as a float),
/// nor is anything carrying an operator or a space at the top level.
pub fn is_postfix_operand(written: &str) -> bool {
    if !written
        .chars()
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_')
    {
        return false;
    }
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for character in written.chars() {
        if in_string {
            match character {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => match depth.checked_sub(1) {
                Some(outer) => depth = outer,
                None => return false,
            },
            _ if depth > 0 => {}
            _ if character.is_alphanumeric() || matches!(character, '_' | '.' | '?') => {}
            _ => return false,
        }
    }
    depth == 0 && !in_string
}

/// The left or right operand's span of the `Binary` node whose span is `span`.
fn operand_span(source: &str, span: Span, left: bool) -> Option<Span> {
    let (parsed, _) = parsing::parse(source);
    let parsed = parsed?;
    fn find(node: &Spanned<Node<'_>>, span: Span, left: bool) -> Option<Span> {
        if let Node::Binary(_, lhs, rhs) = &node.0
            && node.1.start == span.start
            && node.1.end == span.end
        {
            return Some(if left { lhs.1 } else { rhs.1 });
        }
        let mut found = None;
        node.0.for_each_child(&mut |child| {
            if found.is_none() && child.1.start <= span.start && span.end <= child.1.end {
                found = find(child, span, left);
            }
        });
        found
    }
    parsed.0.iter().find_map(|item| find(item, span, left))
}

/// The counter to declare `usize` instead of converting at a use: `written`
/// is a bare local name, and the nearest binding of it earlier in the same
/// function is `let`/`mut NAME = <unsuffixed integer literal>;` with no
/// annotation. Answers the name and the offset to insert `: usize` at (the end
/// of the name).
fn literal_counter(source: &str, use_span: Span, written: &str) -> Option<(String, usize)> {
    let is_name = written
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_lowercase() || first == '_')
        && written
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_');
    if !is_name {
        return None;
    }
    let (parsed, _) = parsing::parse(source);
    let parsed = parsed?;
    fn enclosing<'a, 'src>(
        node: &'a Spanned<Node<'src>>,
        at: usize,
        found: &mut Option<&'a Spanned<Node<'src>>>,
    ) {
        if node.1.start <= at && at < node.1.end {
            if matches!(node.0, Node::Func(_) | Node::MacroFun(_)) {
                *found = Some(node);
            }
            node.0
                .for_each_child(&mut |child| enclosing(child, at, found));
        }
    }
    let mut function = None;
    for item in &parsed.0 {
        enclosing(item, use_span.start, &mut function);
    }
    fn declaration(node: &Spanned<Node<'_>>, name: &str, before: usize, best: &mut Option<Span>) {
        if let Node::Let(binding, None, Some(value), _, _, _) = &node.0
            && binding.0 == name
            && node.1.start < before
            && matches!(value.0, Node::Number(_, None, None))
        {
            *best = Some(binding.1);
        }
        node.0
            .for_each_child(&mut |child| declaration(child, name, before, best));
    }
    let mut best = None;
    declaration(function?, written, use_span.start, &mut best);
    best.map(|binding| (written.to_string(), binding.end))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span_of(source: &str, needle: &str) -> Span {
        let start = source.find(needle).expect("the needle is in the source");
        Span::from(start..start + needle.len())
    }

    #[test]
    fn a_steered_mismatch_converts_the_value_it_spans() {
        let source = "fun main() {\n\tlet xs = [1];\n\tlet n: u53 = xs.len() + 1;\n}\n";
        let fixes = numeric_fixes(
            source,
            span_of(source, "xs.len() + 1"),
            "Expected u53, but got usize (an index: a position, a length or a count) instead. \
             There are no implicit numeric conversions; convert with `.as_u53()`",
        );
        assert_eq!(fixes.len(), 1, "{fixes:?}");
        assert_eq!(fixes[0].replacement, "(xs.len() + 1).as_u53()");
        assert_eq!(fixes[0].edit, NumericEdit::Convert("as_u53".to_string()));
        assert!(fixes[0].index, "the message names `usize`");
    }

    #[test]
    fn a_literal_counter_is_declared_before_it_is_converted() {
        let source = "fun main() {\n\tmut at = 0;\n\ttake(at);\n}\n";
        let fixes = numeric_fixes(
            source,
            span_of(source, "at)"),
            "Expected usize, but got i32 instead. There are no implicit numeric conversions; \
             convert with `.as_usize()`",
        );
        // The span above is `at)` — not a whole expression, so nothing.
        assert!(fixes.is_empty());
        let use_at = source.rfind("at)").unwrap();
        let fixes = numeric_fixes(
            source,
            Span::from(use_at..use_at + 2),
            "Expected usize, but got i32 instead. There are no implicit numeric conversions; \
             convert with `.as_usize()`",
        );
        assert_eq!(fixes.len(), 2, "{fixes:?}");
        assert_eq!(fixes[0].edit, NumericEdit::DeclareUsize("at".to_string()));
        let (declared, applied) = apply_numeric_fixes(source, fixes);
        assert_eq!(applied, 1, "the declaration wins the round alone");
        assert_eq!(
            declared,
            "fun main() {\n\tmut at: usize = 0;\n\ttake(at);\n}\n"
        );
    }

    #[test]
    fn the_non_index_operand_of_a_mixed_comparison_converts_to_usize() {
        let source = "fun f(limit: i32, xs: List<i32>) {\n\tlet ok = xs.len() < limit;\n}\n";
        let fixes = numeric_fixes(
            source,
            span_of(source, "xs.len() < limit"),
            "`<` compares two values of the same type, but the operands are `usize` and `i32`: \
             there are no implicit conversions; suffix the literal or convert with `as_*`",
        );
        assert_eq!(fixes.len(), 1, "{fixes:?}");
        assert_eq!(&source[fixes[0].span.into_range()], "limit");
        assert_eq!(fixes[0].replacement, "limit.as_usize()");
    }

    #[test]
    fn two_non_index_widths_carry_no_operand_fix() {
        let source = "fun f(a: u8, b: i32) {\n\tlet ok = a < b;\n}\n";
        assert!(
            numeric_fixes(
                source,
                span_of(source, "a < b"),
                "`<` compares two values of the same type, but the operands are `u8` and `i32`: \
                 there are no implicit conversions; suffix the literal or convert with `as_*`",
            )
            .is_empty()
        );
    }

    #[test]
    fn a_span_that_is_not_a_whole_expression_is_declined() {
        let source = "fun main() {\n\tlet n: u53 = {\n\t\t1\n\t};\n}\n";
        let start = source.find('{').unwrap();
        let brace = source.rfind('}').unwrap();
        assert!(
            numeric_fixes(
                source,
                Span::from(start..brace),
                "Expected u53, but got i32 instead. There are no implicit numeric conversions; \
                 convert with `.as_u53()`",
            )
            .is_empty()
        );
    }

    #[test]
    fn an_unrelated_message_carries_nothing() {
        let source = "fun main() {\n\tlet n = 1;\n}\n";
        assert!(numeric_fixes(source, span_of(source, "1"), "cannot find 'x'").is_empty());
    }

    #[test]
    fn a_postfix_operand_is_a_name_path_or_call_chain() {
        for written in [
            "n",
            "xs.len()",
            "self.cells[at].value",
            "list.get(\"a b\")",
            "a?",
        ] {
            assert!(is_postfix_operand(written), "{written} takes a method call");
        }
        for written in [
            "5",
            "-n",
            "a + b",
            "(a) + b",
            "\"text\"",
            "a)(",
            "if c { a } else { b }",
        ] {
            assert!(!is_postfix_operand(written), "{written} needs parentheses");
        }
    }
}
