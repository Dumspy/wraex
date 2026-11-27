//! JavaScript regular expressions on top of `fancy-regex`.
//!
//! The references' regexes (in `client_strings.js` and in code) are written for
//! JavaScript, whose syntax and semantics differ from Rust's in small ways that
//! change results: `\d`, `\w` and `\s` are ASCII-only or JavaScript's own
//! whitespace set, `.` excludes `\r` and the Unicode line separators, `\uXXXX`
//! escapes, identity escapes like `\/`. [`Regex::new`] translates a JavaScript
//! source into the equivalent `fancy-regex` pattern; anything it cannot translate
//! faithfully is an error, not a guess.
//!
//! Not modelled: the `u`, `y`, `d` and `v` flags (none are used), the `g` flag's
//! `lastIndex` state (callers pick first match or all matches explicitly), and
//! matching on UTF-16 units instead of code points (only differs for characters
//! outside the BMP).

use std::fmt;

/// What `\s` matches in JavaScript (`WhiteSpace` and `LineTerminator`).
const SPACE_CLASS: &str =
    r"\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";
const WORD_CLASS: &str = "A-Za-z0-9_";
const DOT: &str = r"[^\n\r\x{2028}\x{2029}]";

#[derive(Clone)]
pub struct Regex {
    source: String,
    flags: String,
    inner: fancy_regex::Regex,
}

impl fmt::Debug for Regex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "/{}/{}", self.source, self.flags)
    }
}

impl PartialEq for Regex {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source && self.flags == other.flags
    }
}

impl Eq for Regex {}

/// One match: `match[i]` and `match.groups[name]`, `None` for groups that did not
/// participate (`undefined`).
#[derive(Debug)]
pub struct Captures<'t> {
    inner: fancy_regex::Captures<'t, str>,
}

impl<'t> Captures<'t> {
    /// `match[i]`; `0` is the whole match.
    pub fn get(&self, i: usize) -> Option<&'t str> {
        self.inner.get(i).map(|m| m.as_str())
    }

    /// `match.groups[name]`
    pub fn name(&self, name: &str) -> Option<&'t str> {
        self.inner.name(name).map(|m| m.as_str())
    }

    /// `match[0]`
    pub fn whole(&self) -> &'t str {
        self.matched().as_str()
    }

    /// Group 0 is the whole match, so it always participates.
    #[expect(clippy::expect_used, reason = "group 0 always participates in a match")]
    fn matched(&self) -> fancy_regex::Match<'t> {
        self.inner.get(0).expect("group 0 always matches")
    }
}

/// `fancy-regex`'s only runtime error is its backtrack limit, which the reference's
/// engine has no equivalent of: it stays a hard failure rather than silently
/// changing a match.
#[expect(clippy::expect_used, reason = "a backtrack-limit error must fail loudly, as before")]
fn limit<T>(result: Result<T, fancy_regex::Error>) -> T {
    result.expect("regex backtrack limit")
}

impl Regex {
    /// Compiles JavaScript regex `source` with `flags` (`i`, `m`, `s`, `g`).
    pub fn new(source: &str, flags: &str) -> Result<Self, String> {
        let mut prefix = String::new();
        let mut dot_all = false;
        for flag in flags.chars() {
            match flag {
                'i' => prefix.push_str("(?i)"),
                'm' => prefix.push_str("(?m)"),
                's' => dot_all = true,
                'g' => {}
                other => return Err(format!("unsupported regex flag `{other}`")),
            }
        }
        let pattern = format!("{prefix}{}", translate(source, dot_all)?);
        let inner = fancy_regex::Regex::new(&pattern).map_err(|e| format!("/{source}/{flags}: {e}"))?;
        Ok(Self { source: source.to_owned(), flags: flags.to_owned(), inner })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn flags(&self) -> &str {
        &self.flags
    }

    /// `re.test(text)` (for a regex without the `g` flag).
    pub fn is_match(&self, text: &str) -> bool {
        limit(self.inner.is_match(text))
    }

    /// `re.exec(text)` / `text.match(re)` (without the `g` flag).
    pub fn captures<'t>(&self, text: &'t str) -> Option<Captures<'t>> {
        limit(self.inner.captures(text)).map(|inner| Captures { inner })
    }

    /// `text.replace(re, fn)` with the `g` flag (`replaceAll`): every
    /// non-overlapping match replaced by `replace(&match)`.
    pub fn replace_all(&self, text: &str, mut replace: impl FnMut(&Captures<'_>) -> String) -> String {
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for caps in self.inner.captures_iter(text) {
            let caps = Captures { inner: limit(caps) };
            let m = caps.matched();
            // Both offsets are match boundaries.
            out.push_str(text.get(last..m.start()).unwrap_or_default());
            out.push_str(&replace(&caps));
            last = m.end();
        }
        out.push_str(text.get(last..).unwrap_or_default());
        out
    }

    /// `text.replace(re, fn)` without the `g` flag: the first match only.
    pub fn replace_first(&self, text: &str, replace: impl FnOnce(&Captures<'_>) -> String) -> String {
        self.captures(text).map_or_else(
            || text.to_owned(),
            |caps| {
                let m = caps.matched();
                // Both offsets are match boundaries.
                format!(
                    "{}{}{}",
                    text.get(..m.start()).unwrap_or_default(),
                    replace(&caps),
                    text.get(m.end()..).unwrap_or_default()
                )
            },
        )
    }
}

/// Characters `regex-syntax` treats as meta outside a class; JavaScript identity
/// escapes of anything else become plain literals.
fn is_meta(c: char) -> bool {
    r"\.+*?()|[]{}^$#&-~".contains(c)
}

fn hex_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, len: usize) -> Option<u32> {
    let digits: String = (0..len).map_while(|_| chars.next_if(char::is_ascii_hexdigit)).collect();
    if digits.len() != len {
        return None;
    }
    u32::from_str_radix(&digits, 16).ok()
}

