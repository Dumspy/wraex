//! Item-text helpers that are the same in both references' `renderer/src/parser`.
//!
//! PLAN.md, phase 3: byte-identical helpers live in core. If one reference changes
//! one of these, the copy moves into that game's crate.

use std::sync::OnceLock;

use crate::js;
use crate::js_regex::Regex;

/// `itemTextToSections` (`Parser.ts`): lines split on `--------`, empty
/// sections dropped.
pub fn item_text_to_sections(text: &str) -> Vec<Vec<String>> {
    let mut lines: Vec<&str> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let mut sections: Vec<Vec<String>> = vec![Vec::new()];
    for line in lines {
        if line == "--------" {
            sections.push(Vec::new());
        } else if let Some(section) = sections.last_mut() {
            section.push(line.to_owned());
        }
    }
    sections.retain(|s| !s.is_empty());
    sections
}

#[expect(clippy::expect_used, reason = "a literal pattern, compiled by the tests below")]
fn set_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new("<<set:.+?>>", "g").expect("valid pattern"))
}

#[expect(clippy::expect_used, reason = "a literal pattern, compiled by the tests below")]
fn condition_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new("<(if:.+?|elif:.+?|else)>{(.+?)}", "g").expect("valid pattern"))
}

/// `markupConditionParser` (`Parser.ts`): ignores `<<set:__>>` state and always
/// takes the first `<if:__>{...}` branch.
pub fn markup_condition_parser(text: &str) -> String {
    let text = set_regex().replace_all(text, |_| String::new());
    condition_regex().replace_all(&text, |caps| {
        if caps.get(1).is_some_and(|t| t.starts_with("if:")) {
            caps.get(2).unwrap_or("").to_owned()
        } else {
            String::new()
        }
    })
}

/// `isModInfoLine` (`advanced-mod-desc.ts`)
pub fn is_mod_info_line(line: &str) -> bool {
    line.starts_with('{') && line.ends_with('}')
}

/// `groupLinesByMod` (`advanced-mod-desc.ts`): each mod info line with the stat
/// lines after it. Nothing unless the first line is a mod info line.
pub fn group_lines_by_mod(lines: &[String]) -> Vec<(String, Vec<String>)> {
    if lines.first().is_none_or(|l| !is_mod_info_line(l)) {
        return Vec::new();
    }
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for line in lines {
        if is_mod_info_line(line) {
            out.push((line.clone(), Vec::new()));
        } else if let Some(last) = out.last_mut() {
            last.1.push(line.clone());
        }
    }
    out
}

/// `incrRoll` (`advanced-mod-desc.ts`)
pub fn incr_roll(value: f64, p: f64, dp: i32) -> f64 {
    let res = value + (value * p / 100.0);
    let rounding = 10f64.powi(dp);
    ((res + f64::EPSILON) * rounding).trunc() / rounding
}

/// A candidate stat text (`StatString` in `stat-translations.ts`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatString {
    pub string: String,
    pub unscalable: bool,
}

/// `LOCALIZED_PAREN_LEFT`: `/^\s*(?:\(|（)/`
fn paren_left(line: &str) -> bool {
    let t = js::trim_start(line);
    t.starts_with('(') || t.starts_with('（')
}

/// `LOCALIZED_PAREN_RIGHT`: `/(?:\)|）)\s*$/`
fn paren_right(line: &str) -> bool {
    let t = js::trim_end(line);
    t.ends_with(')') || t.ends_with('）')
}

/// `linesToStatStrings` (`stat-translations.ts`).
///
/// The reference is a generator the caller answers with whether each candidate
/// parsed; here `try_parse` gives that answer. Reminder text in parentheses is
/// skipped. Returns the lines no candidate covered, without empty ones.
pub fn lines_to_stat_strings<E>(
    lines: &[String],
    unscalable_value: &str,
    mut try_parse: impl FnMut(&StatString) -> Result<bool, E>,
) -> Result<Vec<String>, E> {
    let mut not_parsed = Vec::new();
    let mut reminder_string = false;
    let mut start = 0;
    while let Some(line) = lines.get(start) {
        if paren_left(line) {
            reminder_string = true;
        }
        if reminder_string && paren_right(line) {
            reminder_string = false;
            start = start.saturating_add(1);
            continue;
        }
        if reminder_string {
            start = start.saturating_add(1);
            continue;
        }
        let mut parsed_until = None;
        for end in start..lines.len() {
            let mut string = lines.get(start..=end).unwrap_or_default().join("\n");
            let unscalable = string.ends_with(unscalable_value);
            if unscalable {
                string.truncate(string.len().saturating_sub(unscalable_value.len()));
            }
            if try_parse(&StatString { string, unscalable })? {
                parsed_until = Some(end);
                break;
            }
        }
        if let Some(end) = parsed_until {
            start = end.saturating_add(1);
            continue;
        }
        not_parsed.push(line.clone());
        start = start.saturating_add(1);
    }
    Ok(not_parsed.into_iter().filter(|l| !l.is_empty()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_sections() {
        let sections = item_text_to_sections("a\r\nb\n--------\n--------\nc\n");
        assert_eq!(sections, vec![vec!["a".to_owned(), "b".to_owned()], vec!["c".to_owned()]]);
        assert_eq!(item_text_to_sections("").len(), 0);
    }

    #[test]
    fn markup_conditions_take_the_first_branch() {
        assert_eq!(
            markup_condition_parser("<<set:MS>><if:MS>{Mother}<elif:FS>{Father} of Spirits"),
            "Mother of Spirits"
        );
        assert_eq!(markup_condition_parser("Plain"), "Plain");
    }

    #[test]
    fn groups_lines_by_mod() {
        let lines: Vec<String> = ["{ a }", "x", "y", "{ b }"].map(String::from).to_vec();
        assert_eq!(
            group_lines_by_mod(&lines),
            vec![("{ a }".into(), vec!["x".into(), "y".into()]), ("{ b }".into(), vec![])]
        );
        assert_eq!(group_lines_by_mod(&lines[1..]).len(), 0);
    }

    #[test]
    fn incr_roll_truncates() {
        assert_eq!(incr_roll(10.0, 20.0, 0), 12.0);
        assert_eq!(incr_roll(1.55, 10.0, 2), 1.7);
        assert_eq!(incr_roll(7.0, 15.0, 0), 8.0);
    }

    fn stat_strings(lines: &[&str], parses: &[&str]) -> (Vec<String>, Vec<String>) {
        let lines: Vec<String> = lines.iter().map(std::string::ToString::to_string).collect();
        let mut tried = Vec::new();
        let rest = lines_to_stat_strings::<()>(&lines, " — Unscalable Value", |s| {
            tried.push(s.string.clone());
            Ok(parses.contains(&s.string.as_str()))
        })
        .unwrap();
        (tried, rest)
    }

    #[test]
    fn groups_lines_into_candidates_and_skips_reminders() {
        let (tried, rest) = stat_strings(&["a", "b", "(reminder", "text)", "c"], &["a\nb"]);
        assert_eq!(tried, ["a", "a\nb", "c"]);
        assert_eq!(rest, ["c"]);
        let (_, rest) = stat_strings(&["x — Unscalable Value", ""], &["x"]);
        assert_eq!(rest.len(), 0);
    }
}
