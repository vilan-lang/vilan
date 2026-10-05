//! The edit B495's mode-mismatch refusal carries (E263, `closure-type-views.md`
//! §4): a value closure and a view closure are different types, and no adapter
//! is inserted, so a closure handed where its type takes the parameter the
//! other way is refused — "this closure takes `str` by value where its type
//! takes a view `&str` … `|c| f(*c)`". Computed here for the language server's
//! quick fix, from the refusal's own text and the source its span covers.
//!
//! Two edits, one per shape of what the refusal spans:
//!
//! - **The literal is right there** (`apply(|s: str| 1)`): its parameter is
//!   rewritten in the type's mode — `s: str` becomes `s: &str`, and the reverse
//!   drops the `&`. The parameter's name, its type and every other parameter
//!   stay as written.
//! - **A name** (`apply(g)`, where `g` is a value closure bound earlier): the
//!   adapter the refusal names is written around it — `|c| g(*c)` copies the
//!   view's value out for a value closure, `|c| g(&c)` lends a view closure the
//!   value. Only for a one-parameter closure (the refusal does not say how many
//!   parameters a NAMED closure takes, and an adapter must spell them all), and
//!   only around a plain name or path: around any other expression the adapter
//!   would evaluate it on every call instead of once.
//!
//! A writable view meeting a value closure has no adapter (a closure that takes
//! a value writes only its own copy, never the caller's place), and the two
//! views have none either; those get the literal's edit or nothing.

use vilan_core::lexing::tokenize;
use vilan_core::node::{Convention, Node};
use vilan_core::parsing;
use vilan_core::span::{Span, Spanned};
use vilan_core::token::Token;

/// How a closure parameter receives its argument, as the refusal names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// "`T` by value".
    Value,
    /// "a view `&T`".
    View,
    /// "a writable view `&mut T`".
    MutView,
}

impl Mode {
    /// The type prefix the mode is written with: ``, `&`, `&mut `.
    fn prefix(self) -> &'static str {
        match self {
            Mode::Value => "",
            Mode::View => "&",
            Mode::MutView => "&mut ",
        }
    }
}

/// What a [`ClosureModeFix`] does, for its title.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClosureModeEdit {
    /// The literal's parameter, rewritten: its type as now written (`&str`).
    Parameter { written: String },
    /// The adapter written around a named closure (`|c| g(*c)`).
    Adapter { written: String },
}

/// One edit: replace `span` with `replacement`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosureModeFix {
    pub span: Span,
    pub replacement: String,
    pub edit: ClosureModeEdit,
}

/// The refusal's facts: which parameter (0-based), the mode the closure takes
/// it in, the mode its type takes it in, and whether the closure has exactly
/// one parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Mismatch {
    index: usize,
    single: bool,
    got: Mode,
    expected: Mode,
}

/// The refusal's fixed text after the two modes (the analyzer's
/// `closure_mode_mismatch_message`).
const DIFFERENT_TYPES: &str = ": a value closure and a view closure are different types";

/// The mode a refusal's description names: "`str` by value", "a view `&str`",
/// "a writable view `&mut str`".
fn described_mode(description: &str) -> Option<Mode> {
    if description.starts_with("a writable view `&mut ") {
        Some(Mode::MutView)
    } else if description.starts_with("a view `&") {
        Some(Mode::View)
    } else if description.starts_with('`') && description.ends_with("` by value") {
        Some(Mode::Value)
    } else {
        None
    }
}

/// The refusal's facts, when `message` is B495's mode mismatch.
fn mismatch(message: &str) -> Option<Mismatch> {
    let (index, single, rest) = if let Some(rest) = message.strip_prefix("this closure takes ") {
        (0, true, rest)
    } else {
        let rest = message.strip_prefix("this closure's parameter ")?;
        let (number, rest) = rest.split_once(" takes ")?;
        (number.parse::<usize>().ok()?.checked_sub(1)?, false, rest)
    };
    let (got, rest) = rest.split_once(" where its type takes ")?;
    let (expected, _) = rest.split_once(DIFFERENT_TYPES)?;
    let mismatch = Mismatch {
        index,
        single,
        got: described_mode(got)?,
        expected: described_mode(expected)?,
    };
    (mismatch.got != mismatch.expected).then_some(mismatch)
}

/// The fixes B495's refusal `message`, anchored at `span` in `source`,
/// carries: at most one. Empty for every other message, and wherever the
/// edit would be a guess.
pub fn closure_mode_fixes(source: &str, span: Span, message: &str) -> Vec<ClosureModeFix> {
    let Some(mismatch) = mismatch(message) else {
        return Vec::new();
    };
    let Some(written) = source.get(span.into_range()) else {
        return Vec::new();
    };
    let fix = if written.starts_with('|') {
        parameter_fix(written, span.start, mismatch)
    } else {
        adapter_fix(written, span, mismatch)
    };
    fix.into_iter().collect()
}

