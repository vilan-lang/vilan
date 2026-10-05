//! `dbg(..)`'s runtime (debugging.md S1): the document the generated
//! printers build, its layout, and the scalar spellings.
//!
//! This is the native twin of the JS backend's `__dbg_*` helpers
//! (`vilan-core`'s `transformer.rs`), and the two must write the same bytes:
//! the native differential compares every `dbg` line. A generated printer
//! (`show_*` in the emitted `main.rs`) answers a [`Doc`] for its type; the
//! layout puts a document on one line when that fits 80 columns from where it
//! starts, else one entry per line, two spaces deeper, each with a trailing
//! comma (Q1). Widths count characters, as the JS helper's `for .. of` does.

use crate::{Location, js_number};

/// What a printer answers: text, or a group of labelled entries between an
/// open and a close text.
#[derive(Clone, Debug, PartialEq)]
pub enum Doc {
    Text(String),
    Group {
        open: String,
        close: String,
        /// `Point { x = 1 }` pads its entries with a space; `[1, 2]` does not.
        padded: bool,
        entries: Vec<(String, Doc)>,
    },
}

impl Doc {
    pub fn text(text: impl Into<String>) -> Doc {
        Doc::Text(text.into())
    }

    pub fn group(open: &str, close: &str, padded: bool, entries: Vec<(String, Doc)>) -> Doc {
        Doc::Group {
            open: open.to_string(),
            close: close.to_string(),
            padded,
            entries,
        }
    }

    /// The document on one line.
    pub fn flat(&self) -> String {
        match self {
            Doc::Text(text) => text.clone(),
            Doc::Group {
                open,
                close,
                padded,
                entries,
            } => {
                if entries.is_empty() {
                    return format!("{open}{close}");
                }
                let inner: Vec<String> = entries
                    .iter()
                    .map(|(label, document)| format!("{label}{}", document.flat()))
                    .collect();
                if *padded {
                    format!("{open} {} {close}", inner.join(", "))
                } else {
                    format!("{open}{}{close}", inner.join(", "))
                }
            }
        }
    }

    /// The document laid out from `column`, its broken entries at `indent + 2`.
    pub fn layout(&self, column: usize, indent: usize) -> String {
        let flat = self.flat();
        let Doc::Group {
            open,
            close,
            entries,
            ..
        } = self
        else {
            return flat;
        };
        if entries.is_empty() || column + width(&flat) <= 80 {
            return flat;
        }
        let pad = " ".repeat(indent + 2);
        let mut out = format!("{open}\n");
        for (label, document) in entries {
            let head = format!("{pad}{label}");
            out.push_str(&head);
            out.push_str(&document.layout(width(&head), indent + 2));
            out.push_str(",\n");
        }
        out.push_str(&" ".repeat(indent));
        out.push_str(close);
        out
    }
}

/// Characters, as the JS helper counts them.
fn width(text: &str) -> usize {
    text.chars().count()
}

/// `dbg(..)`'s lines: `[file:line:col] expr = value` per entry, or the bare
/// `[file:line:col]` for none — to stderr, as node's `console.error` writes.
pub fn dbg(location: Location, entries: Vec<(&str, Doc)>) {
    if entries.is_empty() {
        eprintln!("[{}]", location.0);
        return;
    }
    for (text, document) in entries {
        let head = format!("[{}] {text} = ", location.0);
        let laid_out = document.layout(width(&head), 0);
        eprintln!("{head}{laid_out}");
    }
}

/// A `List`'s document: at most 100 entries, then `… N more`.
pub fn list<T>(items: &[T], show: impl Fn(&T) -> Doc) -> Doc {
    let shown = items.len().min(100);
    let mut entries: Vec<(String, Doc)> = items[..shown]
        .iter()
        .map(|item| (String::new(), show(item)))
        .collect();
    if items.len() > shown {
        entries.push((
            String::new(),
            Doc::text(format!("\u{2026} {} more", items.len() - shown)),
        ));
    }
    Doc::group("[", "]", false, entries)
}

