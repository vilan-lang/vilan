//! Where a paragraph's graphemes are, for editing (`std::ui::kit`'s text
//! input and text area, native-ui.md P4 and P5): the caret's rectangle for an
//! index, the index nearest a point, the next and previous grapheme and word
//! boundaries, the ends of a visual line and the index a line above or below,
//! and the rectangles a selection covers.
//!
//! # Engine-neutral, and only when asked for
//!
//! An engine records [`EditingGeometry`] while it shapes, and only for a
//! layout shaped for editing ([`TextSystem::editable_layout`](super::TextSystem::editable_layout)):
//! every other paragraph pays nothing for it. What it records is what any text
//! engine knows after laying a paragraph out — each grapheme's byte range, its
//! box on its line and its direction, and each line's box and byte range — so
//! a platform engine behind the same interface (R15) answers the same
//! questions; nothing here names `parley`. What a grapheme IS (a word's, a
//! hard line break) is read from the text here, not from the engine, and a
//! byte of a line the engine drew nothing for (`parley` draws no cluster for
//! a line holding only a line break) gets a box of no width at the line's end
//! ([`EditingGeometry::new`]), so every byte of the text is in a grapheme.
//!
//! # Graphemes, in bytes
//!
//! Every index here is a byte offset into the shaped text, on a grapheme
//! boundary: a caret never lands inside a grapheme (an emoji with its
//! modifiers, a letter with its combining marks), and a step moves by one
//! grapheme. An index inside one is read as the grapheme's start. The host
//! converts to and from the program's UTF-16 indices at the seam.
//!
//! # Lines: which line an index is on
//!
//! A visual line holds the indices where its graphemes START (and the last
//! line also the end of the text). So an index at a SOFT WRAP — the end of
//! one visual line and the start of the next — is on the LOWER line, and its
//! caret is drawn at the start of that line (one rule, no affinity to carry
//! with the index). The last caret position of a wrapped line is therefore
//! the start of its last grapheme: before the space the line broke at, or,
//! for a line ended by a line break, before the break (after the line's
//! visible text). A point past a line's end, End, and a move up or down all
//! land there, and none of them ever answers an index drawn on another line.
//!
//! # Directions
//!
//! Steps are LOGICAL (the next grapheme in the text, whichever way it is
//! drawn); a caret is drawn at the grapheme's LEADING edge (its left in a
//! left-to-right run, its right in a right-to-left one), and at the end of
//! its line's text at the line's last grapheme's trailing edge. A selection
//! is the boxes of the graphemes it covers, wherever bidirectional order put
//! them, merged where they touch on a line: one rectangle per visual line
//! for a selection in one direction. A selected hard line break shows as a
//! small extent at its line's end ([`BREAK_EXTENT`]), so a selected empty
//! line is seen to be selected.

use std::ops::Range;

use crate::geometry::{Point, Rect};

/// How wide a selected hard line break is drawn, as a part of its line's
/// height: a quarter, about a narrow space at ordinary line heights.
pub(crate) const BREAK_EXTENT: f32 = 0.25;

/// One grapheme as laid out (intersected with a font run, as an engine
/// shapes it: a grapheme a font change splits is two entries).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GraphemeBox {
    /// Its bytes in the text: `start..end`.
    pub(crate) start: usize,
    pub(crate) end: usize,
    /// Its left edge and its advance, in dp from the layout's left.
    pub(crate) x: f32,
    pub(crate) width: f32,
    /// The line it is on.
    pub(crate) line: usize,
    pub(crate) rtl: bool,
    /// It begins with a letter or a digit: part of a word, for word steps.
    /// Read from the text by [`EditingGeometry::new`].
    pub(crate) word: bool,
    /// It is a hard line break (a line feed, a carriage return, U+2028 or
    /// U+2029). Read from the text by [`EditingGeometry::new`].
    pub(crate) hard_break: bool,
}

impl GraphemeBox {
    /// A grapheme as an engine reports it: where it is, not what it is.
    pub(crate) fn placed(
        bytes: Range<usize>,
        x: f32,
        width: f32,
        line: usize,
        rtl: bool,
    ) -> GraphemeBox {
        GraphemeBox {
            start: bytes.start,
            end: bytes.end,
            x,
            width,
            line,
            rtl,
            word: false,
            hard_break: false,
        }
    }

    /// Its leading edge: where a caret before it is drawn.
    fn leading(&self) -> f32 {
        if self.rtl {
            self.x + self.width
        } else {
            self.x
        }
    }

