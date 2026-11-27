//! `filters/pseudo/index.ts`: pseudo stats (total resistances, attributes, life,
//! ...) that replace the stats they sum.

pub mod anointments;
pub mod atzoatl_rules;
pub mod flasks;
pub mod heist;
pub mod item_property;
pub mod maps;
pub mod mercenary;
pub mod reflection_rules;
pub mod timeless_jewel;
pub mod utils;

use wraex_core::Rarity;
use wraex_core::num::usize_to_f64;

use super::create_stat_filters::{Ctx, calculated_stat_to_filter};
use super::{FilterError, R, StatFilter, js_equal, missing};
use crate::parser::modifiers::{ModifierType, StatCalculated, StatRoll, StatSource};

struct RuleStat {
    stat_ref: &'static str,
    multiplier: f64,
    required: bool,
    keep: bool,
}

struct PseudoRule {
    group: Option<&'static str>,
    pseudo: &'static str,
    disabled: Option<bool>,
    replaces: Option<&'static str>,
    stats: Vec<RuleStat>,
    mutate: Option<fn(&mut StatFilter)>,
}

const fn s(stat_ref: &'static str) -> RuleStat {
    RuleStat { stat_ref, multiplier: 1.0, required: false, keep: false }
}

const fn required(stat_ref: &'static str) -> RuleStat {
    RuleStat { required: true, ..s(stat_ref) }
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
    ResistanceInfo { stat_ref: "+#% to All Resistances", elements: &["fire", "cold", "lightning"], chaos: true },
    ResistanceInfo {
        stat_ref: "+#% to all Elemental Resistances",
        elements: &["fire", "cold", "lightning"],
        chaos: false,
    },
    ResistanceInfo { stat_ref: "+#% to Fire Resistance", elements: &["fire"], chaos: false },
    ResistanceInfo { stat_ref: "+#% to Cold Resistance", elements: &["cold"], chaos: false },
    ResistanceInfo { stat_ref: "+#% to Lightning Resistance", elements: &["lightning"], chaos: false },
    ResistanceInfo {
        stat_ref: "+#% to Fire and Lightning Resistances",
        elements: &["fire", "lightning"],
        chaos: false,
    },
    ResistanceInfo { stat_ref: "+#% to Fire and Cold Resistances", elements: &["fire", "cold"], chaos: false },
    ResistanceInfo {
        stat_ref: "+#% to Cold and Lightning Resistances",
        elements: &["cold", "lightning"],
        chaos: false,
    },
    ResistanceInfo { stat_ref: "+#% to Chaos Resistance", elements: &[], chaos: true },
    ResistanceInfo { stat_ref: "+#% to Fire and Chaos Resistances", elements: &["fire"], chaos: true },
    ResistanceInfo { stat_ref: "+#% to Cold and Chaos Resistances", elements: &["cold"], chaos: true },
    ResistanceInfo { stat_ref: "+#% to Lightning and Chaos Resistances", elements: &["lightning"], chaos: true },
];

struct AttributeInfo {
    stat_ref: &'static str,
    attributes: &'static [&'static str],
}

const ATTRIBUTES_INFO: [AttributeInfo; 7] = [
    AttributeInfo { stat_ref: "+# to all Attributes", attributes: &["str", "dex", "int"] },
    AttributeInfo { stat_ref: "+# to Strength", attributes: &["str"] },
    AttributeInfo { stat_ref: "+# to Dexterity", attributes: &["dex"] },
    AttributeInfo { stat_ref: "+# to Intelligence", attributes: &["int"] },
    AttributeInfo { stat_ref: "+# to Strength and Intelligence", attributes: &["str", "int"] },
    AttributeInfo { stat_ref: "+# to Strength and Dexterity", attributes: &["str", "dex"] },
    AttributeInfo { stat_ref: "+# to Dexterity and Intelligence", attributes: &["dex", "int"] },
];

fn element_res(element: &str) -> Vec<RuleStat> {
    RESISTANCES_INFO
        .iter()
        .filter(|i| i.elements.contains(&element))
        .map(|i| RuleStat { keep: i.elements.len() > 1 || i.chaos, ..s(i.stat_ref) })
        .collect()
}

fn attribute(attr: &str) -> impl Iterator<Item = &'static AttributeInfo> {
    ATTRIBUTES_INFO.iter().filter(move |i| i.attributes.contains(&attr))
}

