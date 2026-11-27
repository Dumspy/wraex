//! `ref/apt/renderer/src/web/client-log/client-log.ts`: the chat lines of the
//! game's `Client.txt`.
//!
//! Upstream reads the log in its overlay window (`MAIN->CLIENT::game-log`) and
//! builds a [`LogEntry`] per line, but never uses it yet (`console.log(entry)` is
//! commented out), so the port returns the entry instead of dropping it. Reading
//! the file is `wraex_core::game_files`' `LogWatcher`; join each batch's lines
//! into one string and pass it in.
//!
//! The chat channels come from `CLIENT_STRINGS` (translated); the trade site's
//! buy whispers come from the module's own per-language tables.

use serde::Serialize;
use wraex_core::js;
use wraex_core::js_regex::Regex;

use crate::data::Data;

/// `const enum MessageChannel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MessageChannel {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "trade")]
    Trade,
    #[serde(rename = "global")]
    Global,
    #[serde(rename = "party")]
    Party,
    #[serde(rename = "guild")]
    Guild,
    #[serde(rename = "whisper.from")]
    WhisperFrom,
    #[serde(rename = "whisper.to")]
    WhisperTo,
}

/// `interface LogEntry`. Upstream spells the first field `сhannel` with a
/// Cyrillic `с`; it is never serialised, so this port uses the plain spelling.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogEntry {
    pub channel: MessageChannel,
    /// The `<guild tag>` before a chat name, when the client shows one.
    #[serde(rename = "guildTag", skip_serializing_if = "Option::is_none")]
    pub guild_tag: Option<String>,
    #[serde(rename = "charName", skip_serializing_if = "Option::is_none")]
    pub char_name: Option<String>,
    pub body: String,
    /// The trade site's buy whisper, for `whisper.from` bodies it recognises.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trade: Option<Trade>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Trade {
    pub item: TradeItem,
    pub price: TradePrice,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab: Option<TradeTab>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TradeItem {
    pub name: String,
    /// Bulk whispers (`I'd like to buy your 3 …`) carry an amount.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gem: Option<Gem>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TradePrice {
    pub name: String,
    pub amount: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TradeTab {
    pub name: String,
    pub left: f64,
    pub top: f64,
}

/// `gem: { level, quality }`, from `CLIENT_STRINGS.CHAT_WEBTRADE_GEM`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Gem {
    pub level: f64,
    pub quality: f64,
}

/// `TRADE_WHISPER`: the trade site's item whisper per language, keyed by
/// `AppConfig().language` in the reference.
const TRADE_WHISPER: [(&str, &str); 9] = [
    (
        "en",
        r#"^Hi, I would like to buy your (?<item>.+) listed for (?<price>.+) in (?<league>.+) \(stash tab "(?<tab_name>.*)"; position: left (?<tab_left>\d+), top (?<tab_top>\d+)\)(?<message>.+)?$"#,
    ),
    (
        "ru",
        r#"^Здравствуйте, хочу купить у вас (?<item>.+) за (?<price>.+) в лиге (?<league>.+) \(секция "(?<tab_name>.*)"; позиция: (?<tab_left>\d+) столбец, (?<tab_top>\d+) ряд\)(?<message>.+)?$"#,
    ),
    (
        "ko",
        r#"^안녕하세요, (?<league>.+)\(보관함 탭 "(?<tab_name>.*)", 위치: 왼쪽 (?<tab_left>\d+), 상단 (?<tab_top>\d+)\)에 (?<price>.+)\(으\)로 올려놓은 (?<item>.+)\(을\)를 구매하고 싶습니다(?<message>.+)?$"#,
    ),
    (
        "de",
        r#"^Hi, ich möchte '(?<item>.+)' zum angebotenen Preis von (?<price>.+) in der (?<league>.+)-Liga kaufen \(Truhenfach "(?<tab_name>.*)"; Position: (?<tab_left>\d+) von links, (?<tab_top>\d+) von oben\)(?<message>.+)?$"#,
    ),
    (
        "fr",
        r#"^Bonjour, je souhaiterais t'acheter (?<item>.+) pour (?<price>.+) dans la ligue (?<league>.+) \(onglet de réserve "(?<tab_name>.*)" ; (?<tab_left>\d+)e en partant de la gauche, (?<tab_top>\d+)e en partant du haut\)(?<message>.+)?$"#,
    ),
    (
        "es",
        r#"^Hola, quisiera comprar tu (?<item>.+) listado por (?<price>.+) en (?<league>.+) \(pestaña de alijo "(?<tab_name>.*)"; posición: izquierda(?<tab_left>\d+), arriba (?<tab_top>\d+)\)(?<message>.+)?$"#,
    ),
    (
        "pt",
        r#"^Olá, eu gostaria de comprar o seu item (?<item>.+) listado por (?<price>.+) na (?<league>.+) \(aba do baú: "(?<tab_name>.*)"; posição: esquerda (?<tab_left>\d+), topo (?<tab_top>\d+)\)(?<message>.+)?$"#,
    ),
    (
        "th",
        r#"^สวัสดี, เราต้องการจะชื้อของคุณ (?<item>.+) ใน ราคา (?<price>.+) ใน (?<league>.+) \(stash tab "(?<tab_name>.*)"; ตำแหน่ง: ซ้าย (?<tab_left>\d+), บน (?<tab_top>\d+)\)(?<message>.+)?$"#,
    ),
    (
        "cmn-Hant",
        r#"^你好，我想購買 (?<item>.+) 標價 (?<price>.+) 在 (?<league>.+) \(倉庫頁 "(?<tab_name>.*)"; 位置: 左 (?<tab_left>\d+), 上 (?<tab_top>\d+)\)(?<message>.+)?$"#,
    ),
];