    /// Its trailing edge: where a caret after it is drawn.
    fn trailing(&self) -> f32 {
        if self.rtl {
            self.x
        } else {
            self.x + self.width
        }
    }
}

/// One line's box: its top and height, in dp from the layout's top, and its
/// bytes in the text (`start..end`; the last line of a text ending in a line
/// break is empty, at the text's end).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LineBox {
    pub(crate) top: f32,
    pub(crate) height: f32,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

/// See the module documentation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EditingGeometry {
    /// The text's length in bytes: the last caret position.
    pub(crate) text_len: usize,
    /// Every grapheme, line by line, each line's in visual order.
    pub(crate) graphemes: Vec<GraphemeBox>,
    /// At least one line, even for an empty text (where the caret goes).
    pub(crate) lines: Vec<LineBox>,
    /// Each line's graphemes: a range of `graphemes`.
    spans: Vec<Range<usize>>,
    /// `graphemes`' indices in logical (text) order.
    logical: Vec<usize>,
}

/// The characters that end a line wherever they are (Unicode's mandatory
/// breaks that a text field can hold).
fn is_hard_break(character: char) -> bool {
    matches!(
        character,
        '\n' | '\r' | '\u{000B}' | '\u{000C}' | '\u{0085}' | '\u{2028}' | '\u{2029}'
    )
}