/// Appends `args`; a `String` never refuses them.
fn push_fmt(out: &mut String, args: fmt::Arguments<'_>) {
    fmt::Write::write_fmt(out, args).unwrap_or_default();
}

/// Pushes `[class]`, or `[^class]` when `negated`.
fn push_class(out: &mut String, class: &str, negated: bool) {
    out.push('[');
    if negated {
        out.push('^');
    }
    out.push_str(class);
    out.push(']');
}

/// Pushes `\x{...}` with `code` in upper-case hex, `regex-syntax`'s notation.
fn push_hex_escape(out: &mut String, code: u32) {
    out.push_str(r"\x{");
    push_fmt(out, format_args!("{code:X}"));
    out.push('}');
}

/// Expands the escape `\e` (`in_class` when a `[...]` body is being written).
fn push_escape(
    e: char,
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    in_class: bool,
    out: &mut String,
) -> Result<(), String> {
    match e {
        'd' if in_class => out.push_str("0-9"),
        'd' => out.push_str("[0-9]"),
        'w' if in_class => out.push_str(WORD_CLASS),
        'w' => push_class(out, WORD_CLASS, false),
        's' if in_class => out.push_str(SPACE_CLASS),
        's' => push_class(out, SPACE_CLASS, false),
        'D' => out.push_str("[^0-9]"),
        'W' => push_class(out, WORD_CLASS, true),
        'S' => push_class(out, SPACE_CLASS, true),
        'b' | 'B' if in_class => return Err(format!("`\\{e}` in a class")),
        // Unicode word boundaries in Rust; identical for the texts the
        // references match (no non-ASCII word characters next to them).
        'b' | 'B' => {
            out.push('\\');
            out.push(e);
        }
        'n' | 'r' | 't' | 'f' | 'v' => {
            let code = match e {
                'n' => 0x0a,
                'r' => 0x0d,
                't' => 0x09,
                'f' => 0x0c,
                _ => 0x0b,
            };
            push_hex_escape(out, code);
        }
        '0' if !chars.peek().is_some_and(char::is_ascii_digit) => out.push_str(r"\x{0}"),
        'x' => {
            let code = hex_escape(chars, 2).ok_or("bad \\x escape")?;
            push_hex_escape(out, code);
        }
        'u' => {
            let code = hex_escape(chars, 4).ok_or("bad \\u escape")?;
            push_hex_escape(out, code);
        }
        '1'..='9' if !in_class => {
            out.push('\\');
            out.push(e);
            while let Some(d) = chars.next_if(char::is_ascii_digit) {
                out.push(d);
            }
        }
        'k' | 'c' | 'p' | 'P' => return Err(format!("unsupported escape `\\{e}`")),
        c if c.is_ascii_alphanumeric() => return Err(format!("unsupported escape `\\{c}`")),
        c if is_meta(c) => {
            out.push('\\');
            out.push(c);
        }
        c => out.push(c),
    }
    Ok(())
}