/// `TRADE_BULK_WHISPER`: the bulk-exchange whisper per language. Korean is a
/// placeholder in the reference (`/^_FIX_ME_$/`), kept as-is.
const TRADE_BULK_WHISPER: [(&str, &str); 4] = [
    ("en", r"^Hi, I'd like to buy your (?<item>.+) for my (?<price>.+) in (?<league>.+)\.(?<message>.+)?$"),
    ("ru", r"^Здравствуйте, хочу купить у вас (?<item>.+) за (?<price>.+) в лиге (?<league>.+)\.(?<message>.+)?$"),
    ("cmn-Hant", r"^你好，我想用 (?<price>.+) 購買 (?<item>.+) in (?<league>.+)\.(?<message>.+)?$"),
    ("ko", r"^_FIX_ME_$"),
];

/// The table's pattern for `language`, if it has one. The reference would index
/// `undefined` and then throw on the missing groups; a language with no pattern
/// simply gets no trade details here. The static patterns always compile
/// (`whisper_tables_compile` tests every one); a pattern that did not would leave
/// the language without trade details rather than fail at startup.
fn whisper_regex(language: &str, table: &[(&str, &str)]) -> Option<Regex> {
    table.iter().find(|(lang, _)| *lang == language).and_then(|(_, source)| Regex::new(source, "").ok())
}

/// `s.split(' ', 1).toString()` and `s.split(' ').slice(1).join(' ')`: the first
/// token, and the rest after the first space.
fn amount_and_name(s: &str) -> (String, String) {
    match s.split_once(' ') {
        Some((amount, rest)) => (amount.to_owned(), rest.to_owned()),
        None => (s.to_owned(), String::new()),
    }
}

/// The chat patterns for one game language, plus the trade whispers.
#[derive(Debug, Clone)]
pub struct ClientLog {
    chat_global: Regex,
    chat_trade: Regex,
    chat_system: Regex,
    chat_whisper_to: Regex,
    chat_whisper_from: Regex,
    chat_party: Regex,
    chat_guild: Regex,
    webtrade_gem: Regex,
    trade_whisper: Option<Regex>,
    bulk_whisper: Option<Regex>,
}

