//! Item art: the game's CDN, which the reference renderers load directly.
//!
//! Upstream's `<img>` tags point at two places. Small UI art (currency, stash, dust, oils'
//! and influences' badges) ships in `renderer/public/images/` and is served by the page.
//! An item's own art is a [`web.poecdn.com`](CDN_HOST) URL that comes from the game data
//! (`items.ndjson`) and from trade listings, and the browser fetches it itself: it never
//! goes through the trade proxy, whose `ALLOWED_HOSTS` stay the pathofexile/poe.ninja
//! hosts. [`crate::trade::api::ALLOWED_HOSTS`] therefore does not contain it; the app's
//! transport allows it next to the update host.

/// The host the game's item art lives on (`web.poecdn.com`).
pub const CDN_HOST: &str = "web.poecdn.com";

/// Whether `url` is an item image on [`CDN_HOST`]: `https://web.poecdn.com/<path>`.
///
/// The app only ever loads images it recognises this way, whatever a data file or a trade
/// answer claims, so a stray URL cannot make it talk to another host.
pub fn is_cdn_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    match rest.split_once('/') {
        Some((host, path)) => host == CDN_HOST && !path.is_empty(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_cdn_is_an_item_image() {
        assert!(is_cdn_url(
            "https://web.poecdn.com/gen/image/WzI1LDE0LHsiZiI6IjJESXRlbXMvUmluZ3MvTGFrZVJpbmcxIn1d/40b81baee3/LakeRing1.png"
        ));
        assert!(!is_cdn_url("http://web.poecdn.com/gen/image/x.png"), "https only");
        assert!(!is_cdn_url("https://web.poecdn.com"), "no path");
        assert!(!is_cdn_url("https://web.poecdn.com.evil.test/x.png"));
        assert!(!is_cdn_url("https://www.pathofexile.com/x.png"));
        assert!(!is_cdn_url("/images/chaos.png"));
    }
}