fn total_attribute(attr: &str) -> Vec<RuleStat> {
    attribute(attr).map(|i| RuleStat { keep: i.attributes.len() > 1, ..s(i.stat_ref) }).collect()
}

/// `+# to maximum Life`/`Mana` plus half the Strength/Intelligence.
fn with_half_attribute(stat: &'static str, attr: &str) -> Vec<RuleStat> {
    let mut stats = vec![required(stat)];
    stats.extend(attribute(attr).map(|i| RuleStat { multiplier: 5.0 / 10.0, keep: true, ..s(i.stat_ref) }));
    stats
}

fn mutate_chaos(filter: &mut StatFilter) {
    let sources: Vec<_> = filter.stat_sources().collect();
    let crafted = sources.len() == 1 && sources.first().is_some_and(|s| s.modifier.info.kind == ModifierType::Crafted);
    if crafted {
        filter.hidden = Some("filters.hide_crafted_chaos");
    } else {
        filter.disabled = false;
    }
}

/// `PSEUDO_RULES`
#[expect(clippy::too_many_lines, reason = "line-for-line port of upstream `PSEUDO_RULES` (`pseudo/index.ts`)")]
fn pseudo_rules() -> Vec<PseudoRule> {
    vec![
        PseudoRule {
            disabled: Some(false),
            ..rule(
                "+#% total Elemental Resistance",
                RESISTANCES_INFO
                    .iter()
                    .filter(|i| !i.elements.is_empty())
                    .map(|i| RuleStat { multiplier: usize_to_f64(i.elements.len()), keep: i.chaos, ..s(i.stat_ref) })
                    .collect(),
            )
        },
        PseudoRule { group: Some("to_x_ele_res"), ..rule("+#% total to Fire Resistance", element_res("fire")) },
        PseudoRule { group: Some("to_x_ele_res"), ..rule("+#% total to Cold Resistance", element_res("cold")) },
        PseudoRule {
            group: Some("to_x_ele_res"),
            ..rule("+#% total to Lightning Resistance", element_res("lightning"))
        },
        PseudoRule {
            mutate: Some(mutate_chaos),
            ..rule(
                "+#% total to Chaos Resistance",
                RESISTANCES_INFO
                    .iter()
                    .filter(|i| i.chaos)
                    .map(|i| RuleStat { keep: !i.elements.is_empty(), ..s(i.stat_ref) })
                    .collect(),
            )
        },
        // Not including the other sources from `ATTRIBUTES_INFO`.
        PseudoRule {
            group: Some("to_all_attrs"),
            ..rule("+# total to all Attributes", vec![s("+# to all Attributes")])
        },
        PseudoRule { group: Some("to_x_attr"), ..rule("+# total to Strength", total_attribute("str")) },
        PseudoRule { group: Some("to_x_attr"), ..rule("+# total to Dexterity", total_attribute("dex")) },
        PseudoRule { group: Some("to_x_attr"), ..rule("+# total to Intelligence", total_attribute("int")) },
        PseudoRule {
            disabled: Some(false),
            ..rule("+# total maximum Life", with_half_attribute("+# to maximum Life", "str"))
        },
        rule("+# total maximum Mana", with_half_attribute("+# to maximum Mana", "int")),
        rule("#% total increased maximum Energy Shield", vec![s("#% increased maximum Energy Shield")]),
        // global
        rule("+# total maximum Energy Shield", vec![s("+# to maximum Energy Shield")]),
        // global; not `#% increased Attack and Cast Speed`
        rule("+#% total Attack Speed", vec![s("#% increased Attack Speed")]),
        rule("+#% total Cast Speed", vec![s("#% increased Cast Speed")]),
        rule("#% increased Movement Speed", vec![s("#% increased Movement Speed")]),
        rule("#% total increased Physical Damage", vec![s("#% increased Global Physical Damage")]),
        PseudoRule {
            group: Some("global_crit_chance"),
            ..rule("+#% Global Critical Strike Chance", vec![s("#% increased Global Critical Strike Chance")])
        },
        PseudoRule {
            replaces: Some("global_crit_chance"),
            ..rule(
                "+#% total Critical Strike Chance for Spells",
                vec![
                    required("#% increased Spell Critical Strike Chance"),
                    s("#% increased Global Critical Strike Chance"),
                ],
            )
        },
        rule("+#% Global Critical Strike Multiplier", vec![s("+#% to Global Critical Strike Multiplier")]),
        PseudoRule {
            group: Some("incr_ele_dmg"),
            ..rule("#% increased Elemental Damage", vec![s("#% increased Elemental Damage")])
        },
        PseudoRule {
            replaces: Some("incr_ele_dmg"),
            ..rule(
                "#% increased Lightning Damage",
                vec![required("#% increased Lightning Damage"), s("#% increased Elemental Damage")],
            )
        },
        PseudoRule {
            replaces: Some("incr_ele_dmg"),
            ..rule(
                "#% increased Cold Damage",
                vec![required("#% increased Cold Damage"), s("#% increased Elemental Damage")],
            )
        },
        PseudoRule {
            group: Some("incr_fire_dmg"),
            replaces: Some("incr_ele_dmg"),
            ..rule(
                "#% increased Fire Damage",
                vec![required("#% increased Fire Damage"), s("#% increased Elemental Damage")],
            )
        },
        PseudoRule {
            group: Some("incr_spell_dmg"),
            ..rule("#% increased Spell Damage", vec![s("#% increased Spell Damage")])
        },
        PseudoRule {
            replaces: Some("incr_spell_dmg"),
            ..rule(
                "#% increased Lightning Spell Damage",
                vec![required("#% increased Lightning Spell Damage"), s("#% increased Spell Damage")],
            )
        },
        PseudoRule {
            replaces: Some("incr_spell_dmg"),
            ..rule(
                "#% increased Cold Spell Damage",
                vec![required("#% increased Cold Spell Damage"), s("#% increased Spell Damage")],
            )
        },
        PseudoRule {
            replaces: Some("incr_spell_dmg"),
            ..rule(
                "#% increased Fire Spell Damage",
                vec![required("#% increased Fire Spell Damage"), s("#% increased Spell Damage")],
            )
        },
        PseudoRule {
            replaces: Some("incr_ele_dmg"),
            ..rule(
                "#% increased Elemental Damage with Attack Skills",
                vec![required("#% increased Elemental Damage with Attack Skills"), s("#% increased Elemental Damage")],
            )
        },
        PseudoRule {
            replaces: Some("incr_fire_dmg"),
            ..rule(
                "#% increased Burning Damage",
                vec![
                    required("#% increased Burning Damage"),
                    s("#% increased Fire Damage"),
                    s("#% increased Elemental Damage"),
                ],
            )
        },
        rule("# Life Regenerated per Second", vec![s("Regenerate # Life per second")]),
        rule("#% of Life Regenerated per Second", vec![s("Regenerate #% of Life per second")]),
        rule("#% of Physical Attack Damage Leeched as Life", vec![s("#% of Physical Attack Damage Leeched as Life")]),
        rule("#% of Physical Attack Damage Leeched as Mana", vec![s("#% of Physical Attack Damage Leeched as Mana")]),
        rule("#% increased Mana Regeneration Rate", vec![s("#% increased Mana Regeneration Rate")]),
    ]
}

