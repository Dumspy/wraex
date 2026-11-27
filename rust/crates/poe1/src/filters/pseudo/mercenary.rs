//! `filters/pseudo/mercenary.ts`: a Mercenary Warrant's skills and supports.

use super::super::{
    FilterGroup, FilterOrGroup, FilterSource, FilterTag, GroupKind, MercenaryFilterInfo, R, StatFilter, ValueFilter,
    missing,
};
use super::item_property::{Prop, prop_to_filter};
use super::utils::{find_and_resolve_by_ref, meta_not_filter};
use crate::data::{Data, MercenaryBuildRef, Stat};
use crate::parser::item::Item;
use crate::parser::meta::ItemCategory;
use crate::parser::modifiers::{ModifierType, StatRoll};

struct BuildRule {
    build: &'static str,
    bad_skills: &'static [&'static str],
}

const BUILD_RULES: [BuildRule; 3] = [
    BuildRule { build: "Kineticist", bad_skills: &["Kinetic Bolt"] },
    BuildRule { build: "Manyshot", bad_skills: &["Icicle Rain"] },
    BuildRule { build: "Combatant", bad_skills: &["Wild Strike", "Spectral Helix"] },
];

/// `SearchMode`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Optional = 0,
    Required = 1,
}

impl SearchMode {
    /// The value of the filter's `option`, which the trade site matches.
    const fn as_value(self) -> f64 {
        match self {
            Self::Optional => 0.0,
            Self::Required => 1.0,
        }
    }
}

fn mercenary(stat: &Stat) -> R<&crate::data::MercenaryStat> {
    stat.mercenary.as_ref().ok_or_else(|| missing("stat.mercenary"))
}

/// `tier & 0x3`, where `undefined & 3` is 0.
fn tier_bits(stat: &Stat) -> u32 {
    stat.mercenary.as_ref().and_then(|m| m.tier).unwrap_or(0) & 0x3
}

/// `stat.modFamily?.at(-1) === stat.ref`
fn is_last_of_family(stat: &Stat) -> bool {
    stat.mod_family.as_ref().and_then(|f| f.last()) == Some(&stat.stat_ref)
}

