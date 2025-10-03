use anyhow::{anyhow, Result};
use std::path::Path;

mod cookies;
mod config;
mod download;
mod login;
mod playurl;
mod resolve;
mod select;
mod wbi;
mod term;
mod mux;
mod cover;

fn print_usage() {
    eprintln!(
        "用法:\n  selfani login                      扫码登录并保存 cookies.json\n  selfani message <id|url>          展示解析到的标题/分P/EP 简要信息\n  selfani search <id|url> [--ep N]  选出最佳音视频并输出 URL（调试用）\n  selfani download <id|url> [--ep N] 解析输入，自动选流并下载音视频（合流提示）\n  selfani stream <id|url> [--ep N]  直接通过 ffmpeg 流式合流为 MP4（不落盘 m4s）\n\n参数:\n  --ep N    指定番剧集数（从 1 开始）"
    );
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        print_usage();
        return Ok(());
    }

    // 初始化 HTTP 客户端和 Cookie
    let store = cookies::load_cookie_store()?;
    let is_logged_in = cookies::is_logged_in(&store);
    let (client, cookie_mutex) = cookies::build_client(store)?;

    match args.remove(0).as_str() {
        "login" => {
            login::login_qr(&client).await?;
            let guard = cookie_mutex.lock().unwrap();
            cookies::save_cookie_store(&*guard)?;
        }
        "message" => {
            if args.is_empty() {
                return Err(anyhow!("缺少 <id|url>"));
            }
            match resolve::resolve_input(&client, &args[0]).await? {
                resolve::Resolved::Pub { cid, bvid, title, .. } => {
                    println!("{} {}", term::paint("类型:", term::Style::Label), term::paint("UGC 视频", term::Style::Title));
                    println!("{} {}", term::paint("标题:", term::Style::Label), title);
                    println!("{} {}", term::paint("BV号:", term::Style::Label), bvid);
                    println!("{} {}", term::paint("CID:", term::Style::Label), cid);
                }
                resolve::Resolved::Pgc { ep_id, season_id, season_title, evaluate, episodes, title: _title, .. } => {
                    println!("{} {}", term::paint("类型:", term::Style::Label), term::paint("番剧", term::Style::Title));
                    println!("{} {}", term::paint("标题:", term::Style::Label), season_title);
                    println!("{} {}", term::paint("Season ID:", term::Style::Label), season_id);
                    if !evaluate.is_empty() {
                        println!("\n{}\n  {}", term::paint("简介:", term::Style::Label), evaluate);
                    }

                    // 找出当前选中的集数
                    let current_ep_index = if ep_id != 0 {
                        episodes.iter().position(|e| e.ep_id == Some(ep_id))
                    } else {
                        None
                    };

                    println!("\n{} {}", term::paint("共", term::Style::Label), episodes.len());
                    for (idx, ep) in episodes.iter().enumerate() {
                        let ep_title = ep.long_title.as_ref()
                            .or(ep.title.as_ref())
                            .map(|s| s.as_str())
                            .unwrap_or("未知标题");
                        let short_title = ep.title.as_ref().map(|s| s.as_str()).unwrap_or("");
                        let marker = if Some(idx) == current_ep_index { " ← 当前" } else { "" };
                        println!("  [{}] ep{} {} - {}{}",
                            idx + 1,
                            ep.ep_id.unwrap_or(0),
                            short_title,
                            ep_title,
                            marker
                        );
                    }

                    println!("\n{} 请使用 'search' 或 'download' 命令并携带 --ep N 参数指定要操作的集数（N 从 1 开始）", term::paint("提示:", term::Style::Info));
                }
            }
        }
        "search" => {
            if args.is_empty() {
                return Err(anyhow!("缺少 <id|url>"));
            }

            // 解析 --ep 参数（用户输入从 1 开始，需要转换为从 0 开始的索引）
            let ep_index = args.iter().position(|s| s == "--ep")
                .and_then(|pos| args.get(pos + 1))
                .and_then(|s| s.parse::<usize>().ok())
                .and_then(|n| n.checked_sub(1));

            let input = &args[0];
            let resolved = resolve::resolve_input_with_ep(&client, input, ep_index).await?;

            // 检查是否为未指定集数的番剧
            if let resolve::Resolved::Pgc { ep_id: 0, season_title, episodes, .. } = &resolved {
                println!("错误: 番剧 '{}' 需要指定集数", season_title);
                println!("共 {} 集，请使用 --ep N 参数指定（N 从 1 开始）", episodes.len());
                return Ok(());
            }

            // 显示当前选择的内容信息
            match &resolved {
                resolve::Resolved::Pub { bvid, title, .. } => {
                    println!("{} {} ({})\n", term::paint("视频:", term::Style::Label), title, term::paint(bvid, term::Style::Dim));
                }
                resolve::Resolved::Pgc { season_title, title, ep_id, episodes, .. } => {
                    // 找到当前集的索引
                    if let Some(idx) = episodes.iter().position(|e| e.ep_id == Some(*ep_id)) {
                        let ep = &episodes[idx];
                        let ep_title = ep.long_title.as_ref()
                            .or(ep.title.as_ref())
                            .map(|s| s.as_str())
                            .unwrap_or("未知标题");
                        let short_title = ep.title.as_ref().map(|s| s.as_str()).unwrap_or("");
                        println!("{} {}", term::paint("番剧:", term::Style::Label), season_title);
                        println!("  [{}] ep{} {} - {}\n", idx + 1, ep_id, short_title, ep_title);
                    } else {
                        println!("{} {} - {} {}\n", term::paint("番剧:", term::Style::Label), season_title, title, term::paint(format!("(ep={})", ep_id), term::Style::Dim));
                    }
                }
            }

            let (aid, cid) = match &resolved {
                resolve::Resolved::Pub { aid, cid, .. } => (*aid, *cid),
                resolve::Resolved::Pgc { aid, cid, .. } => (*aid, *cid),
            };
            let dash = match resolved {
                resolve::Resolved::Pub { .. } => playurl::fetch_dash_ugc(&client, aid, cid, is_logged_in).await?,
                resolve::Resolved::Pgc { ep_id, season_id, .. } => playurl::fetch_dash_pgc(&client, ep_id, season_id, is_logged_in).await?,
            };

            // 显示所有视频流
            if !dash.video.is_empty() {
                println!("{}", term::paint("可用视频流:", term::Style::Title));
                for (i, v) in dash.video.iter().enumerate() {
                    println!("  [v{}] qn={} codec={:?} bw={:?} size={}x{}",
                        i,
                        v.id,
                        v.codecid,
                        v.bandwidth,
                        v.width.unwrap_or(0),
                        v.height.unwrap_or(0)
                    );
                    let mut urls = vec![v.base_url.clone()];
                    if let Some(backup) = &v.backup_url {
                        urls.extend(backup.clone());
                    }
                    let filtered_urls = select::url_filter(urls);
                    for (j, url) in filtered_urls.iter().enumerate() {
                        println!("      [{}] {}", j, term::paint(url, term::Style::Dim));
                    }
                }
                println!();
            } else {
                println!("{}\n", term::paint("无可用视频流", term::Style::Warn));
            }

            // 显示所有音频流
            if !dash.audio.is_empty() {
                println!("{}", term::paint("可用音频流:", term::Style::Title));
                for (i, a) in dash.audio.iter().enumerate() {
                    println!("  [a{}] id={} codec={:?} bw={:?}",
                        i,
                        a.id,
                        a.codecs,
                        a.bandwidth
                    );
                    let mut urls = vec![a.base_url.clone()];
                    if let Some(backup) = &a.backup_url {
                        urls.extend(backup.clone());
                    }
                    let filtered_urls = select::url_filter(urls);
                    for (j, url) in filtered_urls.iter().enumerate() {
                        println!("      [{}] {}", j, term::paint(url, term::Style::Dim));
                    }
                }
                println!();
            } else {
                println!("{}\n", term::paint("无可用音频流", term::Style::Warn));
            }

            // 显示最佳选择
            let vbest = select::select_best_video(&dash.video);
            let abest = select::select_best_audio(&dash.audio);
            if vbest.is_some() && abest.is_some() {
                let v_idx = dash.video.iter().position(|v| std::ptr::eq(v, vbest.unwrap())).unwrap_or(0);
                let a_idx = dash.audio.iter().position(|a| std::ptr::eq(a, abest.unwrap())).unwrap_or(0);
                println!("{} 视频=[v{}] 音频=[a{}]", term::paint("最佳选择:", term::Style::Ok), v_idx, a_idx);
            }
        }
        "download" => {
            if args.is_empty() {
                return Err(anyhow!("缺少 <id|url>"));
            }

            // 解析 --ep 参数（用户输入从 1 开始，需要转换为从 0 开始的索引）
            let ep_index = args.iter().position(|s| s == "--ep")
                .and_then(|pos| args.get(pos + 1))
                .and_then(|s| s.parse::<usize>().ok())
                .and_then(|n| n.checked_sub(1));

            let input = &args[0];
            let resolved = resolve::resolve_input_with_ep(&client, input, ep_index).await?;

            // 检查是否为未指定集数的番剧
            if let resolve::Resolved::Pgc { ep_id: 0, season_title, episodes, .. } = &resolved {
                eprintln!("{} 番剧 '{}' 需要指定集数", term::paint("错误:", term::Style::Err), season_title);
                eprintln!("共 {} 集，请使用 --ep N 参数指定（N 从 1 开始）", episodes.len());
                return Ok(());
            }
            
            let (aid, cid, title) = match &resolved {
                resolve::Resolved::Pub { aid, cid, title, .. } => (*aid, *cid, title.clone()),
                resolve::Resolved::Pgc { aid, cid, title, .. } => (*aid, *cid, title.clone()),
            };
            let dash = match resolved {
                resolve::Resolved::Pub { .. } => playurl::fetch_dash_ugc(&client, aid, cid, is_logged_in).await?,
                resolve::Resolved::Pgc { ep_id, season_id, .. } => playurl::fetch_dash_pgc(&client, ep_id, season_id, is_logged_in).await?,
            };
            let vbest = select::select_best_video(&dash.video).ok_or_else(|| anyhow!("没有可用视频流"))?;
            let abest = select::select_best_audio(&dash.audio).ok_or_else(|| anyhow!("没有可用音频流"))?;
            // 生成下载 URL 列表（做一次过滤）
            let mut vurls = vec![vbest.base_url.clone()];
            if let Some(b) = &vbest.backup_url {
                vurls.extend(b.clone());
            }
            let mut aurls = vec![abest.base_url.clone()];
            if let Some(b) = &abest.backup_url {
                aurls.extend(b.clone());
            }
            let vurls = select::url_filter(vurls);
            let aurls = select::url_filter(aurls);
            // 生成保存路径（根据 UGC/PGC 使用不同模板）
            let (vpath, apath, hint_out_path) = match &resolved {
                resolve::Resolved::Pub { bvid, .. } => {
                    let base = config::render_storage_path_for_ugc(&title, bvid, aid, cid);
                    let parent = base.parent().unwrap_or_else(|| std::path::Path::new("."));
                    std::fs::create_dir_all(parent).ok();
                    let stem = base.file_name().and_then(|s| s.to_str()).unwrap_or("bili").to_string();
                    let v = parent.join(format!("{}-video.m4s", stem));
                    let a = parent.join(format!("{}-audio.m4s", stem));
                    let out = config::render_stream_output_from_base(&base);
                    (v, a, out)
                }
                resolve::Resolved::Pgc { season_title, episodes, ep_id, .. } => {
                    // 找到当前 ep 的索引（用于 {ep} 变量）
                    let ep_index = episodes.iter().position(|e| e.ep_id == Some(*ep_id)).map(|i| i + 1).unwrap_or(1);
                    let base = config::render_storage_path_for_pgc(season_title, &title, ep_index, *ep_id, aid, cid);
                    let parent = base.parent().unwrap_or_else(|| std::path::Path::new("."));
                    std::fs::create_dir_all(parent).ok();
                    let stem = base.file_name().and_then(|s| s.to_str()).unwrap_or("bili").to_string();
                    let v = parent.join(format!("{}-video.m4s", stem));
                    let a = parent.join(format!("{}-audio.m4s", stem));
                    let out = config::render_stream_output_from_base(&base);
                    (v, a, out)
                }
            };
            // 下载（按 URL 优先级尝试）
            println!("{} 下载视频中...", term::paint("[1/2]", term::Style::Info));
            download_with_priority(&client, vurls, &vpath).await?;
            println!("{} 下载音频中...", term::paint("[2/2]", term::Style::Info));
            download_with_priority(&client, aurls, &apath).await?;
            println!("{}\n  {}\n  {}", term::paint("下载完成:", term::Style::Ok), vpath.display(), apath.display());

            // 获取封面（内存中，不落盘）
            let cover_bytes_opt: Option<Vec<u8>> = match cover::fetch_cover_url(&client, &resolved).await {
                Some(cu) => cover::download_cover_bytes(&client, &cu).await,
                None => None,
            };

            // 自动合流为最终文件，并附加封面（如有）
            println!("{} 自动合流中...", term::paint("[合流]", term::Style::Info));
            if let Err(e) = mux::mux_files_to_mp4_with_cover_bytes(&vpath, &apath, cover_bytes_opt.as_deref(), &hint_out_path).await {
                eprintln!("{} 合流失败：{}", term::paint("错误:", term::Style::Err), e);
                eprintln!("{} 你可以手动合流： ffmpeg -i \"{}\" -i \"{}\" -c copy \"{}\"", term::paint("提示:", term::Style::Label), vpath.display(), apath.display(), hint_out_path.display());
            } else {
                println!("{} {}", term::paint("合流完成:", term::Style::Ok), hint_out_path.display());
            }
        }
        "stream" => {
            if args.is_empty() {
                return Err(anyhow!("缺少 <id|url>"));
            }

            // 解析 --ep 参数
            let ep_index = args.iter().position(|s| s == "--ep")
                .and_then(|pos| args.get(pos + 1))
                .and_then(|s| s.parse::<usize>().ok())
                .and_then(|n| n.checked_sub(1));

            let input = &args[0];
            let resolved = resolve::resolve_input_with_ep(&client, input, ep_index).await?;

            // 检查是否为未指定集数的番剧
            if let resolve::Resolved::Pgc { ep_id: 0, season_title, episodes, .. } = &resolved {
                eprintln!("{} 番剧 '{}' 需要指定集数", term::paint("错误:", term::Style::Err), season_title);
                eprintln!("共 {} 集，请使用 --ep N 参数指定（N 从 1 开始）", episodes.len());
                return Ok(());
            }

            let (aid, cid, title) = match &resolved {
                resolve::Resolved::Pub { aid, cid, title, .. } => (*aid, *cid, title.clone()),
                resolve::Resolved::Pgc { aid, cid, title, .. } => (*aid, *cid, title.clone()),
            };
            let dash = match resolved {
                resolve::Resolved::Pub { .. } => playurl::fetch_dash_ugc(&client, aid, cid, is_logged_in).await?,
                resolve::Resolved::Pgc { ep_id, season_id, .. } => playurl::fetch_dash_pgc(&client, ep_id, season_id, is_logged_in).await?,
            };
            let vbest = select::select_best_video(&dash.video).ok_or_else(|| anyhow!("没有可用视频流"))?;
            let abest = select::select_best_audio(&dash.audio).ok_or_else(|| anyhow!("没有可用音频流"))?;

            // 生成下载 URL 列表（做一次过滤）
            let mut vurls = vec![vbest.base_url.clone()];
            if let Some(b) = &vbest.backup_url { vurls.extend(b.clone()); }
            let mut aurls = vec![abest.base_url.clone()];
            if let Some(b) = &abest.backup_url { aurls.extend(b.clone()); }
            let vurls = select::url_filter(vurls);
            let aurls = select::url_filter(aurls);

            // 输出文件名（根据 UGC/PGC 模板生成，并使用配置的后缀与扩展名）
            let out = match &resolved {
                resolve::Resolved::Pub { bvid, .. } => {
                    let base = config::render_storage_path_for_ugc(&title, bvid, aid, cid);
                    let parent = base.parent().unwrap_or_else(|| std::path::Path::new("."));
                    std::fs::create_dir_all(parent).ok();
                    config::render_stream_output_from_base(&base)
                }
                resolve::Resolved::Pgc { season_title, episodes, ep_id, .. } => {
                    let ep_index = episodes.iter().position(|e| e.ep_id == Some(*ep_id)).map(|i| i + 1).unwrap_or(1);
                    let base = config::render_storage_path_for_pgc(season_title, &title, ep_index, *ep_id, aid, cid);
                    let parent = base.parent().unwrap_or_else(|| std::path::Path::new("."));
                    std::fs::create_dir_all(parent).ok();
                    config::render_stream_output_from_base(&base)
                }
            };

            println!("{} 通过 ffmpeg 流式合流中...", term::paint("[1/1]", term::Style::Info));

            // 按优先级尝试组合
            let mut last_err: Option<anyhow::Error> = None;
            'outer: for vu in &vurls {
                for au in &aurls {
                    match mux::mux_streams_to_mp4(vu, au, &out).await {
                        Ok(()) => { last_err = None; break 'outer; }
                        Err(e) => { last_err = Some(e); }
                    }
                }
            }

            if let Some(e) = last_err {
                return Err(anyhow!(format!("合流失败：{}", e)));
            }

            println!("{} {}", term::paint("合流完成:", term::Style::Ok), out.display());
        }
        _ => print_usage(),
    }

    Ok(())
}

async fn download_with_priority(client: &reqwest::Client, urls: Vec<String>, path: &Path) -> Result<()> {
    for u in urls { if let Ok(()) = download::download_to_file(client, &u, path).await { return Ok(()); } }
    Err(anyhow!("所有 URL 均下载失败"))
}
