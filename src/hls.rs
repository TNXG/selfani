use actix_web::{get, web, HttpRequest, HttpResponse, Responder};
use anyhow::{Result, anyhow};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tokio::time::{sleep, Duration};
use tokio::process::Command;
use std::fs;

use crate::{config, playurl, cookies};

#[get("/hls/{season_id}/{sort}/index.m3u8")]
pub async fn hls_playlist(
    path: web::Path<(String, String)>,
    data: web::Data<crate::AppState>,
) -> impl Responder {
    match prepare_hls_pipeline(&data.client, &path.0, &path.1).await {
        Ok(dir) => match wait_for_file(dir.join("index.m3u8"), 50, 100).await {
            Ok(content) => HttpResponse::Ok()
                .insert_header(("Content-Type", "application/vnd.apple.mpegurl"))
                .insert_header(("Cache-Control", "no-store"))
                .body(content),
            Err(e) => HttpResponse::ServiceUnavailable().body(format!("等待 playlist 超时: {e}")),
        },
        Err(e) => HttpResponse::InternalServerError()
            .insert_header(("Content-Type", "application/json; charset=utf-8"))
            .body(format!("{{\"error\":\"{}\"}}", escape_json(&e.to_string()))),
    }
}

#[get("/hls/{season_id}/{sort}/{seg}")]
pub async fn hls_segment(
    path: web::Path<(String, String, String)>,
    _req: HttpRequest,
) -> impl Responder {
    let (season_id, sort, seg) = path.into_inner();
    let cfg = config::get();
    let dir = PathBuf::from(&cfg.api.cache_dir)
        .join("hls")
        .join(&season_id)
        .join(&sort);
    let seg_path = dir.join(&seg);
    match wait_for_existing(&seg_path, 80, 100).await {
        Ok(_) => match tokio::fs::read(&seg_path).await {
            Ok(bytes) => HttpResponse::Ok()
                .insert_header(("Content-Type", "video/mp2t"))
                .insert_header(("Cache-Control", "public, max-age=86400"))
                .body(bytes),
            Err(e) => HttpResponse::InternalServerError().body(format!("读取分片失败: {e}")),
        },
        Err(_) => HttpResponse::NotFound().body("分片不存在"),
    }
}

fn escape_json(s: &str) -> String { s.replace('"', "\\\"") }

async fn prepare_hls_pipeline(client: &reqwest::Client, season_id: &str, sort: &str) -> Result<PathBuf> {
    let season_id_num: i64 = season_id.parse()?;
    let sort_num: usize = sort.parse()?;
    let cfg = config::get();
    let base_cache = PathBuf::from(&cfg.api.cache_dir).join("hls");
    tokio::fs::create_dir_all(&base_cache).await.ok();
    let work_dir = base_cache.join(season_id).join(sort);
    tokio::fs::create_dir_all(&work_dir).await?;
    let playlist = work_dir.join("index.m3u8");
    if playlist.exists() { return Ok(work_dir); }

    // 防止并发重复启动，使用锁文件
    let lock_path = work_dir.join(".lock");
    let created = match tokio::fs::OpenOptions::new().write(true).create_new(true).open(&lock_path).await {
        Ok(_) => true,
        Err(_) => false,
    };
    if !created { return Ok(work_dir); } // 其它并发请求已在生成

    // 获取 episode -> dash
    let (ep_id, _aid, _cid) = fetch_episode_ids(client, season_id_num, sort_num).await?;
    let dash = playurl::fetch_dash_pgc(client, ep_id, season_id_num as u64, true).await?;
    // 选择视频轨：最高带宽（最高画质）；如果不是 AVC(codecid=7) 则后续转码到 H.264
    let video_list = dash.video.clone();
    if video_list.is_empty() { return Err(anyhow!("无视频轨")); }
    let video = video_list
        .iter()
        .max_by_key(|v| v.bandwidth.unwrap_or(0))
        .unwrap()
        .clone();
    let need_transcode = video.codecid != Some(7);
    if need_transcode {
        log::info!(
            "最高画质轨需要转码: id={} codecid={:?} bandwidth={} width={:?} height={:?}",
            video.id, video.codecid, video.bandwidth.unwrap_or(0), video.width, video.height
        );
    }
    let audio = dash
        .audio
        .iter()
        .max_by_key(|a| a.bandwidth.unwrap_or(0))
        .ok_or_else(|| anyhow::anyhow!("无音频轨"))?;

    // 输出所选音视频参数
    log::info!(
        "选择视频: id={} codecid={:?} bandwidth={} width={:?} height={:?} mode={} | 音频: id={} bandwidth={} codecs={:?}",
        video.id,
        video.codecid,
        video.bandwidth.unwrap_or(0),
        video.width,
        video.height,
        if need_transcode { "transcode(h264)" } else { "copy" },
        audio.id,
        audio.bandwidth.unwrap_or(0),
        audio.codecs
    );

    let v_url = video.base_url.clone();
    let a_url = audio.base_url.clone();

    // 预构造 UA & Cookie 头（失败不致命）
    let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
    let cookie_header = build_cookie_string();
    let extra_headers = build_ffmpeg_headers(ua, cookie_header.as_deref());
    
    // 使用 FFmpeg 命令行直接处理
    let playlist_path = playlist.clone();
    let work_dir_clone = work_dir.clone();
    tokio::spawn(async move {
        let cleanup_lock = lock_path.clone();
        if let Err(e) = run_ffmpeg_hls(&v_url, &a_url, &work_dir_clone, &playlist_path, &extra_headers, !need_transcode).await {
            log::error!("启动 FFmpeg 失败: {e}");
            let _ = fs::remove_file(&playlist_path); // 失败时移除空的 playlist
        }
        let _ = fs::remove_file(&cleanup_lock);
    });
    
    Ok(work_dir)
}

