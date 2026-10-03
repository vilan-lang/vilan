//! Node's `util.inspect`, as `console.log` reaches it: the one printer rule
//! `print` follows on BOTH backends (F68).
//!
//! The JS backend binds `print` to `console.log`, and node lays a container out
//! by rules of its own: an array of more than six entries is GROUPED into
//! aligned columns, a container whose entries do not fit 80 columns breaks one
//! entry per line, a string nested past the line splits at its newlines, a
//! fourth level of nesting prints `[Array]`, and an array past 100 entries
//! says how many more it has. The native backend printed every container on
//! one line, so a 22-element `List<str>` printed one line natively and nine on
//! node. This module is the port, so the native `print` writes node's bytes.
//!
//! It follows `lib/internal/util/inspect.js` (node v24) function by function —
//! `formatValue`'s depth cut, `formatProperty`'s indentation, `groupArrayElements`,
//! `isBelowBreakLength`, `reduceToSingleString`, `formatPrimitive` and
//! `strEscape` — at `console.log`'s options: `breakLength` 80, `depth` 2,
//! `compact` 3, `maxArrayLength` 100, `maxStringLength` 10000, no colours. Two
//! measures are node's own and kept apart: `groupArrayElements` aligns by a
//! string's display WIDTH (`getStringWidth`: full-width code points count two,
//! zero-width none), and `isBelowBreakLength` counts UTF-16 code units (JS's
//! `.length`). The width is node's non-ICU table; an ICU build can differ for
//! code points outside it, which only a grouped array of such strings shows.
//!
//! The walk's two pieces of state — the indentation the current container sits
//! at and how deep it is — are node's `ctx.indentationLvl` and `recurseTimes`.
//! They are thread-local here because the [`Js`] trait renders a value with no
//! context argument, and every container enters and leaves them through
//! [`Level`], which restores them even when a nested `Js` impl panics.

use std::cell::Cell;

use crate::Js;

/// `console.log`'s `breakLength`.
const BREAK_LENGTH: usize = 80;
/// `depth`: a container nested deeper than this prints as its kind's name.
const DEPTH: usize = 2;
/// `compact`: the grouped column count's cap is four times it.
const COMPACT: usize = 3;
/// `maxArrayLength`, which node applies to arrays, sets and maps alike.
const MAX_ARRAY_LENGTH: usize = 100;
/// `maxStringLength`.
const MAX_STRING_LENGTH: usize = 10_000;
/// `kMinLineLength`: a nested string shorter than this never splits.
const MIN_LINE_LENGTH: usize = 16;

thread_local! {
    /// `ctx.indentationLvl`.
    static INDENTATION: Cell<usize> = const { Cell::new(0) };
    /// `recurseTimes`: how many containers enclose the value being rendered.
    static RECURSE_TIMES: Cell<usize> = const { Cell::new(0) };
}

/// One container's kind: what its braces are, what it is called when the
/// depth cut elides it, and whether its entries GROUP (arrays only).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Array,
    Object,
    Map,
    Set,
}

impl Kind {
    fn elided(self) -> &'static str {
        match self {
            Kind::Array => "[Array]",
            Kind::Object => "[Object]",
            Kind::Map => "[Map]",
            Kind::Set => "[Set]",
        }
    }

    fn close(self) -> &'static str {
        match self {
            Kind::Array => "]",
            Kind::Object | Kind::Map | Kind::Set => "}",
        }
    }
}

/// One rendered entry, and whether the value it renders is a JS `number` (or
/// `bigint`): a grouped array of numbers pads its columns at the START, and
/// any other array at the end.
struct Entry {
    text: String,
    number: bool,
}

/// Entering a container: one level deeper, two columns further in. Dropping it
/// restores both.
struct Level {
    indentation: usize,
    recurse_times: usize,
}

impl Level {
    fn enter() -> Level {
        let indentation = INDENTATION.with(Cell::get);
        let recurse_times = RECURSE_TIMES.with(Cell::get);
        INDENTATION.with(|cell| cell.set(indentation + 2));
        RECURSE_TIMES.with(|cell| cell.set(recurse_times + 1));
        Level {
            indentation,
            recurse_times,
        }
    }
}

