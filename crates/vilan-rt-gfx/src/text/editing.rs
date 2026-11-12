//! Where a paragraph's graphemes are, for editing (`std::ui::kit`'s text
//! input, native-ui.md P4): the caret's rectangle for an index, the index
//! nearest a point, the next and previous grapheme and word boundaries, and
//! the rectangles a selection covers.
//!
//! # Engine-neutral, and only when asked for
//!
//! An engine records [`EditingGeometry`] while it shapes, and only for a
//! layout shaped for editing ([`TextSystem::editable_layout`](super::TextSystem::editable_layout)):
//! every other paragraph pays nothing for it. What it records is what any text
//! engine knows after laying a paragraph out — each grapheme's byte range, its
//! box on its line and its direction — so a platform engine behind the same
//! interface (R15) answers the same questions; nothing here names `parley`.
//!
//! # Graphemes, in bytes
//!
//! Every index here is a byte offset into the shaped text, on a grapheme
//! boundary: a caret never lands inside a grapheme (an emoji with its
//! modifiers, a letter with its combining marks), and a step moves by one
//! grapheme. An index inside one is read as the grapheme's start. The host
//! converts to and from the program's UTF-16 indices at the seam.
//!
//! # Directions
//!
//! Steps are LOGICAL (the next grapheme in the text, whichever way it is
//! drawn); a caret is drawn at the grapheme's LEADING edge (its left in a
//! left-to-right run, its right in a right-to-left one), and at the end of
//! the text at the last grapheme's trailing edge. A selection is the boxes of
//! the graphemes it covers, wherever bidirectional order put them, merged
//! where they touch on a line.

use crate::geometry::{Point, Rect};

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
    pub(crate) word: bool,
}

/// One line's box: its top and height, in dp from the layout's top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LineBox {
    pub(crate) top: f32,
    pub(crate) height: f32,
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
}

impl EditingGeometry {
    /// The graphemes in logical (text) order.
    fn logical(&self) -> Vec<&GraphemeBox> {
        let mut logical: Vec<&GraphemeBox> = self.graphemes.iter().collect();
        logical.sort_by_key(|grapheme| grapheme.start);
        logical
    }

    /// `index` moved back to the start of the grapheme it is inside, and
    /// kept within the text.
    pub(crate) fn snap(&self, index: usize) -> usize {
        let index = index.min(self.text_len);
        self.graphemes
            .iter()
            .find(|grapheme| grapheme.start < index && index < grapheme.end)
            .map_or(index, |grapheme| grapheme.start)
    }

    fn line(&self, line: usize) -> LineBox {
        self.lines.get(line).copied().unwrap_or(LineBox {
            top: 0.0,
            height: 0.0,
        })
    }

    /// The caret at `index`: a `width` wide rectangle at the leading edge of
    /// the grapheme starting there (the trailing edge of the last one at the
    /// end of the text), as tall as its line.
    pub(crate) fn caret(&self, index: usize, width: f32) -> Rect {
        let index = self.snap(index);
        let starting = self
            .graphemes
            .iter()
            .find(|grapheme| grapheme.start == index);
        let (x, line) = match starting {
            Some(grapheme) => (
                if grapheme.rtl {
                    grapheme.x + grapheme.width
                } else {
                    grapheme.x
                },
                grapheme.line,
            ),
            None => match self.logical().last() {
                Some(last) => (
                    if last.rtl {
                        last.x
                    } else {
                        last.x + last.width
                    },
                    last.line,
                ),
                None => (0.0, 0),
            },
        };
        let line = self.line(line);
        Rect::new(x, line.top, width, line.height)
    }

    /// The caret position nearest `point` (dp from the layout's top-left):
    /// on the line nearest its y, the grapheme edge nearest its x.
    pub(crate) fn index_at(&self, point: Point) -> usize {
        let line = self
            .lines
            .iter()
            .enumerate()
            .min_by(|(_, first), (_, second)| {
                distance(point.y, first.top, first.top + first.height).total_cmp(&distance(
                    point.y,
                    second.top,
                    second.top + second.height,
                ))
            })
            .map_or(0, |(line, _)| line);
        let nearest = self
            .graphemes
            .iter()
            .filter(|grapheme| grapheme.line == line)
            .min_by(|first, second| {
                distance(point.x, first.x, first.x + first.width).total_cmp(&distance(
                    point.x,
                    second.x,
                    second.x + second.width,
                ))
            });
        let Some(grapheme) = nearest else {
            // An empty line: the end of the text before it.
            return self
                .graphemes
                .iter()
                .filter(|grapheme| grapheme.line < line)
                .map(|grapheme| grapheme.end)
                .max()
                .unwrap_or(0);
        };
        let left_half = point.x < grapheme.x + grapheme.width / 2.0;
        if left_half != grapheme.rtl {
            grapheme.start
        } else {
            grapheme.end
        }
    }

