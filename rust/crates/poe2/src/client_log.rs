//! `ref/ee2/renderer/src/web/client-log/client-log-parser.ts` and
//! `…/client-log/client-log.ts`: the game's `Client.txt` lines turned into the
//! events EE2 sends to its main process.
//!
//! [`ClientLog`] is `useClientLog()`'s state without Vue: the zone, area level,
//! player level and last character the level tracker reads, plus the game-start
//! millis and the log-parse version. [`ClientLog::handle_line`] takes one line of
//! `Client.txt` (see `wraex_core::game_files::LogWatcher`, which yields them
//! trimmed and without the newline) and returns the [`ClientLogEvent`] EE2
//! forwards as `CLIENT->MAIN::write-data` with `action: "client-log-event"`.
//! Plain `log` lines are not forwarded, so they come back as `None`.
//!
//! Serialise with [`wraex_core::js::json_value`] (or `js::stringify`) when
//! producing fixtures: the reference's numbers are JavaScript doubles and
//! `JSON.stringify` writes integral ones without a fraction.
//!
//! Timestamps: the reference calls `Date.parse("YYYY/MM/DD HH:mm:ss")`, and V8
//! reads a slash-separated date as *local* time, so `ts` depends on the machine's
//! zone. EE2's suite gets away with that because CI runs in UTC (the only
//! timestamp assertion checks the UTC date). This port makes the offset explicit:
//! [`ClientLog::new`] takes the UTC offset in seconds, so fixtures do not depend
//! on where they run.

use std::sync::OnceLock;

use serde::Serialize;
use wraex_core::js;
use wraex_core::js_regex::Regex;

use crate::data::Data;

/// `ClientLogInfoType`: the `type` of every [`Event`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InfoType {
    Log,
    GameStart,
    Login,
    LoadZone,
    LevelUp,
    GameVersion,
    AltTab,
    Npc,
    PlayerDeath,
    PassiveTree,
    PermanentBonus,
    SkillPoint,
    MapNav,
    Afk,
}

/// `SkillPointEvent['pointType']`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PointType {
    Passive,
    WeaponSet,
    Atlas,
    MapBoss,
    ArbiterBoss,
    Abyss,
    Ritual,
    Delirium,
    Expedition,
    Breach,
}

/// `ClientLogEvent`: the payload of `CLIENT->MAIN::write-data`'s
/// `client-log-event`, flattened to `{ ts, ms, type, …fields }`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClientLogEvent {
    /// `Date.parse(datetime)`, in milliseconds since the epoch.
    pub ts: f64,
    /// The log line's `millis` column, which restarts with the game.
    pub ms: f64,
    #[serde(flatten)]
    pub data: Event,
}

/// `ClientLogEvent` without its timestamp fields.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Event {
    /// `GeneralLogEvent` with `type: "log"`: everything the parser does not
    /// recognise. Not forwarded to main.
    Log,
    GameStart,
    /// In the `ClientLogEvent` union, but `parseLogVersion0` never produces it.
    Login,
    LoadZone {
        zone: String,
        #[serde(rename = "areaLevel")]
        area_level: f64,
        seed: f64,
    },
    LevelUp {
        #[serde(rename = "charName")]
        char_name: String,
        #[serde(rename = "charClass")]
        char_class: String,
        level: f64,
    },
    GameVersion {
        version: String,
    },
    AltTab {
        #[serde(rename = "gameFocused")]
        game_focused: bool,
    },
    Npc {
        #[serde(rename = "npcName")]
        npc_name: String,
        message: String,
    },
    PlayerDeath {
        #[serde(rename = "charName")]
        char_name: String,
    },
    PassiveTree {
        allocate: bool,
        #[serde(rename = "nodeId")]
        node_id: String,
        #[serde(rename = "nodeName")]
        node_name: String,
    },
    PermanentBonus {
        #[serde(rename = "permanentBonus")]
        permanent_bonus: String,
        #[serde(rename = "charName")]
        char_name: String,
    },
    SkillPoint {
        points: f64,
        #[serde(rename = "pointType")]
        point_type: PointType,
    },
    MapNav {
        #[serde(rename = "mapName")]
        map_name: String,
    },
    Afk {
        #[serde(rename = "isAfk")]
        is_afk: bool,
    },
}

impl Event {
    /// `data.type`; the same strings as [`InfoType`]'s serde form.
    pub fn info_type(&self) -> InfoType {
        match self {
            Event::Log => InfoType::Log,
            Event::GameStart => InfoType::GameStart,
            Event::Login => InfoType::Login,
            Event::LoadZone { .. } => InfoType::LoadZone,
            Event::LevelUp { .. } => InfoType::LevelUp,
            Event::GameVersion { .. } => InfoType::GameVersion,
            Event::AltTab { .. } => InfoType::AltTab,
            Event::Npc { .. } => InfoType::Npc,
            Event::PlayerDeath { .. } => InfoType::PlayerDeath,
            Event::PassiveTree { .. } => InfoType::PassiveTree,
            Event::PermanentBonus { .. } => InfoType::PermanentBonus,
            Event::SkillPoint { .. } => InfoType::SkillPoint,
            Event::MapNav { .. } => InfoType::MapNav,
            Event::Afk { .. } => InfoType::Afk,
        }
    }
}

