//! `filters/pseudo/index.ts`: pseudo stats (total resistances, attributes, life,
//! ...) that replace the stats they sum.

pub mod anointments;
pub mod atzoatl_rules;
pub mod item_property;
pub mod missing_fractured_rules;
pub mod reflection_rules;

use wraex_core::filters::{Rounding, percent_roll, round_roll};

use super::create_stat_filters::{Ctx, calculated_stat_to_filter};
use super::{FilterError, FilterRoll, MinMax, R, StatFilter, missing};
use crate::parser::modifiers::{ModifierType, StatCalculated, StatRoll, StatSource};

struct RuleStat {
    stat_ref: &'static str,
    multiplier: f64,
    required: bool,
}

struct PseudoRule {
    group: Option<&'static str>,
    pseudo: &'static str,
    disabled: Option<bool>,
    replaces: Option<&'static str>,
    stats: Vec<RuleStat>,
    mutate: Option<fn(&mut StatFilter)>,
}

fn s(stat_ref: &'static str) -> RuleStat {
    RuleStat { stat_ref, multiplier: 1.0, required: false }
}

fn rule(pseudo: &'static str, stats: Vec<RuleStat>) -> PseudoRule {
    PseudoRule { group: None, pseudo, disabled: None, replaces: None, stats, mutate: None }
}

struct ResistanceInfo {
    stat_ref: &'static str,
    elements: &'static [&'static str],
    chaos: bool,
}

const RESISTANCES_INFO: [ResistanceInfo; 12] = [
    ResistanceInfo { stat_ref: "#% to All Resistances", elements: &["fire", "cold", "lightning"], chaos: true },
    ResistanceInfo {
        stat_ref: "#% to all Elemental Resistances",
        elements: &["fire", "cold", "lightning"],
        chaos: false,
    },
    ResistanceInfo { stat_ref: "#% to Fire Resistance", elements: &["fire"], chaos: false },
    ResistanceInfo { stat_ref: "#% to Cold Resistance", elements: &["cold"], chaos: false },
    ResistanceInfo { stat_ref: "#% to Lightning Resistance", elements: &["lightning"], chaos: false },
    ResistanceInfo { stat_ref: "#% to Fire and Lightning Resistances", elements: &["fire", "lightning"], chaos: false },
    ResistanceInfo { stat_ref: "#% to Fire and Cold Resistances", elements: &["fire", "cold"], chaos: false },
    ResistanceInfo { stat_ref: "#% to Cold and Lightning Resistances", elements: &["cold", "lightning"], chaos: false },
    ResistanceInfo { stat_ref: "#% to Chaos Resistance", elements: &[], chaos: true },
    ResistanceInfo { stat_ref: "#% to Fire and Chaos Resistances", elements: &["fire"], chaos: true },
    ResistanceInfo { stat_ref: "#% to Cold and Chaos Resistances", elements: &["cold"], chaos: true },
    ResistanceInfo { stat_ref: "#% to Lightning and Chaos Resistances", elements: &["lightning"], chaos: true },
];

const ATTRIBUTES_INFO: [(&str, &[&str]); 7] = [
    ("# to all Attributes", &["str", "dex", "int"]),
    ("# to Strength", &["str"]),
    ("# to Dexterity", &["dex"]),
    ("# to Intelligence", &["int"]),
    ("# to Strength and Intelligence", &["str", "int"]),
    ("# to Strength and Dexterity", &["str", "dex"]),
    ("# to Dexterity and Intelligence", &["dex", "int"]),
];

fn elemental() -> impl Iterator<Item = &'static ResistanceInfo> {
    RESISTANCES_INFO.iter().filter(|i| !i.elements.is_empty())
}

fn element_res(element: &str) -> Vec<RuleStat> {
    RESISTANCES_INFO.iter().filter(|i| i.elements.contains(&element)).map(|i| s(i.stat_ref)).collect()
}

