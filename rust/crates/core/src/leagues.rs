//! `web/background/Leagues.ts`: the trade leagues and which one is selected.
//!
//! PoE1 asks `/api/leagues?type=main&realm=pc` and drops hardcore, SSF and
//! (non-event) ruthless leagues; PoE2 asks the trade site's
//! `/api/trade2/data/leagues` and keeps every league. When the configured
//! league is gone (and not private), both fall back to the temporary challenge
//! league, PoE1 at index 1, PoE2 at index 2, else the first.

use serde::Deserialize;

use crate::trade::api::{HttpRequest, HttpResponse, Method, Transport};

/// `PERMANENT_SC` (pc-ggg, pc-garena)
pub const PERMANENT_SC: [&str; 2] = ["Standard", "標準模式"];
/// `PERMANENT_HC`
pub const PERMANENT_HC: [&str; 2] = ["Hardcore", "專家模式"];

/// A league the trade site lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct League {
    pub id: String,
    pub is_popular: bool,
    /// The trade site's display name (PoE2 only).
    pub text: Option<String>,
}

/// The selected league (`selected`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedLeague {
    pub id: String,
    pub realm: String,
    pub is_popular: bool,
}

/// Which game's leagues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeagueSource {
    /// `/api/leagues?type=main&realm=pc`
    Poe1,
    /// `/api/trade2/data/leagues`
    Poe2,
}

impl LeagueSource {
    pub fn request(self, host: &str) -> HttpRequest {
        let path = match self {
            LeagueSource::Poe1 => "api/leagues?type=main&realm=pc",
            LeagueSource::Poe2 => "api/trade2/data/leagues",
        };
        HttpRequest { method: Method::Get, url: format!("https://{host}/{path}"), headers: Vec::new(), body: None }
    }

    /// `TMP_CHALLENGE`: the temporary challenge league's place in the list.
    fn challenge_index(self) -> usize {
        match self {
            LeagueSource::Poe1 => 1,
            LeagueSource::Poe2 => 2,
        }
    }

    /// The leagues in a response body.
    pub fn parse(self, body: &str) -> Result<Vec<League>, String> {
        match self {
            LeagueSource::Poe1 => {
                #[derive(Deserialize)]
                struct Rule {
                    id: String,
                }
                #[derive(Deserialize)]
                struct ApiLeague {
                    id: String,
                    #[serde(default)]
                    event: Option<bool>,
                    #[serde(default)]
                    rules: Vec<Rule>,
                }
                let leagues: Vec<ApiLeague> = serde_json::from_str(body).map_err(|e| e.to_string())?;
                Ok(leagues
                    .into_iter()
                    .filter(|l| {
                        !PERMANENT_HC.contains(&l.id.as_str())
                            && !l
                                .rules
                                .iter()
                                .any(|r| r.id == "NoParties" || (r.id == "HardMode" && l.event != Some(true)))
                    })
                    .map(|l| League { id: l.id, is_popular: true, text: None })
                    .collect())
            }
            LeagueSource::Poe2 => {
                #[derive(Deserialize)]
                struct TradeLeague {
                    id: String,
                    text: String,
                }
                #[derive(Deserialize)]
                struct Leagues {
                    result: Vec<TradeLeague>,
                }
                let leagues: Leagues = serde_json::from_str(body).map_err(|e| e.to_string())?;
                Ok(leagues
                    .result
                    .into_iter()
                    .map(|l| League { id: l.id, is_popular: true, text: Some(l.text) })
                    .collect())
            }
        }
    }
}

