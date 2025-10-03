use anyhow::{anyhow, Result};
use regex::Regex;
use reqwest::{Client, Url};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub enum Resolved {
    Pub { aid: u64, cid: u64, bvid: String, title: String },
    Pgc { 
        ep_id: u64, 
        season_id: u64, 
        aid: u64, 
        cid: u64, 
        title: String,
        season_title: String,
        evaluate: String,
        episodes: Vec<EpisodeInfo>,
    },
}

#[derive(Debug, Deserialize)]
pub struct BangumiResult<T> { pub result: T }

#[derive(Debug, Deserialize, Clone)]
pub struct EpisodeInfo { 
    pub id: Option<u64>, 
    pub ep_id: Option<u64>, 
    pub aid: Option<u64>, 
    #[allow(dead_code)]
    pub bvid: Option<String>, 
    pub cid: Option<u64>, 
    pub title: Option<String>,
    #[serde(default)]
    pub long_title: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub share_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BangumiSeasonInfo { 
    pub season_id: u64, 
    pub season_title: String, 
    #[serde(default)]
    pub evaluate: String,
    pub episodes: Vec<EpisodeInfo>,
}

pub async fn fetch_bangumi_info(client: &Client, id: &str) -> Result<BangumiSeasonInfo> {
    let id_lower = id.to_lowercase();
    let re = Regex::new(r"(?i)(ss|ep|md)(\d+)")?;
    let (endpoint, params): (&str, Vec<(&str, String)>) = if let Some(caps) = re.captures(&id_lower) {
        let kind = caps.get(1).unwrap().as_str();
        let num = caps.get(2).unwrap().as_str().to_string();
        match kind {
            "ss" => ("https://api.bilibili.com/pgc/view/web/season", vec![("season_id", num)]),
            "ep" => ("https://api.bilibili.com/pgc/view/web/season", vec![("ep_id", num)]),
            "md" => {
                let map_resp = client.get("https://api.bilibili.com/pgc/review/user").query(&[("media_id", num.clone())]).send().await?.error_for_status()?;
                let v: serde_json::Value = map_resp.json().await?;
                let sid = v["result"]["media"]["season_id"].as_u64().ok_or_else(|| anyhow!("md 转 season 失败"))?;
                ("https://api.bilibili.com/pgc/view/web/season", vec![("season_id", sid.to_string())])
            }
            _ => unreachable!(),
        }
    } else if id_lower.starts_with("http") {
        let url = Url::parse(id)?;
        let path = url.path();
        let caps = Regex::new(r"/(ss|ep)(\d+)")?.captures(path).ok_or_else(|| anyhow!("无法从 URL 解析番剧 ID"))?;
        let kind = caps.get(1).unwrap().as_str();
        let num = caps.get(2).unwrap().as_str().to_string();
        ("https://api.bilibili.com/pgc/view/web/season", vec![(if kind=="ss" {"season_id"} else {"ep_id"}, num)])
    } else {
        return Err(anyhow!("不支持的番剧标识: {id}"));
    };

    let resp = client.get(endpoint).query(&params).send().await?.error_for_status()?;
    let json: BangumiResult<serde_json::Value> = resp.json().await?;
    let result = json.result;
    let season_id = result["season_id"].as_u64().unwrap_or_default();
    let season_title = result["season_title"].as_str().unwrap_or("").to_string();
    let evaluate = result["evaluate"].as_str().unwrap_or("").to_string();
    let episodes: Vec<EpisodeInfo> = result["episodes"].as_array().unwrap_or(&vec![]).iter().map(|e| EpisodeInfo{
        id: e["id"].as_u64(), 
        ep_id: e["ep_id"].as_u64(), 
        aid: e["aid"].as_u64(), 
        bvid: e["bvid"].as_str().map(|s| s.to_string()), 
        cid: e["cid"].as_u64(), 
        title: e["title"].as_str().map(|s| s.to_string()),
        long_title: e["long_title"].as_str().map(|s| s.to_string()),
        share_url: e["share_url"].as_str().map(|s| s.to_string()),
    }).collect();
    Ok(BangumiSeasonInfo{ season_id, season_title, evaluate, episodes })
}

#[derive(Debug, Deserialize)]
struct UgcViewPage { cid: u64, _part: Option<String>, _duration: Option<u64> }
#[derive(Debug, Deserialize)]
struct UgcViewData { aid: u64, bvid: String, cid: Option<u64>, title: String, pages: Option<Vec<UgcViewPage>> }
#[derive(Debug, Deserialize)]
struct UgcViewResp { code: i32, data: UgcViewData }

async fn fetch_ugc_view(client: &Client, id: &str, page: Option<usize>) -> Result<UgcViewData> {
    let mut url = Url::parse("https://api.bilibili.com/x/web-interface/view")?;
    if id.to_lowercase().starts_with("av") {
        let num = id[2..].parse::<u64>().unwrap_or_default();
        url.query_pairs_mut().append_pair("aid", &num.to_string());
    } else { url.query_pairs_mut().append_pair("bvid", id); }
    let resp = client.get(url).send().await?.error_for_status()?;
    let v: UgcViewResp = resp.json().await?;
    if v.code != 0 { return Err(anyhow!("view 返回失败 code={}", v.code)); }
    let mut data = v.data;
    if let Some(pages) = &data.pages { let idx = page.unwrap_or(1).saturating_sub(1); data.cid = pages.get(idx).map(|p| p.cid).or_else(|| pages.first().map(|p| p.cid)); }
    Ok(data)
}

pub async fn resolve_input(client: &Client, input: &str) -> Result<Resolved> {
    resolve_input_with_ep(client, input, None).await
}

pub async fn resolve_input_with_ep(client: &Client, input: &str, ep_index: Option<usize>) -> Result<Resolved> {
    let raw = input.trim();
    let re_id = Regex::new(r"(?i)^(BV[0-9A-Za-z]{10}|av\d+|ep\d+|ss\d+|md\d+)$")?;
    let (maybe_id, maybe_url) = if re_id.is_match(raw) { (Some(raw.to_string()), None) } else { (None, Some(raw.to_string())) };
    if let Some(id) = maybe_id { return resolve_by_id(client, &id, None, ep_index).await; }
    let url = Url::parse(maybe_url.as_ref().unwrap())?;
    let path = url.path().to_lowercase();
    if path.contains("/video/") {
        let caps = Regex::new(r"/video/(bv[0-9a-z]{10})")?.captures(&path).ok_or_else(|| anyhow!("无法从 URL 解析 BV"))?;
        let bvid = caps.get(1).unwrap().as_str().to_uppercase();
        let p = url.query_pairs().find(|(k, _)| k == "p").and_then(|(_, v)| v.parse::<usize>().ok());
        return resolve_by_id(client, &bvid, p, ep_index).await;
    }
    if path.contains("/bangumi/play/") {
        if let Some(caps) = Regex::new(r"/(ep|ss)(\d+)")?.captures(&path) {
            let id = format!("{}{}", &caps[1], &caps[2]);
            return resolve_by_id(client, &id, None, ep_index).await;
        }
    }
    Err(anyhow!("不支持的输入: {raw}"))
}

async fn resolve_by_id(client: &Client, id: &str, page: Option<usize>, ep_index: Option<usize>) -> Result<Resolved> {
    let id_lower = id.to_lowercase();
    if id_lower.starts_with("bv") || id_lower.starts_with("av") {
        let data = fetch_ugc_view(client, id, page).await?;
        let aid = data.aid; let bvid = data.bvid; let cid = data.cid.ok_or_else(|| anyhow!("未找到 cid"))?;
        return Ok(Resolved::Pub { aid, cid, bvid, title: data.title });
    }
    if id_lower.starts_with("ep") || id_lower.starts_with("ss") || id_lower.starts_with("md") {
        let season = fetch_bangumi_info(client, id).await?;
        
        // 选择目标集数
        let target = if id_lower.starts_with("ep") {
            // 如果是 ep 开头，找到对应的集
            let n: u64 = id_lower[2..].parse().unwrap_or_default();
            season.episodes.iter().find(|e| e.ep_id == Some(n) || e.id == Some(n)).cloned()
        } else if let Some(idx) = ep_index {
            // 如果指定了集数索引
            season.episodes.get(idx).cloned()
        } else {
            // 返回包含所有集数信息的结果，不选择具体集数
            return Ok(Resolved::Pgc {
                ep_id: 0,
                season_id: season.season_id,
                aid: 0,
                cid: 0,
                title: String::new(),
                season_title: season.season_title,
                evaluate: season.evaluate,
                episodes: season.episodes,
            });
        };
        
        let ep = target.ok_or_else(|| anyhow!("未找到目标剧集"))?;
        let aid = ep.aid.ok_or_else(|| anyhow!("剧集缺少 aid"))?; 
        let cid = ep.cid.ok_or_else(|| anyhow!("剧集缺少 cid"))?;
        let title = ep.long_title.clone()
            .or_else(|| ep.title.clone())
            .unwrap_or_else(|| season.season_title.clone());
        
        return Ok(Resolved::Pgc { 
            ep_id: ep.ep_id.unwrap_or(0), 
            season_id: season.season_id, 
            aid, 
            cid, 
            title,
            season_title: season.season_title,
            evaluate: season.evaluate,
            episodes: season.episodes,
        });
    }
    Err(anyhow!("无法解析输入: {id}"))
}