/// `LogRegex` from `client-log.ts`: the date, millis and source columns, the
/// `[INFO Client 438888]` tag, then the message.
const LOG_REGEX: &str = r"^(?<datetime>\d{4}\/\d{2}\/\d{2} \d{2}:\d{2}:\d{2}) (?<millis>\d+) (?<source>[a-f0-9]+) \[\S+ \S+ \S+\] (?<text>.*)$";
const GAME_VERSION_LINE: &str = "User agent: PoE poe2_production/tags/";
const AFK_MODE_ON: &str = "AFK mode is now ON. Autoreply \"This player is AFK.\"";
const AFK_MODE_OFF: &str = "AFK mode is now OFF.";
const KNOWN_NPC_NAMES: [&str; 7] = [
    "Renly",
    "Ghostly Voice",
    "The Rust King",
    "Beira of the Rotten Pack",
    "The Bloated Miller",
    "Mortimer",
    "Wounded Man",
];
const SLAIN_REGEX: &str = r"(?<char_name>.+) has been slain.";
const PASSIVE_SKILL_REGEX: &str =
    r"Successfully (?<direction>(un)?allocated) passive skill id: (?<node_id>.+), name: (?<node_name>.+)";
const PERMANENT_BONUS_REGEX: &str = r"(?<char_name>.+) has received (?<bonus>.+)";
const SKILL_POINT_REGEX: &str = r"You have received (?<points>(\d+|an?)) (?<point_type>.*) Skill Points?\.";
const MAP_NAV_REGEX: &str = r"Set Source \[(?<map_name>.+)\]";

fn compile(source: &str) -> Regex {
    Regex::new(source, "").unwrap_or_else(|e| panic!("{e}"))
}

fn whole_line() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile(LOG_REGEX))
}

fn slain() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile(SLAIN_REGEX))
}

fn passive_skill() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile(PASSIVE_SKILL_REGEX))
}

fn permanent_bonus() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile(PERMANENT_BONUS_REGEX))
}

fn skill_point() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile(SKILL_POINT_REGEX))
}

fn map_nav() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile(MAP_NAV_REGEX))
}

/// `LogNamespace`: the bracketed prefix of a log message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Namespace {
    /// No brackets at all (`LogNamespace.None`), where chat and zone lines live.
    Plain,
    Startup,
    Http2,
    Window,
    Scene,
    /// Any other namespace; the reference casts the string but handles none.
    Other,
}

/// `text.startsWith("[")`: split off the namespace and trim the rest.
fn split_namespace(text: &str) -> (Namespace, &str) {
    let Some(rest) = text.strip_prefix('[') else {
        return (Namespace::Plain, text);
    };
    let Some(end) = rest.find(']') else {
        return (Namespace::Plain, text);
    };
    let namespace = match &rest[..end] {
        "STARTUP" => Namespace::Startup,
        "HTTP2" => Namespace::Http2,
        "WINDOW" => Namespace::Window,
        "SCENE" => Namespace::Scene,
        _ => Namespace::Other,
    };
    (namespace, js::trim(&rest[end + 1..]))
}

/// `getClientLogParseVersion`. The reference compares the client version against
/// `0.4.0j` through a `lessThanVersion` whose every branch returns `0`, so both
/// sides of the comparison and the version itself have no effect yet.
pub fn get_client_log_parse_version(_version: &str) -> i32 {
    0
}

/// `useClientLog()`'s state and `handleLine`.
#[derive(Debug, Clone)]
pub struct ClientLog {
    /// `areaLevel`: the current zone's monster level.
    pub area_level: f64,
    /// `zoneName`: the last real zone, lowercased by the log parser; starts at
    /// the Riverbank like the reference.
    pub zone_name: String,
    /// `playerLevel`, from level-ups (`setPlayerLevel` overwrites it).
    pub player_level: f64,
    /// `lastCharacter`, the character of the last level-up.
    last_character: String,
    /// `gameStartMillis`, the `ms` of the last `[STARTUP] Game Start`.
    game_start_millis: f64,
    /// `logParseVersion`, set from every `game-version` event.
    log_parse_version: i32,
    /// The `Date.parse` offset: the machine's UTC offset in seconds.
    utc_offset_seconds: i32,
    /// `CLIENT_STRINGS.LOG_LEVEL_UP` for the data's language.
    level_up: Regex,
    /// `CLIENT_STRINGS.LOG_ZONE_GEN` for the data's language.
    zone_gen: Regex,
}

impl ClientLog {
    /// The composable's initial state (area level 1, Riverbank, player level 1),
    /// with `utc_offset_seconds` as the zone its timestamps are written in.
    pub fn new(data: &Data, utc_offset_seconds: i32) -> Self {
        Self {
            area_level: 1.0,
            zone_name: "G1_1".to_owned(),
            player_level: 1.0,
            last_character: String::new(),
            game_start_millis: 0.0,
            log_parse_version: 0,
            utc_offset_seconds,
            level_up: data.re("LOG_LEVEL_UP").clone(),
            zone_gen: data.re("LOG_ZONE_GEN").clone(),
        }
    }