    /// The next (`forward`) or previous grapheme boundary from `index`; with
    /// `word`, the end of the next word or the start of the previous one
    /// (the non-word graphemes on the way are passed over first). Stays at
    /// the text's end or start.
    pub(crate) fn step(&self, index: usize, forward: bool, word: bool) -> usize {
        let index = self.snap(index);
        let logical = self.logical();
        if forward {
            let mut at = logical.partition_point(|grapheme| grapheme.start < index);
            if !word {
                return logical
                    .get(at)
                    .map_or(self.text_len, |grapheme| grapheme.end);
            }
            while logical.get(at).is_some_and(|grapheme| !grapheme.word) {
                at += 1;
            }
            while logical.get(at).is_some_and(|grapheme| grapheme.word) {
                at += 1;
            }
            at.checked_sub(1)
                .and_then(|last| logical.get(last))
                .map_or(index, |grapheme| grapheme.end.max(index))
        } else {
            let mut at = logical.partition_point(|grapheme| grapheme.end <= index);
            if !word {
                return at
                    .checked_sub(1)
                    .and_then(|before| logical.get(before))
                    .map_or(0, |grapheme| grapheme.start);
            }
            while at > 0 && !logical[at - 1].word {
                at -= 1;
            }
            while at > 0 && logical[at - 1].word {
                at -= 1;
            }
            logical
                .get(at)
                .map_or(0, |grapheme| grapheme.start.min(index))
        }
    }

    /// The boxes the graphemes in `from..to` cover (either order), merged
    /// where they touch on a line, line by line.
    pub(crate) fn selection(&self, from: usize, to: usize) -> Vec<Rect> {
        let (from, to) = (self.snap(from.min(to)), self.snap(from.max(to)));
        let mut rects: Vec<(usize, f32, f32)> = Vec::new();
        for grapheme in &self.graphemes {
            if grapheme.start < from || grapheme.end > to || grapheme.start == grapheme.end {
                continue;
            }
            let (left, right) = (grapheme.x, grapheme.x + grapheme.width);
            match rects.last_mut() {
                Some((line, _, end)) if *line == grapheme.line && (*end - left).abs() < 1.0e-3 => {
                    *end = right;
                }
                _ => rects.push((grapheme.line, left, right)),
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

    /// A one-line geometry of `widths` graphemes, each of `bytes` bytes, left
    /// to right from x 0 (or right to left when `rtl`, from x 0 too: the
    /// first grapheme in the text is the rightmost).
    fn line_of(graphemes: &[(usize, f32, bool)], rtl: bool) -> EditingGeometry {
        let mut boxes = Vec::new();
        let mut start = 0;
        for (bytes, width, word) in graphemes {
            boxes.push(GraphemeBox {
                start,
                end: start + bytes,
                x: 0.0,
                width: *width,
                line: 0,
                rtl,
                word: *word,
            });
            start += bytes;
        }
        // Visual order, x from the left.
        if rtl {
            boxes.reverse();
        }
        let mut x = 0.0;
        for grapheme in &mut boxes {
            grapheme.x = x;
            x += grapheme.width;
        }
        EditingGeometry {
            text_len: start,
            graphemes: boxes,
            lines: vec![LineBox {
                top: 0.0,
                height: 20.0,
            }],
        }
    }

    /// "ab cd", with "b" a four-byte grapheme (an emoji, say).
    fn latin() -> EditingGeometry {
        line_of(
            &[
                (1, 10.0, true),
                (4, 20.0, true),
                (1, 5.0, false),
                (1, 10.0, true),
                (1, 10.0, true),
            ],
            false,
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
        let geometry = EditingGeometry {
            text_len: 0,
            graphemes: Vec::new(),
            lines: vec![LineBox {
                top: 0.0,
                height: 17.5,
            }],
        };
        assert_eq!(geometry.caret(0, 1.0), Rect::new(0.0, 0.0, 1.0, 17.5));
        assert_eq!(geometry.index_at(Point::new(40.0, 5.0)), 0);
        assert_eq!(geometry.step(0, true, false), 0);
        assert_eq!(geometry.step(0, false, true), 0);
        assert!(geometry.selection(0, 0).is_empty());
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
        // From the start: to the end of "ab"; again: past the space, to the
        // end of "cd"; at the end it stays.
        assert_eq!(geometry.step(0, true, true), 5);
        assert_eq!(geometry.step(5, true, true), 8);
        assert_eq!(geometry.step(8, true, true), 8);
        // Back from the end: the start of "cd"; from there, the start of "ab".
        assert_eq!(geometry.step(8, false, true), 6);
        assert_eq!(geometry.step(6, false, true), 0);
        // From inside "cd": its start.
        assert_eq!(geometry.step(7, false, true), 6);
        assert_eq!(geometry.step(0, false, true), 0);
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
        let geometry = line_of(&[(1, 10.0, true), (1, 10.0, true), (1, 10.0, true)], true);
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
        let mut geometry = line_of(&[(1, 10.0, true), (1, 10.0, true)], false);
        geometry.text_len = 4;
        geometry.graphemes.push(GraphemeBox {
            start: 3,
            end: 4,
            x: 20.0,
            width: 10.0,
            line: 0,
            rtl: true,
            word: true,
        });
        geometry.graphemes.push(GraphemeBox {
            start: 2,
            end: 3,
            x: 30.0,
            width: 10.0,
            line: 0,
            rtl: true,
            word: true,
        });
        // "b" and "C": x 10..20 and 30..40, with "D" between them unselected.
        assert_eq!(
            geometry.selection(1, 3),
            vec![
                Rect::new(10.0, 0.0, 10.0, 20.0),
                Rect::new(30.0, 0.0, 10.0, 20.0)
            ]
        );
    }
}
