//! `fastest-levenshtein`'s `distance`, the fuzzy matcher behind the item search's
//! OCR fallback.
//!
//! The reference uses it in `web/item-search/WidgetItemSearch.vue`
//! (`fuzzyFindHeistGem`) to pick the skill gem a noisy OCR line meant. Only the
//! value matters, not the bit-parallel way the package computes it, so this is
//! the textbook edit distance. It runs over UTF-16 code units, like the
//! package's `charCodeAt` loops, so a string outside the BMP costs what it costs
//! in the browser.

/// The Levenshtein distance between `a` and `b`: the fewest single-character
/// insertions, deletions and substitutions that turn one into the other.
pub fn distance(a: &str, b: &str) -> usize {
    let a = as_utf16(a);
    let b = as_utf16(b);
    // `if (a.length < b.length) { swap }`, so the row is the shorter string's.
    let (a, b) = if a.len() < b.len() { (&b, &a) } else { (&a, &b) };
    if b.is_empty() {
        return a.len();
    }

    // `prev[j]` is the distance between `a[..i]` and `b[..j]`; each row only
    // needs the one above it. Walking `b` alongside `prev` and `prev[1..]` gives
    // the cells up-left and above; `left` is the cell just written. Distances
    // never exceed `a.len()`, so the additions cannot overflow.
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, &ca) in a.iter().enumerate() {
        let mut left = i.saturating_add(1);
        let mut row = Vec::with_capacity(prev.len());
        row.push(left);
        for ((&cb, &above_left), &above) in b.iter().zip(&prev).zip(prev.iter().skip(1)) {
            let cost = usize::from(ca != cb);
            left = above.saturating_add(1).min(left.saturating_add(1)).min(above_left.saturating_add(cost));
            row.push(left);
        }
        prev = row;
    }
    prev.last().copied().unwrap_or_default()
}

fn as_utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_edits() {
        assert_eq!(distance("", ""), 0);
        assert_eq!(distance("", "kitten"), 6);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(distance("flaw", "lawn"), 2);
        // Symmetric, whichever argument is longer.
        assert_eq!(distance("anomalous empower", "anomalous empower"), 0);
        assert_eq!(distance("anomalus empower", "anomalous empower"), 1);
        assert_eq!(distance("anomalous empower", "anomalus empower"), 1);
    }

    /// The package swaps its arguments and uses Myers' bit-parallel algorithm for
    /// the short case, which computes the same distance.
    #[test]
    fn matches_the_reference_cases() {
        // `ref/apt/renderer/node_modules/fastest-levenshtein/test.js`.
        let cases = [
            ("a", "a", 0),
            ("ab", "ba", 2),
            ("abc", "abc", 0),
            ("abc", "abd", 1),
            ("anomalous phantasmal", "anomalous phantasma", 1),
            ("Divergent", "divergent", 1),
        ];
        for (a, b, expected) in cases {
            assert_eq!(distance(a, b), expected, "{a:?} vs {b:?}");
        }
    }
}