async fn run_ffmpeg_hls(video_url: &str, audio_url: &str, work_dir: &Path, playlist_path: &Path, headers: &str, can_copy_video: bool) -> Result<()> {
    // 使用 FFmpeg 直接从 URL 下载并合流，输出为 HLS。改为 spawn，实时写出 index.m3u8 与分片。
    let output_pattern = work_dir.join("%010d.ts");
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-hide_banner")
        .arg("-loglevel").arg("warning") // 保留告警，便于排查
        // 为每个输入附加头
        .arg("-headers").arg(headers).arg("-i").arg(video_url)
        .arg("-headers").arg(headers).arg("-i").arg(audio_url);

    if can_copy_video {
        cmd.arg("-c:v").arg("copy");
    } else {
        cmd.arg("-c:v").arg("libx264")
            .arg("-preset").arg("veryfast")
            .arg("-crf").arg("23")
            .arg("-profile:v").arg("high")
            .arg("-level").arg("4.1")
            .arg("-sc_threshold").arg("0")
            .arg("-g").arg("60")
            .arg("-keyint_min").arg("60")
            .arg("-force_key_frames").arg("expr:gte(t,n_forced*2)");
    }
    cmd.arg("-c:a").arg("copy")
        .arg("-map").arg("0:v:0")
        .arg("-map").arg("1:a:0")
        .arg("-f").arg("hls")
        .arg("-hls_time").arg("2")
        .arg("-hls_list_size").arg("0")
        .arg("-hls_segment_type").arg("mpegts")
        .arg("-hls_flags").arg("independent_segments+delete_segments") // 允许独立关键帧，必要时清理
        .arg("-hls_segment_filename").arg(output_pattern.to_string_lossy().as_ref())
        .arg("-start_number").arg("0")
        .arg("-y")
        .arg(playlist_path.to_string_lossy().as_ref());

    log::info!("启动 FFmpeg (实时 HLS) cmd={:?}", cmd);
    let child = cmd.spawn()?; // 不等待完成
    // 可选：在后台等待并记录退出状态
    tokio::spawn(async move {
        match child.wait_with_output().await {
            Ok(out) => {
                if !out.status.success() {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    log::error!("FFmpeg 退出非 0: {}", stderr);
                } else {
                    log::info!("FFmpeg 结束，HLS 生成完成");
                }
            }
            Err(e) => log::error!("等待 FFmpeg 退出失败: {e}"),
        }
    });
    Ok(())
}