impl EditingGeometry {
    /// The geometry of `text` from an engine's boxes: each grapheme's kind
    /// read from the text, and every byte of a line no box covers given a
    /// box of no width at the line's visual end (see the module
    /// documentation). `lines` must be at least one, in order, covering the
    /// text (a range past its end is cut to it).
    pub(crate) fn new(
        text: &str,
        mut graphemes: Vec<GraphemeBox>,
        mut lines: Vec<LineBox>,
    ) -> EditingGeometry {
        // Within the text: an engine may end the last line past it (`parley`
        // gives an empty text's line one byte).
        for line in &mut lines {
            line.end = line.end.min(text.len());
            line.start = line.start.min(line.end);
        }
        // Line by line, keeping each line's visual order: a stable sort, so
        // every pass below reads one line's boxes, never the whole text's per
        // line (a min-content shaping breaks after every grapheme).
        graphemes.sort_by_key(|grapheme| grapheme.line);
        let mut placed = Vec::with_capacity(graphemes.len());
        let mut spans = Vec::with_capacity(lines.len());
        let mut first = 0;
        for (index, line) in lines.iter().enumerate() {
            let count = graphemes[first..]
                .iter()
                .take_while(|grapheme| grapheme.line == index)
                .count();
            let on_line = &graphemes[first..first + count];
            first += count;
            let span_start = placed.len();
            placed.extend_from_slice(on_line);
            // The bytes of the line no box covers: a box of no width at the
            // line's end, after its others.
            let mut covered: Vec<(usize, usize)> = on_line
                .iter()
                .map(|grapheme| (grapheme.start, grapheme.end))
                .collect();
            covered.sort_unstable();
            let end_x = on_line
                .iter()
                .map(|grapheme| grapheme.x + grapheme.width)
                .fold(0.0_f32, f32::max);
            let mut at = line.start;
            for (start, end) in covered.into_iter().chain([(line.end, line.end)]) {
                if start > at {
                    placed.push(GraphemeBox::placed(at..start, end_x, 0.0, index, false));
                }
                at = at.max(end);
            }
            spans.push(span_start..placed.len());
        }
        let mut graphemes = placed;
        for grapheme in &mut graphemes {
            let first = text
                .get(grapheme.start..grapheme.end)
                .and_then(|grapheme| grapheme.chars().next());
            grapheme.word = first.is_some_and(char::is_alphanumeric);
            grapheme.hard_break = first.is_some_and(is_hard_break);
        }
        let mut logical: Vec<usize> = (0..graphemes.len()).collect();
        logical.sort_by(|first, second| {
            let (first, second) = (&graphemes[*first], &graphemes[*second]);
            (first.start, first.x)
                .partial_cmp(&(second.start, second.x))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        EditingGeometry {
            text_len: text.len(),
            graphemes,
            lines,
            spans,
            logical,
        }
    }

    /// The `position`th grapheme in logical order.
    fn nth(&self, position: usize) -> &GraphemeBox {
        &self.graphemes[self.logical[position]]
    }

    /// How many graphemes (in logical order) start before `index`.
    fn starting_before(&self, index: usize) -> usize {
        self.logical
            .partition_point(|grapheme| self.graphemes[*grapheme].start < index)
    }

    /// The graphemes on `line`, in visual order.
    fn on_line(&self, line: usize) -> &[GraphemeBox] {
        self.spans
            .get(line)
            .map_or(&[], |span| &self.graphemes[span.clone()])
    }

    /// `index` moved back to the start of the grapheme it is inside, and
    /// kept within the text.
    pub(crate) fn snap(&self, index: usize) -> usize {
        let index = index.min(self.text_len);
        match self.starting_before(index).checked_sub(1) {
            Some(before) if index < self.nth(before).end => self.nth(before).start,
            _ => index,
        }
    }

    fn line(&self, line: usize) -> LineBox {
        self.lines.get(line).copied().unwrap_or(LineBox {
            top: 0.0,
            height: 0.0,
            start: 0,
            end: 0,
        })
    }

    fn last_line(&self) -> usize {
        self.lines.len().saturating_sub(1)
    }

    /// The visual line `index` is on (see the module documentation): the
    /// line whose bytes hold it, and the last line for the end of the text.
    pub(crate) fn line_of(&self, index: usize) -> usize {
        let index = self.snap(index);
        self.lines
            .partition_point(|line| line.end <= index)
            .min(self.last_line())
    }

    /// The grapheme logically last on `line`: the line break of a line a
    /// break ends, the space a wrapped line broke at.
    fn last_on_line(&self, line: usize) -> Option<&GraphemeBox> {
        self.on_line(line)
            .iter()
            .max_by_key(|grapheme| grapheme.start)
    }

    /// The caret at `index`: a `width` wide rectangle at the leading edge of
    /// the grapheme starting there (at the end of a line's text, the
    /// trailing edge of its last grapheme), as tall as its line — the line
    /// [`line_of`](Self::line_of) answers.
    pub(crate) fn caret(&self, index: usize, width: f32) -> Rect {
        let index = self.snap(index);
        let line = self.line_of(index);
        let on_line = self.on_line(line);
        let x = match on_line.iter().find(|grapheme| grapheme.start == index) {
            Some(grapheme) => grapheme.leading(),
            None => self.last_on_line(line).map_or(0.0, GraphemeBox::trailing),
        };
        let line = self.line(line);
        Rect::new(x, line.top, width, line.height)
    }

    /// The caret position nearest `point` (dp from the layout's top-left):
    /// on the line nearest its y, the grapheme edge nearest its x.
    pub(crate) fn index_at(&self, point: Point) -> usize {
        self.index_on_line(self.nearest_line(point.y), point.x)
    }

    /// The caret position on `line` nearest `x`: the nearer edge of the
    /// grapheme nearest it, never an index of another line (past a wrapped
    /// line's end, the start of its last grapheme).
    fn index_on_line(&self, line: usize, x: f32) -> usize {
        let Some(grapheme) = nearest(self.on_line(line), x) else {
            // An empty line (the last of a text ending in a line break, or
            // an empty text): its one position.
            return self.line(line).start.min(self.text_len);
        };
        let left_half = x < grapheme.x + grapheme.width / 2.0;
        let index = if left_half != grapheme.rtl {
            grapheme.start
        } else {
            grapheme.end
        };
        if self.line_of(index) == line {
            index
        } else {
            self.last_on_line(line).map_or(index, |last| last.start)
        }
    }

    /// The start of the grapheme UNDER `point` (dp from the layout's
    /// top-left): on the line nearest its y, the grapheme nearest its x —
    /// where [`index_at`](Self::index_at) answers the nearer EDGE. What a
    /// double click selects the word of. An empty line's one position, and
    /// 0 for an empty text.
    pub(crate) fn grapheme_at(&self, point: Point) -> usize {
        let line = self.nearest_line(point.y);
        nearest(self.on_line(line), point.x).map_or_else(
            || self.line(line).start.min(self.text_len),
            |grapheme| grapheme.start,
        )
    }

    /// The start (or with `end`, the last caret position) of the visual line
    /// `index` is on: Home's and End's. The last position of a line other
    /// than the last is the start of its last grapheme (the module
    /// documentation's rule); the last line's is the end of the text.
    pub(crate) fn line_end(&self, index: usize, end: bool) -> usize {
        let line = self.line_of(index);
        if !end {
            return self.line(line).start.min(self.text_len);
        }
        if line == self.last_line() {
            return self.text_len;
        }
        self.last_on_line(line)
            .map_or(self.line(line).end, |last| last.start)
    }

    /// The caret position `lines` visual lines below `index`'s (above, when
    /// negative) nearest `goal_x` (dp from the layout's left: the x a run of
    /// vertical moves started from). Past the first line, the start of the
    /// text; past the last, its end.
    pub(crate) fn vertical(&self, index: usize, goal_x: f32, lines: isize) -> usize {
        let line = self.line_of(index);
        let target = line.cast_signed().saturating_add(lines);
        if target < 0 {
            0
        } else if target.cast_unsigned() > self.last_line() {
            self.text_len
        } else {
            self.index_on_line(target.cast_unsigned(), goal_x)
        }
    }

    /// The run of graphemes of one kind — word graphemes (a letter or a digit
    /// first) or the others — around the grapheme starting at `index` (the
    /// last grapheme when `index` is the end of the text), as `start..end`:
    /// what a double click selects. A hard line break is a run of its own,
    /// so no run crosses a line break. An empty text answers `index..index`.
    pub(crate) fn word_around(&self, index: usize) -> (usize, usize) {
        let index = self.snap(index);
        let count = self.logical.len();
        let Some(mut first) = (0..count)
            .find(|position| self.nth(*position).start == index)
            .or_else(|| count.checked_sub(1))
        else {
            return (index, index);
        };
        let kind = |grapheme: &GraphemeBox| (grapheme.hard_break, grapheme.word);
        let around = self.nth(first);
        if around.hard_break {
            return (around.start, around.end);
        }
        let mut last = first;
        while first > 0 && kind(self.nth(first - 1)) == kind(around) {
            first -= 1;
        }
        while last + 1 < count && kind(self.nth(last + 1)) == kind(around) {
            last += 1;
        }
        (self.nth(first).start, self.nth(last).end)
    }

    fn nearest_line(&self, y: f32) -> usize {
        self.lines
            .iter()
            .enumerate()
            .min_by(|(_, first), (_, second)| {
                distance(y, first.top, first.top + first.height).total_cmp(&distance(
                    y,
                    second.top,
                    second.top + second.height,
                ))
            })
            .map_or(0, |(line, _)| line)
    }

    /// The next (`forward`) or previous grapheme boundary from `index`; with
    /// `word`, the end of the next word or the start of the previous one
    /// (the non-word graphemes on the way are passed over first). Stays at
    /// the text's end or start.
    pub(crate) fn step(&self, index: usize, forward: bool, word: bool) -> usize {
        let index = self.snap(index);
        let count = self.logical.len();
        if forward {
            let mut at = self.starting_before(index);
            if !word {
                return if at < count {
                    self.nth(at).end
                } else {
                    self.text_len
                };
            }
            while at < count && !self.nth(at).word {
                at += 1;
            }
            while at < count && self.nth(at).word {
                at += 1;
            }
            at.checked_sub(1)
                .map_or(index, |last| self.nth(last).end.max(index))
        } else {
            let mut at = self
                .logical
                .partition_point(|grapheme| self.graphemes[*grapheme].end <= index);
            if !word {
                return at.checked_sub(1).map_or(0, |before| self.nth(before).start);
            }
            while at > 0 && !self.nth(at - 1).word {
                at -= 1;
            }
            while at > 0 && self.nth(at - 1).word {
                at -= 1;
            }
            if at < count {
                self.nth(at).start.min(index)
            } else {
                0
            }
        }
    }

    /// The boxes the graphemes in `from..to` cover (either order), merged
    /// where they touch on a line, line by line; a hard line break covers
    /// [`BREAK_EXTENT`] of its line's height.
    pub(crate) fn selection(&self, from: usize, to: usize) -> Vec<Rect> {
        let (from, to) = (self.snap(from.min(to)), self.snap(from.max(to)));
        if from == to {
            return Vec::new();
        }
        let mut rects: Vec<(usize, f32, f32)> = Vec::new();
        for line in self.line_of(from)..=self.line_of(to) {
            let height = self.line(line).height;
            for grapheme in self.on_line(line) {
                if grapheme.start < from || grapheme.end > to || grapheme.start == grapheme.end {
                    continue;
                }
                let width = if grapheme.hard_break {
                    grapheme.width.max(height * BREAK_EXTENT)
                } else {
                    grapheme.width
                };
                let (left, right) = (grapheme.x, grapheme.x + width);
                match rects.last_mut() {
                    Some((on, _, end)) if *on == line && (*end - left).abs() < 1.0e-3 => {
                        *end = right;
                    }
                    _ => rects.push((line, left, right)),
                }
            }
        }
        rects
            .into_iter()
            .map(|(line, left, right)| {
                let line = self.line(line);
                Rect::new(left, line.top, right - left, line.height)
            })
            .collect()
    }
}

/// The first of `graphemes` nearest `x` (distance 0 inside one).
fn nearest(graphemes: &[GraphemeBox], x: f32) -> Option<&GraphemeBox> {
    graphemes.iter().min_by(|first, second| {
        distance(x, first.x, first.x + first.width).total_cmp(&distance(
            x,
            second.x,
            second.x + second.width,
        ))
    })
}

/// How far `value` is outside `start..end` (0 inside it).
fn distance(value: f32, start: f32, end: f32) -> f32 {
    if value < start {
        start - value
    } else if value > end {
        value - end
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One line of `text`, each grapheme as wide as `widths` says (in text
    /// order; a `text` of one-character graphemes unless a width is given
    /// for a longer one), drawn left to right from x 0 — or right to left,
    /// from x 0 too, when `rtl` (the first grapheme in the text the
    /// rightmost).
    fn line_of(text: &str, graphemes: &[(usize, f32)], rtl: bool) -> EditingGeometry {
        let mut boxes = Vec::new();
        let mut start = 0;
        for (bytes, width) in graphemes {
            boxes.push(GraphemeBox::placed(
                start..start + bytes,
                0.0,
                *width,
                0,
                rtl,
            ));
            start += bytes;
        }
        assert_eq!(start, text.len(), "the widths cover the text");
        if rtl {
            boxes.reverse();
        }
        let mut x = 0.0;
        for grapheme in &mut boxes {
            grapheme.x = x;
            x += grapheme.width;
        }
        EditingGeometry::new(
            text,
            boxes,
            vec![LineBox {
                top: 0.0,
                height: 20.0,
                start: 0,
                end: text.len(),
            }],
        )
    }

    /// "a𝐛 cd", with "𝐛" a four-byte letter.
    fn latin() -> EditingGeometry {
        line_of(
            "a\u{1D41B} cd",
            &[(1, 10.0), (4, 20.0), (1, 5.0), (1, 10.0), (1, 10.0)],
            false,
        )
    }

    /// Lines of one-byte graphemes 10 dp wide, each line `(text, broke)`:
    /// 20 dp tall, left to right. A line's text includes the break or the
    /// space it ended at; an engine draws no box for a line holding only a
    /// line break (as `parley` does not), and draws the others' breaks with
    /// no width.
    fn lines_of(lines: &[&str]) -> EditingGeometry {
        let text: String = lines.concat();
        let mut boxes = Vec::new();
        let mut line_boxes = Vec::new();
        let mut start = 0;
        for (index, line) in lines.iter().enumerate() {
            let mut x = 0.0;
            let only_a_break = *line == "\n";
            for (offset, character) in line.char_indices() {
                if only_a_break {
                    break;
                }
                let width = if character == '\n' { 0.0 } else { 10.0 };
                let at = start + offset;
                boxes.push(GraphemeBox::placed(at..at + 1, x, width, index, false));
                x += width;
            }
            line_boxes.push(LineBox {
                top: 20.0 * index as f32,
                height: 20.0,
                start,
                end: start + line.len(),
            });
            start += line.len();
        }
        EditingGeometry::new(&text, boxes, line_boxes)
    }

    /// An empty text, its one line ending a byte past it as `parley` ends it.
    fn empty() -> EditingGeometry {
        EditingGeometry::new(
            "",
            Vec::new(),
            vec![LineBox {
                top: 0.0,
                height: 17.5,
                start: 0,
                end: 1,
            }],
        )
    }

    #[test]
    fn a_caret_stands_at_the_leading_edge_and_past_the_last_grapheme_at_the_end() {
        let geometry = latin();
        assert_eq!(geometry.caret(0, 1.0), Rect::new(0.0, 0.0, 1.0, 20.0));
        assert_eq!(geometry.caret(1, 1.0).left(), 10.0);
        assert_eq!(geometry.caret(5, 1.0).left(), 30.0);
        assert_eq!(geometry.caret(8, 2.0), Rect::new(55.0, 0.0, 2.0, 20.0));
        // Inside the four-byte grapheme: its start.
        assert_eq!(geometry.caret(3, 1.0).left(), 10.0);
        // Past the end: the end.
        assert_eq!(geometry.caret(99, 1.0).left(), 55.0);
    }

    #[test]
    fn an_empty_text_has_its_caret_at_the_start_of_its_one_line() {
        let geometry = empty();
        assert!(geometry.graphemes.is_empty(), "no byte to cover");
        assert_eq!(geometry.caret(0, 1.0), Rect::new(0.0, 0.0, 1.0, 17.5));
        assert_eq!(geometry.index_at(Point::new(40.0, 5.0)), 0);
        assert_eq!(geometry.step(0, true, false), 0);
        assert_eq!(geometry.step(0, false, true), 0);
        assert!(geometry.selection(0, 0).is_empty());
        assert_eq!(geometry.line_end(0, true), 0);
        assert_eq!(geometry.vertical(0, 30.0, 1), 0);
        assert_eq!(geometry.vertical(0, 30.0, -1), 0);
    }

    #[test]
    fn steps_move_one_grapheme_never_into_one() {
        let geometry = latin();
        assert_eq!(geometry.step(0, true, false), 1);
        assert_eq!(geometry.step(1, true, false), 5);
        assert_eq!(geometry.step(5, false, false), 1);
        assert_eq!(geometry.step(1, false, false), 0);
        assert_eq!(geometry.step(0, false, false), 0);
        assert_eq!(geometry.step(8, true, false), 8);
        // From inside a grapheme: from its start.
        assert_eq!(geometry.step(3, true, false), 5);
    }

    #[test]
    fn word_steps_pass_the_gap_then_the_word() {
        let geometry = latin();
        // From the start: to the end of "a𝐛"; again: past the space, to the
        // end of "cd"; at the end it stays.
        assert_eq!(geometry.step(0, true, true), 5);
        assert_eq!(geometry.step(5, true, true), 8);
        assert_eq!(geometry.step(8, true, true), 8);
        // Back from the end: the start of "cd"; from there, the start of "a𝐛".
        assert_eq!(geometry.step(8, false, true), 6);
        assert_eq!(geometry.step(6, false, true), 0);
        // From inside "cd": its start.
        assert_eq!(geometry.step(7, false, true), 6);
        assert_eq!(geometry.step(0, false, true), 0);
    }

    #[test]
    fn a_word_is_the_run_of_one_kind_around_the_grapheme_under_a_point() {
        let geometry = latin();
        // "a𝐛 cd": the right half of "𝐛" is still "𝐛", whose word is "a𝐛";
        // where `index_at` would answer the edge after it.
        assert_eq!(geometry.grapheme_at(Point::new(25.0, 10.0)), 1);
        assert_eq!(geometry.index_at(Point::new(25.0, 10.0)), 5);
        assert_eq!(geometry.word_around(1), (0, 5));
        // Inside the four-byte grapheme: its start, so its word.
        assert_eq!(geometry.word_around(3), (0, 5));
        // The space is a run of its own.
        assert_eq!(geometry.grapheme_at(Point::new(32.0, 10.0)), 5);
        assert_eq!(geometry.word_around(5), (5, 6));
        assert_eq!(geometry.word_around(6), (6, 8));
        // Past the end: the last grapheme, so the last word.
        assert_eq!(geometry.grapheme_at(Point::new(90.0, 10.0)), 7);
        assert_eq!(geometry.word_around(8), (6, 8));
        // Nothing to select in an empty text.
        let empty = empty();
        assert_eq!(empty.grapheme_at(Point::new(5.0, 5.0)), 0);
        assert_eq!(empty.word_around(0), (0, 0));
    }

    #[test]
    fn a_point_lands_on_the_nearer_edge_of_the_grapheme_under_it() {
        let geometry = latin();
        assert_eq!(geometry.index_at(Point::new(4.0, 10.0)), 0);
        assert_eq!(geometry.index_at(Point::new(6.0, 10.0)), 1);
        assert_eq!(geometry.index_at(Point::new(19.0, 10.0)), 1);
        assert_eq!(geometry.index_at(Point::new(21.0, 10.0)), 5);
        // Left of everything, right of everything, above and below the line.
        assert_eq!(geometry.index_at(Point::new(-50.0, 10.0)), 0);
        assert_eq!(geometry.index_at(Point::new(500.0, 10.0)), 8);
        assert_eq!(geometry.index_at(Point::new(52.0, -30.0)), 8);
        assert_eq!(geometry.index_at(Point::new(2.0, 90.0)), 0);
    }

    #[test]
    fn right_to_left_text_has_its_caret_and_hits_mirrored() {
        // Three one-byte graphemes, drawn right to left: the first in the
        // text is at x 20..30.
        let geometry = line_of("abc", &[(1, 10.0), (1, 10.0), (1, 10.0)], true);
        assert_eq!(geometry.caret(0, 1.0).left(), 30.0);
        assert_eq!(geometry.caret(1, 1.0).left(), 20.0);
        assert_eq!(geometry.caret(3, 1.0).left(), 0.0);
        // The right half of the first grapheme is before it; the left half
        // after it.
        assert_eq!(geometry.index_at(Point::new(28.0, 5.0)), 0);
        assert_eq!(geometry.index_at(Point::new(22.0, 5.0)), 1);
        assert_eq!(geometry.step(0, true, false), 1);
    }

    #[test]
    fn a_selection_is_the_boxes_of_its_graphemes_merged_where_they_touch() {
        let geometry = latin();
        assert_eq!(
            geometry.selection(1, 7),
            vec![Rect::new(10.0, 0.0, 35.0, 20.0)]
        );
        // Either order; an empty one covers nothing.
        assert_eq!(geometry.selection(7, 1), geometry.selection(1, 7));
        assert!(geometry.selection(5, 5).is_empty());
        // Ends inside a grapheme snap to its start.
        assert_eq!(
            geometry.selection(0, 3),
            vec![Rect::new(0.0, 0.0, 10.0, 20.0)]
        );
    }

    #[test]
    fn a_selection_across_directions_is_one_box_per_visual_stretch() {
        // "ab" left to right, then "CD" right to left: visually a b D C.
        let geometry = EditingGeometry::new(
            "abCD",
            vec![
                GraphemeBox::placed(0..1, 0.0, 10.0, 0, false),
                GraphemeBox::placed(1..2, 10.0, 10.0, 0, false),
                GraphemeBox::placed(3..4, 20.0, 10.0, 0, true),
                GraphemeBox::placed(2..3, 30.0, 10.0, 0, true),
            ],
            vec![LineBox {
                top: 0.0,
                height: 20.0,
                start: 0,
                end: 4,
            }],
        );
        // "b" and "C": x 10..20 and 30..40, with "D" between them unselected.
        assert_eq!(
            geometry.selection(1, 3),
            vec![
                Rect::new(10.0, 0.0, 10.0, 20.0),
                Rect::new(30.0, 0.0, 10.0, 20.0)
            ]
        );
    }

    // ------------------------------------------------------- many lines ---

    #[test]
    fn a_line_the_engine_drew_nothing_for_gets_its_break_as_a_box_of_no_width() {
        // "ab\n" "\n" "cd": the middle line holds only its break.
        let geometry = lines_of(&["ab\n", "\n", "cd"]);
        let breaks: Vec<(usize, usize, f32, f32, usize)> = geometry
            .graphemes
            .iter()
            .filter(|grapheme| grapheme.hard_break)
            .map(|grapheme| {
                (
                    grapheme.start,
                    grapheme.end,
                    grapheme.x,
                    grapheme.width,
                    grapheme.line,
                )
            })
            .collect();
        assert_eq!(breaks, [(2, 3, 20.0, 0.0, 0), (3, 4, 0.0, 0.0, 1)]);
        assert!(
            !geometry
                .graphemes
                .iter()
                .any(|grapheme| grapheme.word && grapheme.hard_break)
        );
    }

    #[test]
    fn an_index_is_on_the_line_whose_graphemes_start_there_so_a_wrap_point_is_on_the_lower() {
        // A soft wrap after "ab " (the space ends the upper line), then a
        // line break, then an empty last line: "ab cd\n".
        let geometry = lines_of(&["ab ", "cd\n", ""]);
        let lines: Vec<usize> = (0..=6).map(|index| geometry.line_of(index)).collect();
        assert_eq!(lines, [0, 0, 0, 1, 1, 1, 2]);
        // The wrap point's caret is at the START of the lower line.
        assert_eq!(geometry.caret(3, 1.0), Rect::new(0.0, 20.0, 1.0, 20.0));
        // Before the space: the upper line, after "ab".
        assert_eq!(geometry.caret(2, 1.0), Rect::new(20.0, 0.0, 1.0, 20.0));
        // Before the break: after "cd"; the end of the text, after the
        // break, is on the empty last line.
        assert_eq!(geometry.caret(5, 1.0), Rect::new(20.0, 20.0, 1.0, 20.0));
        assert_eq!(geometry.caret(6, 1.0), Rect::new(0.0, 40.0, 1.0, 20.0));
    }

    #[test]
    fn a_point_past_a_lines_end_lands_before_its_last_grapheme_never_on_the_next_line() {
        let geometry = lines_of(&["ab ", "cd\n", ""]);
        // Far right of the wrapped line: before the space it broke at.
        assert_eq!(geometry.index_at(Point::new(200.0, 5.0)), 2);
        // On the space's right half: the same (its end is the next line's).
        assert_eq!(geometry.index_at(Point::new(28.0, 5.0)), 2);
        // Far right of the line a break ends: before the break.
        assert_eq!(geometry.index_at(Point::new(200.0, 25.0)), 5);
        // On the empty last line: the end of the text.
        assert_eq!(geometry.index_at(Point::new(200.0, 45.0)), 6);
        assert_eq!(geometry.index_at(Point::new(-5.0, 45.0)), 6);
        // An empty line in the middle: before its break.
        let middle = lines_of(&["ab\n", "\n", "cd"]);
        assert_eq!(middle.index_at(Point::new(50.0, 25.0)), 3);
        assert_eq!(middle.index_at(Point::new(-50.0, 25.0)), 3);
        assert_eq!(middle.grapheme_at(Point::new(50.0, 25.0)), 3);
    }

    #[test]
    fn home_and_end_go_to_the_ends_of_the_visual_line() {
        let geometry = lines_of(&["ab ", "cd\n", "ef"]);
        // From anywhere on the wrapped first line: 0, and before the space.
        assert_eq!(geometry.line_end(1, false), 0);
        assert_eq!(geometry.line_end(1, true), 2);
        assert_eq!(geometry.line_end(2, true), 2);
        // The second line (from its start, the wrap point): 3, before "\n".
        assert_eq!(geometry.line_end(3, false), 3);
        assert_eq!(geometry.line_end(3, true), 5);
        // The last line's end is the end of the text.
        assert_eq!(geometry.line_end(6, false), 6);
        assert_eq!(geometry.line_end(6, true), 8);
    }

    #[test]
    fn up_and_down_keep_the_goal_x_across_lines_of_different_lengths() {
        // "abcdef\n" "ab\n" "abcd": a goal at x 50 (after "abcde").
        let geometry = lines_of(&["abcdef\n", "ab\n", "abcd"]);
        let goal = 50.0;
        // Down to the short line: its end (before its break).
        let second = geometry.vertical(5, goal, 1);
        assert_eq!(second, 9);
        // Down again with the same goal: x 40 is the last line's end.
        assert_eq!(geometry.vertical(second, goal, 1), 14);
        // Two lines at once, and back up to the first line's x 50.
        assert_eq!(geometry.vertical(5, goal, 2), 14);
        assert_eq!(geometry.vertical(14, goal, -2), 5);
        // Past the first line: the start of the text; past the last, its end.
        assert_eq!(geometry.vertical(3, goal, -1), 0);
        assert_eq!(geometry.vertical(11, 0.0, 1), 14);
        assert_eq!(geometry.vertical(11, 0.0, 9), 14);
        // A goal past a wrapped line's end lands before its space.
        let wrapped = lines_of(&["abc ", "abcdefgh"]);
        assert_eq!(wrapped.vertical(10, 70.0, -1), 3);
    }

    #[test]
    fn a_selection_over_lines_is_one_box_a_line_and_a_selected_break_shows() {
        let geometry = lines_of(&["ab\n", "\n", "cd"]);
        // From "b" to "c": "b" and the break (5 dp: a quarter of 20) on the
        // first line, the empty line's break, "c".
        assert_eq!(
            geometry.selection(1, 5),
            vec![
                Rect::new(10.0, 0.0, 15.0, 20.0),
                Rect::new(0.0, 20.0, 5.0, 20.0),
                Rect::new(0.0, 40.0, 10.0, 20.0),
            ]
        );
        // Up to the break, not over it: no extent.
        assert_eq!(
            geometry.selection(0, 2),
            vec![Rect::new(0.0, 0.0, 20.0, 20.0)]
        );
    }

    #[test]
    fn a_word_never_runs_across_a_line_break() {
        // "ab\n  cd": the break is a run of its own, then the spaces.
        let geometry = lines_of(&["ab\n", "  cd"]);
        assert_eq!(geometry.word_around(2), (2, 3));
        assert_eq!(geometry.word_around(3), (3, 5));
        assert_eq!(geometry.word_around(1), (0, 2));
        // Word steps pass a break like any other gap.
        assert_eq!(geometry.step(0, true, true), 2);
        assert_eq!(geometry.step(2, true, true), 7);
        assert_eq!(geometry.step(7, false, true), 5);
        assert_eq!(geometry.step(5, false, true), 0);
    }
}