/// `createMercenaryFilters`
#[expect(
    clippy::too_many_lines,
    reason = "line-for-line port of upstream `createMercenaryFilters` (`pseudo/mercenary.ts`)"
)]
pub fn create_mercenary_filters(d: &Data, item: &Item) -> R<Vec<FilterOrGroup>> {
    let mut out = Vec::new();

    let build_info = item.mercenary_build.as_ref().ok_or_else(|| missing("mercenaryBuild"))?;
    let rules = BUILD_RULES.iter().find(|r| r.build == build_info.ref_name);
    let bad_skills = rules.map(|r| r.bad_skills);
    let Some(MercenaryBuildRef::Build(build)) = &build_info.mercenary_build else {
        return Err(missing("mercenaryBuild.mercenaryBuild"));
    };
    let skills = item.mercenary_skills.as_ref().ok_or_else(|| missing("mercenarySkills"))?;
    let find_gem = |name: &str| {
        find_and_resolve_by_ref(d, name, ModifierType::Pseudo, Some(&ItemCategory::MercenaryWarrant), None)
    };

    for group in skills {
        let (skill, raw_supports) = group.split_first().ok_or_else(|| missing("mercenary skill"))?;
        let mut supports = Vec::new();
        for support in raw_supports {
            let m = mercenary(&support.stat)?;
            if m.synthetic_family != Some(true) || m.tier == Some(3) {
                supports.push(support);
            }
        }

        let skill_type = build
            .skills
            .iter()
            .find(|s| s.name == skill.stat.stat_ref)
            .ok_or_else(|| missing("mercenary build skill"))?
            .kind
            .as_str();
        let mut skill_filter = skill_to_filter(&skill.stat, skill_type, true, bad_skills)?;

        if supports.is_empty() {
            if skill_type == "primary" && raw_supports.is_empty() {
                skill_filter.hidden = Some("filters.mercenary_primary_no_support");
            }
            out.push(FilterOrGroup::Stat(skill_filter));
            continue;
        }

        let mut filter_group =
            FilterGroup { group: GroupKind::Mercenary, expanded: false, meta: skill_filter, stats: Vec::new() };

        // `& 0x3` turns tier 4 into 0, so it goes at the very end. Stable.
        supports.sort_by_key(|s| std::cmp::Reverse(tier_bits(&s.stat)));

        let mut possible_supports: Vec<Vec<Stat>> = Vec::new();
        for family_name in mercenary(&skill.stat)?.supports.as_deref().unwrap_or_default() {
            let family_stat = find_gem(family_name)?;
            possible_supports.push(match &family_stat.mod_family {
                Some(family) => family.iter().map(|name| find_gem(name)).collect::<R<_>>()?,
                None => vec![family_stat],
            });
        }

        let six_link = supports.len() == 5 && !possible_supports.is_empty();
        if six_link {
            let mut tier3_count: u32 = 0;
            for support in &supports {
                if mercenary(&support.stat)?.tier.is_some_and(|t| t >= 3) || is_last_of_family(&support.stat) {
                    tier3_count = tier3_count.saturating_add(1);
                }
            }
            let prop = Prop {
                sources: possible_supports.iter().cloned().map(FilterSource::Family).collect(),
                disabled: Some(true),
                ..Prop::new(
                    "6-Link",
                    "item.mercenary_6link",
                    Some(StatRoll { value: f64::from(tier3_count), min: 0.0, max: 5.0 }),
                )
            };
            filter_group.stats.push(prop_to_filter(prop, item, 0.0));
        }

        for support in &supports {
            let m = mercenary(&support.stat)?;
            let tier = m.tier;

            let trade_id = match (&support.stat.mod_family, tier) {
                (Some(family), t) if t != Some(3) => {
                    let mut ids = Vec::new();
                    for name in family {
                        let family_stat = find_gem(name)?;
                        // `undefined >= x` and `x >= undefined` are false.
                        let family_tier = mercenary(&family_stat)?.tier;
                        if let (Some(ft), Some(t)) = (family_tier, tier)
                            && ft >= t
                        {
                            ids.extend(family_stat.trade.ids.get("pseudo").cloned().unwrap_or_default());
                        }
                    }
                    Some(ids)
                }
                _ => support.stat.trade.ids.get("pseudo").cloned(),
            };

            let canon_stat = match &m.canonical {
                Some(canonical) => find_gem(canonical)?,
                None => support.stat.clone(),
            };

            filter_group.stats.push(StatFilter {
                trade_id,
                stat_ref: canon_stat.stat_ref.clone(),
                text: canon_stat.matchers.first().map(|m| m.string.clone()).unwrap_or_default(),
                tag: FilterTag::MercenarySupport,
                oils: None,
                mercenary: Some(MercenaryFilterInfo {
                    icon: None,
                    tier,
                    max_tier: Some(tier.is_some_and(|t| t >= 3) || is_last_of_family(&support.stat)),
                }),
                reminder_text: None,
                sources: Vec::new(),
                not: None,
                roll: None,
                option: Some(ValueFilter { value: SearchMode::Required.as_value() }),
                hidden: None,
                disabled: true,
            });
        }

        if six_link {
            for family in &possible_supports {
                let first = family.first().ok_or_else(|| missing("support family"))?;
                let canonical = mercenary(first)?.canonical.as_ref();
                let linked = match canonical {
                    Some(canonical) => {
                        let mut linked = false;
                        for s in raw_supports {
                            if mercenary(&s.stat)?.canonical.as_ref() == Some(canonical) {
                                linked = true;
                                break;
                            }
                        }
                        linked
                    }
                    None => raw_supports.iter().any(|s| s.stat.stat_ref == first.stat_ref),
                };
                if linked {
                    continue;
                }

                let canon_stat = match canonical {
                    Some(canonical) => {
                        family.iter().find(|s| &s.stat_ref == canonical).ok_or_else(|| missing("canonical support"))?
                    }
                    None => first,
                };

                filter_group.stats.push(StatFilter {
                    trade_id: Some(
                        family.iter().flat_map(|s| s.trade.ids.get("pseudo").cloned().unwrap_or_default()).collect(),
                    ),
                    stat_ref: canon_stat.stat_ref.clone(),
                    text: canon_stat.matchers.first().map(|m| m.string.clone()).unwrap_or_default(),
                    tag: FilterTag::MercenarySupport,
                    oils: None,
                    mercenary: None,
                    reminder_text: None,
                    sources: Vec::new(),
                    not: Some(true),
                    roll: None,
                    option: None,
                    hidden: None,
                    disabled: true,
                });
            }
        }

        out.push(FilterOrGroup::Group(filter_group));
    }

    let mut not_group =
        FilterGroup { group: GroupKind::Not, expanded: false, meta: meta_not_filter(true), stats: Vec::new() };

    for build_skill in &build.skills {
        if skills.iter().any(|g| g.first().is_some_and(|s| s.stat.stat_ref == build_skill.name)) {
            continue;
        }

        let mut filter = skill_to_filter(&find_gem(&build_skill.name)?, &build_skill.kind, true, bad_skills)?;
        if filter.tag == FilterTag::Brick {
            filter.disabled = false;
        }
        not_group.stats.push(filter);
    }

    if not_group.stats.iter().any(|s| !s.disabled) {
        not_group.meta.disabled = false;
    }
    out.push(FilterOrGroup::Group(not_group));

    Ok(out)
}

/// `skillToFilter`
fn skill_to_filter(stat: &Stat, kind: &str, disabled: bool, bad_skills: Option<&[&str]>) -> R<StatFilter> {
    let tag = match kind {
        "primary" => FilterTag::MercenaryPrimary,
        "secondary" => FilterTag::MercenarySecondary,
        _ => FilterTag::MercenaryUtility,
    };
    let mut filter = StatFilter {
        trade_id: stat.trade.ids.get("pseudo").cloned(),
        stat_ref: stat.stat_ref.clone(),
        text: stat.matchers.first().map(|m| m.string.clone()).unwrap_or_default(),
        tag,
        oils: None,
        mercenary: Some(MercenaryFilterInfo { icon: mercenary(stat)?.icon.clone(), tier: None, max_tier: None }),
        reminder_text: None,
        sources: Vec::new(),
        not: None,
        roll: None,
        option: None,
        hidden: None,
        disabled,
    };

    if bad_skills.is_some_and(|b| b.contains(&stat.stat_ref.as_str())) {
        filter.tag = FilterTag::Brick;
    }
    Ok(filter)
}

/// Every ref this module passes to `stat()`.
pub fn stat_refs() -> Vec<&'static str> {
    BUILD_RULES.iter().flat_map(|r| r.bad_skills.iter().copied()).collect()
}