impl Drop for Level {
    fn drop(&mut self) {
        INDENTATION.with(|cell| cell.set(self.indentation));
        RECURSE_TIMES.with(|cell| cell.set(self.recurse_times));
    }
}

/// An ARRAY — a `List`, a tuple, a struct, an enum value, `Option`, `Result` —
/// whose entries are `items`, each rendered by `render` inside the array (a
/// string is quoted there; see [`Js::js_nested`]).
pub fn array<'a, I>(items: I, render: fn(&dyn Js) -> String) -> String
where
    I: ExactSizeIterator<Item = &'a dyn Js>,
{
    let length = items.len();
    if length == 0 {
        return "[]".to_string();
    }
    container(Kind::Array, "[", length, |entries| {
        for item in items.take(MAX_ARRAY_LENGTH) {
            entries.push(Entry {
                text: render(item),
                number: item.js_is_number(),
            });
        }
    })
}

/// A plain OBJECT, `{ key: value }` — a `Shared` cell's `{ v: .. }`, a parsed
/// JSON object. Keys are written as given (the caller quotes one that is not an
/// identifier).
pub fn object(entries: &[(&str, &dyn Js)]) -> String {
    if entries.is_empty() {
        return "{}".to_string();
    }
    container(Kind::Object, "{", entries.len(), |rendered| {
        for (key, value) in entries {
            rendered.push(Entry {
                text: format!("{key}: {}", value.js_nested()),
                number: false,
            });
        }
    })
}

/// `Map(2) { 'a' => 1, 'b' => 2 }`.
pub fn map<'a, I>(entries: I) -> String
where
    I: ExactSizeIterator<Item = (&'a dyn Js, &'a dyn Js)>,
{
    let size = entries.len();
    let open = format!("Map({size}) {{");
    if size == 0 {
        return format!("{open}}}");
    }
    container(Kind::Map, &open, size, |rendered| {
        for (key, value) in entries.take(MAX_ARRAY_LENGTH) {
            rendered.push(Entry {
                text: format!("{} => {}", key.js_nested(), value.js_nested()),
                number: false,
            });
        }
    })
}

/// `Set(2) { 1, 2 }`.
pub fn set<'a, I>(items: I) -> String
where
    I: ExactSizeIterator<Item = &'a dyn Js>,
{
    let size = items.len();
    let open = format!("Set({size}) {{");
    if size == 0 {
        return format!("{open}}}");
    }
    container(Kind::Set, &open, size, |rendered| {
        for item in items.take(MAX_ARRAY_LENGTH) {
            rendered.push(Entry {
                text: item.js_nested(),
                number: false,
            });
        }
    })
}

/// `formatRaw` from the depth cut on: elide past [`DEPTH`], render the entries
/// one level in, add node's "... n more items", and reduce.
fn container(
    kind: Kind,
    open: &str,
    length: usize,
    render: impl FnOnce(&mut Vec<Entry>),
) -> String {
    if RECURSE_TIMES.with(Cell::get) > DEPTH {
        return kind.elided().to_string();
    }
    let mut entries = Vec::new();
    {
        let _level = Level::enter();
        render(&mut entries);
    }
    let has_more = length > MAX_ARRAY_LENGTH;
    if has_more {
        let remaining = length - MAX_ARRAY_LENGTH;
        entries.push(Entry {
            text: format!(
                "... {remaining} more item{}",
                if remaining > 1 { "s" } else { "" }
            ),
            number: false,
        });
    }
    reduce_to_single_string(kind, open, entries, has_more)
}

