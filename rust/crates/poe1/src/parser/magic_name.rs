//! `ref/apt/renderer/src/parser/magic-name.ts`: the base type inside a magic
//! item's name.

use wraex_core::js;

use super::Fail;
use crate::data::{Data, Namespace};

/// `magicBasetype`: the longest run of words in `name` that is a craftable item.
pub fn magic_basetype(d: &Data, name: &str) -> Result<Option<String>, Fail> {
    // Chinese and Japanese don't use spaces between words: fall back to characters.
    let words: Vec<String> = if name.contains(' ') {
        name.split(' ').map(str::to_owned).collect()
    } else {
        // `split('')` splits UTF-16 units; a non-BMP character would split into
        // halves there, here it stays whole (no item names have one).
        name.chars().map(String::from).collect()
    };
    let separator = if name.contains(' ') { " " } else { "" };

    let mut found: Vec<String> = Vec::new();
    for start in 0..words.len() {
        for end in start..words.len() {
            let candidate = words[start..=end].join(separator);
            // `result && result[0].craftable` throws on an empty list (a hash collision).
            let craftable = match d.item_by_translated(Namespace::Item, &candidate) {
                None => false,
                Some(r) => r.first().ok_or(Fail::THROW)?.craftable.is_some(),
            };
            if craftable {
                found.push(candidate);
            }
        }
    }
    // Stable sort, longest (in UTF-16 units) first.
    found.sort_by_key(|n| std::cmp::Reverse(js::length(n)));
    Ok(found.into_iter().next())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::tests::en;

    #[test]
    fn finds_the_base_type() {
        let d = en();
        assert_eq!(
            magic_basetype(d, "Seething Divine Life Flask of Staunching").unwrap().as_deref(),
            Some("Divine Life Flask")
        );
        assert_eq!(magic_basetype(d, "Nothing Here").unwrap(), None);
    }
}
