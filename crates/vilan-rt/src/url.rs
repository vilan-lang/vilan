//! `vilan-rt::url` — `std::url`'s percent-encoding pair (tracker M181,
//! `proposal/percent-encoding.md` §3).
//!
//! # What this is a twin OF
//!
//! `vilan/std/src/url.vl` binds `__percent_encode` and `__percent_decode`,
//! which on the JS backend are `encodeURIComponent` and the router's total
//! decoder (`decodeURIComponent` applied to each maximal run of `%XX`
//! escapes, a run that throws kept as written). The native differential
//! (`native/url.vl`) holds the two to the same bytes, so the rules here are
//! JavaScript's, written down:
//!
//! - **Encode**: every byte of the UTF-8 encoding is `%XX` (upper-case hex)
//!   except the unreserved `A-Z a-z 0-9 - _ . ! ~ * ' ( )` — ECMA-262's
//!   `uriUnreserved` plus the `uriMark`s, which is `encodeURIComponent`'s set.
//! - **Decode**: the text splits into maximal runs of `%` + two hex digits and
//!   everything else. "Everything else" is copied; each run's bytes are read
//!   as UTF-8, and a run that is not valid UTF-8 AS A WHOLE is copied as
//!   written — `decodeURIComponent` on that run would throw, and the total
//!   decoder answers the run itself. So `%41%FF` stays `%41%FF` on both
//!   backends rather than half-decoding to `A%FF`. A lone `%` and `%G1` are
//!   not escapes at all, so they never start a run.
//!
//! `decodeURIComponent` refuses exactly what [`std::str::from_utf8`] refuses
//! (overlong forms, encoded surrogates, code points past U+10FFFF, truncated
//! sequences), which is what makes the run-level answer identical.
//!
//! # Why by hand
//!
//! Order 37's R8: no crates.io dependencies, and this is forty lines.

use crate::Str;

/// `encodeURIComponent(text)`.
pub fn percent_encode(text: &str) -> Str {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if is_unreserved(byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0F)]));
        }
    }
    Str::from(encoded)
}

/// The total decoder: each maximal run of `%XX` escapes decodes to its bytes
/// read as UTF-8, or stays as written when they are not UTF-8; nothing else
/// changes.
pub fn percent_decode(text: &str) -> Str {
    let bytes = text.as_bytes();
    let mut decoded = String::with_capacity(text.len());
    let mut run = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let run_start = index;
        run.clear();
        while let Some(byte) = escape_at(bytes, index) {
            run.push(byte);
            index += 3;
        }
        if run.is_empty() {
            // Not an escape: copy up to the next `%` (always a char boundary,
            // since `%` is ASCII), or the end.
            let next = bytes[index + 1..]
                .iter()
                .position(|byte| *byte == b'%')
                .map_or(bytes.len(), |offset| index + 1 + offset);
            decoded.push_str(&text[index..next]);
            index = next;
        } else {
            match std::str::from_utf8(&run) {
                Ok(text) => decoded.push_str(text),
                Err(_) => decoded.push_str(&text[run_start..index]),
            }
        }
    }
    Str::from(decoded)
}

/// `encodeURIComponent`'s unreserved set.
fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
        )
}

/// The byte a `%XX` escape at `index` names, if one is there.
fn escape_at(bytes: &[u8], index: usize) -> Option<u8> {
    if bytes.get(index) != Some(&b'%') {
        return None;
    }
    let high = hex_value(*bytes.get(index + 1)?)?;
    let low = hex_value(*bytes.get(index + 2)?)?;
    Some(high << 4 | low)
}

fn hex_value(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_encoder_keeps_exactly_the_unreserved_set() {
        let unreserved = "ABCXYZabcxyz0189-_.!~*'()";
        assert_eq!(&*percent_encode(unreserved), unreserved);
        assert_eq!(
            &*percent_encode(" /?#[]@$&+,;=:%\"<>\\^`{|}"),
            "%20%2F%3F%23%5B%5D%40%24%26%2B%2C%3B%3D%3A%25%22%3C%3E%5C%5E%60%7B%7C%7D"
        );
        assert_eq!(&*percent_encode("é😀"), "%C3%A9%F0%9F%98%80");
        assert_eq!(&*percent_encode(""), "");
    }

    #[test]
    fn the_decoder_decodes_runs_and_keeps_what_is_not_an_escape() {
        assert_eq!(&*percent_decode("hello%20world"), "hello world");
        assert_eq!(&*percent_decode("caf%c3%a9"), "café");
        assert_eq!(&*percent_decode("%F0%9F%98%80!"), "😀!");
        assert_eq!(&*percent_decode("100%"), "100%");
        assert_eq!(&*percent_decode("%G1%2"), "%G1%2");
        assert_eq!(&*percent_decode("é%"), "é%");
        assert_eq!(&*percent_decode(""), "");
    }

    #[test]
    fn a_run_that_is_not_utf8_as_a_whole_is_kept_as_written() {
        assert_eq!(&*percent_decode("%FF"), "%FF");
        assert_eq!(&*percent_decode("%41%FF"), "%41%FF");
        assert_eq!(&*percent_decode("%41%FF %41"), "%41%FF A");
        // Truncated, overlong and surrogate encodings, as decodeURIComponent.
        assert_eq!(&*percent_decode("%C3"), "%C3");
        assert_eq!(&*percent_decode("%C0%80"), "%C0%80");
        assert_eq!(&*percent_decode("%ED%A0%80"), "%ED%A0%80");
        // Split by a non-escape, each half is its own run.
        assert_eq!(&*percent_decode("%C3x%A9"), "%C3x%A9");
    }

    #[test]
    fn decoding_inverts_encoding() {
        for text in ["", "a b/c?d=e&f", "é😀%", "%41", "100%"] {
            assert_eq!(&*percent_decode(&percent_encode(text)), text);
        }
    }
}