/// `reduceToSingleString` at `compact: 3`. With `depth` 2 the "innermost
/// depth" condition (`ctx.currentDepth - recurseTimes < ctx.compact`) always
/// holds — no container can sit three levels above a rendered one — so it is
/// not carried.
fn reduce_to_single_string(kind: Kind, open: &str, entries: Vec<Entry>, has_more: bool) -> String {
    let indentation = INDENTATION.with(Cell::get);
    let count = entries.len();
    let output = if kind == Kind::Array && count > 6 {
        group_array_elements(&entries, has_more, indentation)
    } else {
        entries.into_iter().map(|entry| entry.text).collect()
    };
    if output.len() == count {
        let start = output.len() + indentation + utf16_length(open) + 10;
        if is_below_break_length(&output, start) {
            let joined = output.join(", ");
            if !joined.contains('\n') {
                return format!("{open} {joined} {}", kind.close());
            }
        }
    }
    let line = format!("\n{}", " ".repeat(indentation));
    format!(
        "{open}{line}  {}{line}{}",
        output.join(&format!(",{line}  ")),
        kind.close()
    )
}

/// `isBelowBreakLength`: the entries, each followed by at least a comma, fit
/// [`BREAK_LENGTH`] counted in UTF-16 code units from `start`.
fn is_below_break_length(output: &[String], start: usize) -> bool {
    let mut total = output.len() + start;
    if total + output.len() > BREAK_LENGTH {
        return false;
    }
    for entry in output {
        total += utf16_length(entry);
        if total > BREAK_LENGTH {
            return false;
        }
    }
    true
}

/// `groupArrayElements`: an array of more than six entries laid out in
/// columns when at least three fit side by side and no entry dwarfs the rest.
fn group_array_elements(entries: &[Entry], has_more: bool, indentation: usize) -> Vec<String> {
    const SEPARATOR_SPACE: usize = 2;
    let output_length = if has_more {
        entries.len() - 1
    } else {
        entries.len()
    };
    let data_length: Vec<usize> = entries[..output_length]
        .iter()
        .map(|entry| display_width(&entry.text))
        .collect();
    let total_length: usize = data_length
        .iter()
        .map(|length| length + SEPARATOR_SPACE)
        .sum();
    let max_length = data_length.iter().copied().max().unwrap_or(0);
    let actual_max = max_length + SEPARATOR_SPACE;
    let ungrouped = || entries.iter().map(|entry| entry.text.clone()).collect();
    if !(actual_max * 3 + indentation < BREAK_LENGTH
        && (total_length as f64 / actual_max as f64 > 5.0 || max_length <= 6))
    {
        return ungrouped();
    }
    let approx_char_heights = 2.5;
    let average_bias = (actual_max as f64 - total_length as f64 / entries.len() as f64).sqrt();
    let biased_max = (actual_max as f64 - 3.0 - average_bias).max(1.0);
    let columns = [
        js_round((approx_char_heights * biased_max * output_length as f64).sqrt() / biased_max),
        ((BREAK_LENGTH - indentation) / actual_max) as f64,
        (COMPACT * 4) as f64,
        15.0,
    ]
    .into_iter()
    .fold(f64::INFINITY, f64::min);
    if columns <= 1.0 {
        return ungrouped();
    }
    let columns = columns as usize;
    let max_line_length: Vec<usize> = (0..columns)
        .map(|column| {
            (column..output_length)
                .step_by(columns)
                .map(|index| data_length[index])
                .max()
                .unwrap_or(0)
                + SEPARATOR_SPACE
        })
        .collect();
    // node reads `value[i]` for every OUTPUT index, the "more items" slot
    // included — which is the array's next element, rendered or not; the
    // entry list carries no value there, and every array this decides for
    // holds one type, so its first element answers for it.
    let pad_start = entries[..output_length].iter().all(|entry| entry.number);
    let mut grouped = Vec::new();
    for row_start in (0..output_length).step_by(columns) {
        let row_end = (row_start + columns).min(output_length);
        let mut line = String::new();
        for index in row_start..row_end - 1 {
            let text = format!("{}, ", entries[index].text);
            let padding = max_line_length[index - row_start] + utf16_length(&entries[index].text)
                - data_length[index];
            line.push_str(&pad(&text, padding, pad_start));
        }
        let last = row_end - 1;
        if pad_start {
            let padding = max_line_length[last - row_start] + utf16_length(&entries[last].text)
                - data_length[last]
                - SEPARATOR_SPACE;
            line.push_str(&pad(&entries[last].text, padding, true));
        } else {
            line.push_str(&entries[last].text);
        }
        grouped.push(line);
    }
    if has_more {
        grouped.push(entries[output_length].text.clone());
    }
    grouped
}