/// `filterPseudo`. The reference tracks the filters of a group by identity;
/// here every pseudo filter gets an id.
#[expect(clippy::too_many_lines, reason = "line-for-line port of upstream `filterPseudo` (`pseudo/index.ts`)")]
pub fn filter_pseudo(ctx: &mut Ctx) -> R<()> {
    let item = ctx.item;
    let is_unique = item.rarity == Some(Rarity::Unique);
    let mut filters: Vec<(Option<usize>, StatFilter)> =
        std::mem::take(&mut ctx.filters).into_iter().map(|f| (None, f)).collect();
    let mut by_group: Vec<(&'static str, Vec<usize>)> = Vec::new();
    let rules = pseudo_rules();
    let mut applied: Vec<&PseudoRule> = Vec::new();

    'rules: for (id, rule) in rules.iter().enumerate() {
        let mut sources: Vec<StatSource> = Vec::new();
        for calc in &ctx.stats_by_type {
            let Some(info) = rule.stats.iter().find(|i| i.stat_ref == calc.stat.stat_ref) else { continue };
            for source in &calc.sources {
                let c = source.contributes.ok_or_else(|| missing("contributes"))?;
                let m = info.multiplier;
                sources.push(StatSource {
                    contributes: Some(StatRoll { value: c.value * m, min: c.min * m, max: c.max * m }),
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

        if is_unique {
            if item.is_corrupted.is_some() {
                if sources.len() < 2 {
                    continue;
                }
            } else if sources.iter().filter(|s| s.modifier.info.kind == ModifierType::Explicit).count() < 2 {
                continue;
            }
        }

        let stat = ctx
            .d
            .pseudo_stat_by_ref(rule.pseudo)
            .ok_or_else(|| FilterError(format!("no pseudo stat: {}", rule.pseudo)))?;
        let calc = StatCalculated { stat, kind: ModifierType::Pseudo, sources };
        let mut filter = calculated_stat_to_filter(&calc, ctx.search_in_range, item)?;

        filter.disabled = rule.disabled.unwrap_or(true);
        if let Some(mutate) = rule.mutate {
            mutate(&mut filter);
        }

        filters.push((Some(id), filter));
        applied.push(rule);

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

    ctx.stats_by_type.retain(|m| {
        !applied.iter().any(|rule| rule.stats.iter().any(|s| m.stat.stat_ref == s.stat_ref && (!s.keep || !is_unique)))
    });

    let group = |name: &str| by_group.iter().find(|(g, _)| *g == name).map(|(_, ids)| ids.clone());
    let value = |filters: &[(Option<usize>, StatFilter)], id: usize| -> Option<f64> {
        filters.iter().find(|(fid, _)| *fid == Some(id)).and_then(|(_, f)| f.roll.as_ref()).map(|r| r.value)
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
        if let Some((_, f)) = filters.iter_mut().find(|(fid, _)| *fid == Some(id)) {
            f.hidden = Some(hidden);
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
        if let [_, second, third] = attrs.as_slice() {
            let to_all = group("to_all_attrs");
            let values: Vec<Option<f64>> = attrs.iter().map(|&id| value(&filters, id)).collect();
            let all_equal = values.split_first().is_some_and(|(first, rest)| rest.iter().all(|v| v == first));
            if all_equal && to_all.is_some() {
                filters.retain(|(fid, _)| !fid.is_some_and(|fid| attrs.contains(&fid)));
            } else {
                if let Some(to_all) = &to_all {
                    filters.retain(|(fid, _)| !fid.is_some_and(|fid| to_all.contains(&fid)));
                }
                let [v0, v1, v2] =
                    [values.first(), values.get(1), values.get(2)].map(|v| v.copied().flatten().unwrap_or(f64::NAN));
                if v2 / v0 < 0.3 {
                    if js_equal(v1, v2) {
                        hide(&mut filters, *second, "hide_attr_same_2nd_n_3rd");
                        hide(&mut filters, *third, "hide_attr_same_2nd_n_3rd");
                    } else {
                        hide(&mut filters, *third, "hide_attr_smallest_total");
                    }
                }
            }
        }
    }

    ctx.filters = filters.into_iter().map(|(_, f)| f).collect();
    Ok(())
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
        d.check_stat_refs(maps::stat_refs()).unwrap();
        d.check_stat_refs(flasks::stat_refs()).unwrap();
        d.check_stat_refs(heist::stat_refs()).unwrap();
        d.check_stat_refs(atzoatl_rules::stat_refs()).unwrap();
        d.check_stat_refs(reflection_rules::stat_refs()).unwrap();
        d.check_stat_refs(mercenary::stat_refs()).unwrap();
    }

    #[test]
    fn rules_match_the_reference() {
        let rules = pseudo_rules();
        assert_eq!(rules.len(), 35);
        let ele = &rules[0];
        assert_eq!(ele.stats.len(), 11);
        assert_eq!(ele.stats[0].multiplier, 3.0);
        assert!(ele.stats[0].keep);
        let fire = &rules[1];
        assert_eq!(
            fire.stats.iter().map(|s| s.stat_ref).collect::<Vec<_>>(),
            [
                "+#% to All Resistances",
                "+#% to all Elemental Resistances",
                "+#% to Fire Resistance",
                "+#% to Fire and Lightning Resistances",
                "+#% to Fire and Cold Resistances",
                "+#% to Fire and Chaos Resistances",
            ]
        );
        let life = &rules[9];
        assert_eq!(life.stats.len(), 5);
        assert_eq!(life.stats[1].multiplier, 0.5);
    }
}
