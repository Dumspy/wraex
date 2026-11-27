//! `FilterGroup.vue` and `FilterModifierLinks.vue`: a group's own row plus, while expanded,
//! its child rows.

use rune::{El, col, el, row};
use wraex_poe1::filters::{FilterGroup, GroupKind, StatFilter};
use wraex_poe1::parser::item::Item;

use crate::poe1::price_check::view::Project;
use crate::poe1::price_check::{PriceCheck, StatPath};
use crate::theme;

use super::modifier;

/// `FilterGroup.vue`.
pub fn view<S: 'static>(check: &PriceCheck, group: &FilterGroup, item: &Item, index: usize, project: Project<S>) -> El {
    let links = (group.group == GroupKind::Mercenary).then(|| links(group));
    let mut out = col().key(format!("group-{index}")).w_full();
    out = out.child(modifier::view(
        check,
        &group.meta,
        item,
        false,
        Some(group.expanded),
        StatPath::Meta(index),
        links,
        project,
    ));
    if group.expanded {
        for (child, filter) in group.stats.iter().enumerate() {
            out = out.child(modifier::view(
                check,
                filter,
                item,
                true,
                None,
                StatPath::Child(index, child),
                None,
                project,
            ));
        }
    }
    out
}

/// `FilterModifierLinks.vue`: the skill gem's diamond and the support gems' dots, joined by a
/// line. Checked ones take an outline, and the gem's diamond turns yellow at six links.
fn links(group: &FilterGroup) -> El {
    let supports: Vec<&StatFilter> = group.stats.iter().filter(|stat| stat.mercenary.is_some()).collect();
    let meta_checked = !group.meta.disabled;
    let six_link = group.stats.iter().any(|stat| {
        !stat.disabled
            && ((stat.not == Some(true) && supports.len() == 5)
                || stat.first_trade_id() == Some("item.mercenary_6link"))
    });
    let skill_color = if supports.len() == 5 { theme::yellow(500) } else { theme::gray(600) };
    let mut out = row().key("group-links").items_center().gap(theme::rem(0.625)).child(diamond(
        skill_color,
        (supports.len() == 5).then_some("6"),
        meta_checked && six_link,
    ));
    for (index, gem) in supports.into_iter().enumerate() {
        let color = match gem.mercenary.as_ref().and_then(|mercenary| mercenary.tier) {
            Some(3) => theme::yellow(500),
            Some(4) => theme::yellow(700),
            _ => theme::gray(700),
        };
        out = out.child(dot(index, color, meta_checked && !gem.disabled));
    }
    out
}

/// The rotated square (`w-2.5 h-2.5`, `rotate-45`).
fn diamond(color: rune::Color, label: Option<&str>, checked: bool) -> El {
    let shape = rune::Shape::new().fill(vec![[5.0, 0.0], [10.0, 5.0], [5.0, 10.0], [0.0, 5.0]], color);
    let mut wrapper = el().key("link-skill").size(14.0, 14.0).center();
    if checked {
        let outline = rune::Shape::new().stroke(vec![[7.0, 0.0], [14.0, 7.0], [7.0, 14.0], [0.0, 7.0]], 1.0, color);
        wrapper = wrapper.child(rune::canvas(outline).size(14.0, 14.0));
    }
    let mut inner = row().size(14.0, 14.0).center().child(rune::canvas(shape).size(10.0, 10.0));
    if let Some(label) = label {
        inner = inner.child(rune::text(label).key("link-six").font(crate::theme::leading_none(crate::theme::text::XS)));
    }
    wrapper.child(inner)
}

/// A support gem's filled circle (`rounded-full`).
fn dot(index: usize, color: rune::Color, checked: bool) -> El {
    let mut wrapper = el().key(format!("link-support-{index}")).size(14.0, 14.0).center();
    let mut circle = el().size(10.0, 10.0).radius(5.0).fill(color);
    if checked {
        circle = circle.border(1.0, color).border_layout();
    }
    wrapper = wrapper.child(circle);
    wrapper
}