/// `Math.round` for the non-negative values it is handed here.
fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// `padStart`/`padEnd` with spaces to `target` UTF-16 code units.
fn pad(text: &str, target: usize, at_start: bool) -> String {
    let length = utf16_length(text);
    if length >= target {
        return text.to_string();
    }
    let spaces = " ".repeat(target - length);
    if at_start {
        format!("{spaces}{text}")
    } else {
        format!("{text}{spaces}")
    }
}

/// JS's `.length`.
fn utf16_length(text: &str) -> usize {
    text.encode_utf16().count()
}

/// node's `getStringWidth` (its non-ICU table): a control character is zero
/// wide, a full-width code point two, a combining mark or other zero-width
/// code point zero, anything else one.
fn display_width(text: &str) -> usize {
    text.chars()
        .map(|character| {
            let code = character as u32;
            if code < 32 || (0x7f..0xa0).contains(&code) {
                0
            } else if is_full_width(code) {
                2
            } else if is_zero_width(code) {
                0
            } else {
                1
            }
        })
        .sum()
}

fn is_full_width(code: u32) -> bool {
    code >= 0x1100
        && (code <= 0x115f
            || code == 0x2329
            || code == 0x232a
            || ((0x2e80..=0x3247).contains(&code) && code != 0x303f)
            || (0x3250..=0x4dbf).contains(&code)
            || (0x4e00..=0xa4c6).contains(&code)
            || (0xa960..=0xa97c).contains(&code)
            || (0xac00..=0xd7a3).contains(&code)
            || (0xf900..=0xfaff).contains(&code)
            || (0xfe10..=0xfe19).contains(&code)
            || (0xfe30..=0xfe6b).contains(&code)
            || (0xff01..=0xff60).contains(&code)
            || (0xffe0..=0xffe6).contains(&code)
            || (0x1b000..=0x1b001).contains(&code)
            || (0x1f200..=0x1f251).contains(&code)
            || (0x1f300..=0x1f64f).contains(&code)
            || (0x20000..=0x3fffd).contains(&code))
}

/// node's `isZeroWidthCodePoint`.
fn is_zero_width(code: u32) -> bool {
    code <= 0x1f
        || (0x7f..=0x9f).contains(&code)
        || (0x300..=0x36f).contains(&code)
        || (0x200b..=0x200f).contains(&code)
        || (0x20d0..=0x20ff).contains(&code)
        || (0xfe00..=0xfe0f).contains(&code)
        || (0xfe20..=0xfe2f).contains(&code)
        || (0xe0100..=0xe01ef).contains(&code)
}

/// A string INSIDE a container — `formatPrimitive` for a string: cut at
/// [`MAX_STRING_LENGTH`], split at its newlines when it is longer than the
/// line has room for, and quoted by `strEscape`'s rule.
pub fn string(text: &str) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    let (text, trailer) = if units.len() > MAX_STRING_LENGTH {
        let remaining = units.len() - MAX_STRING_LENGTH;
        (
            String::from_utf16_lossy(&units[..MAX_STRING_LENGTH]),
            format!(
                "... {remaining} more character{}",
                if remaining > 1 { "s" } else { "" }
            ),
        )
    } else {
        (text.to_string(), String::new())
    };
    let length = utf16_length(&text);
    let indentation = INDENTATION.with(Cell::get);
    if length > MIN_LINE_LENGTH && length + indentation + 4 > BREAK_LENGTH {
        let joiner = format!(" +\n{}", " ".repeat(indentation + 2));
        let lines: Vec<String> = text.split_inclusive('\n').map(escape).collect();
        return lines.join(&joiner) + &trailer;
    }
    escape(&text) + &trailer
}