    /// `lastCharacter`
    pub fn last_character(&self) -> &str {
        &self.last_character
    }

    /// `gameStartMillis`
    pub fn game_start_millis(&self) -> f64 {
        self.game_start_millis
    }

    /// `logParseVersion`
    pub fn log_parse_version(&self) -> i32 {
        self.log_parse_version
    }

    /// `handleLine`: parse one line, update the tracked state, and return the
    /// event main receives. `None` for lines outside the log format (the
    /// reference returns before parsing) and for plain `log` events.
    ///
    /// The reference also logs a warning when `ts` goes backwards and when `ms`
    /// rolls over below `gameStartMillis`; neither changes state.
    pub fn handle_line(&mut self, line: &str) -> Option<ClientLogEvent> {
        let caps = whole_line().captures(line)?;
        let datetime = caps.name("datetime").expect("the log regex captures datetime");
        let millis = js::parse_int(caps.name("millis").expect("the log regex captures millis"));
        let text = caps.name("text").expect("the log regex captures text");

        let event = self.parse_client_log_text(text, datetime, millis, self.log_parse_version);
        self.update_refs(&event);

        match event.data {
            Event::Log => None,
            _ => Some(event),
        }
    }

    /// `parseClientLogText`: one message (what follows the `[INFO Client 1234]`
    /// tag) plus its log columns, without touching the tracked state.
    /// `log_parse_version` comes from [`get_client_log_parse_version`]; both of
    /// `parseClientLogText`'s branches currently run `parseLogVersion0`, so the
    /// version has no effect yet.
    pub fn parse_client_log_text(
        &self,
        text: &str,
        datetime: &str,
        ms: f64,
        _log_parse_version: i32,
    ) -> ClientLogEvent {
        let (namespace, text) = split_namespace(text);

        let data = match namespace {
            Namespace::Startup => {
                if text == "Game Start" {
                    Event::GameStart
                } else {
                    Event::Log
                }
            }
            Namespace::Http2 => match text.strip_prefix(GAME_VERSION_LINE) {
                Some(rest) => Event::GameVersion { version: rest.split(' ').next().unwrap_or("").to_owned() },
                None => Event::Log,
            },
            Namespace::Window => match text {
                "Gained focus" => Event::AltTab { game_focused: true },
                "Lost focus" => Event::AltTab { game_focused: false },
                _ => Event::Log,
            },
            Namespace::Scene => match map_nav().captures(text) {
                Some(caps) => Event::MapNav {
                    map_name: caps.name("map_name").expect("MAP_NAV_REGEX captures map_name").to_owned(),
                },
                None => Event::Log,
            },
            Namespace::Plain => self.parse_plain(text),
            Namespace::Other => Event::Log,
        };

        ClientLogEvent { ts: timestamp(datetime, self.utc_offset_seconds), ms, data }
    }

    /// The `namespace === LogNamespace.None` half of `parseLogVersion0`: player
    /// messages, chat, zone generation, passive points, then NPC dialogue.
    fn parse_plain(&self, text: &str) -> Event {
        if let Some(message) = text.strip_prefix(':') {
            // An info (local) message shown to the player in chat.
            return self.parse_player_message(js::trim(message), text);
        }
        if matches!(text.as_bytes().first(), Some(b'#' | b'@' | b'&' | b'$')) {
            // Global, whisper, guild and trade chat: no events in the reference.
            return Event::Log;
        }
        if let Some(caps) = self.zone_gen.captures(text) {
            return Event::LoadZone {
                zone: caps.name("zone").expect("LOG_ZONE_GEN captures zone").to_lowercase(),
                area_level: js::parse_int(caps.name("area_level").expect("LOG_ZONE_GEN captures area_level")),
                seed: js::parse_int(caps.name("seed").expect("LOG_ZONE_GEN captures seed")),
            };
        }
        if let Some(caps) = passive_skill().captures(text) {
            return Event::PassiveTree {
                allocate: caps.name("direction").expect("the regex captures direction") == "allocated",
                node_id: caps.name("node_id").expect("the regex captures node_id").to_owned(),
                node_name: caps.name("node_name").expect("the regex captures node_name").to_owned(),
            };
        }
        // Either a normal log line or an NPC/local chat message.
        if let Some(colon) = text.find(':') {
            let possible_name = js::trim(&text[..colon]);
            let message = js::trim(&text[colon + 1..]);
            if KNOWN_NPC_NAMES.contains(&possible_name) {
                return Event::Npc { npc_name: possible_name.to_owned(), message: message.to_owned() };
            }
        }
        Event::Log
    }