/// The literal `written` (starting at `offset` in the file) with its
/// parameter `mismatch.index` rewritten in the expected mode.
fn parameter_fix(written: &str, offset: usize, mismatch: Mismatch) -> Option<ClosureModeFix> {
    // The literal is read by the parser, in a position that takes any
    // expression; the wrapper's length is subtracted from every span.
    let prefix = "fun probe() {\n\tlet value = (";
    let probe = format!("{prefix}{written});\n}}\n");
    let (tree, errors) = parsing::parse(&probe);
    if !errors.is_empty() {
        return None;
    }
    let tree = tree?;
    let closure = outermost_closure(&tree.0, prefix.len())?;
    let Node::Closure(closure) = &closure.0 else {
        return None;
    };
    let parameter = closure.parameters.0.get(mismatch.index)?;
    // A keyword that fixes how the parameter receives its argument (`own`,
    // `mut`, `lazy`, `...`) cannot sit beside a view; the edit is the
    // author's.
    if parameter.convention == Convention::Own
        || parameter.mutable
        || parameter.lazy
        || parameter.spread
    {
        return None;
    }
    let declared = parameter.declared_type.as_ref()?;
    // The mode is written as the type's prefix: everything from the `:` to
    // the bare type, `&` or `&mut` included whether the parser read it as the
    // parameter's convention or as the type's own. The parameter's span is
    // its binder's, so the `:` is the first one after it.
    let between = probe.get(parameter.span.end..declared.1.start)?;
    let colon = parameter.span.end + between.find(':')?;
    let mode_start = colon + 1 + whitespace_after(&probe[colon + 1..]);
    let mut bare_start = mode_start;
    let rest = &probe[mode_start..];
    if let Some(after) = rest.strip_prefix("&mut") {
        if after.starts_with(|character: char| character.is_whitespace()) {
            bare_start += "&mut".len() + whitespace_after(after);
        } else {
            return None;
        }
    } else if let Some(after) = rest.strip_prefix('&') {
        bare_start += 1 + whitespace_after(after);
    }
    let bare_end = declared.1.end;
    if bare_start >= bare_end {
        return None;
    }
    let bare = probe.get(bare_start..bare_end)?;
    let replacement = format!("{}{bare}", mismatch.expected.prefix());
    let start = mode_start - prefix.len() + offset;
    let end = bare_end - prefix.len() + offset;
    Some(ClosureModeFix {
        span: Span::from(start..end),
        replacement: replacement.clone(),
        edit: ClosureModeEdit::Parameter {
            written: replacement,
        },
    })
}

/// The closure node that STARTS at `start` in the probe — the literal itself,
/// not one nested in its body.
fn outermost_closure<'tree, 'src>(
    nodes: &'tree [Spanned<Node<'src>>],
    start: usize,
) -> Option<&'tree Spanned<Node<'src>>> {
    for node in nodes {
        if matches!(node.0, Node::Closure(_)) && node.1.start == start {
            return Some(node);
        }
        let mut found = None;
        node.0.for_each_child(&mut |child| {
            if found.is_none() {
                found = outermost_closure(std::slice::from_ref(child), start);
            }
        });
        if found.is_some() {
            return found;
        }
    }
    None
}

/// How many bytes of whitespace `text` opens with.
fn whitespace_after(text: &str) -> usize {
    text.len() - text.trim_start().len()
}

