use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::Deserialize;
use serde::de::{Deserialize as DeDeserialize, Deserializer, Error as DeError};

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct PlayurlResp {
    pub code: i32,
    pub data: PlayurlData,
}
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct PlayurlData {
    pub dash: Option<PlayurlDash>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PlayVideo {
    pub id: i32,
    pub base_url: String,
    pub backup_url: Option<Vec<String>>,
    pub codecid: Option<i32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub bandwidth: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PlayAudio {
    pub id: i32,
    pub base_url: String,
    pub backup_url: Option<Vec<String>>,
    pub bandwidth: Option<u64>,
    pub codecs: Option<String>,
}

// 中间结构：同时接收 camelCase 与 snake_case，避免 alias 导致的 duplicate field 错误
#[derive(Debug, Deserialize)]
struct PlayVideoWire {
    id: i32,
    #[serde(rename = "baseUrl")]
    base_url_camel: Option<String>,
    base_url: Option<String>,
    #[serde(rename = "backupUrl")]
    backup_url_camel: Option<Vec<String>>,
    backup_url: Option<Vec<String>>,
    #[serde(default)]
    codecid: Option<i32>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    bandwidth: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct PlayAudioWire {
    id: i32,
    #[serde(rename = "baseUrl")]
    base_url_camel: Option<String>,
    base_url: Option<String>,
    #[serde(rename = "backupUrl")]
    backup_url_camel: Option<Vec<String>>,
    backup_url: Option<Vec<String>>,
    #[serde(default)]
    bandwidth: Option<u64>,
    #[serde(default)]
    codecs: Option<String>,
}

impl<'de> DeDeserialize<'de> for PlayVideo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let w = PlayVideoWire::deserialize(deserializer)?;
        let base_url = w
            .base_url
            .or(w.base_url_camel)
            .ok_or_else(|| DeError::custom("missing baseUrl/base_url"))?;
        let backup_url = w.backup_url.or(w.backup_url_camel);
        Ok(PlayVideo {
            id: w.id,
            base_url,
            backup_url,
            codecid: w.codecid,
            width: w.width,
            height: w.height,
            bandwidth: w.bandwidth,
        })
    }
}

impl<'de> DeDeserialize<'de> for PlayAudio {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let w = PlayAudioWire::deserialize(deserializer)?;
        let base_url = w
            .base_url
            .or(w.base_url_camel)
            .ok_or_else(|| DeError::custom("missing baseUrl/base_url"))?;
        let backup_url = w.backup_url.or(w.backup_url_camel);
        Ok(PlayAudio {
            id: w.id,
            base_url,
            backup_url,
            bandwidth: w.bandwidth,
            codecs: w.codecs,
        })
    }
}
#[derive(Debug, Deserialize, Clone)]
pub struct Dolby {
    #[serde(default)]
    pub audio: Option<Vec<PlayAudio>>,
}
#[derive(Debug, Deserialize, Clone)]
pub struct Flac {
    pub audio: PlayAudio,
}
#[derive(Debug, Deserialize, Clone)]
pub struct PlayurlDash {
    #[serde(default)]
    pub video: Vec<PlayVideo>,
    #[serde(default)]
    pub audio: Vec<PlayAudio>,
    #[serde(default)]
    pub dolby: Option<Dolby>,
    #[serde(default)]
    pub flac: Option<Flac>,
}

