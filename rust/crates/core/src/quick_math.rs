//! The quick-math bar's expression evaluator (issue #6): a trader types
//! `1.5k / 3 + (20 - 2.5) * 4` and sees the result before copying it.
//!
//! Neither reference has this, so there is nothing to port; it is Wraex's own and
//! stays game-agnostic. The language is small on purpose: decimal numbers with an
//! optional unit suffix (`k` and `m` by default), `+ - * /` with the usual
//! precedence, unary signs and parentheses. [`Units`] is the hook for
//! currency-aware maths: issue #7 passes units that know `div` and `c` to
//! [`evaluate_with`], so this module never learns a game's currency names.
//!
//! A tokenizer with byte offsets feeds a recursive-descent parser that evaluates
//! while it parses. Nesting (parentheses and unary signs) is capped at
//! [`MAX_DEPTH`], so no input can exhaust the stack.

/// What a word after a number means: `k` → 1 000. Issue #7 will add currency
/// words (`div`, `c`) through this hook, so keep it a trait.
pub trait Units {
    /// The factor a number followed by `word` is multiplied by, or `None` for a
    /// word it does not know. `word` arrives as typed; the implementation
    /// decides whether it is case-insensitive.
    fn factor(&self, word: &str) -> Option<f64>;
}

/// The default units: `k` and `m`, case-insensitive.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Suffixes;

impl Units for Suffixes {
    fn factor(&self, word: &str) -> Option<f64> {
        if word.eq_ignore_ascii_case("k") {
            Some(1_000.0)
        } else if word.eq_ignore_ascii_case("m") {
            Some(1_000_000.0)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    Empty,
    /// A character that starts no token, at byte offset `at`.
    UnexpectedChar {
        at: usize,
        found: char,
    },
    /// A token where it cannot go (`5 * * 2`, `)` without `(`), at byte offset `at`.
    UnexpectedToken {
        at: usize,
    },
    /// The input ended where a number or `(` was needed (`5 +`, `(`).
    UnexpectedEnd,
    /// A `(` that is never closed; `at` is the `(`'s byte offset.
    UnclosedParen {
        at: usize,
    },
    UnknownUnit {
        at: usize,
        word: String,
    },
    /// A malformed number such as `1.2.3`.
    BadNumber {
        at: usize,
    },
    DivisionByZero,
    Overflow,
    /// More than `MAX_DEPTH` nested parentheses / unary operators.
    TooDeep,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Empty => f.write_str("empty expression"),
            Error::UnexpectedChar { found, .. } => write!(f, "unexpected character {found:?}"),
            Error::UnexpectedToken { .. } => f.write_str("unexpected token"),
            Error::UnexpectedEnd => f.write_str("unexpected end of expression"),
            Error::UnclosedParen { .. } => f.write_str("missing \")\""),
            Error::UnknownUnit { word, .. } => write!(f, "unknown unit {word:?}"),
            Error::BadNumber { .. } => f.write_str("bad number"),
            Error::DivisionByZero => f.write_str("division by zero"),
            Error::Overflow => f.write_str("number out of range"),
            Error::TooDeep => f.write_str("expression nested too deeply"),
        }
    }
}

impl std::error::Error for Error {}

/// How many levels of parentheses and unary signs may nest; one more is
/// [`Error::TooDeep`].
pub const MAX_DEPTH: usize = 64;

/// The most decimals [`format_plain`] and [`format_grouped`] keep.
pub const MAX_DECIMALS: usize = 4;

/// `input` with the default units.
pub fn evaluate(input: &str) -> Result<f64, Error> {
    evaluate_with(input, &Suffixes)
}

/// `input` with the given units.
pub fn evaluate_with(input: &str, units: &dyn Units) -> Result<f64, Error> {
    let tokens = tokenize(input)?;
    if tokens.is_empty() {
        return Err(Error::Empty);
    }
    let mut parser = Parser { tokens: &tokens, pos: 0, units };
    let value = parser.expression(0)?;
    match parser.peek_at() {
        Some(at) => Err(Error::UnexpectedToken { at }),
        None => Ok(value),
    }
}