/// The adapter around the named closure `written` (at `span`).
fn adapter_fix(written: &str, span: Span, mismatch: Mismatch) -> Option<ClosureModeFix> {
    if !mismatch.single {
        return None;
    }
    let adapt = match (mismatch.expected, mismatch.got) {
        // The type lends a view; the closure wants the value: copy it out.
        (Mode::View, Mode::Value) => "*",
        // The type hands the value; the closure wants a view: lend it.
        (Mode::Value, Mode::View) => "&",
        (Mode::Value, Mode::MutView) => "&mut ",
        _ => return None,
    };
    let (tokens, errors) = tokenize(written);
    let is_path = errors.is_empty()
        && !tokens.is_empty()
        && tokens.iter().enumerate().all(|(index, (token, _))| {
            if index % 2 == 0 {
                matches!(token, Token::Ident(_))
            } else {
                matches!(token, Token::Op("::"))
            }
        })
        && tokens.len() % 2 == 1;
    if !is_path {
        return None;
    }
    // The adapter's parameter must not shadow a name the path spells.
    let name = ["c", "value", "argument"].into_iter().find(|candidate| {
        !tokens
            .iter()
            .any(|(token, _)| matches!(token, Token::Ident(name) if name == candidate))
    })?;
    let replacement = format!("|{name}| {written}({adapt}{name})");
    Some(ClosureModeFix {
        span,
        replacement: replacement.clone(),
        edit: ClosureModeEdit::Adapter {
            written: replacement,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW_FOR_VALUE: &str = "this closure takes `str` by value where its type takes a view \
         `&str`: a value closure and a view closure are different types, and no adapter is \
         inserted; write the parameter the way the type does (`|c: &T|`, or a bare `|c|`), or \
         adapt the value closure with one that copies the view's value out: `|c| f(*c)`.";

    fn applied(source: &str, needle: &str, message: &str) -> Option<String> {
        let start = source.find(needle)?;
        let span = Span::from(start..start + needle.len());
        let fix = closure_mode_fixes(source, span, message)
            .into_iter()
            .next()?;
        let mut text = source.to_string();
        text.replace_range(fix.span.into_range(), &fix.replacement);
        Some(text)
    }

    #[test]
    fn the_refusals_facts_are_read() {
        assert_eq!(
            mismatch(VIEW_FOR_VALUE),
            Some(Mismatch {
                index: 0,
                single: true,
                got: Mode::Value,
                expected: Mode::View
            })
        );
        assert_eq!(
            mismatch(
                "this closure's parameter 2 takes a writable view `&mut i32` where its type takes \
                 a view `&i32`: a value closure and a view closure are different types, and no \
                 adapter is inserted; write the parameter's view the way the type does (`&` and \
                 `&mut` are different views)."
            ),
            Some(Mismatch {
                index: 1,
                single: false,
                got: Mode::MutView,
                expected: Mode::View
            })
        );
        assert_eq!(
            mismatch("Expected |&str| i32, but got fn count(str): i32 instead."),
            None
        );
    }

    #[test]
    fn a_literals_parameter_takes_the_types_mode() {
        let source = "let a = apply(|s: str| 1);";
        assert_eq!(
            applied(source, "|s: str| 1", VIEW_FOR_VALUE).as_deref(),
            Some("let a = apply(|s: &str| 1);")
        );
        // The second of two parameters, a closure type in the first's type,
        // and a tuple binder in front: only the named parameter moves.
        let message = "this closure's parameter 2 takes `i32` by value where its type takes a \
             writable view `&mut i32`: a value closure and a view closure are different types";
        let source = "f(|(a, b): (i32, i32), n: i32| {})";
        assert_eq!(
            applied(source, "|(a, b): (i32, i32), n: i32| {}", message).as_deref(),
            Some("f(|(a, b): (i32, i32), n: &mut i32| {})")
        );
        let message = "this closure's parameter 1 takes a view `&i32` where its type takes `i32` \
             by value: a value closure and a view closure are different types";
        let source = "f(|k: & i32, g: |&i32| void| {})";
        assert_eq!(
            applied(source, "|k: & i32, g: |&i32| void| {}", message).as_deref(),
            Some("f(|k: i32, g: |&i32| void| {})")
        );
        let message = "this closure takes a writable view `&mut i32` where its type takes a view \
             `&i32`: a value closure and a view closure are different types";
        assert_eq!(
            applied("f(|n: &mut i32| {})", "|n: &mut i32| {}", message).as_deref(),
            Some("f(|n: &i32| {})")
        );
    }

    #[test]
    fn a_named_closure_is_adapted() {
        assert_eq!(
            applied("apply(g)", "g", VIEW_FOR_VALUE).as_deref(),
            Some("apply(|c| g(*c))")
        );
        let message = "this closure takes a view `&str` where its type takes `str` by value: a \
             value closure and a view closure are different types";
        assert_eq!(
            applied("apply(shapes::c)", "shapes::c", message).as_deref(),
            Some("apply(|value| shapes::c(&value))")
        );
    }

    #[test]
    fn a_guess_is_declined() {
        // Not a path: the adapter would evaluate it on every call.
        assert_eq!(applied("apply(make())", "make()", VIEW_FOR_VALUE), None);
        assert_eq!(applied("apply(self.f)", "self.f", VIEW_FOR_VALUE), None);
        // A writable view meeting a value closure has no adapter.
        let message = "this closure takes `i32` by value where its type takes a writable view \
             `&mut i32`: a value closure and a view closure are different types";
        assert_eq!(applied("apply(g)", "g", message), None);
        // A named closure of several parameters: the arity is not in the
        // refusal.
        let message = "this closure's parameter 2 takes `i32` by value where its type takes a \
             view `&i32`: a value closure and a view closure are different types";
        assert_eq!(applied("apply(g)", "g", message), None);
        // An unannotated parameter, and an `own` one: nothing to rewrite.
        assert_eq!(applied("apply(|s| 1)", "|s| 1", VIEW_FOR_VALUE), None);
        assert_eq!(
            applied("apply(|own s: str| 1)", "|own s: str| 1", VIEW_FOR_VALUE),
            None
        );
    }
}
