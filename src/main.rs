mod config;
mod cookies;
mod playurl;
mod search;
mod wbi;
mod hls;

use actix_web::{App, HttpResponse, HttpServer, Responder, get, web};
use actix_web::middleware::Logger as ActixLogger;
use actix_cors::Cors;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub struct ApiResult<T> {
    code: i32, // 0 成功；其他为错误码（本地或上游）
    success: bool,
    message: String, // 错误描述或空
    data: T,
}

#[derive(Serialize)]
struct SearchItem {
    id: String,
    title: String,
    cover: String,
    description: String,
    year: String,
    status: String,
    #[serde(rename = "type")]
    type_field: String,
    url: String,
}

#[derive(Serialize)]
struct DetailSourceItem {
    name: String,
    sort: usize,
    url: String,
}

#[derive(Serialize)]
struct DetailData {
    id: String,
    title: String,
    cover: String,
    description: String,
    year: String,
    status: String,
    #[serde(rename = "type")]
    type_field: String,
    sources: Vec<DetailSourceItem>,
}

#[get("/search")]
async fn search_endpoint(
    q: web::Query<std::collections::HashMap<String, String>>,
    data: web::Data<AppState>,
) -> impl Responder {
    let keyword = q.get("q").map(|s| s.trim()).filter(|s| !s.is_empty());
    if keyword.is_none() {
        return HttpResponse::BadRequest().json(ApiResult {
            code: 400,
            success: false,
            message: "缺少 q 参数".to_string(),
            data: Vec::<SearchItem>::new(),
        });
    }
    match do_search(&data.client, keyword.unwrap(), &data.public_base).await {
        Ok(items) => HttpResponse::Ok().json(ApiResult {
            code: 0,
            success: true,
            message: String::new(),
            data: items,
        }),
        Err(e) => {
            log::error!("search error: {e:?}");
            let (code, msg) = map_error_code(&e);
            let status_code = if code == -412 {
                actix_web::http::StatusCode::PRECONDITION_FAILED
            } else {
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
            };
            HttpResponse::build(status_code).json(ApiResult {
                code,
                success: false,
                message: msg,
                data: Vec::<SearchItem>::new(),
            })
        }
    }
}

pub struct AppState {
    client: Client,
    public_base: String,
}

fn map_error_code(err: &anyhow::Error) -> (i32, String) {
    let s = format!("{err:#}");
    if s.contains("status=412") || s.contains("code=-412") {
        return (-412, "请求被拦截(需要有效 Cookie)".into());
    }
    if s.contains("解析JSON失败") {
        return (1001, "上游返回非 JSON".into());
    }
    (500, s.lines().next().unwrap_or("内部错误").to_string())
}

async fn do_search(client: &Client, keyword: &str, public_base: &str) -> Result<Vec<SearchItem>> {
    let raw = search::search_media_bangumi(client, keyword).await?;
    // 针对每个 season_id 获取详情（做简单并发限制）
    use futures::stream::{self, StreamExt};
    const CONCURRENCY: usize = 5;
    let items: Vec<SearchItem> = stream::iter(raw.into_iter())
        .map(|r| async move {
            let id = r.season_id;
            let detail = fetch_season_detail(client, id).await;
            let (cover, desc, year, status, type_name) = match detail {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("season detail error season_id={} err={}", id, e);
                    (
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                        String::from("TV"),
                    )
                }
            };
            SearchItem {
                id: id.to_string(),
                title: r.title,
                cover,
                description: desc,
                year,
                status,
                type_field: type_name,
                url: format!("{}/detail/{}", public_base.trim_end_matches('/'), id),
            }
        })
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;
    Ok(items)
}

#[get("/detail/{id}")]
async fn detail_endpoint(path: web::Path<(String,)>, data: web::Data<AppState>) -> impl Responder {
    let season_id_str = path.into_inner().0;
    let season_id: i64 = match season_id_str.parse() {
        Ok(v) => v,
        Err(_) => {
            return HttpResponse::BadRequest().json(ApiResult {
                code: 400,
                success: false,
                message: "id 参数应为数字".into(),
                data: serde_json::json!({}),
            });
        }
    };
    match fetch_season_full(&data.client, season_id, &data.public_base).await {
        Ok(detail) => HttpResponse::Ok().json(ApiResult {
            code: 0,
            success: true,
            message: String::new(),
            data: detail,
        }),
        Err(e) => {
            log::error!("detail error id={} err={e:#}", season_id);
            let (code, msg) = map_error_code(&e);
            let status_code = if code == -412 {
                actix_web::http::StatusCode::PRECONDITION_FAILED
            } else if code == 404 {
                actix_web::http::StatusCode::NOT_FOUND
            } else {
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
            };
            HttpResponse::build(status_code).json(ApiResult {
                code,
                success: false,
                message: msg,
                data: serde_json::json!({}),
            })
        }
    }
}

