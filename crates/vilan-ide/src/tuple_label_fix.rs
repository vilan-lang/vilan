//! The two edits B569's label contradiction carries (`named-tuple-fields.md`
//! §4.3, RULED with "two quick fixes"): a tuple value landing at a tuple type
//! whose labels sit at other positions is refused — "… the label `x` names
//! slot 0 of the value and slot 1 here, so the two do not convert …: by name,
//! `(y = p.y, x = p.x)`; by position, `(p.0, p.1)`" — and the refusal itself
//! spells both rewrites of the value its span covers. The analyzer computes
//! them (it alone knows which labels the value carries, and spells them only
//! for a value that is a place, which reads the same once per slot), so this
//! module only reads them back, the way `numeric_fix` reads `.as_X()`.
//!
//! By name keeps what each label means; by position keeps where each value
//! sits. The two give different values for a reordered set, which is why the
//! language refuses rather than picking one.

use vilan_core::analyzer::LABEL_CONTRADICTION_STEER;
use vilan_core::span::Span;

/// Which reading a fix writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelReading {
    /// Each label of the position read from the value's same label.
    ByName,
    /// The value's slots in order, its labels dropped.
    ByPosition,
}

/// One edit: replace `span` (the value) with `replacement`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TupleLabelFix {
    pub span: Span,
    pub replacement: String,
    pub reading: LabelReading,
}

/// The fixes the refusal `message` at `span` spells, by name first. Empty
/// for any other message, and for a contradiction whose value is not a place.
pub fn tuple_label_fixes(span: Span, message: &str) -> Vec<TupleLabelFix> {
    if !message.contains(LABEL_CONTRADICTION_STEER) || !message.starts_with("Expected ") {
        return Vec::new();
    }
    let mut fixes = Vec::new();
    for (marker, reading) in [
        ("by name, `", LabelReading::ByName),
        ("by position, `", LabelReading::ByPosition),
    ] {
        // The spellings follow the sentence's last `: `, so a type that
        // happens to print the marker's words cannot be read as one.
        let Some(tail_at) = message.rfind(": by ") else {
            break;
        };
        let tail = &message[tail_at..];
        let Some(start) = tail.find(marker).map(|at| at + marker.len()) else {
            continue;
        };
        let Some(length) = tail[start..].find('`') else {
            continue;
        };
        fixes.push(TupleLabelFix {
            span,
            replacement: tail[start..start + length].to_string(),
            reading,
        });
    }
    fixes
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFUSAL: &str = "Expected (y: f64, x: f64), but got (x: f64, y: f64) instead: the label \
        `x` names slot 0 of the value and slot 1 here, so the two do not convert — match by \
        name, writing the labels out, or by position, dropping them: by name, \
        `(y = p.y, x = p.x)`; by position, `(p.0, p.1)`";

    #[test]
    fn both_spellings_are_read_off_the_refusal_by_name_first() {
        let span = Span::from(10..11);
        assert_eq!(
            tuple_label_fixes(span, REFUSAL),
            vec![
                TupleLabelFix {
                    span,
                    replacement: "(y = p.y, x = p.x)".to_string(),
                    reading: LabelReading::ByName,
                },
                TupleLabelFix {
                    span,
                    replacement: "(p.0, p.1)".to_string(),
                    reading: LabelReading::ByPosition,
                },
            ]
        );
    }

    #[test]
    fn a_refusal_without_a_by_name_spelling_offers_the_position_alone() {
        let message = "Expected (y: f64, z: f64), but got (x: f64, y: f64) instead: the label \
            `y` names slot 1 of the value and slot 0 here, so the two do not convert — match by \
            name, writing the labels out, or by position, dropping them: by position, `(p.0, p.1)`";
        let fixes = tuple_label_fixes(Span::from(0..1), message);
        assert_eq!(fixes.len(), 1);
        assert_eq!(fixes[0].reading, LabelReading::ByPosition);
    }

    #[test]
    fn a_value_that_is_no_place_and_any_other_message_offer_nothing() {
        let no_place = "Expected (y: f64, x: f64), but got (x: f64, y: f64) instead: the label \
            `x` names slot 0 of the value and slot 1 here, so the two do not convert — match by \
            name, writing the labels out, or by position, dropping them";
        assert!(tuple_label_fixes(Span::from(0..1), no_place).is_empty());
        assert!(
            tuple_label_fixes(Span::from(0..1), "Expected i32, but got str instead.").is_empty()
        );
    }
}