impl ClientLog {
    /// `handleLine`'s patterns: `CLIENT_STRINGS.CHAT_*` for the data's language,
    /// and `TRADE_WHISPER`/`TRADE_BULK_WHISPER` for that same language (the
    /// reference keys those by the app's language setting, which selects the
    /// game language too).
    pub fn new(data: &Data) -> Self {
        let language = data.lang();
        Self {
            chat_global: data.re("CHAT_GLOBAL").clone(),
            chat_trade: data.re("CHAT_TRADE").clone(),
            chat_system: data.re("CHAT_SYSTEM").clone(),
            chat_whisper_to: data.re("CHAT_WHISPER_TO").clone(),
            chat_whisper_from: data.re("CHAT_WHISPER_FROM").clone(),
            chat_party: data.re("CHAT_PARTY").clone(),
            chat_guild: data.re("CHAT_GUILD").clone(),
            webtrade_gem: data.re("CHAT_WEBTRADE_GEM").clone(),
            trade_whisper: whisper_regex(language, &TRADE_WHISPER),
            bulk_whisper: whisper_regex(language, &TRADE_BULK_WHISPER),
        }
    }

    /// `handleLine`: the entry for one `Client.txt` line, or `None` when the line
    /// carries no chat message. The line's date, millis and source columns are
    /// dropped like upstream does: everything before the first `] `, and the
    /// `[INFO Client 1234]` tag after it.
    pub fn handle_line(&self, line: &str) -> Option<LogEntry> {
        let text = line.split("] ").skip(1).collect::<Vec<_>>().join("] ");
        if text.is_empty() {
            return None;
        }

        // The order the reference tries the channels in: global, trade, system,
        // whisper to, whisper from, party, guild.
        let mut found = None;
        for (channel, regex) in [
            (MessageChannel::Global, &self.chat_global),
            (MessageChannel::Trade, &self.chat_trade),
            (MessageChannel::System, &self.chat_system),
            (MessageChannel::WhisperTo, &self.chat_whisper_to),
            (MessageChannel::WhisperFrom, &self.chat_whisper_from),
            (MessageChannel::Party, &self.chat_party),
            (MessageChannel::Guild, &self.chat_guild),
        ] {
            if let Some(caps) = regex.captures(&text) {
                found = Some((channel, caps));
                break;
            }
        }
        let (channel, caps) = found?;

        // Every `CHAT_*` pattern captures `body`; one that did not would be skipped,
        // as a line that matches no channel is.
        let body = caps.name("body")?.to_owned();
        let mut entry = LogEntry {
            channel,
            guild_tag: caps.name("guild_tag").map(str::to_owned),
            char_name: caps.name("char_name").map(str::to_owned),
            body,
            trade: None,
        };

        if entry.channel == MessageChannel::WhisperFrom {
            entry.trade = self.parse_trade(&entry.body);
        }
        Some(entry)
    }

