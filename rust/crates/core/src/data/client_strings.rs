//! Reads `client_strings.js`, the references' per-language `TranslationDict`.
//!
//! The files are JavaScript modules, but every one of them is a single
//! `export default { ... }` object literal with one `KEY: literal,` per line, where
//! the literal is a string, a regular expression or an array of strings. This
//! reads exactly that and rejects anything else, so a format change upstream fails
//! loudly instead of being half-read.

use indexmap::IndexMap;

use super::DataError;
use crate::js_regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientString {
    Text(String),
    /// Compiled with JavaScript semantics ([`crate::js_regex`]).
    Regex(Regex),
    List(Vec<String>),
}

/// One language's `client_strings.js`, in file order.
#[derive(Debug, Clone, Default)]
pub struct ClientStrings {
    entries: IndexMap<String, ClientString>,
}

impl ClientStrings {
    /// `path` is only used in errors.
    pub fn parse(path: &str, source: &str) -> Result<Self, DataError> {
        let mut entries = IndexMap::new();
        let mut opened = false;
        let mut closed = false;
        for (n, raw) in source.lines().enumerate() {
            let err = |message: String| DataError::Syntax { path: path.to_owned(), line: n + 1, message };
            let line = raw.trim();
            if line.is_empty() || line.starts_with("//") || (line.starts_with("/**") && line.ends_with("*/")) {
                continue;
            }
            if closed {
                return Err(err(format!("unexpected `{line}` after the object")));
            }
            if line == "export default {" {
                if opened {
                    return Err(err("second `export default`".into()));
                }
                opened = true;
                continue;
            }
            if !opened {
                return Err(err(format!("expected `export default {{`, found `{line}`")));
            }
            if line == "}" || line == "};" {
                closed = true;
                continue;
            }
            let (key, rest) =
                line.split_once(':').ok_or_else(|| err(format!("expected `KEY: value`, found `{line}`")))?;
            if key.is_empty() || !key.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_') {
                return Err(err(format!("bad key `{key}`")));
            }
            let mut cursor = Cursor { rest: rest.trim_start() };
            let value = cursor.value().map_err(err)?;
            let tail = cursor.rest.trim_start();
            let tail = tail.strip_prefix(',').unwrap_or(tail).trim_start();
            if !tail.is_empty() && !tail.starts_with("//") {
                return Err(err(format!("unexpected `{tail}` after the value")));
            }
            if entries.insert(key.to_owned(), value).is_some() {
                return Err(err(format!("duplicate key `{key}`")));
            }
        }
        if !closed {
            return Err(DataError::Syntax {
                path: path.to_owned(),
                line: source.lines().count(),
                message: "unclosed object".into(),
            });
        }
        Ok(Self { entries })
    }

    pub fn get(&self, key: &str) -> Option<&ClientString> {
        self.entries.get(key)
    }

    pub fn text(&self, key: &str) -> Option<&str> {
        match self.entries.get(key)? {
            ClientString::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn regex(&self, key: &str) -> Option<&Regex> {
        match self.entries.get(key)? {
            ClientString::Regex(r) => Some(r),
            _ => None,
        }
    }

    pub fn list(&self, key: &str) -> Option<&[String]> {
        match self.entries.get(key)? {
            ClientString::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &ClientString)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

struct Cursor<'a> {
    rest: &'a str,
}

impl Cursor<'_> {
    fn bump(&mut self) -> Option<char> {
        let c = self.rest.chars().next()?;
        self.rest = &self.rest[c.len_utf8()..];
        Some(c)
    }

    fn value(&mut self) -> Result<ClientString, String> {
        match self.rest.chars().next() {
            Some('\'' | '"') => self.string().map(ClientString::Text),
            Some('/') => self.regex().map(ClientString::Regex),
            Some('[') => self.list().map(ClientString::List),
            _ => Err(format!("expected a string, regex or array, found `{}`", self.rest)),
        }
    }

    fn list(&mut self) -> Result<Vec<String>, String> {
        self.bump();
        let mut out = Vec::new();
        loop {
            self.rest = self.rest.trim_start();
            if let Some(rest) = self.rest.strip_prefix(']') {
                self.rest = rest;
                return Ok(out);
            }
            out.push(self.string()?);
            self.rest = self.rest.trim_start();
            if let Some(rest) = self.rest.strip_prefix(',') {
                self.rest = rest;
            } else if !self.rest.starts_with(']') {
                return Err(format!("expected `,` or `]`, found `{}`", self.rest));
            }
        }
    }

    /// A single- or double-quoted JavaScript string literal.
    fn string(&mut self) -> Result<String, String> {
        let quote = match self.bump() {
            Some(q @ ('\'' | '"')) => q,
            _ => return Err("expected a string".into()),
        };
        let mut out = String::new();
        loop {
            match self.bump().ok_or("unterminated string")? {
                c if c == quote => return Ok(out),
                '\\' => match self.bump().ok_or("unterminated escape")? {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'v' => out.push('\u{b}'),
                    '0' if !self.rest.starts_with(|c: char| c.is_ascii_digit()) => out.push('\0'),
                    'x' => out.push(self.hex(2)?),
                    'u' if self.rest.starts_with('{') => {
                        let end = self.rest.find('}').ok_or("unterminated \\u{")?;
                        let code = u32::from_str_radix(&self.rest[1..end], 16).map_err(|e| e.to_string())?;
                        out.push(char::from_u32(code).ok_or("bad code point")?);
                        self.rest = &self.rest[end + 1..];
                    }
                    'u' => {
                        let unit = self.hex_unit(4)?;
                        if (0xd800..0xdc00).contains(&unit) && self.rest.starts_with("\\u") {
                            self.rest = &self.rest[2..];
                            let low = self.hex_unit(4)?;
                            let code = 0x10000 + ((unit - 0xd800) << 10) + (low.wrapping_sub(0xdc00));
                            out.push(char::from_u32(code).ok_or("bad surrogate pair")?);
                        } else {
                            out.push(char::from_u32(unit).ok_or("lone surrogate")?);
                        }
                    }
                    c if c.is_ascii_digit() => return Err("octal escapes are not supported".into()),
                    // `\'`, `\"`, `\\` and identity escapes.
                    c => out.push(c),
                },
                '\n' => return Err("newline in string".into()),
                c => out.push(c),
            }
        }
    }

    fn hex_unit(&mut self, len: usize) -> Result<u32, String> {
        let digits = self.rest.get(..len).ok_or("short escape")?;
        let unit = u32::from_str_radix(digits, 16).map_err(|e| e.to_string())?;
        self.rest = &self.rest[len..];
        Ok(unit)
    }

    fn hex(&mut self, len: usize) -> Result<char, String> {
        let unit = self.hex_unit(len)?;
        char::from_u32(unit).ok_or_else(|| "bad escape".into())
    }

    /// A regex literal `/source/flags`: ends at the first `/` that is neither
    /// escaped nor inside a `[...]` class.
    fn regex(&mut self) -> Result<Regex, String> {
        self.bump();
        let mut source = String::new();
        let mut in_class = false;
        loop {
            let c = self.bump().ok_or("unterminated regex")?;
            match c {
                '\\' => {
                    source.push(c);
                    source.push(self.bump().ok_or("unterminated regex")?);
                }
                '[' => {
                    in_class = true;
                    source.push(c);
                }
                ']' => {
                    in_class = false;
                    source.push(c);
                }
                '/' if !in_class => break,
                c => source.push(c),
            }
        }
        if source.is_empty() {
            return Err("empty regex".into());
        }
        let flags_len = self.rest.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(self.rest.len());
        let flags = self.rest[..flags_len].to_owned();
        if let Some(bad) = flags.chars().find(|c| !"dgimsuvy".contains(*c)) {
            return Err(format!("unknown regex flag `{bad}`"));
        }
        self.rest = &self.rest[flags_len..];
        Regex::new(&source, &flags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"// @ts-check
/** @type{import('../../../src/assets/data/interfaces').TranslationDict} */
export default {
  // [Manual]
  RARITY_NORMAL: 'Normal',
  QUOTED: 'It\'s "fine" \\',
  DOUBLE: "a'b",
  MAP_TIER: / \(Tier (\d+)\)$/,
  CLASS: /^[/\]]+ x$/gi,
  SHAPER_MODS: ["of Shaping","The Shaper's"],
  EMPTY: [],
  LAST: 'x'
}
"#;

    #[test]
    fn reads_strings_regexes_and_lists() {
        let strings = ClientStrings::parse("en", SAMPLE).unwrap();
        assert_eq!(strings.len(), 8);
        assert_eq!(strings.text("RARITY_NORMAL"), Some("Normal"));
        assert_eq!(strings.text("QUOTED"), Some("It's \"fine\"\u{a0}\\"));
        assert_eq!(strings.text("DOUBLE"), Some("a'b"));
        let tier = strings.regex("MAP_TIER").unwrap();
        assert_eq!((tier.source(), tier.flags()), (r" \(Tier (\d+)\)$", ""));
        assert_eq!(tier.captures("Map (Tier 16)").unwrap().get(1), Some("16"));
        let class = strings.regex("CLASS").unwrap();
        assert_eq!((class.source(), class.flags()), (r"^[/\]]+ x$", "gi"));
        assert!(class.is_match("/] X"));
        assert_eq!(strings.list("SHAPER_MODS").unwrap(), ["of Shaping", "The Shaper's"]);
        assert_eq!(strings.list("EMPTY").unwrap().len(), 0);
        assert_eq!(strings.text("MAP_TIER"), None);
        assert_eq!(strings.iter().next().unwrap().0, "RARITY_NORMAL");
    }

    #[test]
    fn rejects_anything_else() {
        for bad in [
            "export default {\n  A: `template`,\n}\n",
            "export default {\n  A: 'x' + 'y',\n}\n",
            "export default {\n  A: 'x',\n  A: 'y',\n}\n",
            "export default {\n  a: 'x',\n}\n",
            "export default {\n  A: 'x',\n",
            "const x = 1\nexport default {\n}\n",
            "export default {\n  A: /x/z,\n}\n",
            "export default {\n  A: /\\p{L}/,\n}\n",
        ] {
            assert!(ClientStrings::parse("bad", bad).is_err(), "accepted {bad:?}");
        }
    }
}