/// How the result is copied to the clipboard: plain ASCII, no grouping, at most
/// [`MAX_DECIMALS`] decimals, rounded half away from zero, trailing zeros and a
/// trailing `.` dropped, `-0` → `0`.
pub fn format_plain(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() { "-inf".to_owned() } else { "inf".to_owned() };
    }
    let (int_part, frac_part) = decimals(value.abs());
    let zero = int_part == "0" && frac_part.is_empty();
    let mut text = String::new();
    if value.is_sign_negative() && !zero {
        text.push('-');
    }
    text.push_str(&int_part);
    if !frac_part.is_empty() {
        text.push('.');
        text.push_str(&frac_part);
    }
    text
}

/// How the bar shows the result: like [`format_plain`], but the integer part is
/// grouped with `,` every three digits.
pub fn format_grouped(value: f64) -> String {
    let plain = format_plain(value);
    let (sign, rest) = match plain.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", plain.as_str()),
    };
    let (int_part, frac_part) = match rest.split_once('.') {
        Some((int, frac)) => (int, Some(frac)),
        None => (rest, None),
    };
    let mut text = String::from(sign);
    let length = int_part.chars().count();
    for (index, digit) in int_part.chars().enumerate() {
        if index > 0 && (length - index) % 3 == 0 {
            text.push(',');
        }
        text.push(digit);
    }
    if let Some(frac) = frac_part {
        text.push('.');
        text.push_str(frac);
    }
    text
}

/// `magnitude`'s digits, rounded half away from zero to [`MAX_DECIMALS`]: the
/// integer part and the fraction without trailing zeros.
fn decimals(magnitude: f64) -> (String, String) {
    // Rust's `Display` prints the shortest digits that round-trip and never uses
    // an exponent, so 1e300 arrives as 301 characters rather than "1e300".
    let text = magnitude.to_string();
    let (int_part, frac_part) = match text.split_once('.') {
        Some((int, frac)) => (int, frac),
        None => (text.as_str(), ""),
    };
    let mut int_len = int_part.len();
    let mut digits: Vec<u8> = int_part.bytes().chain(frac_part.bytes().take(MAX_DECIMALS)).collect();
    if frac_part.as_bytes().get(MAX_DECIMALS).is_some_and(|digit| *digit >= b'5') {
        // Half away from zero: the magnitude only ever grows.
        let mut index = digits.len();
        loop {
            if index == 0 {
                digits.insert(0, b'1');
                int_len += 1;
                break;
            }
            index -= 1;
            match digits.get_mut(index) {
                Some(digit) if *digit == b'9' => *digit = b'0',
                Some(digit) => {
                    *digit += 1;
                    break;
                }
                None => break,
            }
        }
    }
    let mut frac: String = digits.split_off(int_len).into_iter().map(char::from).collect();
    let int: String = digits.into_iter().map(char::from).collect();
    while frac.ends_with('0') {
        frac.pop();
    }
    (int, frac)
}

/// One token and the byte offset it starts at.
#[derive(Debug, Clone, PartialEq)]
struct Token {
    at: usize,
    kind: TokenKind,
}

#[derive(Debug, Clone, PartialEq)]
enum TokenKind {
    Number(f64),
    Word(String),
    Plus,
    Minus,
    Star,
    Slash,
    Open,
    Close,
}

/// `input` as tokens, with whitespace dropped.
fn tokenize(input: &str) -> Result<Vec<Token>, Error> {
    let mut tokens = Vec::new();
    let mut chars = input.char_indices().peekable();
    while let Some(&(at, c)) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        let kind = match c {
            '(' => TokenKind::Open,
            ')' => TokenKind::Close,
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            c if c.is_ascii_digit() || (c == '.' && is_leading_dot(&chars)) => {
                let number = lex_number(input, &mut chars, at)?;
                tokens.push(Token { at, kind: TokenKind::Number(number) });
                continue;
            }
            c if c.is_alphabetic() => {
                let word = lex_word(&mut chars);
                tokens.push(Token { at, kind: TokenKind::Word(word) });
                continue;
            }
            c => return Err(Error::UnexpectedChar { at, found: c }),
        };
        chars.next();
        tokens.push(Token { at, kind });
    }
    Ok(tokens)
}