    /// The trade body checks of `handleLine`, against the item whisper first and
    /// the bulk whisper second.
    fn parse_trade(&self, body: &str) -> Option<Trade> {
        // Every named group used below is mandatory in its pattern: a pattern that
        // lacked one would leave this whisper without trade details.
        if let Some(caps) = self.trade_whisper.as_ref().and_then(|re| re.captures(body)) {
            let price = amount_and_name(caps.name("price")?);
            let offered = caps.name("item")?.to_owned();
            let gem_caps = self.webtrade_gem.captures(&offered);
            // `CHAT_WEBTRADE_GEM` captures both; a language without them would read
            // the level and quality as `Number(undefined)`.
            let gem = gem_caps.as_ref().map(|caps| Gem {
                level: caps.name("gem_lvl").map_or(0.0, js::number),
                quality: caps.name("gem_qual").map_or(0.0, js::number),
            });
            // The reference renames a gem offer to just the gem's name.
            let gem_name = gem_caps.as_ref().and_then(|caps| caps.name("gem_name")).map(str::to_owned);
            let item = gem_name.unwrap_or(offered);
            return Some(Trade {
                item: TradeItem { name: item, amount: None, gem },
                price: TradePrice { name: price.1, amount: js::number(&price.0) },
                tab: Some(TradeTab {
                    name: caps.name("tab_name")?.to_owned(),
                    left: js::number(caps.name("tab_left")?),
                    top: js::number(caps.name("tab_top")?),
                }),
            });
        }

        let caps = self.bulk_whisper.as_ref().and_then(|re| re.captures(body))?;
        let price = amount_and_name(caps.name("price")?);
        let item = amount_and_name(caps.name("item")?);
        Some(Trade {
            item: TradeItem { name: item.1, amount: Some(js::number(&item.0)), gem: None },
            price: TradePrice { name: price.1, amount: js::number(&price.0) },
            tab: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use super::*;

    /// `init("en")` from the reference's data layer: the chat patterns are the
    /// English `CLIENT_STRINGS`.
    fn data() -> &'static Data {
        static DATA: OnceLock<Data> = OnceLock::new();
        DATA.get_or_init(|| Data::load(&crate::data::reference_source(), "en").unwrap())
    }

    fn log() -> ClientLog {
        ClientLog::new(data())
    }

    /// A `Client.txt` line: date, millis, source, level tag, then `text`.
    fn line(text: &str) -> String {
        format!("2024/03/01 12:00:00 123456789 a1b2c3d4 [INFO Client 1234] {text}")
    }

    /// The channels `CHAT_GLOBAL`…`CHAT_GUILD` recognise, in the order
    /// `handleLine` tries them.
    #[test]
    fn chat_channels_are_recognised() {
        let log = log();
        let parse = |text: &str| log.handle_line(&line(text)).unwrap_or_else(|| panic!("{text}"));

        let global = parse("#Kvan: WTS stuff");
        assert_eq!(
            (global.channel, global.char_name.as_deref(), global.guild_tag, global.body.as_str()),
            (MessageChannel::Global, Some("Kvan"), None, "WTS stuff")
        );

        let tagged = parse("#<ABC> Kvan: hi");
        assert_eq!((tagged.guild_tag.as_deref(), tagged.char_name.as_deref()), (Some("ABC"), Some("Kvan")));

        let trade = parse("$Kvan: WTS");
        assert_eq!(trade.channel, MessageChannel::Trade);
        let party = parse("%Kvan: hi");
        assert_eq!(party.channel, MessageChannel::Party);
        let guild = parse("&Kvan: hi");
        assert_eq!(guild.channel, MessageChannel::Guild);

        // System messages have no name, only a body.
        let system = parse(": You have joined global chat channel 100 English.");
        assert_eq!(
            (system.channel, system.char_name, system.body.as_str()),
            (MessageChannel::System, None, "You have joined global chat channel 100 English.")
        );

        let to = parse("@To Kvan: hi");
        assert_eq!(
            (to.channel, to.char_name.as_deref(), to.body.as_str()),
            (MessageChannel::WhisperTo, Some("Kvan"), "hi")
        );
        let from = parse("@From <ABC> Kvan: hi");
        assert_eq!(
            (from.channel, from.char_name.as_deref(), from.guild_tag.as_deref(), from.body.as_str()),
            (MessageChannel::WhisperFrom, Some("Kvan"), Some("ABC"), "hi")
        );
        assert_eq!(from.trade, None, "a plain whisper has no trade body");
    }

    /// The date, millis and source columns are not validated, only skipped: a
    /// line without `] ` (or without a chat prefix) yields nothing.
    #[test]
    fn non_chat_lines_are_ignored() {
        let log = log();
        assert_eq!(log.handle_line(""), None);
        assert_eq!(log.handle_line("2024/03/01 12:00:00 123456789 a1b2c3d4 is not a log line"), None);
        assert_eq!(log.handle_line(&line("[SCENE] Set Source [The Riverbank]")), None);
        assert_eq!(log.handle_line(&line("not a chat message")), None);
        // Everything after `: ` is a system message, whatever it says.
        assert_eq!(log.handle_line(&line(": not a message we know")).unwrap().channel, MessageChannel::System);
    }

    /// An item whisper: item, price and the stash tab position.
    #[test]
    fn item_whisper_is_parsed() {
        let entry = log()
            .handle_line(&line(
                r#"@From Buyer: Hi, I would like to buy your Tabula Rasa listed for 10 chaos in Standard (stash tab "trade 1"; position: left 3, top 4)"#,
            ))
            .unwrap();
        assert_eq!(entry.channel, MessageChannel::WhisperFrom);
        assert_eq!(
            entry.trade,
            Some(Trade {
                item: TradeItem { name: "Tabula Rasa".to_owned(), amount: None, gem: None },
                price: TradePrice { name: "chaos".to_owned(), amount: 10.0 },
                tab: Some(TradeTab { name: "trade 1".to_owned(), left: 3.0, top: 4.0 }),
            })
        );

        // A price with two words keeps the rest as the currency name.
        let entry = log()
            .handle_line(&line(
                r#"@From Buyer: Hi, I would like to buy your Tabula Rasa listed for 2 divine orbs in Standard (stash tab "x"; position: left 0, top 0)"#,
            ))
            .unwrap();
        let trade = entry.trade.unwrap();
        assert_eq!((trade.price.amount, trade.price.name.as_str()), (2.0, "divine orbs"));
    }

    /// A levelled gem offer is renamed to the gem, with its level and quality.
    #[test]
    fn gem_whisper_keeps_the_gems_level_and_quality() {
        let entry = log()
            .handle_line(&line(
                r#"@From Buyer: Hi, I would like to buy your level 20 20% Raise Zombie listed for 1 divine in Standard (stash tab "gems"; position: left 1, top 2)"#,
            ))
            .unwrap();
        let trade = entry.trade.unwrap();
        assert_eq!(trade.item.name, "Raise Zombie");
        assert_eq!(trade.item.gem, Some(Gem { level: 20.0, quality: 20.0 }));
    }

    /// A bulk-exchange whisper carries amounts on both sides and no tab.
    #[test]
    fn bulk_whisper_is_parsed() {
        let entry = log()
            .handle_line(&line("@From Buyer: Hi, I'd like to buy your 3 Divine Orb for my 100 Chaos Orb in Standard."))
            .unwrap();
        assert_eq!(
            entry.trade,
            Some(Trade {
                item: TradeItem { name: "Divine Orb".to_owned(), amount: Some(3.0), gem: None },
                price: TradePrice { name: "Chaos Orb".to_owned(), amount: 100.0 },
                tab: None,
            })
        );
    }

    /// Only whispers *from* other players are trade offers; whispers to them and
    /// other channels keep the body alone.
    #[test]
    fn only_incoming_whispers_are_trade_offers() {
        let body = r#"@To Buyer: Hi, I would like to buy your Tabula Rasa listed for 10 chaos in Standard (stash tab "x"; position: left 1, top 2)"#;
        let entry = log().handle_line(&line(body)).unwrap();
        assert_eq!(entry.channel, MessageChannel::WhisperTo);
        assert_eq!(entry.trade, None);
    }

    /// Every per-language whisper pattern compiles, `TRADE_BULK_WHISPER`'s
    /// Korean placeholder included.
    #[test]
    fn whisper_tables_compile() {
        for (language, source) in TRADE_WHISPER.iter().chain(TRADE_BULK_WHISPER.iter()) {
            let _ = Regex::new(source, "").unwrap_or_else(|e| panic!("{language}: {e}"));
        }
    }

    /// `split(' ', 1)` keeps empty tokens of the rest, as JavaScript does.
    #[test]
    fn amounts_split_like_javascript() {
        assert_eq!(amount_and_name("10 chaos"), ("10".to_owned(), "chaos".to_owned()));
        assert_eq!(amount_and_name("10"), ("10".to_owned(), String::new()));
        assert_eq!(amount_and_name("10  chaos"), ("10".to_owned(), " chaos".to_owned()));
    }

    /// A `whisper.from` in a language without a whisper pattern keeps the entry
    /// but has no trade details (the reference would throw on the missing groups).
    #[test]
    fn unsupported_languages_keep_the_whisper_without_trade() {
        let log = ClientLog::new(data());
        assert!(log.trade_whisper.is_some());
        // The tables cover every language upstream's settings offer; the port is
        // English-only for now, so only English is exercised end to end.
        assert_eq!(whisper_regex("xx", &TRADE_WHISPER), None);
        assert_eq!(whisper_regex("xx", &TRADE_BULK_WHISPER), None);
    }
}
