//! `filters/pseudo/missing-fractured-rules.ts`: a fractured item whose text
//! marks no mod as fractured searches its explicits as fractured.

use super::super::{FilterTag, StatFilter};

/// `applyRules`
pub fn apply_rules(filters: &mut Vec<StatFilter>, explicit_stats: Vec<StatFilter>) {
    if filters.iter().any(|f| f.tag == FilterTag::FRACTURED) {
        return;
    }
    filters.extend(
        explicit_stats
            .into_iter()
            .filter(|f| f.tag == FilterTag::EXPLICIT)
            .map(|f| StatFilter { tag: FilterTag::FRACTURED, ..f }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(stat_ref: &str, tag: FilterTag) -> StatFilter {
        StatFilter::plain(stat_ref, stat_ref, tag, false)
    }

    // `specs/web/price-check/filters/missing-fracture-rules.test.ts`. The first
    // case there passed the filters as their own explicit stats and compared the
    // array with a fresh parse of itself, which pins nothing; the port keeps the
    // intent (nothing to append) instead.

    /// `specs/web/price-check/filters/missing-fracture-rules.test.ts` (`Should do
    /// nothing if item is not fractured`).
    #[test]
    fn does_nothing_without_explicit_stats() {
        let mut filters: Vec<StatFilter> = Vec::new();
        apply_rules(&mut filters, Vec::new());
        assert!(filters.is_empty());
    }

    /// `specs/web/price-check/filters/missing-fracture-rules.test.ts` (`Should do
    /// nothing if fractured and a modifier is fractured`).
    #[test]
    fn keeps_the_filters_when_one_is_already_fractured() {
        let mut filters = vec![filter("", FilterTag::FRACTURED)];
        apply_rules(&mut filters, vec![filter("", FilterTag::EXPLICIT)]);
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0].tag, FilterTag::FRACTURED);
    }

    /// `specs/web/price-check/filters/missing-fracture-rules.test.ts` (`Should do
    /// nothing if fractured and item has some fractured mod and non fractured
    /// mods`).
    #[test]
    fn keeps_the_filters_when_fractured_and_explicit_stats_are_mixed() {
        let mut filters = vec![filter("", FilterTag::FRACTURED), filter("", FilterTag::EXPLICIT)];
        let explicit = filters.iter().filter(|f| f.tag == FilterTag::EXPLICIT).cloned().collect();
        apply_rules(&mut filters, explicit);
        assert_eq!(filters.len(), 2);
    }

    /// `specs/web/price-check/filters/missing-fracture-rules.test.ts` (`Should
    /// append explicit stats, as fractured mods to filters`).
    #[test]
    fn appends_explicit_stats_as_fractured() {
        let mut filters = vec![filter("a", FilterTag::EXPLICIT), filter("b", FilterTag::EXPLICIT)];
        let explicit = filters.clone();
        apply_rules(&mut filters, explicit);
        assert_eq!(filters.iter().filter(|f| f.tag == FilterTag::FRACTURED).count(), 2);
        assert_eq!(filters.len(), 4);
    }
}