/// A map's or a set's document (S1b): a padded group of the members, in the
/// table's order, at most 100 of them and then `… N more`.
pub fn members<T>(open: &str, items: &[T], show: impl Fn(&T) -> (String, Doc)) -> Doc {
    let shown = items.len().min(100);
    let mut entries: Vec<(String, Doc)> = items[..shown].iter().map(show).collect();
    if items.len() > shown {
        entries.push((
            String::new(),
            Doc::text(format!("\u{2026} {} more", items.len() - shown)),
        ));
    }
    Doc::group(open, "}", true, entries)
}

thread_local! {
    /// The cells the current print is inside, by address — a `Shared` met
    /// again on the way down closes a cycle (S1b).
    static SEEN: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// `Shared(<value>)`, or `<cycle>` for a cell this print is already inside.
pub fn shared<T: Clone>(cell: &crate::Shared<T>, show: impl Fn(&T) -> Doc) -> Doc {
    let address = cell.address();
    if SEEN.with(|seen| seen.borrow().contains(&address)) {
        return Doc::text("<cycle>");
    }
    SEEN.with(|seen| seen.borrow_mut().push(address));
    let inner = show(&cell.get());
    SEEN.with(|seen| seen.borrow_mut().pop());
    Doc::group("Shared(", ")", false, vec![(String::new(), inner)])
}

/// A string as vilan writes it: quoted, with `\\`, `\"`, `\n`, `\t`, `\r`
/// and `\0` escaped.
pub fn string(text: &str) -> Doc {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            other => out.push(other),
        }
    }
    out.push('"');
    Doc::Text(out)
}

/// A float as `dbg` writes it: the language's number text, `.0` on an
/// integral value (`3.0`), and nothing added to an exponent form (`1e+21`)
/// or a non-finite one (`NaN`, `Infinity`). `-0.0` is `0.0`, as `String(-0)`
/// is `"0"` (N136).
pub fn float(value: f64) -> Doc {
    let text = js_number(value);
    if value.is_finite() && value.fract() == 0.0 && !text.contains('e') {
        Doc::Text(format!("{text}.0"))
    } else {
        Doc::Text(text)
    }
}

/// An integer of any width, or a `BigInt`, by its `Display`.
pub fn integer(value: impl std::fmt::Display) -> Doc {
    Doc::Text(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: i32, y: i32) -> Doc {
        Doc::group(
            "Point {",
            "}",
            true,
            vec![
                ("x = ".to_string(), integer(x)),
                ("y = ".to_string(), integer(y)),
            ],
        )
    }

    #[test]
    fn a_group_that_fits_is_one_line() {
        assert_eq!(point(1, 2).layout(0, 0), "Point { x = 1, y = 2 }");
        assert_eq!(Doc::group("[", "]", false, vec![]).layout(0, 0), "[]");
        assert_eq!(
            Doc::group("HashMap {", "}", true, vec![]).layout(0, 0),
            "HashMap {}"
        );
    }

    #[test]
    fn a_group_past_80_columns_breaks_one_entry_per_line_with_trailing_commas() {
        let list = list(&(0..30).collect::<Vec<i32>>(), |n| point(*n, *n));
        let laid_out = list.layout(10, 0);
        assert!(laid_out.starts_with("[\n  Point { x = 0, y = 0 },\n"));
        assert!(laid_out.ends_with("  Point { x = 29, y = 29 },\n]"));
    }

    #[test]
    fn a_long_list_is_cut_at_100_entries() {
        let laid_out = list(&(0..250).collect::<Vec<i32>>(), |n| integer(*n)).flat();
        assert!(laid_out.ends_with("98, 99, \u{2026} 150 more]"));
    }

    #[test]
    fn floats_keep_their_point_and_strings_their_quotes() {
        assert_eq!(float(3.0).flat(), "3.0");
        assert_eq!(float(-0.0).flat(), "0.0");
        assert_eq!(float(1.5).flat(), "1.5");
        assert_eq!(float(1e21).flat(), "1e+21");
        assert_eq!(float(f64::NAN).flat(), "NaN");
        assert_eq!(float(f64::INFINITY).flat(), "Infinity");
        assert_eq!(string("a\"b\\c\nd").flat(), "\"a\\\"b\\\\c\\nd\"");
    }
}