/// `.5`: a dot starts a number only when a digit follows it. The cursor is on the
/// dot itself.
fn is_leading_dot(chars: &std::iter::Peekable<std::str::CharIndices<'_>>) -> bool {
    let mut lookahead = chars.clone();
    lookahead.next();
    lookahead.next().is_some_and(|(_, c)| c.is_ascii_digit())
}

/// A decimal number: digits with at most one `.`, either part optional.
fn lex_number(
    input: &str,
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    at: usize,
) -> Result<f64, Error> {
    let mut seen_dot = false;
    while let Some(&(_, c)) = chars.peek() {
        if c.is_ascii_digit() {
            chars.next();
        } else if c == '.' && !seen_dot {
            seen_dot = true;
            chars.next();
        } else {
            break;
        }
    }
    if chars.peek().is_some_and(|&(_, c)| c == '.') {
        return Err(Error::BadNumber { at });
    }
    let end = chars.peek().map_or(input.len(), |&(offset, _)| offset);
    let text = input.get(at..end).unwrap_or_default();
    let mut canonical = String::with_capacity(text.len() + 2);
    if text.starts_with('.') {
        canonical.push('0');
    }
    canonical.push_str(text);
    if text.ends_with('.') {
        canonical.push('0');
    }
    match canonical.parse::<f64>() {
        Ok(number) if number.is_finite() => Ok(number),
        Ok(_) => Err(Error::Overflow),
        Err(_) => Err(Error::BadNumber { at }),
    }
}

/// An alphabetic run: a unit word.
fn lex_word(chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>) -> String {
    let mut word = String::new();
    while let Some(&(_, c)) = chars.peek() {
        if !c.is_alphabetic() {
            break;
        }
        word.push(c);
        chars.next();
    }
    word
}

/// The binary operator a token is, for the two operand loops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    units: &'a dyn Units,
}