// 构造 ffmpeg -headers 需要的多行头（以 CRLF 分隔，并末尾再追加一个 CRLF）
fn build_ffmpeg_headers(user_agent: &str, cookie: Option<&str>) -> String {
    // 必备：Referer + User-Agent；可选：Origin + Cookie
    let mut lines = Vec::new();
    lines.push("Referer: https://www.bilibili.com".to_string());
    lines.push("Origin: https://www.bilibili.com".to_string());
    lines.push(format!("User-Agent: {}", user_agent));
    if let Some(c) = cookie { if !c.is_empty() { lines.push(format!("Cookie: {}", c)); } }
    let mut s = lines.join("\r\n");
    s.push_str("\r\n\r\n"); // ffmpeg 要求末尾再加一个空行
    s
}

// 从本地 cookie store 构造 Cookie 字符串，过滤 bilibili 相关域
fn build_cookie_string() -> Option<String> {
    let store = match cookies::load_cookie_store() { Ok(s) => s, Err(_) => return None };
    let mut pairs: Vec<(String,String)> = Vec::new();
    for cookie in store.iter_any() {
        if let Some(domain) = cookie.domain() {
            if domain.ends_with("bilibili.com") || domain.ends_with("bilivideo.com") {
                let name = cookie.name().to_string();
                let value = cookie.value().to_string();
                if !pairs.iter().any(|(n, _)| n == &name) {
                    pairs.push((name, value));
                }
            }
        }
    }
    if pairs.is_empty() { return None; }
    Some(pairs.into_iter().map(|(k,v)| format!("{}={}", k, v)).collect::<Vec<_>>().join("; "))
}

async fn wait_for_file(path: PathBuf, retries: usize, interval_ms: u64) -> Result<String> {
    for _ in 0..retries {
        if path.exists() {
            return Ok(tokio::fs::read_to_string(&path).await?);
        }
        sleep(Duration::from_millis(interval_ms)).await;
    }
    Err(anyhow::anyhow!("超时未生成: {:?}", path))
}

async fn wait_for_existing(path: &Path, retries: usize, interval_ms: u64) -> Result<()> {
    use tokio::fs::metadata;
    let mut last_nonzero = false;
    for _ in 0..retries {
        if let Ok(meta) = metadata(path).await {
            if meta.len() > 0 { // 确保文件已写入数据
                if last_nonzero { return Ok(()); }
                last_nonzero = true; // 连续两次非空更保险
            }
        }
        sleep(Duration::from_millis(interval_ms)).await;
    }
    Err(anyhow::anyhow!("timeout"))
}

async fn fetch_episode_ids(
    client: &reqwest::Client,
    season_id: i64,
    sort: usize,
) -> Result<(u64, u64, u64)> {
    let mut url = reqwest::Url::parse("https://api.bilibili.com/pgc/view/web/season")?;
    url.query_pairs_mut().append_pair("season_id", &season_id.to_string());
    let text = client
        .get(url)
        .header("Referer", "https://www.bilibili.com")
        .send()
        .await?
        .text()
        .await?;
    let v: Value = serde_json::from_str(&text)?;
    let root = v
        .get("result")
        .or_else(|| v.get("data"))
        .and_then(|r| r.get("episodes"))
        .and_then(|e| e.as_array())
        .ok_or_else(|| anyhow::anyhow!("episodes not found"))?;
    let ep = root
        .get(sort - 1)
        .ok_or_else(|| anyhow::anyhow!("ep index out of range"))?;
    let ep_id = ep.get("ep_id").or_else(|| ep.get("id")).and_then(|v| v.as_u64()).ok_or_else(|| anyhow::anyhow!("ep_id missing"))?;
    let aid = ep.get("aid").and_then(|v| v.as_u64()).ok_or_else(|| anyhow::anyhow!("aid missing"))?;
    let cid = ep.get("cid").and_then(|v| v.as_u64()).ok_or_else(|| anyhow::anyhow!("cid missing"))?;
    Ok((ep_id, aid, cid))
}