    /// The `text[0] === ":"` half: AFK, level-up, permanent bonus, skill points
    /// and deaths.
    fn parse_player_message(&self, colon_less: &str, whole: &str) -> Event {
        if colon_less == AFK_MODE_ON || colon_less == AFK_MODE_OFF {
            return Event::Afk { is_afk: colon_less == AFK_MODE_ON };
        }
        if let Some(caps) = self.level_up.captures(colon_less) {
            return Event::LevelUp {
                level: js::parse_int(caps.name("level").expect("LOG_LEVEL_UP captures level")),
                char_name: caps.name("char_name").expect("LOG_LEVEL_UP captures char_name").to_owned(),
                char_class: caps.name("char_class").expect("LOG_LEVEL_UP captures char_class").to_owned(),
            };
        }
        if let Some(caps) = permanent_bonus().captures(colon_less) {
            return Event::PermanentBonus {
                permanent_bonus: caps.name("bonus").expect("the regex captures bonus").to_owned(),
                char_name: caps.name("char_name").expect("the regex captures char_name").to_owned(),
            };
        }
        if let Some(caps) = skill_point().captures(colon_less) {
            let points = caps.name("points").expect("SKILL_POINT_REGEX captures points");
            let count = if points.starts_with('a') { 1.0 } else { js::parse_int(points) };
            return Event::SkillPoint {
                points: count,
                point_type: point_type(caps.name("point_type").expect("SKILL_POINT_REGEX captures point_type")),
            };
        }
        // The reference matches deaths against the untrimmed text, so the colon
        // stays part of `char_name`.
        if let Some(caps) = slain().captures(whole) {
            return Event::PlayerDeath {
                char_name: caps.name("char_name").expect("SLAIN_REGEX captures char_name").to_owned(),
            };
        }
        Event::Log
    }

    /// `updateRefsFromData`: the refs the widgets read, the game-start millis
    /// and the parse version.
    fn update_refs(&mut self, event: &ClientLogEvent) {
        match &event.data {
            Event::GameStart => self.game_start_millis = event.ms,
            Event::LoadZone { zone, area_level, .. } => {
                // Towns and hideouts are ignored for level tracking.
                if !(zone.contains("town") || zone.contains("hideout")) {
                    if zone == "g1_1" {
                        // Only a new character starts on the Riverbank.
                        self.player_level = 1.0;
                    }
                    self.area_level = *area_level;
                    self.zone_name = zone.clone();
                }
            }
            Event::LevelUp { level, char_name, .. } => {
                self.player_level = *level;
                self.last_character = char_name.clone();
            }
            Event::GameVersion { version } => {
                self.log_parse_version = get_client_log_parse_version(version);
            }
            _ => {}
        }
    }

    /// `setPlayerLevel(number | "")`: the settings widget's empty input leaves
    /// the level alone.
    pub fn set_player_level(&mut self, level: Option<f64>) {
        if let Some(level) = level {
            self.player_level = level;
        }
    }

    /// `testOnlyResetGameMillis`, kept for the tests that replay several logs.
    pub fn reset_game_start_millis(&mut self) {
        self.game_start_millis = 0.0;
    }
}

/// `SKILL_POINT_REGEX`'s `point_type`: the atlas sub-trees each have their own
/// points, and anything else (including `"Passive"`) is a passive point.
fn point_type(name: &str) -> PointType {
    match name {
        "Atlas" => PointType::Atlas,
        "Map Boss Atlas" => PointType::MapBoss,
        "Arbiter Boss Atlas" => PointType::ArbiterBoss,
        "Abyss Atlas" => PointType::Abyss,
        "Ritual Atlas" => PointType::Ritual,
        "Delirium Atlas" => PointType::Delirium,
        "Expedition Atlas" => PointType::Expedition,
        "Breach Atlas" => PointType::Breach,
        "Weapon Set Passive" => PointType::WeaponSet,
        _ => PointType::Passive,
    }
}