/// `strEscape`: single quotes, unless the string holds one — then double
/// quotes if it holds none, else backticks if it holds neither a backtick nor
/// `${`, else single quotes with the single quote escaped. Control characters,
/// the backslash and C1 controls are escaped as node spells them.
fn escape(text: &str) -> String {
    let (quote, escapes_single) = if text.contains('\'') {
        if !text.contains('"') {
            ('"', false)
        } else if !text.contains('`') && !text.contains("${") {
            ('`', false)
        } else {
            ('\'', true)
        }
    } else {
        ('\'', true)
    };
    let mut out = String::with_capacity(text.len() + 2);
    out.push(quote);
    for character in text.chars() {
        let code = character as u32;
        match character {
            '\'' if escapes_single => out.push_str("\\'"),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            _ if code < 32 || (0x7f..0xa0).contains(&code) => {
                let _ = std::fmt::Write::write_fmt(&mut out, format_args!("\\x{code:02X}"));
            }
            _ => out.push(character),
        }
    }
    out.push(quote);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(count: usize) -> Vec<crate::Str> {
        [
            "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
            "lambda", "mu", "nu", "xi", "omicron", "pi", "rho", "sigma", "tau", "upsilon", "phi",
            "chi",
        ]
        .iter()
        .take(count)
        .map(|word| crate::str_new(word))
        .collect()
    }

    /// The outputs below are node v24's `console.log`, verbatim.
    #[test]
    fn a_long_list_of_strings_groups_into_padded_columns() {
        assert_eq!(
            words(22).js(),
            "[\n  'alpha', 'beta',    'gamma',\n  'delta', 'epsilon', 'zeta',\n  'eta',   \
             'theta',   'iota',\n  'kappa', 'lambda',  'mu',\n  'nu',    'xi',      \
             'omicron',\n  'pi',    'rho',     'sigma',\n  'tau',   'upsilon', 'phi',\n  \
             'chi'\n]"
        );
    }

    #[test]
    fn seven_numbers_group_and_pad_at_the_start() {
        assert_eq!(
            vec![1, 2, 3, 4, 5, 6, 7].js(),
            "[\n  1, 2, 3, 4,\n  5, 6, 7\n]"
        );
        assert_eq!(vec![1, 2, 3, 4, 5, 6].js(), "[ 1, 2, 3, 4, 5, 6 ]");
        assert_eq!(
            (1..=12).map(|n| n * 100).collect::<Vec<i32>>().js(),
            "[\n  100,  200,  300,  400,\n  500,  600,  700,  800,\n  900, 1000, 1100, 1200\n]"
        );
    }

    #[test]
    fn a_list_past_one_hundred_entries_says_how_many_more() {
        let rendered = (0..103).collect::<Vec<i32>>().js();
        let rows: Vec<String> = (0..9)
            .map(|row| {
                (row * 12..(row * 12 + 12).min(100))
                    .map(|n| format!("{n:>2}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect();
        assert_eq!(
            rendered,
            format!("[\n  {},\n  ... 3 more items\n]", rows.join(",\n  "))
        );
    }

    #[test]
    fn a_fourth_level_of_nesting_is_elided() {
        assert_eq!(vec![vec![vec![vec![1]]]].js(), "[ [ [ [Array] ] ] ]");
        assert_eq!(vec![vec![vec![Vec::<i32>::new()]]].js(), "[ [ [ [] ] ] ]");
    }

    #[test]
    fn a_container_past_the_break_length_takes_a_line_per_entry() {
        let long = crate::str_new(&"x".repeat(40));
        assert_eq!(
            vec![long.clone(), long.clone()].js(),
            format!("[\n  '{0}',\n  '{0}'\n]", "x".repeat(40))
        );
    }

    #[test]
    fn a_nested_string_is_quoted_by_strescapes_rule() {
        assert_eq!(string("plain"), "'plain'");
        assert_eq!(string("it's"), "\"it's\"");
        assert_eq!(string("it's \"q\""), "`it's \"q\"`");
        assert_eq!(string("it's \"q\" `b`"), "'it\\'s \"q\" `b`'");
        assert_eq!(string("tab\there\n"), "'tab\\there\\n'");
        assert_eq!(string("\u{1}\u{7f}"), "'\\x01\\x7F'");
    }
}