#[allow(dead_code)]
pub async fn fetch_dash_ugc(
    client: &Client,
    aid: u64,
    cid: u64,
    use_wbi: bool,
) -> Result<PlayurlDash> {
    // 与 BiliTools 对齐：统一使用 WBI 接口并进行签名
    let base_url = "https://api.bilibili.com/x/player/wbi/playurl";

    // 参考 BiliTools: 登录用户使用 qn=127, fnval=4048; 未登录用户使用 qn=64, fnval=16
    let (qn, fnval, fourk) = if use_wbi {
        ("127", "4048", "1") // 登录：最高画质 + 完整功能（包括杜比、无损音频等）
    } else {
        ("64", "16", "0") // 未登录：服务器会按权限降级
    };

    let params = vec![
        ("avid", aid.to_string()),
        ("cid", cid.to_string()),
        ("qn", qn.to_string()),
        ("fnval", fnval.to_string()),
        ("fnver", "0".to_string()),
        ("fourk", fourk.to_string()),
    ];

    // 始终进行 WBI 签名
    let wbi_params: Vec<(&str, String)> = params.iter().map(|(k, v)| (*k, v.clone())).collect();
    let query = crate::wbi::sign_wbi(client, &wbi_params).await?;
    let url = format!("{}?{}", base_url, query);
    log::debug!(
        "UGC playurl request: {} (aid={}, cid={}, qn={}, fnval={})",
        url,
        aid,
        cid,
        qn,
        fnval
    );

    let v: PlayurlResp = client
        .get(&url)
        .header("Referer", "https://www.bilibili.com")
        .send()
        .await?
        .json()
        .await?;
    log::debug!("UGC playurl response code: {}", v.code);
    if v.code != 0 {
        log::warn!(
            "UGC playurl failed: code={} aid={} cid={}",
            v.code,
            aid,
            cid
        );
        return Err(anyhow!("获取播放地址失败 code={}", v.code));
    }
    let mut dash = v.data.dash.ok_or_else(|| anyhow!("没有 dash 返回"))?;
    if let Some(d) = &dash.dolby {
        if let Some(list) = &d.audio {
            for a in list {
                dash.audio.push(a.clone());
            }
        }
    }
    if let Some(f) = &dash.flac {
        dash.audio.push(f.audio.clone());
    }
    log::debug!(
        "UGC dash parsed: videos={} audios={} dolby={} flac={}",
        dash.video.len(),
        dash.audio.len(),
        dash.dolby
            .as_ref()
            .and_then(|d| d.audio.as_ref())
            .map(|v| v.len())
            .unwrap_or(0),
        if dash.flac.is_some() { 1 } else { 0 }
    );
    Ok(dash)
}

pub async fn fetch_dash_pgc(
    client: &Client,
    ep_id: u64,
    season_id: u64,
    use_wbi: bool,
) -> Result<PlayurlDash> {
    let base_url = "https://api.bilibili.com/pgc/player/web/v2/playurl";

    // 参考 BiliTools: 登录用户使用 qn=127, fnval=4048; 未登录用户使用 qn=64, fnval=16
    let (qn, fnval, fourk) = if use_wbi {
        ("127", "4048", "1")
    } else {
        ("64", "16", "0")
    };

    // 构造参数并在登录状态下进行 WBI 签名
    let params = vec![
        ("ep_id", ep_id.to_string()),
        ("season_id", season_id.to_string()),
        ("qn", qn.to_string()),
        ("fnval", fnval.to_string()),
        ("fnver", "0".to_string()),
        ("fourk", fourk.to_string()),
    ];

    // 与 BiliTools 对齐：PGC 也始终使用 WBI 签名
    let wbi_params: Vec<(&str, String)> = params.iter().map(|(k, v)| (*k, v.clone())).collect();
    let query = crate::wbi::sign_wbi(client, &wbi_params).await?;
    let url = format!("{}?{}", base_url, query);

    let v: serde_json::Value = client
        .get(&url)
        .header("Referer", "https://www.bilibili.com")
        .send()
        .await?
        .json()
        .await?;

    let dash_v = v
        .get("data")
        .and_then(|d| d.get("dash"))
        .or_else(|| v.get("result").and_then(|r| r.get("dash")))
        .or_else(|| {
            v.get("result")
                .and_then(|r| r.get("video_info"))
                .and_then(|vi| vi.get("dash"))
        })
        .ok_or_else(|| anyhow!("PGC 未返回 dash"))?;
    let mut dash: PlayurlDash = serde_json::from_value(dash_v.clone())?;
    if let Some(d) = &dash.dolby {
        if let Some(list) = &d.audio {
            for a in list {
                dash.audio.push(a.clone());
            }
        }
    }
    if let Some(f) = &dash.flac {
        dash.audio.push(f.audio.clone());
    }
    log::debug!(
        "PGC dash parsed: videos={} audios={} dolby={} flac={}",
        dash.video.len(),
        dash.audio.len(),
        dash.dolby
            .as_ref()
            .and_then(|d| d.audio.as_ref())
            .map(|v| v.len())
            .unwrap_or(0),
        if dash.flac.is_some() { 1 } else { 0 }
    );
    Ok(dash)
}
