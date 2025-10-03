use reqwest::Client;
use crate::resolve::Resolved;

pub async fn fetch_cover_url(client: &Client, resolved: &Resolved) -> Option<String> {
    match resolved {
        Resolved::Pub { bvid, .. } => {
            // x/web-interface/view returns `pic`
            let mut url = reqwest::Url::parse("https://api.bilibili.com/x/web-interface/view").ok()?;
            url.query_pairs_mut().append_pair("bvid", bvid);
            let v: serde_json::Value = client.get(url).send().await.ok()?.json().await.ok()?;
            v.get("data").and_then(|d| d.get("pic")).and_then(|s| s.as_str()).map(|s| s.to_string())
        }
        Resolved::Pgc { ep_id, season_id, .. } => {
            // Prefer explicit episode cover API
            if *ep_id != 0 {
                let mut url = reqwest::Url::parse("https://api.bilibili.com/pgc/view/web/ep").ok()?;
                url.query_pairs_mut().append_pair("ep_id", &ep_id.to_string());
                let v: serde_json::Value = client.get(url).send().await.ok()?.json().await.ok()?;
                if let Some(data) = v.get("data") {
                    if let Some(sq) = data.get("square_cover").and_then(|s| s.as_str()) { return Some(sq.to_string()); }
                    if let Some(c) = data.get("cover").and_then(|s| s.as_str()) { return Some(c.to_string()); }
                }
            }
            // Fallback to season cover
            let mut url = reqwest::Url::parse("https://api.bilibili.com/pgc/view/web/season").ok()?;
            url.query_pairs_mut().append_pair("season_id", &season_id.to_string());
            let v: serde_json::Value = client.get(url).send().await.ok()?.json().await.ok()?;
            let root = v.get("result").or_else(|| v.get("data"));
            root.and_then(|r| r.get("cover").or_else(|| r.get("season_cover"))).and_then(|s| s.as_str()).map(|s| s.to_string())
        }
    }
}

pub async fn download_cover_bytes(client: &Client, url: &str) -> Option<Vec<u8>> {
    let resp = client
        .get(url)
        .header("Referer", "https://www.bilibili.com")
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36")
        .send().await.ok()?;
    resp.bytes().await.ok().map(|b| b.to_vec())
}
