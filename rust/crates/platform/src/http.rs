//! The [`Transport`] the app sends requests with: `main/src/proxy.ts` without
//! the proxy. Every request carries [`USER_AGENT`]; hosts are the trade proxy's
//! [`ALLOWED_HOSTS`] plus the update check's [`UPDATE_HOST`] and the item art's
//! [`CDN_HOST`]. Non-2xx answers come back as responses (the trade site's errors
//! are JSON bodies), and the body is decoded (gzip) like `net.request` does.
//!
//! Item art is binary, so it goes through [`HttpTransport::get_bytes`] rather than
//! [`Transport::send`], whose body is the decoded text the trade answers are.

use std::time::Duration;

use ureq::Agent;
use wraex_core::images::CDN_HOST;
use wraex_core::trade::api::{ALLOWED_HOSTS, HttpRequest, HttpResponse, Method, Transport, USER_AGENT};
use wraex_core::updates::UPDATE_HOST;

/// Blocking HTTPS. Call it from a worker thread, not the UI thread.
#[derive(Debug, Clone)]
pub struct HttpTransport {
    agent: Agent,
}

impl Default for HttpTransport {
    fn default() -> Self {
        let config = Agent::config_builder()
            .http_status_as_error(false)
            .user_agent(USER_AGENT)
            .timeout_global(Some(Duration::from_secs(30)))
            .build();
        Self { agent: config.into() }
    }
}

/// The host of an `https://` URL.
fn host(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("https://")?;
    Some(rest.split(['/', '?']).next().unwrap_or(rest))
}

/// [`ALLOWED_HOSTS`] plus [`UPDATE_HOST`] and [`CDN_HOST`].
fn allowed(host: &str) -> bool {
    ALLOWED_HOSTS.contains(&host) || host == UPDATE_HOST || host == CDN_HOST
}

impl HttpTransport {
    /// A `GET` whose body is bytes, for an image: same agent, same [`USER_AGENT`], same
    /// host allowlist as [`Transport::send`]. Only a 2xx answer is returned; anything else
    /// is an error, because an error page is not a picture.
    pub fn get_bytes(&self, url: &str) -> Result<Vec<u8>, String> {
        // The page's hosts, the update check's, and the item CDN's.
        let host = host(url).ok_or_else(|| format!("not an https URL: {url}"))?;
        if !allowed(host) {
            return Err(format!("host not allowed: {host}"));
        }
        let mut response = self.agent.get(url).call().map_err(|e| e.to_string())?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("{status} from {url}"));
        }
        response.body_mut().read_to_vec().map_err(|e| e.to_string())
    }
}

impl Transport for HttpTransport {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
        // `PROXY_HOSTS`, plus the update check.
        let host = host(&request.url).ok_or_else(|| format!("not an https URL: {}", request.url))?;
        if !allowed(host) {
            return Err(format!("host not allowed: {host}"));
        }

        let result = match request.method {
            Method::Get => {
                let mut r = self.agent.get(&request.url);
                for (name, value) in &request.headers {
                    r = r.header(*name, value);
                }
                r.call()
            }
            Method::Post => {
                let mut r = self.agent.post(&request.url);
                for (name, value) in &request.headers {
                    r = r.header(*name, value);
                }
                r.send(request.body.as_deref().unwrap_or(""))
            }
        };
        let mut response = result.map_err(|e| e.to_string())?;

        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| Some((name.as_str().to_ascii_lowercase(), value.to_str().ok()?.to_owned())))
            .collect();
        let body = response.body_mut().read_to_string().map_err(|e| e.to_string())?;
        Ok(HttpResponse { status, headers, body })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_proxy_hosts_the_update_host_and_the_cdn_are_reached() {
        assert_eq!(host("https://www.pathofexile.com/api/trade/search/x"), Some("www.pathofexile.com"));
        assert_eq!(host("https://poe.ninja?x"), Some("poe.ninja"));
        assert_eq!(host("https://api.github.com/repos/Dumspy/wraex/releases/latest"), Some("api.github.com"));
        assert_eq!(host("https://web.poecdn.com/gen/image/x.png"), Some("web.poecdn.com"));
        assert!(allowed("www.pathofexile.com"));
        assert!(allowed(UPDATE_HOST));
        assert!(allowed(CDN_HOST));
        assert!(!allowed("github.com"));
        assert!(!allowed("cdn.poecdn.com"));
        let request =
            HttpRequest { method: Method::Get, url: "https://example.com/".into(), headers: Vec::new(), body: None };
        assert_eq!(HttpTransport::default().send(&request).unwrap_err(), "host not allowed: example.com");
        assert_eq!(
            HttpTransport::default().get_bytes("https://example.com/x.png").unwrap_err(),
            "host not allowed: example.com"
        );
    }

    /// The one place a real image is fetched: run it by hand (`cargo test -p
    /// wraex-platform -- --ignored live_cdn`), since CI has no network. `art.rs`'s replay
    /// path, which every screenshot and test uses, reads the recordings instead.
    #[test]
    #[ignore = "needs the network"]
    fn live_cdn_fetch_returns_the_image_bytes() {
        let bytes = HttpTransport::default()
            .get_bytes(
                "https://web.poecdn.com/gen/image/WzI1LDE0LHsiZiI6IjJESXRlbXMvQ3VycmVuY3kvQ3VycmVuY3lNb2RWYWx1ZXMiLCJzY2FsZSI6MX1d/ec48896769/CurrencyModValues.png",
            )
            .expect("the divine orb's art");
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "a PNG, not an error page");
    }
}