/// `isPrivateLeague`: ruthless and `(PL123)` leagues.
pub fn is_private_league(id: &str) -> bool {
    if id.contains("Ruthless") {
        return true;
    }
    // `/\(PL\d+\)$/`
    id.strip_suffix(')')
        .and_then(|rest| rest.rsplit_once("(PL"))
        .is_some_and(|(_, digits)| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

/// The trade leagues, and the configured league kept valid (`useLeagues`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Leagues {
    pub list: Vec<League>,
    pub error: Option<String>,
}

impl Leagues {
    /// `selectedId`: the configured league, once the list has loaded.
    pub fn selected_id<'a>(&self, league_id: Option<&'a str>) -> Option<&'a str> {
        if self.list.is_empty() { None } else { league_id }
    }

    /// `selected`
    pub fn selected(&self, league_id: Option<&str>, realm: &str) -> Option<SelectedLeague> {
        let id = league_id?;
        let listed = self.list.iter().find(|l| l.id == id);
        Some(SelectedLeague {
            id: id.to_owned(),
            realm: realm.to_owned(),
            is_popular: !is_private_league(id) && listed.is_some_and(|l| l.is_popular),
        })
    }

    /// `load`: fetches the list. Returns the league to select instead of the
    /// configured `league_id` when that one is gone; failures land in `error`.
    pub fn load(
        &mut self,
        transport: &dyn Transport,
        source: LeagueSource,
        host: &str,
        league_id: Option<&str>,
    ) -> Option<String> {
        self.error = None;
        match Self::fetch(transport, source, host) {
            Ok(list) => {
                self.list = list;
                match self.fallback(source, league_id) {
                    Ok(new_id) => new_id,
                    Err(e) => {
                        self.error = Some(e);
                        None
                    }
                }
            }
            Err(e) => {
                self.error = Some(e);
                None
            }
        }
    }

    fn fetch(transport: &dyn Transport, source: LeagueSource, host: &str) -> Result<Vec<League>, String> {
        let response: HttpResponse = transport.send(&source.request(host))?;
        if !(200..300).contains(&response.status) {
            // `JSON.stringify(Object.fromEntries(response.headers))`
            let mut headers: Vec<(&String, &String)> = response.headers.iter().collect();
            headers.sort();
            let map: serde_json::Map<String, serde_json::Value> =
                headers.into_iter().map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone()))).collect();
            return Err(serde_json::Value::Object(map).to_string());
        }
        source.parse(&response.body)
    }

    /// The league to switch to when the configured one is no longer listed.
    fn fallback(&self, source: LeagueSource, league_id: Option<&str>) -> Result<Option<String>, String> {
        let selected = self.selected_id(league_id);
        let alive = self.list.iter().any(|l| Some(l.id.as_str()) == selected);
        if alive || is_private_league(selected.unwrap_or("")) {
            return Ok(None);
        }
        let index = if self.list.len() > source.challenge_index() { source.challenge_index() } else { 0 };
        // `tradeLeagues.value[0].id` throws on an empty list.
        let league = self.list.get(index).ok_or("Cannot read properties of undefined (reading 'id')")?;
        Ok(Some(league.id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(HttpResponse);

    impl Transport for Fixed {
        fn send(&self, _: &HttpRequest) -> Result<HttpResponse, String> {
            Ok(self.0.clone())
        }
    }

    fn ok(body: &str) -> Fixed {
        Fixed(HttpResponse { status: 200, body: body.to_owned(), ..Default::default() })
    }

    const POE1: &str = r#"[
        {"id":"Standard","rules":[]},
        {"id":"Hardcore","rules":[{"id":"Hardcore"}]},
        {"id":"Settlers","rules":[]},
        {"id":"SSF Settlers","rules":[{"id":"NoParties"}]},
        {"id":"Ruthless Settlers","rules":[{"id":"HardMode"}]},
        {"id":"Event Ruthless","event":true,"rules":[{"id":"HardMode"}]}
    ]"#;

    #[test]
    fn poe1_drops_hardcore_ssf_and_ruthless() {
        let ids: Vec<String> = LeagueSource::Poe1.parse(POE1).unwrap().into_iter().map(|l| l.id).collect();
        assert_eq!(ids, ["Standard", "Settlers", "Event Ruthless"]);
    }

    #[test]
    fn a_gone_league_falls_back_to_the_challenge_league() {
        let mut leagues = Leagues::default();
        assert_eq!(leagues.load(&ok(POE1), LeagueSource::Poe1, "x", Some("Old League")), Some("Settlers".into()));
        assert_eq!(leagues.load(&ok(POE1), LeagueSource::Poe1, "x", Some("Standard")), None);
        // Private leagues stay.
        assert_eq!(leagues.load(&ok(POE1), LeagueSource::Poe1, "x", Some("My League (PL1234)")), None);
        assert!(leagues.selected(Some("Settlers"), "pc-ggg").unwrap().is_popular);
        assert!(!leagues.selected(Some("My League (PL1234)"), "pc-ggg").unwrap().is_popular);
    }

    #[test]
    fn poe2_keeps_every_trade_league_and_picks_the_third() {
        let body = r#"{"result":[{"id":"Standard","text":"Standard"},{"id":"Hardcore","text":"Hardcore"},
            {"id":"Dawn","text":"Dawn of the Hunt"},{"id":"HC Dawn","text":"Hardcore Dawn of the Hunt"}]}"#;
        let mut leagues = Leagues::default();
        assert_eq!(leagues.load(&ok(body), LeagueSource::Poe2, "x", None), Some("Dawn".into()));
        assert_eq!(leagues.list[2].text.as_deref(), Some("Dawn of the Hunt"));
    }

    #[test]
    fn failures_are_kept_as_errors() {
        let mut leagues = Leagues::default();
        let mut response = HttpResponse { status: 503, ..Default::default() };
        response.headers.insert("retry-after".into(), "60".into());
        assert_eq!(leagues.load(&Fixed(response), LeagueSource::Poe1, "x", None), None);
        assert_eq!(leagues.error.as_deref(), Some(r#"{"retry-after":"60"}"#));
        assert!(leagues.load(&ok("[]"), LeagueSource::Poe1, "x", None).is_none());
        assert!(leagues.error.is_some());
    }

    #[test]
    fn private_leagues() {
        assert!(is_private_league("Ruthless"));
        assert!(is_private_league("Mine (PL42)"));
        assert!(!is_private_league("Mine (PL)"));
        assert!(!is_private_league("Settlers"));
    }
}