/// `Date.parse("YYYY/MM/DD HH:mm:ss")` with the group's digits. V8 reads such a
/// date in local time; `utc_offset_seconds` is that zone's offset east of UTC.
fn timestamp(datetime: &str, utc_offset_seconds: i32) -> f64 {
    let part = |from: usize, to: usize| {
        datetime[from..to].parse::<i64>().unwrap_or_else(|_| panic!("`{datetime}` is not a log timestamp"))
    };
    let (year, month, day) = (part(0, 4), part(5, 7), part(8, 10));
    let (hour, minute, second) = (part(11, 13), part(14, 16), part(17, 19));
    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second;
    ((seconds - i64::from(utc_offset_seconds)) * 1_000) as f64
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = (month + 9) % 12;
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    /// `init("en")` from `specs/vitest.setup.ts`: the parser reads
    /// `LOG_LEVEL_UP` and `LOG_ZONE_GEN` out of `CLIENT_STRINGS`.
    fn data() -> &'static Data {
        static DATA: OnceLock<Data> = OnceLock::new();
        DATA.get_or_init(|| {
            Data::load(&crate::data::reference_source(), "en", Some(crate::data::reference_trade_data())).unwrap()
        })
    }

    /// The two logs `specs/web/client-log.test.ts` feeds in, byte for byte.
    fn log1() -> &'static str {
        include_str!("../tests/data/client-log-1.txt")
    }

    fn red_vale() -> &'static str {
        include_str!("../tests/data/client-log-start-through-red-vale.txt")
    }

    /// `specs/web/FullCampaign.txt`, read from the reference like the harness does.
    fn full_campaign() -> String {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../ref/ee2/renderer/specs/web/FullCampaign.txt");
        std::fs::read_to_string(path).unwrap()
    }

    fn run(text: &str) -> (ClientLog, Vec<ClientLogEvent>) {
        let mut log = ClientLog::new(data(), 0);
        let events = text.split('\n').filter_map(|line| log.handle_line(line)).collect();
        (log, events)
    }

    fn count(events: &[ClientLogEvent], info: InfoType) -> usize {
        events.iter().filter(|event| event.data.info_type() == info).count()
    }

    fn of(events: &[ClientLogEvent], info: InfoType) -> Vec<&Event> {
        events.iter().map(|event| &event.data).filter(|event| event.info_type() == info).collect()
    }

    /// `clientLog` › "should parse first log": every line twice, without
    /// throwing, and at least one event reaches main.
    #[test]
    fn the_first_log_parses() {
        let mut log = ClientLog::new(data(), 0);
        let mut events = Vec::new();
        for line in log1().split('\n') {
            for _ in 0..2 {
                events.extend(log.handle_line(line));
            }
        }
        assert!(!events.is_empty());
    }

    /// `clientLog` › "should parse second log".
    #[test]
    fn the_second_log_parses() {
        let (_, events) = run(red_vale());
        assert!(!events.is_empty());
    }

    /// `clientLog` › "Should parse full campaign client log".
    #[test]
    fn the_full_campaign_log_parses() {
        let (_, events) = run(&full_campaign());
        assert!(!events.is_empty());
    }

    /// `clientLog` › "Timestamp should be correct format": the log's naive
    /// date-time is local time, and the suite's assertion is on its UTC date.
    #[test]
    fn timestamps_are_the_logs_own_local_date() {
        // 2025-09-11T00:00:00Z .. 2025-09-12T00:00:00Z.
        const DAY_START: f64 = 1_757_548_800_000.0;
        const DAY_END: f64 = 1_757_635_200_000.0;
        let (_, events) = run(log1());
        assert!(!events.is_empty());
        for event in &events {
            assert!(event.ts >= DAY_START && event.ts < DAY_END, "{}", event.ts);
        }
    }

    /// `clientLog` › "Should have expected game starts".
    #[test]
    fn game_start_event_counts() {
        for (label, text, expected) in [("log1", log1(), 0), ("red vale", red_vale(), 2)] {
            let (_, events) = run(text);
            assert_eq!(count(&events, InfoType::GameStart), expected, "{label}");
        }
    }

    /// `clientLog` › "Should have expected load zone events".
    #[test]
    fn load_zone_event_counts() {
        for (label, text, expected) in [("log1", log1(), 2), ("red vale", red_vale(), 15)] {
            let (_, events) = run(text);
            assert_eq!(count(&events, InfoType::LoadZone), expected, "{label}");
        }
    }

    /// `clientLog` › "Should have expected level up events".
    #[test]
    fn level_up_event_counts() {
        for (label, text, expected) in [("log1", log1(), 2), ("red vale", red_vale(), 4)] {
            let (_, events) = run(text);
            let level_ups = of(&events, InfoType::LevelUp);
            assert_eq!(level_ups.len(), expected, "{label}");
            for event in level_ups {
                let Event::LevelUp { char_name, .. } = event else { unreachable!() };
                assert!(char_name.to_lowercase().contains("kvan"), "{label}: {char_name}");
            }
        }
    }

    /// `clientLog` › "Should have game versions".
    #[test]
    fn game_version_event_counts() {
        for (label, text, expected) in [("log1", log1(), 0), ("red vale", red_vale(), 2)] {
            let (_, events) = run(text);
            let versions = of(&events, InfoType::GameVersion);
            assert_eq!(versions.len(), expected, "{label}");
            for event in versions {
                let Event::GameVersion { version } = event else { unreachable!() };
                assert_eq!(version.to_lowercase(), "4.4.0j", "{label}");
            }
        }
    }

    /// `clientLog` › "Should have alt-tabs": the events alternate focus.
    #[test]
    fn alt_tab_event_counts() {
        for (label, text, expected) in [("log1", log1(), 0), ("red vale", red_vale(), 28)] {
            let (_, events) = run(text);
            let alt_tabs = of(&events, InfoType::AltTab);
            assert_eq!(alt_tabs.len(), expected, "{label}");
            let mut last_was = true;
            for event in alt_tabs {
                let Event::AltTab { game_focused } = event else { unreachable!() };
                assert_eq!(*game_focused, !last_was, "{label}");
                last_was = *game_focused;
            }
        }
    }

    /// `clientLog` › "Should have npc dialog".
    #[test]
    fn npc_dialog_event_counts() {
        let expected: &[(&str, &[(&str, usize)])] = &[
            ("log1", &[]),
            (
                "red vale",
                &[
                    ("Wounded Man", 3),
                    ("Mortimer", 2),
                    ("The Bloated Miller", 10),
                    ("Renly", 4),
                    ("Beira of the Rotten Pack", 7),
                    ("Ghostly Voice", 3),
                    ("The Rust King", 4),
                ],
            ),
        ];
        for (label, text, names) in [("log1", log1(), expected[0].1), ("red vale", red_vale(), expected[1].1)] {
            let (_, events) = run(text);
            let npc = of(&events, InfoType::Npc);
            let want: BTreeMap<&str, usize> = names.iter().copied().collect();
            assert_eq!(npc.len(), want.values().sum::<usize>(), "{label}: count");
            let mut npc_counts: BTreeMap<&str, usize> = BTreeMap::new();
            for event in npc {
                let Event::Npc { npc_name, .. } = event else { unreachable!() };
                *npc_counts.entry(npc_name.as_str()).or_insert(0) += 1;
            }
            assert_eq!(npc_counts, want, "{label}: names");
        }
    }

    /// `clientLog` › "Should have deaths".
    #[test]
    fn player_death_event_counts() {
        for (label, text, expected) in [("log1", log1(), 1), ("red vale", red_vale(), 1)] {
            let (_, events) = run(text);
            let deaths = of(&events, InfoType::PlayerDeath);
            assert_eq!(deaths.len(), expected, "{label}");
            for event in deaths {
                let Event::PlayerDeath { char_name } = event else { unreachable!() };
                assert!(char_name.to_lowercase().contains("kvan"), "{label}: {char_name}");
            }
        }
    }

    /// `clientLog` › "Should have passive tree add & removes".
    #[test]
    fn passive_tree_event_counts() {
        let cases = [("log1", log1(), 1, 0, 1), ("red vale", red_vale(), 6, 5, 1)];
        for (label, text, expected, added, removed) in cases {
            let (_, events) = run(text);
            let nodes = of(&events, InfoType::PassiveTree);
            assert_eq!(nodes.len(), expected, "{label}");
            let (mut add, mut remove) = (0, 0);
            for event in nodes {
                let Event::PassiveTree { allocate, node_id, .. } = event else { unreachable!() };
                assert!(node_id.chars().any(|c| c.is_ascii_digit()), "{label}: {node_id}");
                if *allocate {
                    add += 1;
                } else {
                    remove += 1;
                }
            }
            assert_eq!((add, remove), (added, removed), "{label}");
        }
    }

    /// `clientLog` › "Should have permanent bonuses".
    #[test]
    fn permanent_bonus_event_counts() {
        for (label, text, expected) in [("log1", log1(), 0), ("red vale", red_vale(), 1)] {
            let (_, events) = run(text);
            let bonuses = of(&events, InfoType::PermanentBonus);
            assert_eq!(bonuses.len(), expected, "{label}");
            for event in bonuses {
                let Event::PermanentBonus { permanent_bonus, char_name } = event else { unreachable!() };
                assert!(char_name.to_lowercase().contains("kvan"), "{label}: {char_name}");
                assert!(permanent_bonus.chars().any(|c| c.is_ascii_digit()), "{label}: {permanent_bonus}");
            }
        }
    }

    /// `clientLog` › "Should have skill points".
    #[test]
    fn skill_point_event_counts() {
        for (label, text, expected) in [("log1", log1(), 2), ("red vale", red_vale(), 0)] {
            let (_, events) = run(text);
            let points = of(&events, InfoType::SkillPoint);
            assert_eq!(points.len(), expected, "{label}");
            for event in points {
                let Event::SkillPoint { points, point_type } = event else { unreachable!() };
                assert_eq!(*points, 2.0, "{label}");
                assert!(matches!(point_type, PointType::Passive | PointType::WeaponSet), "{label}");
            }
        }
    }

    /// `clientLog` › "Should have source sets".
    #[test]
    fn map_nav_event_counts() {
        let cases = [("log1", log1(), 4, 2, 0), ("red vale", red_vale(), 42, 19, 5)];
        for (label, text, expected, nulls, unknowns) in cases {
            let (_, events) = run(text);
            let sets = of(&events, InfoType::MapNav);
            assert_eq!(sets.len(), expected, "{label}");
            let (mut null_count, mut unknown_count) = (0, 0);
            for event in sets {
                let Event::MapNav { map_name } = event else { unreachable!() };
                match map_name.as_str() {
                    "(null)" => null_count += 1,
                    "(unknown)" => unknown_count += 1,
                    _ => {}
                }
            }
            assert_eq!((null_count, unknown_count), (nulls, unknowns), "{label}");
        }
    }

    /// `clientLog` › "Should have afk".
    #[test]
    fn afk_event_counts() {
        for (label, text, expected) in [("log1", log1(), 0), ("red vale", red_vale(), 2)] {
            let (_, events) = run(text);
            let afks = of(&events, InfoType::Afk);
            assert_eq!(afks.len(), expected, "{label}");
            if expected >= 2 {
                let wants = [true, false];
                for (event, want) in afks.iter().zip(wants) {
                    let Event::Afk { is_afk } = event else { unreachable!() };
                    assert_eq!(*is_afk, want, "{label}");
                }
            }
        }
    }

    /// `LogRegex` is anchored and its source column is lowercase hex only, so
    /// anything else is not a log line.
    #[test]
    fn only_whole_log_lines_are_read() {
        let mut log = ClientLog::new(data(), 0);
        for line in [
            "",
            "not a log line",
            "2025/09/11 18:21:43 ***** LOG FILE OPENING *****",
            "2025/09/11 18:21:43 167690796 F0C29DD2 [INFO Client 438888] [STARTUP] Game Start",
            "2026/04/26 09:28:47 1785270812 44e52699 [CMD][Unrecognized] --nopatch",
        ] {
            assert!(log.handle_line(line).is_none(), "{line}");
        }
        // A plain log line is inside the format but is not forwarded.
        assert!(
            log.handle_line("2025/09/11 18:21:43 167690796 f0c29dd2 [INFO Client 438888] Doodad hash: 1705339803")
                .is_none()
        );
    }

    /// `client-log.ts` › `updateRefsFromData`: towns and hideouts are ignored for
    /// level tracking, the Riverbank resets to level 1, and the game start and
    /// parse version are tracked.
    #[test]
    fn state_follows_the_log() {
        let mut log = ClientLog::new(data(), 0);
        assert_eq!((log.zone_name.as_str(), log.area_level, log.player_level), ("G1_1", 1.0, 1.0));

        let line = |time: &str, millis: u32, text: &str| {
            format!("2026/04/26 {time} {millis} a1b2c3d4 [INFO Client 443804] {text}")
        };

        let start = log.handle_line(&line("09:28:48", 1_785_271_828, "[STARTUP] Game Start")).unwrap();
        assert_eq!(start.data, Event::GameStart);
        assert_eq!(log.game_start_millis(), 1_785_271_828.0);

        // A town is ignored, a real zone is tracked.
        log.handle_line(&line("09:28:58", 1_785_282_093, r#"Generating level 15 area "G1_town" with seed 1"#));
        assert_eq!((log.zone_name.as_str(), log.area_level), ("G1_1", 1.0));
        log.handle_line(&line("09:50:55", 1_786_598_671, r#"Generating level 4 area "G1_4" with seed 3690647437"#));
        assert_eq!((log.zone_name.as_str(), log.area_level), ("g1_4", 4.0));

        log.handle_line(&line("09:55:16", 1_786_860_015, ": SomeChar (Mercenary) is now level 4"));
        assert_eq!((log.player_level, log.last_character()), (4.0, "SomeChar"));

        // The Riverbank can only be a new character.
        log.handle_line(&line("10:00:21", 1_787_165_373, r#"Generating level 1 area "G1_1" with seed 2590316184"#));
        assert_eq!((log.player_level, log.zone_name.as_str()), (1.0, "g1_1"));

        // Hideouts are ignored too, whatever their level.
        log.handle_line(&line("10:01:00", 1_787_165_374, r#"Generating level 80 area "HideoutMine" with seed 2"#));
        assert_eq!((log.zone_name.as_str(), log.area_level), ("g1_1", 1.0));

        log.handle_line(&line(
            "10:02:00",
            1_787_165_375,
            "[HTTP2] User agent: PoE poe2_production/tags/4.4.0j Windows x64",
        ));
        assert_eq!(log.log_parse_version(), 0);

        // `setPlayerLevel` ignores the settings widget's empty input.
        log.set_player_level(Some(42.0));
        assert_eq!(log.player_level, 42.0);
        log.set_player_level(None);
        assert_eq!(log.player_level, 42.0);

        log.reset_game_start_millis();
        assert_eq!(log.game_start_millis(), 0.0);
    }

    /// The events serialise in the `@ipc/types` shape, and each variant's `type`
    /// is its `ClientLogInfoType` string.
    #[test]
    fn events_serialise_with_the_ipc_shape() {
        let event = ClientLogEvent {
            ts: 1_757_614_903_000.0,
            ms: 167_690_843.0,
            data: Event::LoadZone { zone: "g1_4".to_owned(), area_level: 4.0, seed: 9.0 },
        };
        assert_eq!(
            js::json_value(&event),
            serde_json::json!({
                "ts": 1_757_614_903_000i64,
                "ms": 167_690_843,
                "type": "load-zone",
                "zone": "g1_4",
                "areaLevel": 4,
                "seed": 9,
            })
        );

        let variants: [(Event, InfoType); 14] = [
            (Event::Log, InfoType::Log),
            (Event::GameStart, InfoType::GameStart),
            (Event::Login, InfoType::Login),
            (Event::LoadZone { zone: "g1_1".into(), area_level: 1.0, seed: 2.0 }, InfoType::LoadZone),
            (Event::LevelUp { char_name: "A".into(), char_class: "B".into(), level: 3.0 }, InfoType::LevelUp),
            (Event::GameVersion { version: "4.4.0j".into() }, InfoType::GameVersion),
            (Event::AltTab { game_focused: true }, InfoType::AltTab),
            (Event::Npc { npc_name: "Renly".into(), message: "hi".into() }, InfoType::Npc),
            (Event::PlayerDeath { char_name: ": A".into() }, InfoType::PlayerDeath),
            (Event::PassiveTree { allocate: true, node_id: "a1".into(), node_name: "B".into() }, InfoType::PassiveTree),
            (Event::PermanentBonus { permanent_bonus: "+10%".into(), char_name: "A".into() }, InfoType::PermanentBonus),
            (Event::SkillPoint { points: 1.0, point_type: PointType::WeaponSet }, InfoType::SkillPoint),
            (Event::MapNav { map_name: "Act 1".into() }, InfoType::MapNav),
            (Event::Afk { is_afk: false }, InfoType::Afk),
        ];
        for (event, info) in variants {
            let data = serde_json::to_value(&event).unwrap();
            assert_eq!(data["type"], serde_json::to_value(info).unwrap());
            assert_eq!(event.info_type(), info);
        }
    }

    /// `Date.parse` reads the log's date in the machine's zone; the offset is
    /// explicit here.
    #[test]
    fn timestamps_apply_the_utc_offset() {
        let line = "2025/09/11 18:21:43 167690843 775aed1e [INFO Client 438888] [SCENE] Set Source [(null)]";
        let ts = |offset: i32| {
            let mut log = ClientLog::new(data(), offset);
            log.handle_line(line).unwrap().ts
        };
        // 2025-09-11T18:21:43Z
        assert_eq!(ts(0), 1_757_614_903_000.0);
        // Two hours east: the same wall clock is 16:21:43Z.
        assert_eq!(ts(7_200), 1_757_607_703_000.0);
        // Five hours west: 23:21:43Z.
        assert_eq!(ts(-18_000), 1_757_632_903_000.0);
    }

    /// `parseClientLogText` is callable on its own and leaves the tracked state
    /// alone (only `handleLine` calls `updateRefsFromData`).
    #[test]
    fn a_message_parses_without_touching_the_state() {
        let log = ClientLog::new(data(), 0);
        let event = log.parse_client_log_text(
            r#"Generating level 15 area "G1_town" with seed 1"#,
            "2026/04/26 09:28:53",
            1_785_282_093.0,
            0,
        );
        assert_eq!(event.data, Event::LoadZone { zone: "g1_town".to_owned(), area_level: 15.0, seed: 1.0 });
        assert_eq!((event.ts, event.ms), (1_777_195_733_000.0, 1_785_282_093.0));
        assert_eq!((log.zone_name.as_str(), log.area_level), ("G1_1", 1.0));
    }

    /// `getClientLogParseVersion` returns 0 for every version at this revision.
    #[test]
    fn the_parse_version_is_zero() {
        for version in ["", "0.1.0", "0.4.0j", "4.4.0j", "99.99.99z"] {
            assert_eq!(get_client_log_parse_version(version), 0);
        }
    }

    /// Detail checks the event counts cannot see: skill-point kinds, the colon
    /// kept in a death's name, and NPC names that are not in the list.
    #[test]
    fn events_carry_the_parsed_fields() {
        let line = |text: &str| {
            let mut log = ClientLog::new(data(), 0);
            log.handle_line(&format!("2026/04/26 09:29:16 168144002 f0c29dd3 [INFO Client 438888] {text}"))
                .map(|event| event.data)
        };

        assert_eq!(
            line(": You have received an Atlas Skill Point."),
            Some(Event::SkillPoint { points: 1.0, point_type: PointType::Atlas })
        );
        assert_eq!(
            line(": You have received 2 Map Boss Atlas Skill Points."),
            Some(Event::SkillPoint { points: 2.0, point_type: PointType::MapBoss })
        );
        assert_eq!(
            line(": You have received 3 Weapon Set Passive Skill Points."),
            Some(Event::SkillPoint { points: 3.0, point_type: PointType::WeaponSet })
        );
        assert_eq!(line(": Kvan has been slain."), Some(Event::PlayerDeath { char_name: ": Kvan".to_owned() }));
        assert_eq!(line("Somebody Else: not a known npc"), None, "unknown speakers are plain log lines");
        assert_eq!(
            line("Renly: Take a look."),
            Some(Event::Npc { npc_name: "Renly".to_owned(), message: "Take a look.".to_owned() })
        );
        assert_eq!(
            line("Successfully unallocated passive skill id: armour_break37, name: Cruel Methods"),
            Some(Event::PassiveTree {
                allocate: false,
                node_id: "armour_break37".to_owned(),
                node_name: "Cruel Methods".to_owned(),
            })
        );
        assert_eq!(line("[WINDOW] Lost focus"), Some(Event::AltTab { game_focused: false }));
        assert_eq!(line("[JOB] Start"), None, "other namespaces are dropped");
    }

    // `describe("local performance tests")` in the spec is `it.skip`ped there
    // too: both cases read the author's own `Client.txt` from a Windows-only
    // absolute path and only time `split`/regex over it, so there is nothing to
    // port.
}
