//! A small, reusable "closest name" primitive (E58): given a name that failed
//! to resolve and the real names it might have meant, find the one closest to
//! it by edit distance — but only when it is CLEARLY close, never a guess
//! dressed up as a suggestion.
//!
//! Deliberately generic and dependency-free: the first customer is the
//! invalid-initializer-field diagnostic (`analyzer.rs`), and the backlog
//! records unknown-variable/member/module diagnostics as later customers of
//! this same primitive — each its own decision about whether and how to wire
//! it in, which is why this module doesn't know about diagnostics at all.

/// The Levenshtein edit distance between `a` and `b`: the fewest
/// single-character insertions, deletions, or substitutions that turn one
/// into the other. Operates on `char`s (not bytes), so a suggestion over
/// non-ASCII identifiers still counts each character once.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    // One row of the DP table at a time — O(min(a, b)) memory. `row[j]` holds
    // the distance between `a[..i]` and `b[..j]` for the row currently being
    // filled.
    let mut previous_row: Vec<usize> = (0..=b.len()).collect();
    let mut current_row = vec![0usize; b.len() + 1];
    for (i, &a_char) in a.iter().enumerate() {
        current_row[0] = i + 1;
        for (j, &b_char) in b.iter().enumerate() {
            let cost = if a_char == b_char { 0 } else { 1 };
            current_row[j + 1] = (previous_row[j + 1] + 1) // deletion
                .min(current_row[j] + 1) // insertion
                .min(previous_row[j] + cost); // substitution
        }
        std::mem::swap(&mut previous_row, &mut current_row);
    }
    previous_row[b.len()]
}

/// The threshold below which two names are "clearly close": at most a third
/// of the longer name's length, rounded down, and never less than 1 (so two
/// equal-length one-character names could still match at distance 1 — though
/// in practice a name that short rarely clears the ratio against anything
/// longer, which is the point: `"x"` vs `"entries"` needs 6 of `"entries"`'s 7
/// characters inserted, distance 6 (or 7 with the swap counted), against a
/// threshold of `max(1, 7) / 3 = 2` — refused. `"entires"` vs `"entries"` is a
/// single transposed pair, distance 2, against a threshold of `7 / 3 = 2` —
/// suggested. The ratio (not a flat cap) is what lets a longer name absorb a
/// couple of typos while a short one still needs to be nearly exact.
fn threshold(a_len: usize, b_len: usize) -> usize {
    (a_len.max(b_len) / 3).max(1)
}

/// The closest name to `target` among `candidates`, by edit distance — `None`
/// when nothing clears the [`threshold`] (including when `candidates` is
/// empty, or the only matches are exact — `target` itself is not "a closest
/// OTHER name"). Ties keep the first candidate reached, so a deterministic
/// candidate order (declaration order, not a hash-iterated one) gives a
/// deterministic suggestion.
pub fn closest_name<'a, I>(target: &str, candidates: I) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut best: Option<(&str, usize)> = None;
    for candidate in candidates {
        let distance = edit_distance(target, candidate);
        if distance == 0 || distance > threshold(target.len(), candidate.len()) {
            continue;
        }
        if best.is_none_or(|(_, best_distance)| distance < best_distance) {
            best = Some((candidate, distance));
        }
    }
    best.map(|(name, _)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_strings_have_zero_distance() {
        assert_eq!(edit_distance("entries", "entries"), 0);
    }

    #[test]
    fn a_transposed_pair_is_distance_two() {
        // "entires" vs "entries": the `ir`/`ri` pair is swapped, one
        // substitution short of each position — plain Levenshtein (no
        // transposition move) counts that as 2, not 1.
        assert_eq!(edit_distance("entires", "entries"), 2);
    }

    #[test]
    fn distance_is_symmetric() {
        assert_eq!(
            edit_distance("kitten", "sitting"),
            edit_distance("sitting", "kitten")
        );
        assert_eq!(edit_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn empty_strings() {
        assert_eq!(edit_distance("", ""), 0);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("abc", ""), 3);
    }

    // The threshold's suggest side: a clear typo of a real field name is
    // offered, from either direction of the pair.
    #[test]
    fn suggests_a_clear_typo() {
        assert_eq!(
            closest_name("entires", ["entries", "id", "created_at"]),
            Some("entries")
        );
    }

    // The threshold's refuse side: a name with almost nothing in common with
    // any candidate — a single character against a seven-character word —
    // gets no suggestion. This is the case the ratio threshold exists for: a
    // flat small cap (say, "distance <= 2") would ALSO refuse this, but only
    // by accident of the specific lengths involved; the ratio is what keeps a
    // long, heavily-typo'd name from still qualifying (see
    // `a_long_name_with_many_typos_is_still_refused`) while keeping a short
    // name honest.
    #[test]
    fn refuses_an_unrelated_short_name() {
        assert_eq!(closest_name("x", ["entries", "id", "created_at"]), None);
    }

    #[test]
    fn a_long_name_sharing_nothing_is_still_refused() {
        // Ten characters each, not one shared: every position substitutes, so
        // distance is 10 against a threshold of `10 / 3 = 3` — the ratio
        // refuses this even though both names are long, which is the case a
        // flat small cap (e.g. "distance <= 2") would get right only by
        // accident of these particular lengths.
        assert_eq!(edit_distance("aaaaaaaaaa", "bbbbbbbbbb"), 10);
        assert_eq!(closest_name("aaaaaaaaaa", ["bbbbbbbbbb"]), None);
    }

    #[test]
    fn an_exact_match_is_not_a_suggestion() {
        // `closest_name` finds the closest OTHER name; a name that already
        // matches exactly isn't "close", it's already right, and a caller
        // only calls this after resolution already failed — but the
        // primitive stays correct standalone too.
        assert_eq!(closest_name("entries", ["entries"]), None);
    }

    #[test]
    fn empty_candidates_refuses() {
        assert_eq!(closest_name("entries", []), None);
    }

    #[test]
    fn ties_keep_the_first_candidate() {
        // "ba" is distance 1 from both "bar" and "baz" — declaration order
        // decides, not iteration order, so the primitive must be stable.
        assert_eq!(closest_name("ba", ["bar", "baz"]), Some("bar"));
        assert_eq!(closest_name("ba", ["baz", "bar"]), Some("baz"));
    }

    #[test]
    fn picks_the_strictly_closer_candidate_over_a_farther_tie_breaker() {
        assert_eq!(closest_name("entryy", ["entries", "entry"]), Some("entry"));
    }
}