async fn fetch_season_detail(
    client: &Client,
    season_id: i64,
) -> Result<(String, String, String, String, String)> {
    let mut url = reqwest::Url::parse("https://api.bilibili.com/pgc/view/web/season")?;
    url.query_pairs_mut()
        .append_pair("season_id", &season_id.to_string());
    let resp = client
        .get(url)
        .header("Referer", "https://www.bilibili.com")
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    let v: Value = serde_json::from_str(&text).map_err(|e| {
        anyhow::anyhow!(
            "season detail parse fail id={} status={} err={} body_snip={}",
            season_id,
            status,
            e,
            &text.chars().take(120).collect::<String>()
        )
    })?;
    let root = v
        .get("result")
        .or_else(|| v.get("data"))
        .cloned()
        .unwrap_or(Value::Null);
    let cover = root
        .get("cover")
        .or_else(|| root.get("season_cover"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let desc = root
        .get("evaluate")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let publish_time = root
        .get("publish")
        .and_then(|p| p.get("pub_time"))
        .and_then(|s| s.as_str())
        .unwrap_or("");
    let year = if publish_time.len() >= 4 {
        publish_time[..4].to_string()
    } else {
        String::new()
    };
    let is_finish = root
        .get("publish")
        .and_then(|p| p.get("is_finish"))
        .and_then(|b| b.as_i64())
        .unwrap_or(0)
        == 1;
    let status = if is_finish { "完结" } else { "连载" }.to_string();
    let type_name = root
        .get("season_type_name")
        .and_then(|v| v.as_str())
        .unwrap_or("TV")
        .to_string();
    Ok((cover, desc, year, status, type_name))
}

async fn fetch_season_full(
    client: &Client,
    season_id: i64,
    public_base: &str,
) -> Result<DetailData> {
    let mut url = reqwest::Url::parse("https://api.bilibili.com/pgc/view/web/season")?;
    url.query_pairs_mut()
        .append_pair("season_id", &season_id.to_string());
    let resp = client
        .get(url)
        .header("Referer", "https://www.bilibili.com")
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    let v: Value = serde_json::from_str(&text).map_err(|e| {
        anyhow::anyhow!(
            "season detail parse fail id={} status={} err={} body_snip={}",
            season_id,
            status,
            e,
            &text.chars().take(160).collect::<String>()
        )
    })?;
    // 统一 root: 有的返回 result，有的返回 data
    let root = v
        .get("result")
        .or_else(|| v.get("data"))
        .cloned()
        .unwrap_or(Value::Null);
    if root.is_null() {
        return Err(anyhow::anyhow!("season not found id={}", season_id));
    }
    let title = root
        .get("title")
        .or_else(|| root.get("season_title"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if title.is_empty() {
        return Err(anyhow::anyhow!("title empty id={}", season_id));
    }
    let cover = root
        .get("cover")
        .or_else(|| root.get("season_cover"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let desc = root
        .get("evaluate")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let publish_time = root
        .get("publish")
        .and_then(|p| p.get("pub_time"))
        .and_then(|s| s.as_str())
        .unwrap_or("");
    let year = if publish_time.len() >= 4 {
        publish_time[..4].to_string()
    } else {
        String::new()
    };
    let is_finish = root
        .get("publish")
        .and_then(|p| p.get("is_finish"))
        .and_then(|b| b.as_i64())
        .unwrap_or(0)
        == 1;
    let status = if is_finish { "完结" } else { "连载" }.to_string();
    let type_name = root
        .get("season_type_name")
        .and_then(|v| v.as_str())
        .unwrap_or("TV")
        .to_string();
    let eps_arr = root
        .get("episodes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut sources: Vec<DetailSourceItem> = Vec::with_capacity(eps_arr.len());
    for (idx, ep) in eps_arr.iter().enumerate() {
        let ep_index = idx + 1; // 1-based
        let ep_title_num = ep.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let ep_long = ep.get("long_title").and_then(|v| v.as_str()).unwrap_or("");
        let name = if ep_long.is_empty() {
            if ep_title_num.is_empty() {
                format!("第{}集", ep_index)
            } else {
                format!("第{}集", ep_title_num)
            }
        } else {
            if ep_title_num.is_empty() {
                format!("第{}集 {}", ep_index, ep_long)
            } else {
                format!("第{}集 {}", ep_title_num, ep_long)
            }
        };
        sources.push(DetailSourceItem {
            name,
            sort: ep_index,
            url: format!(
                "{}/play/{}/{}",
                public_base.trim_end_matches('/'),
                season_id,
                ep_index
            ),
        });
    }
    Ok(DetailData {
        id: season_id.to_string(),
        title,
        cover,
        description: desc,
        year,
        status,
        type_field: type_name,
        sources,
    })
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    let cfg = config::get();
    let bind_addr = cfg.api.bind.clone();
    let public_base = cfg.api.public_base.clone();
    let (client, _store) =
        cookies::build_client(cookies::load_cookie_store().context("load cookies")?)
            .expect("build client with cookies");
    // Initialize logging with default filter if RUST_LOG is not set.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,actix_web=info,selfani=info"),
    )
    .init();
    log::info!("Starting server at http://{}", bind_addr);
    HttpServer::new(move || {
        App::new()
            .wrap(ActixLogger::default())
            .wrap(
                Cors::default()
                    .allow_any_origin()
                    .allowed_methods(vec!["GET", "OPTIONS", "HEAD"])
                    .allowed_header(actix_web::http::header::CONTENT_TYPE)
                    .allowed_header(actix_web::http::header::RANGE)
                    .expose_headers(vec!["Content-Length", "Accept-Ranges"])
                    .max_age(86400),
            )
            .app_data(web::Data::new(AppState {
                client: client.clone(),
                public_base: public_base.clone(),
            }))
            .service(search_endpoint)
            .service(detail_endpoint)
            .service(hls::hls_playlist)
            .service(hls::hls_segment)
    })
    .bind(bind_addr)?
    .run()
    .await?;
    Ok(())
}