impl Parser<'_> {
    fn peek_kind(&self) -> Option<&TokenKind> {
        self.tokens.get(self.pos).map(|token| &token.kind)
    }

    fn peek_at(&self) -> Option<usize> {
        self.tokens.get(self.pos).map(|token| token.at)
    }

    /// The alphabetic word at the cursor and its offset, if the next token is one.
    fn peek_word(&self) -> Option<(usize, String)> {
        match self.tokens.get(self.pos) {
            Some(Token { at, kind: TokenKind::Word(word) }) => Some((*at, word.clone())),
            _ => None,
        }
    }

    /// Whether the unary sign at the cursor is a minus.
    fn peek_sign(&self) -> Option<bool> {
        match self.peek_kind()? {
            TokenKind::Minus => Some(true),
            TokenKind::Plus => Some(false),
            _ => None,
        }
    }

    fn peek_op(&self) -> Option<Op> {
        match self.peek_kind()? {
            TokenKind::Plus => Some(Op::Add),
            TokenKind::Minus => Some(Op::Sub),
            TokenKind::Star => Some(Op::Mul),
            TokenKind::Slash => Some(Op::Div),
            TokenKind::Number(_) | TokenKind::Word(_) | TokenKind::Open | TokenKind::Close => None,
        }
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    /// `term (('+' | '-') term)*`, so `+` and `-` associate to the left.
    fn expression(&mut self, depth: usize) -> Result<f64, Error> {
        let mut value = self.term(depth)?;
        while let Some(op @ (Op::Add | Op::Sub)) = self.peek_op() {
            self.advance();
            let rhs = self.term(depth)?;
            value = finite(if op == Op::Add { value + rhs } else { value - rhs })?;
        }
        Ok(value)
    }

    /// `unary (('*' | '/') unary)*`, so `*` and `/` bind tighter than `+` and `-`.
    fn term(&mut self, depth: usize) -> Result<f64, Error> {
        let mut value = self.unary(depth)?;
        while let Some(op @ (Op::Mul | Op::Div)) = self.peek_op() {
            self.advance();
            let rhs = self.unary(depth)?;
            if op == Op::Mul {
                value = finite(value * rhs)?;
            } else if rhs == 0.0 {
                return Err(Error::DivisionByZero);
            } else {
                value = finite(value / rhs)?;
            }
        }
        Ok(value)
    }

    /// Repeated `+` and `-`, binding tighter than `*` and `/`.
    fn unary(&mut self, depth: usize) -> Result<f64, Error> {
        match self.peek_sign() {
            Some(negative) => {
                if depth >= MAX_DEPTH {
                    return Err(Error::TooDeep);
                }
                self.advance();
                let value = self.unary(depth + 1)?;
                Ok(if negative { -value } else { value })
            }
            None => self.primary(depth),
        }
    }

    /// A number with an optional unit suffix, or a parenthesised expression.
    fn primary(&mut self, depth: usize) -> Result<f64, Error> {
        let Some(at) = self.peek_at() else {
            return Err(Error::UnexpectedEnd);
        };
        match self.peek_kind() {
            Some(&TokenKind::Number(number)) => {
                self.advance();
                let Some((word_at, word)) = self.peek_word() else {
                    return Ok(number);
                };
                self.advance();
                let Some(factor) = self.units.factor(&word) else {
                    return Err(Error::UnknownUnit { at: word_at, word });
                };
                finite(number * factor)
            }
            Some(&TokenKind::Open) => {
                if depth >= MAX_DEPTH {
                    return Err(Error::TooDeep);
                }
                self.advance();
                let value = self.expression(depth + 1)?;
                if matches!(self.peek_kind(), Some(&TokenKind::Close)) {
                    self.advance();
                    return Ok(value);
                }
                match self.peek_at() {
                    Some(next) => Err(Error::UnexpectedToken { at: next }),
                    None => Err(Error::UnclosedParen { at }),
                }
            }
            _ => Err(Error::UnexpectedToken { at }),
        }
    }
}

