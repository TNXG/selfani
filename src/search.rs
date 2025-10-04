use anyhow::{Result, anyhow};
use regex::Regex;
use reqwest::Client;
use std::collections::HashSet;

use crate::wbi; // WBI 签名

#[derive(Debug)]
#[allow(dead_code)]
pub struct MediaBangumiItem {
    pub title: String,
    pub media_id: i64,
    pub season_id: i64,
    pub eps: i64,
    pub cover: Option<String>,
    pub desc: Option<String>,
    pub is_finish: Option<bool>,
    pub season_type_name: Option<String>,
    pub pub_time: Option<String>,
}

/// 自动翻页搜索番剧（media_bangumi），直到没有新结果或达最大页数
/// 说明：B 站该接口没有返回总页数字段（或字段不稳定），这里通过“当前页无结果”作为停止条件，
/// 另外加入 `MAX_PAGES` 保护，避免潜在无限循环。
pub async fn search_media_bangumi(client: &Client, keyword: &str) -> Result<Vec<MediaBangumiItem>> {
    const MAX_PAGES: u32 = 100; // 安全上限，可视需要调整 / 暴露为配置
    let em_re = Regex::new(r"</?em[^>]*>").unwrap();
    let mut page: u32 = 1;
    let mut out: Vec<MediaBangumiItem> = Vec::new();
    let mut seen: HashSet<i64> = HashSet::new(); // season_id 去重（或可换 media_id）

    loop {
        let params = vec![
            ("keyword", keyword.to_string()),
            ("search_type", "media_bangumi".to_string()),
            ("page", page.to_string()),
        ];
        let query = wbi::sign_wbi(
            client,
            &params
                .iter()
                .map(|(k, v)| (*k, v.clone()))
                .collect::<Vec<_>>(),
        )
        .await?;
        let url = format!(
            "https://api.bilibili.com/x/web-interface/wbi/search/type?{}",
            query
        );
        eprintln!("[search] requesting page={} url={}", page, url);

        let resp = client
            .get(&url)
            .header("Referer", "https://www.bilibili.com")
            .header("Origin", "https://www.bilibili.com")
            .send()
            .await?;
        let status = resp.status();
        let bytes = resp.bytes().await?;
        let resp_v: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
            let text_preview = String::from_utf8_lossy(&bytes);
            let text_preview = if text_preview.len() > 300 {
                &text_preview[..300]
            } else {
                &text_preview
            };
            anyhow!(
                "解析JSON失败 status={} err={} body_preview=<<<{}>>>",
                status,
                e,
                text_preview
            )
        })?;

        let code = resp_v.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
        if code != 0 {
            return Err(anyhow!(
                "搜索失败 page={} code={}: {:?}",
                page,
                code,
                resp_v
            ));
        }

        let results = resp_v.get("data").and_then(|d| d.get("result"));
        // 空或缺失 => 终止
        if results.is_none() {
            break;
        }

        let mut page_new_items = 0usize;

        let push_item = |item: &serde_json::Value,
                         em_re: &Regex,
                         seen: &mut HashSet<i64>,
                         out: &mut Vec<MediaBangumiItem>,
                         page_new_items: &mut usize| {
            let title_raw = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let title = html_unescape(&em_re.replace_all(title_raw, ""));
            let media_id = item.get("media_id").and_then(|v| v.as_i64()).unwrap_or(0);
            let season_id = item.get("season_id").and_then(|v| v.as_i64()).unwrap_or(0);
            let eps = item.get("eps").and_then(|v| v.as_i64()).unwrap_or(0);
            if season_id != 0 && !seen.insert(season_id) {
                return;
            } // 已存在跳过
            let cover = item
                .get("cover")
                .or_else(|| item.get("media_cover"))
                .or_else(|| item.get("season_cover"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let desc = item
                .get("desc")
                .or_else(|| item.get("media_desc"))
                .or_else(|| item.get("evaluate"))
                .and_then(|v| v.as_str())
                .map(|s| html_unescape(s));
            let is_finish = item
                .get("is_finish")
                .or_else(|| item.get("finish"))
                .and_then(|v| match v {
                    serde_json::Value::Bool(b) => Some(*b),
                    serde_json::Value::Number(n) => n.as_i64().map(|i| i == 1),
                    _ => None,
                });
            let season_type_name = item
                .get("season_type_name")
                .or_else(|| item.get("media_type_name"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let pub_time = item
                .get("pub_time")
                .or_else(|| item.get("pubtime"))
                .or_else(|| item.get("publish_time"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            out.push(MediaBangumiItem {
                title,
                media_id,
                season_id,
                eps,
                cover,
                desc,
                is_finish,
                season_type_name,
                pub_time,
            });
            *page_new_items += 1;
        };

        match results.unwrap() {
            serde_json::Value::Array(arr) => {
                for item in arr.iter() {
                    push_item(item, &em_re, &mut seen, &mut out, &mut page_new_items);
                }
            }
            serde_json::Value::Object(map) => {
                for (_k, v) in map.iter() {
                    if let serde_json::Value::Array(arr) = v {
                        for item in arr {
                            push_item(item, &em_re, &mut seen, &mut out, &mut page_new_items);
                        }
                    }
                }
            }
            _ => {}
        }

        // 当前页没有新增 => 结束
        if page_new_items == 0 {
            break;
        }
        page += 1;
        if page > MAX_PAGES {
            break;
        }
    }
    Ok(out)
}

fn html_unescape(s: &str) -> String {
    // 处理最常见实体；如需要更完整可引入 crate（此处保持零依赖）
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}