fn translate(source: &str, dot_all: bool) -> Result<String, String> {
    let mut out = String::with_capacity(source.len().saturating_mul(2));
    let mut chars = source.chars().peekable();
    let mut in_class = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let e = chars.next().ok_or("trailing backslash")?;
                push_escape(e, &mut chars, in_class, &mut out)?;
            }
            '[' if in_class => out.push_str(r"\["),
            '[' => {
                let negated = chars.next_if_eq(&'^').is_some();
                // `[]` matches nothing and `[^]` anything in JavaScript.
                if chars.next_if_eq(&']').is_some() {
                    out.push_str(if negated { r"[\x{0}-\x{10FFFF}]" } else { r"[^\x{0}-\x{10FFFF}]" });
                    continue;
                }
                in_class = true;
                out.push_str(if negated { "[^" } else { "[" });
            }
            ']' if in_class => {
                in_class = false;
                out.push(']');
            }
            // Set operators in Rust classes, literals in JavaScript.
            '&' | '~' if in_class => {
                out.push('\\');
                out.push(c);
            }
            '.' if !in_class => out.push_str(if dot_all { r"[\x{0}-\x{10FFFF}]" } else { DOT }),
            '(' if !in_class && chars.peek() == Some(&'?') => {
                out.push('(');
                // The `?` that the guard peeked.
                if let Some(question) = chars.next() {
                    out.push(question);
                }
                // `(?<name>` (not `(?<=` / `(?<!`) becomes `(?P<name>`.
                if chars.peek() == Some(&'<') {
                    let mut ahead = chars.clone();
                    ahead.next();
                    if !matches!(ahead.peek(), Some('=' | '!')) {
                        out.push('P');
                    }
                }
            }
            '{' if !in_class && is_quantifier(chars.clone()) => {
                out.push('{');
                for q in chars.by_ref() {
                    out.push(q);
                    if q == '}' {
                        break;
                    }
                }
            }
            // A lone `]`, or a `{` that does not start a quantifier, is a literal
            // in JavaScript.
            ']' | '{' | '}' if !in_class => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    if in_class {
        return Err("unterminated character class".into());
    }
    Ok(out)
}

/// Whether the text after a `{` is `n}`, `n,}` or `n,m}`.
fn is_quantifier(mut rest: std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    let mut digits = 0_usize;
    while rest.next_if(char::is_ascii_digit).is_some() {
        digits = digits.saturating_add(1);
    }
    if digits == 0 {
        return false;
    }
    if rest.next_if_eq(&',').is_some() {
        while rest.next_if(char::is_ascii_digit).is_some() {}
    }
    rest.next() == Some('}')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn re(source: &str) -> Regex {
        Regex::new(source, "").unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn classes_follow_javascript() {
        // `\d` is ASCII-only: Arabic-Indic digits are not digits.
        assert!(!re(r"^\d$").is_match("٣"));
        assert!(re(r"^\d$").is_match("3"));
        assert!(re(r"^[^\d/]$").is_match("a"));
        assert!(!re(r"^[^\d/]$").is_match("/"));
        assert!(re(r"^\s$").is_match("\u{feff}"));
        assert!(!re(r"^\s$").is_match("\u{85}"));
        assert!(!re(r"^.$").is_match("\r"));
        assert!(Regex::new(r"^.$", "s").unwrap().is_match("\r"));
        assert!(re(r"^[^ -]$").is_match("W"));
        assert!(!re(r"^[^ -]$").is_match("-"));
        assert!(!re(r"[]").is_match("a"));
        assert!(re(r"^[^]$").is_match("\n"));
        assert!(re(r"^[a&&b]+$").is_match("a&b"));
    }

    #[test]
    fn escapes_follow_javascript() {
        assert!(re(r"x *$").is_match("x\u{a0}\u{a0}"));
        assert!(re(r"^a\/b$").is_match("a/b"));
        assert!(re(r"^\(Tier (\d+)\)$").is_match("(Tier 16)"));
        assert!(re(r"^a{2}$").is_match("aa"));
        assert!(re(r"^a{,2}$").is_match("a{,2}"));
        assert!(re(r"^}$").is_match("}"));
        assert!(re(r"^\x41$").is_match("A"));
        assert!(Regex::new(r"\p{L}", "").is_err());
        assert!(Regex::new(r"\q", "").is_err());
        assert!(Regex::new("a", "y").is_err());
    }

    #[test]
    fn groups_and_lookaround() {
        let line = re(r#"^(?<type>[^"(]+)(?:\s+"(?<name>[^"]*)")?(?:\s+\(Tier: (?<tier>\d+)\))?$"#);
        let caps = line.captures(r#"Prefix Modifier "Abbot's" (Tier: 1)"#).unwrap();
        assert_eq!(caps.name("type"), Some("Prefix Modifier"));
        assert_eq!(caps.name("name"), Some("Abbot's"));
        assert_eq!(caps.name("tier"), Some("1"));
        let caps = line.captures("Implicit Modifier").unwrap();
        assert_eq!(caps.name("name"), None);

        let value = re(r"(?<value>(?<!\d|\))[+-]?\d+(?:\.\d+)?)(?:\((?<min>.[^)-]*)(?:-(?<max>[^)]+))?\))?");
        let out = value
            .replace_all("+17(16-25) to 1+2", |c| format!("<{}|{:?}|{:?}>", c.whole(), c.name("min"), c.name("max")));
        assert_eq!(out, r#"<+17(16-25)|Some("16")|Some("25")> to <1|None|None>+<2|None|None>"#);
        assert!(re(r"(?<=a)b").is_match("ab"));
        assert_eq!(re("b").replace_first("abab", |_| "X".into()), "aXab");
    }

    #[test]
    fn case_insensitive_and_multiline_flags() {
        assert!(Regex::new("^abc$", "i").unwrap().is_match("ABC"));
        assert!(Regex::new("^b$", "m").unwrap().is_match("a\nb"));
        assert!(!re("^b$").is_match("a\nb"));
    }
}