/// An arithmetic result, or [`Error::Overflow`] when it left f64's range.
fn finite(value: f64) -> Result<f64, Error> {
    if value.is_finite() { Ok(value) } else { Err(Error::Overflow) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `evaluate`, unwrapped for the cases that must succeed.
    fn value(input: &str) -> f64 {
        match evaluate(input) {
            Ok(value) => value,
            Err(error) => panic!("{input:?} failed: {error}"),
        }
    }

    /// A currency-aware `Units` for the hook test: issue #7's shape.
    struct Currency;

    impl Units for Currency {
        fn factor(&self, word: &str) -> Option<f64> {
            match word {
                "div" => Some(200.0),
                "c" => Some(1.0),
                _ => None,
            }
        }
    }

    #[test]
    fn precedence_and_associativity() {
        assert_eq!(value("1 + 2 * 3"), 7.0);
        assert_eq!(value("(1 + 2) * 3"), 9.0);
        assert_eq!(value("10 - 3 - 2"), 5.0);
        assert_eq!(value("8 / 4 / 2"), 1.0);
        assert_eq!(value("2 * 3 + 4 * 5"), 26.0);
        assert_eq!(value("100 / 10 * 2"), 20.0);
        assert_eq!(value("2 + 3 - 4 + 5"), 6.0);
        // The issue's own example: 500 + (17.5 × 4) = 570.
        assert_eq!(value("1.5k / 3 + (20 - 2.5) * 4"), 570.0);
    }

    #[test]
    fn unary_signs() {
        assert_eq!(value("-5"), -5.0);
        assert_eq!(value("--5"), 5.0);
        assert_eq!(value("-+-5"), 5.0);
        assert_eq!(value("+5"), 5.0);
        assert_eq!(value("-2 * 3"), -6.0);
        assert_eq!(value("2 * -3"), -6.0);
        assert_eq!(value("2 * +3"), 6.0);
        assert_eq!(value("-(2 + 3)"), -5.0);
        assert_eq!(value("3 - -2"), 5.0);
        assert_eq!(value("-1k"), -1000.0);
        assert_eq!(value("-1.5m"), -1_500_000.0);
    }

    #[test]
    fn parentheses() {
        assert_eq!(value("((2))"), 2.0);
        assert_eq!(value("(1 + (2 * (3 + 4)))"), 15.0);
        assert_eq!(evaluate("(1 + 2"), Err(Error::UnclosedParen { at: 0 }));
        assert_eq!(evaluate("1 + (2"), Err(Error::UnclosedParen { at: 4 }));
        assert_eq!(evaluate("1 + 2)"), Err(Error::UnexpectedToken { at: 5 }));
        assert_eq!(evaluate("(1 + 2))"), Err(Error::UnexpectedToken { at: 7 }));
        assert_eq!(evaluate("()"), Err(Error::UnexpectedToken { at: 1 }));
        assert_eq!(evaluate(")"), Err(Error::UnexpectedToken { at: 0 }));
    }

    #[test]
    fn decimals() {
        assert!((value("0.1 + 0.2") - 0.3).abs() < 1e-12);
        assert_eq!(value(".5 * 4"), 2.0);
        assert_eq!(value("5."), 5.0);
        assert_eq!(value("0."), 0.0);
        assert_eq!(value("007"), 7.0);
        assert_eq!(value("1.5k"), 1500.0);
        assert_eq!(evaluate("1.2.3"), Err(Error::BadNumber { at: 0 }));
        assert_eq!(evaluate("1..2"), Err(Error::BadNumber { at: 0 }));
        assert_eq!(evaluate("."), Err(Error::UnexpectedChar { at: 0, found: '.' }));
    }

    #[test]
    fn suffixes() {
        assert_eq!(value("1k"), 1000.0);
        assert_eq!(value("1K"), 1000.0);
        assert_eq!(value("2.5m"), 2_500_000.0);
        assert_eq!(value("2.5M"), 2_500_000.0);
        assert_eq!(value("1.5 k"), 1500.0);
        assert_eq!(value("1 k"), 1000.0);
        assert_eq!(value("3k / 2"), 1500.0);
        assert_eq!(value("0.5k + 0.5k"), 1000.0);
        assert_eq!(evaluate("1km"), Err(Error::UnknownUnit { at: 1, word: "km".to_owned() }));
        assert_eq!(evaluate("5q"), Err(Error::UnknownUnit { at: 1, word: "q".to_owned() }));
        assert_eq!(evaluate("2 x 3"), Err(Error::UnknownUnit { at: 2, word: "x".to_owned() }));
        assert_eq!(evaluate("k"), Err(Error::UnexpectedToken { at: 0 }));
        assert_eq!(evaluate("k5"), Err(Error::UnexpectedToken { at: 0 }));
        assert_eq!(evaluate("(k)"), Err(Error::UnexpectedToken { at: 1 }));
    }

    #[test]
    fn custom_units_replace_the_defaults() {
        let units = Currency;
        assert_eq!(evaluate_with("2div + 3c", &units), Ok(403.0));
        assert_eq!(evaluate_with("1div / 4", &units), Ok(50.0));
        // The default suffix is not known to a custom implementation.
        assert_eq!(evaluate_with("1k", &units), Err(Error::UnknownUnit { at: 1, word: "k".to_owned() }));
        // The hook sees the word as typed.
        assert_eq!(evaluate_with("2DIV", &units), Err(Error::UnknownUnit { at: 1, word: "DIV".to_owned() }));
    }

    #[test]
    fn default_units_are_the_suffixes() {
        assert_eq!(evaluate("2k + 1"), evaluate_with("2k + 1", &Suffixes));
        assert_eq!(evaluate("2k + 1"), Ok(2001.0));
    }

    #[test]
    fn malformed_input() {
        assert_eq!(evaluate(""), Err(Error::Empty));
        assert_eq!(evaluate("   "), Err(Error::Empty));
        assert_eq!(evaluate("\n\t "), Err(Error::Empty));
        assert_eq!(evaluate("5 +"), Err(Error::UnexpectedEnd));
        assert_eq!(evaluate("5 /"), Err(Error::UnexpectedEnd));
        assert_eq!(evaluate("("), Err(Error::UnexpectedEnd));
        assert_eq!(evaluate("* 5"), Err(Error::UnexpectedToken { at: 0 }));
        assert_eq!(evaluate("5 * * 2"), Err(Error::UnexpectedToken { at: 4 }));
        assert_eq!(evaluate("5 $ 2"), Err(Error::UnexpectedChar { at: 2, found: '$' }));
        assert_eq!(evaluate("5,5"), Err(Error::UnexpectedChar { at: 1, found: ',' }));
        assert_eq!(evaluate("5 5"), Err(Error::UnexpectedToken { at: 2 }));
        assert_eq!(evaluate("5k5"), Err(Error::UnexpectedToken { at: 2 }));
        assert_eq!(evaluate("1_000"), Err(Error::UnexpectedChar { at: 1, found: '_' }));
    }

    #[test]
    fn division_by_zero() {
        assert_eq!(evaluate("1/0"), Err(Error::DivisionByZero));
        assert_eq!(evaluate("0/0"), Err(Error::DivisionByZero));
        assert_eq!(evaluate("5/(2-2)"), Err(Error::DivisionByZero));
        assert_eq!(evaluate("1/0k"), Err(Error::DivisionByZero));
        assert_eq!(evaluate("1/-0"), Err(Error::DivisionByZero));
    }

    #[test]
    fn large_numbers() {
        assert_eq!(value("999999999 * 999999999"), 999_999_998_000_000_000.0);
        assert_eq!(value("1000000m * 1000000m"), 1e24);
        assert_eq!(format_plain(value("999999999 * 999999999")), "999999998000000000");
        assert_eq!(format_plain(value("1000000m * 1000000m")), "1000000000000000000000000");
    }

    #[test]
    fn overflowing_results() {
        // Each factor is 1e6 × 1e6 = 1e12; twenty-six of them is 1e312, past f64::MAX.
        let product = ["1000000m"; 26].join(" * ");
        assert_eq!(evaluate(&product), Err(Error::Overflow));
        // A number literal too large for f64 is caught while lexing.
        let huge = "9".repeat(400);
        assert_eq!(evaluate(&huge), Err(Error::Overflow));
    }

    #[test]
    fn deep_nesting_is_capped() {
        let at_limit = format!("{}1{}", "(".repeat(MAX_DEPTH), ")".repeat(MAX_DEPTH));
        assert_eq!(value(&at_limit), 1.0);
        let too_deep = format!("{}1{}", "(".repeat(MAX_DEPTH + 1), ")".repeat(MAX_DEPTH + 1));
        assert_eq!(evaluate(&too_deep), Err(Error::TooDeep));

        assert_eq!(value(&format!("{}1", "-".repeat(MAX_DEPTH - 1))), -1.0);
        assert_eq!(value(&format!("{}1", "-".repeat(MAX_DEPTH))), 1.0);
        assert_eq!(evaluate(&format!("{}1", "-".repeat(MAX_DEPTH + 1))), Err(Error::TooDeep));

        // A thousand of either is an error, not a stack overflow.
        assert_eq!(evaluate(&format!("{}1", "-".repeat(1000))), Err(Error::TooDeep));
        assert_eq!(evaluate(&format!("{}1{}", "(".repeat(1000), ")".repeat(1000))), Err(Error::TooDeep));
    }

    #[test]
    fn plain_formatting() {
        assert_eq!(format_plain(1500.0), "1500");
        assert_eq!(format_plain(1.0 / 3.0), "0.3333");
        assert_eq!(format_plain(2.5), "2.5");
        assert_eq!(format_plain(-2.5), "-2.5");
        assert_eq!(format_plain(-0.00001), "0");
        assert_eq!(format_plain(1e24), "1000000000000000000000000");
        assert_eq!(format_plain(0.1 + 0.2), "0.3");
        assert_eq!(format_plain(1234567.891), "1234567.891");
        assert_eq!(format_plain(0.0), "0");
        assert_eq!(format_plain(-0.0), "0");
        // Rounded half away from zero.
        assert_eq!(format_plain(2.00005), "2.0001");
        assert_eq!(format_plain(-2.00005), "-2.0001");
        assert_eq!(format_plain(1.00004), "1");
        assert_eq!(format_plain(0.99999), "1");
        assert_eq!(format_plain(-0.99999), "-1");
        assert_eq!(format_plain(9.99999), "10");
        // 1e300 has 301 digits, no exponent.
        let huge = format_plain(1e300);
        assert_eq!(huge.len(), 301);
        assert!(huge.chars().all(|digit| matches!(digit, '0' | '1')));
    }

    #[test]
    fn grouped_formatting() {
        assert_eq!(format_grouped(1234567.891), "1,234,567.891");
        assert_eq!(format_grouped(-1500.0), "-1,500");
        assert_eq!(format_grouped(999.0), "999");
        assert_eq!(format_grouped(1000.0), "1,000");
        assert_eq!(format_grouped(0.5), "0.5");
        assert_eq!(format_grouped(0.0), "0");
        assert_eq!(format_grouped(-1234.5), "-1,234.5");
        assert_eq!(format_grouped(1e24), "1,000,000,000,000,000,000,000,000");
        // Same rounding as `format_plain`; only the integer part is grouped.
        assert_eq!(format_grouped(1234.99999), "1,235");
    }

    #[test]
    fn non_finite_values_never_panic() {
        assert_eq!(format_plain(f64::NAN), "NaN");
        assert_eq!(format_plain(f64::INFINITY), "inf");
        assert_eq!(format_plain(f64::NEG_INFINITY), "-inf");
        assert_eq!(format_grouped(f64::NAN), "NaN");
        assert_eq!(format_grouped(f64::INFINITY), "inf");
        assert_eq!(format_grouped(f64::NEG_INFINITY), "-inf");
    }

    #[test]
    fn errors_read_as_english() {
        assert_eq!(Error::Empty.to_string(), "empty expression");
        assert_eq!(Error::DivisionByZero.to_string(), "division by zero");
        assert_eq!(Error::UnknownUnit { at: 1, word: "q".to_owned() }.to_string(), "unknown unit \"q\"");
        assert_eq!(Error::UnclosedParen { at: 0 }.to_string(), "missing \")\"");
        assert_eq!(Error::TooDeep.to_string(), "expression nested too deeply");
        let error: &dyn std::error::Error = &Error::Overflow;
        assert_eq!(error.to_string(), "number out of range");
    }

    #[test]
    fn whitespace_between_tokens_is_ignored() {
        assert_eq!(value("  1 +\t2 *\n3  "), 7.0);
        assert_eq!(value(" ( 1 + 2 ) * 3 "), 9.0);
        assert_eq!(value(" 1.5 k "), 1500.0);
    }

    #[test]
    fn never_panics_on_garbage() {
        let inputs = [
            "", " ", "+", "-", "(", ")", "()", "(()", "))", ".", "..", ". .", "1.", ".1", "1..2", "1.2.3.4", "e",
            "1e5", "1$2", "1,000", "1_000", "5 5", "5x", "x5", "(x)", "--", "*", "1//2", "1/*2", "1 +", "∞", "١٢٣",
            "1k2", "1kk", "(1)(2)", "((((", "))))", "div", "1 div",
        ];
        for input in inputs {
            let _ = evaluate(input);
            let _ = evaluate_with(input, &Currency);
            if let Ok(value) = evaluate(input) {
                let _ = format_plain(value);
                let _ = format_grouped(value);
            }
        }
    }
}