fn attribute(attr: &'static str) -> impl Iterator<Item = &'static str> {
    ATTRIBUTES_INFO.iter().filter(move |(_, a)| a.contains(&attr)).map(|(r, _)| *r)
}

/// `# to maximum Life`/`Mana` plus twice the Strength/Intelligence.
fn with_double_attribute(stat: &'static str, attr: &'static str) -> Vec<RuleStat> {
    let mut stats = vec![RuleStat { required: true, ..s(stat) }];
    stats.extend(attribute(attr).map(|r| RuleStat { multiplier: 2.0, ..s(r) }));
    stats
}

fn is_augment(kind: ModifierType) -> bool {
    kind == ModifierType::Augment || kind == ModifierType::AddedAugment
}

/// `PSEUDO_RULES`. The commented-out rules upstream are left out.
fn pseudo_rules() -> Vec<PseudoRule> {
    vec![
        PseudoRule {
            disabled: Some(true),
            group: Some("to_all_res"),
            mutate: Some(|f| f.hidden = Some("filters.hide_total_all_res")),
            ..rule("#% total to all Elemental Resistances", elemental().map(|i| s(i.stat_ref)).collect())
        },
        PseudoRule {
            disabled: Some(false),
            ..rule(
                "#% total Elemental Resistance",
                elemental().map(|i| RuleStat { multiplier: i.elements.len() as f64, ..s(i.stat_ref) }).collect(),
            )
        },
        PseudoRule { group: Some("to_x_ele_res"), ..rule("#% total to Fire Resistance", element_res("fire")) },
        PseudoRule { group: Some("to_x_ele_res"), ..rule("#% total to Cold Resistance", element_res("cold")) },
        PseudoRule {
            group: Some("to_x_ele_res"),
            ..rule("#% total to Lightning Resistance", element_res("lightning"))
        },
        PseudoRule {
            mutate: Some(|f| {
                if f.sources.len() == 1 && is_augment(f.sources[0].modifier.info.kind) {
                    f.hidden = Some("filters.hide_crafted_chaos");
                } else {
                    f.disabled = false;
                }
            }),
            ..rule(
                "#% total to Chaos Resistance",
                RESISTANCES_INFO.iter().filter(|i| i.chaos).map(|i| s(i.stat_ref)).collect(),
            )
        },
        // Not including the other sources from `ATTRIBUTES_INFO`.
        PseudoRule {
            group: Some("to_all_attrs"),
            ..rule("+# total to all Attributes", vec![s("# to all Attributes")])
        },
        PseudoRule { group: Some("to_x_attr"), ..rule("+# total to Strength", attribute("str").map(s).collect()) },
        PseudoRule { group: Some("to_x_attr"), ..rule("+# total to Dexterity", attribute("dex").map(s).collect()) },
        PseudoRule { group: Some("to_x_attr"), ..rule("+# total to Intelligence", attribute("int").map(s).collect()) },
        PseudoRule {
            disabled: Some(false),
            ..rule("+# total maximum Life", with_double_attribute("# to maximum Life", "str"))
        },
        rule("+# total maximum Mana", with_double_attribute("# to maximum Mana", "int")),
        rule("#% total increased maximum Energy Shield", vec![s("#% increased maximum Energy Shield")]),
        // global
        rule("+# total maximum Energy Shield", vec![s("# to maximum Energy Shield")]),
        PseudoRule {
            mutate: Some(|f| {
                if f.sources.len() != 1 || f.sources[0].modifier.info.kind != ModifierType::Implicit {
                    f.disabled = false;
                }
            }),
            ..rule("#% increased Movement Speed", vec![s("#% increased Movement Speed")])
        },
        PseudoRule {
            disabled: Some(false),
            ..rule(
                "# uses remaining",
                vec![
                    s("Adds Irradiated to a Map \n# use remaining"),
                    s("Adds Ritual Altars to a Map \n# use remaining"),
                    s("Adds a Kalguuran Expedition to a Map \n# use remaining"),
                    s("Adds a Mirror of Delirium to a Map \n# use remaining"),
                    s("Adds an Otherworldy Breach to a Map \n# use remaining"),
                    s("Empowers the Map Boss of a Map \n# use remaining"),
                    s("Adds Abysses to a Map \n# use remaining"),
                ],
            )
        },
    ]
}

/// `filterPseudo`. The reference tracks the filters of a group by identity;
/// here every pseudo filter gets an id.
pub fn filter_pseudo(ctx: &mut Ctx) -> R<()> {
    let item = ctx.item;
    let mut filters: Vec<(Option<usize>, StatFilter)> =
        std::mem::take(&mut ctx.filters).into_iter().map(|f| (None, f)).collect();
    let mut by_group: Vec<(&'static str, Vec<usize>)> = Vec::new();
    let rules = pseudo_rules();

    'rules: for (id, rule) in rules.iter().enumerate() {
        let mut sources: Vec<StatSource> = Vec::new();
        for calc in &ctx.stats_by_type {
            let Some(info) = rule.stats.iter().find(|i| i.stat_ref == calc.stat.stat_ref) else { continue };
            for source in &calc.sources {
                let c = source.contributes.ok_or_else(|| missing("contributes"))?;
                let m = info.multiplier;
                sources.push(StatSource {
                    contributes: Some(StatRoll { value: c.value * m, min: c.min * m, max: c.max * m, option: None }),
                    ..source.clone()
                });
            }
        }
        if sources.is_empty() {
            continue;
        }

        for stat in rule.stats.iter().filter(|s| s.required) {
            if !sources.iter().any(|s| s.stat.stat.stat_ref == stat.stat_ref) {
                continue 'rules;
            }
        }

        let stat =
            ctx.d.stat_by_ref(rule.pseudo).ok_or_else(|| FilterError(format!("no pseudo stat: {}", rule.pseudo)))?;
        let calc = StatCalculated { stat, kind: ModifierType::Pseudo, sources };
        let mut filter = calculated_stat_to_filter(ctx.d, &calc, ctx.search_in_range, item, true)?;

        filter.disabled = rule.disabled.unwrap_or(true);
        if let Some(mutate) = rule.mutate {
            mutate(&mut filter);
        }

        filters.push((Some(id), filter));

        if let Some((_, replaced)) = rule.replaces.and_then(|r| by_group.iter().find(|(g, _)| *g == r)) {
            filters.retain(|(fid, _)| !fid.is_some_and(|fid| replaced.contains(&fid)));
        }

        if let Some(group) = rule.group {
            match by_group.iter_mut().find(|(g, _)| *g == group) {
                Some((_, ids)) => ids.push(id),
                None => by_group.push((group, vec![id])),
            }
        }
    }

    // Every rule's stats, applied or not.
    ctx.stats_by_type.retain(|m| !rules.iter().any(|rule| rule.stats.iter().any(|s| m.stat.stat_ref == s.stat_ref)));

    let group = |name: &str| by_group.iter().find(|(g, _)| *g == name).map(|(_, ids)| ids.clone());
    let find =
        |filters: &[(Option<usize>, StatFilter)], id: usize| filters.iter().position(|(fid, _)| *fid == Some(id));
    let value = |filters: &[(Option<usize>, StatFilter)], id: usize| -> Option<f64> {
        find(filters, id).and_then(|i| filters[i].1.roll.as_ref()).map(|r| r.value)
    };
    let by_value_desc = |filters: &[(Option<usize>, StatFilter)], ids: &mut Vec<usize>| -> R<()> {
        let mut keyed = Vec::new();
        for &id in ids.iter() {
            keyed.push((id, value(filters, id).ok_or_else(|| missing("roll"))?));
        }
        // Stable, like `Array.prototype.sort`.
        keyed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        *ids = keyed.into_iter().map(|(id, _)| id).collect();
        Ok(())
    };
    let hide = |filters: &mut [(Option<usize>, StatFilter)], id: usize, hidden: &'static str| {
        if let Some(i) = find(filters, id) {
            filters[i].1.hidden = Some(hidden);
        }
    };

    if let Some(mut res) = group("to_x_ele_res") {
        by_value_desc(&filters, &mut res)?;
        let first = res.first().and_then(|&id| value(&filters, id));
        let second = res.get(1).and_then(|&id| value(&filters, id));
        let max_filter = if first == second { None } else { res.first().copied() };

        if let Some(id) = max_filter {
            hide(&mut filters, id, "filters.hide_ele_res");
        }
        filters.retain(|(fid, _)| !fid.is_some_and(|fid| res.contains(&fid) && Some(fid) != max_filter));
    }

    if let Some(mut attrs) = group("to_x_attr") {
        by_value_desc(&filters, &mut attrs)?;
        if attrs.len() == 3 {
            let to_all = group("to_all_attrs");
            let values: Vec<Option<f64>> = attrs.iter().map(|&id| value(&filters, id)).collect();
            if values.iter().all(|v| *v == values[0]) && to_all.is_some() {
                filters.retain(|(fid, _)| !fid.is_some_and(|fid| attrs.contains(&fid)));
            } else {
                if let Some(to_all) = &to_all {
                    filters.retain(|(fid, _)| !fid.is_some_and(|fid| to_all.contains(&fid)));
                }
                let [v0, v1, v2] = [values[0], values[1], values[2]].map(|v| v.unwrap_or(f64::NAN));
                if v2 / v0 < 0.3 {
                    if v1 == v2 {
                        hide(&mut filters, attrs[1], "hide_attr_same_2nd_n_3rd");
                        hide(&mut filters, attrs[2], "hide_attr_same_2nd_n_3rd");
                    } else {
                        hide(&mut filters, attrs[2], "hide_attr_smallest_total");
                    }
                }
            }
        }
    }

    if let Some(ids) = group("to_all_res") {
        if ids.len() > 1 {
            return Err(FilterError("Multiple to_all_res filters".to_owned()));
        }
        // Still in the list: this group is never replaced.
        let index = find(&filters, ids[0]).ok_or_else(|| missing("to_all_res filter"))?;

        let zero = StatRoll { value: 0.0, min: 0.0, max: 0.0, option: None };
        let mut all = zero;
        // fire, cold, lightning
        let mut elements = [zero; 3];
        let element_index = |e: &str| ["fire", "cold", "lightning"].iter().position(|x| *x == e).expect("an element");
        for source in &filters[index].1.sources {
            // Shouldn't skip any.
            let Some(info) = RESISTANCES_INFO.iter().find(|i| i.stat_ref == source.stat.stat.stat_ref) else {
                continue;
            };
            let c = source.contributes.ok_or_else(|| missing("contributes"))?;
            let targets: Vec<usize> = if info.elements.len() == 3 {
                Vec::new()
            } else {
                info.elements.iter().map(|e| element_index(e)).collect()
            };
            if info.elements.len() == 3 {
                all.value += c.value;
                all.min += c.min;
                all.max += c.max;
            }
            for i in targets {
                elements[i].value += c.value;
                elements[i].min += c.min;
                elements[i].max += c.max;
            }
        }

        let mut min_roll = 0;
        if elements[1].value < elements[min_roll].value {
            min_roll = 1;
        }
        if elements[2].value < elements[min_roll].value {
            min_roll = 2;
        }

        let total = StatRoll {
            value: all.value + elements[min_roll].value,
            min: all.min + elements[min_roll].min,
            max: all.max + elements[min_roll].max,
            option: None,
        };

        if total.value == 0.0 {
            // Don't bother without any.
            filters.remove(index);
        } else {
            let dp = false;
            let bounds = MinMax {
                min: percent_roll(total.min, -0.0, Rounding::Floor, dp),
                max: percent_roll(total.max, 0.0, Rounding::Ceil, dp),
            };
            let range = ctx.search_in_range;
            let mut default = MinMax {
                min: percent_roll(total.value, -range, Rounding::Floor, dp),
                max: percent_roll(total.value, range, Rounding::Ceil, dp),
            };
            default.min = js_max(default.min, bounds.min);
            default.max = js_min(default.max, bounds.max);

            filters[index].1.roll = Some(FilterRoll {
                value: round_roll(total.value, dp),
                min: Some(default.min),
                max: None,
                default,
                bounds: None,
                trade_invert: None,
                dp,
                is_negated: false,
                goodness: None,
            });
        }
    }

    ctx.filters = filters.into_iter().map(|(_, f)| f).collect();
    Ok(())
}

fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) }
}

fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.min(b) }
}

/// Every ref this module passes to `stat()`.
pub fn stat_refs() -> Vec<&'static str> {
    let mut refs: Vec<&'static str> = Vec::new();
    for rule in pseudo_rules() {
        refs.push(rule.pseudo);
        refs.extend(rule.stats.iter().map(|s| s.stat_ref));
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::tests::en;

    #[test]
    fn stat_refs_exist() {
        let d = en();
        d.check_stat_refs(stat_refs()).unwrap();
        d.check_stat_refs(item_property::stat_refs()).unwrap();
    }

    #[test]
    fn rules_match_the_reference() {
        let rules = pseudo_rules();
        assert_eq!(rules.len(), 16);
        assert_eq!(rules[0].stats.len(), 11);
        assert_eq!(rules[1].stats[0].multiplier, 3.0);
        assert_eq!(rules[5].stats.len(), 5);
        let life = &rules[10];
        assert_eq!(life.stats.len(), 5);
        assert_eq!(life.stats[1].multiplier, 2.0);
    }

    /// `createStatHelper` (`specs/web/price-check/filters/pseudo/index.test.ts`):
    /// one calculated stat with one contributing source.
    fn create_stat_helper(d: &crate::data::Data, stat_ref: &str, value: f64, kind: ModifierType) -> StatCalculated {
        use crate::parser::advanced_mod_desc::{ModifierInfo, ParsedModifier};
        use crate::parser::modifiers::{StatRoll, StatSource};
        use crate::parser::stat_translations::{ParsedStat, StatRollInfo};

        let stat = d.stat_by_ref(stat_ref).unwrap_or_else(|| panic!("no stat {stat_ref}"));
        let parsed = ParsedStat {
            stat: stat.clone(),
            translation: stat.matchers[0].clone(),
            roll: Some(StatRollInfo {
                unscalable: false,
                legacy: None,
                dp: false,
                value,
                min: value,
                max: value,
                option: None,
            }),
            from_added_augment: None,
        };
        StatCalculated {
            stat,
            kind,
            sources: vec![StatSource {
                modifier: ParsedModifier { info: ModifierInfo::new(kind), stats: vec![parsed.clone()] },
                stat: parsed,
                contributes: Some(StatRoll { value, min: value, max: value, option: None }),
            }],
        }
    }

    fn pseudo_item() -> crate::parser::item::Item {
        let mut item = crate::parser::item::Item::test_item();
        item.info.as_mut().unwrap().ref_name = Some("name".to_owned());
        item
    }

    fn make_ctx<'a>(
        d: &'a crate::data::Data,
        item: &'a crate::parser::item::Item,
        stats_by_type: Vec<StatCalculated>,
    ) -> Ctx<'a> {
        Ctx { d, item, search_in_range: 0.0, filters: vec![], stats_by_type }
    }

    const FIRE_RES: &str = "#% to Fire Resistance";
    const COLD_RES: &str = "#% to Cold Resistance";
    const MOVEMENT_SPEED: &str = "#% increased Movement Speed";
    const STRENGTH: &str = "# to Strength";
    const MAX_LIFE: &str = "# to maximum Life";
    const BREACH_TABLET: &str = "Adds an Otherworldy Breach to a Map \n# use remaining";

    /// `specs/web/price-check/filters/pseudo/index.test.ts` (`filterPseudo`):
    /// nothing without stats.
    #[test]
    fn filter_pseudo_does_nothing_without_stats() {
        let d = en();
        let item = pseudo_item();
        let mut ctx = make_ctx(d, &item, vec![]);
        filter_pseudo(&mut ctx).unwrap();
        assert!(ctx.stats_by_type.is_empty());
        assert!(ctx.filters.is_empty());
    }

    /// `specs/web/price-check/filters/pseudo/index.test.ts` (`filterPseudo`): a
    /// resistance becomes the total and the element's own pseudo.
    #[test]
    fn filter_pseudo_adds_the_total_and_elemental_resistance_stats() {
        let d = en();
        let item = pseudo_item();
        let mut ctx = make_ctx(d, &item, vec![create_stat_helper(d, FIRE_RES, 25.0, ModifierType::Explicit)]);
        filter_pseudo(&mut ctx).unwrap();
        assert!(ctx.stats_by_type.is_empty());
        assert_eq!(ctx.filters.len(), 2);
        let total = ctx.filters.iter().find(|f| f.stat_ref == "#% total Elemental Resistance").unwrap();
        let fire = ctx.filters.iter().find(|f| f.stat_ref == "#% total to Fire Resistance").unwrap();
        assert_eq!((total.disabled, total.roll.as_ref().unwrap().value), (false, 25.0));
        assert_eq!((fire.disabled, fire.roll.as_ref().unwrap().value), (true, 25.0));
    }

    /// `specs/web/price-check/filters/pseudo/index.test.ts` (`filterPseudo`): an
    /// unrelated stat keeps its own filter.
    #[test]
    fn filter_pseudo_keeps_unrelated_stats() {
        let d = en();
        let item = pseudo_item();
        let mut ctx = make_ctx(
            d,
            &item,
            vec![
                create_stat_helper(d, FIRE_RES, 25.0, ModifierType::Explicit),
                create_stat_helper(d, MOVEMENT_SPEED, 25.0, ModifierType::Explicit),
            ],
        );
        filter_pseudo(&mut ctx).unwrap();
        assert!(ctx.stats_by_type.is_empty());
        assert_eq!(ctx.filters.len(), 3);
        assert!(ctx.filters.iter().any(|f| f.stat_ref == "#% total Elemental Resistance"));
        assert!(ctx.filters.iter().any(|f| f.stat_ref == "#% total to Fire Resistance"));
        let movement = ctx.filters.iter().find(|f| f.stat_ref == MOVEMENT_SPEED).unwrap();
        assert!(!movement.disabled);
    }

    /// `specs/web/price-check/filters/pseudo/index.test.ts` (`filterPseudo`): two
    /// of the same stat merge into one filter with both sources.
    #[test]
    fn filter_pseudo_merges_two_of_the_same_stat() {
        let d = en();
        let item = pseudo_item();
        let mut ctx = make_ctx(
            d,
            &item,
            vec![
                create_stat_helper(d, FIRE_RES, 25.0, ModifierType::Explicit),
                create_stat_helper(d, FIRE_RES, 25.0, ModifierType::Explicit),
            ],
        );
        filter_pseudo(&mut ctx).unwrap();
        assert_eq!(ctx.filters.len(), 2);
        let total = ctx.filters.iter().find(|f| f.stat_ref == "#% total Elemental Resistance").unwrap();
        let fire = ctx.filters.iter().find(|f| f.stat_ref == "#% total to Fire Resistance").unwrap();
        assert_eq!((total.sources.len(), total.roll.as_ref().unwrap().value), (2, 50.0));
        assert_eq!((fire.sources.len(), fire.roll.as_ref().unwrap().value), (2, 50.0));
    }

    /// `specs/web/price-check/filters/pseudo/index.test.ts` (`filterPseudo`): life
    /// counts twice the strength's contribution.
    #[test]
    fn filter_pseudo_joins_life_and_strength() {
        let d = en();
        let item = pseudo_item();
        let mut ctx = make_ctx(
            d,
            &item,
            vec![
                create_stat_helper(d, MAX_LIFE, 25.0, ModifierType::Explicit),
                create_stat_helper(d, STRENGTH, 25.0, ModifierType::Explicit),
            ],
        );
        filter_pseudo(&mut ctx).unwrap();
        assert_eq!(ctx.filters.len(), 2);
        let life = ctx.filters.iter().find(|f| f.stat_ref == "+# total maximum Life").unwrap();
        let strength = ctx.filters.iter().find(|f| f.stat_ref == "+# total to Strength").unwrap();
        assert_eq!((life.sources.len(), life.roll.as_ref().unwrap().value), (2, 75.0));
        assert_eq!((strength.sources.len(), strength.roll.as_ref().unwrap().value), (1, 25.0));
    }

    /// `specs/web/price-check/filters/pseudo/index.test.ts` (`filterPseudo`): only
    /// the highest elemental resistance is kept.
    #[test]
    fn filter_pseudo_keeps_only_the_highest_elemental_resistance() {
        let d = en();
        let item = pseudo_item();
        let mut ctx = make_ctx(
            d,
            &item,
            vec![
                create_stat_helper(d, FIRE_RES, 25.0, ModifierType::Explicit),
                create_stat_helper(d, COLD_RES, 50.0, ModifierType::Explicit),
            ],
        );
        filter_pseudo(&mut ctx).unwrap();
        assert_eq!(ctx.filters.len(), 2);
        let total = ctx.filters.iter().find(|f| f.stat_ref == "#% total Elemental Resistance").unwrap();
        let cold = ctx.filters.iter().find(|f| f.stat_ref == "#% total to Cold Resistance").unwrap();
        assert_eq!((total.disabled, total.roll.as_ref().unwrap().value), (false, 75.0));
        assert_eq!((cold.disabled, cold.roll.as_ref().unwrap().value), (true, 50.0));
        assert!(ctx.filters.iter().all(|f| f.stat_ref != "#% total to Fire Resistance"));
    }

    /// `specs/web/price-check/filters/pseudo/index.test.ts` (`filterPseudo`): a
    /// tablet's "uses remaining" implicit becomes its own filter.
    #[test]
    fn filter_pseudo_turns_tablet_implicits_into_uses_remaining() {
        let d = en();
        let item = pseudo_item();
        let mut ctx = make_ctx(d, &item, vec![create_stat_helper(d, BREACH_TABLET, 8.0, ModifierType::Implicit)]);
        filter_pseudo(&mut ctx).unwrap();
        assert_eq!(ctx.filters.len(), 1);
        let uses = ctx.filters.iter().find(|f| f.stat_ref == "# uses remaining").unwrap();
        assert_eq!((uses.disabled, uses.roll.as_ref().unwrap().value), (false, 8.0));
    }
}
