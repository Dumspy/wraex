//! The [`Transport`] the app sends requests with: `main/src/proxy.ts` without
//! the proxy. Every request carries [`USER_AGENT`]; hosts are the trade proxy's
//! [`ALLOWED_HOSTS`] plus the update check's [`UPDATE_HOST`]. Non-2xx answers
//! come back as responses (the trade site's errors are JSON bodies), and the
//! body is decoded (gzip) like `net.request` does.

use std::time::Duration;

use ureq::Agent;
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

/// [`ALLOWED_HOSTS`] plus [`UPDATE_HOST`].
fn allowed(host: &str) -> bool {
    ALLOWED_HOSTS.contains(&host) || host == UPDATE_HOST
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
    fn only_the_proxy_hosts_and_the_update_host_are_reached() {
        assert_eq!(host("https://www.pathofexile.com/api/trade/search/x"), Some("www.pathofexile.com"));
        assert_eq!(host("https://poe.ninja?x"), Some("poe.ninja"));
        assert_eq!(host("https://api.github.com/repos/Dumspy/wraex/releases/latest"), Some("api.github.com"));
        assert!(allowed("www.pathofexile.com"));
        assert!(allowed(UPDATE_HOST));
        assert!(!allowed("github.com"));
        let request =
            HttpRequest { method: Method::Get, url: "https://example.com/".into(), headers: Vec::new(), body: None };
        assert_eq!(HttpTransport::default().send(&request).unwrap_err(), "host not allowed: example.com");
    }
}
