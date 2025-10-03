use anyhow::{anyhow, Result};
use std::path::Path;
use tokio::process::Command;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;

// 通过系统 ffmpeg 可执行文件进行流式合流（不落盘，直接 URL 输入，容器 MP4，流拷贝）
// 需要本机已安装 ffmpeg，且 ffmpeg 支持 https。

pub async fn mux_streams_to_mp4(video_url: &str, audio_url: &str, out_path: &Path) -> Result<()> {
    // 参考 B 站需要的头：Referer 与一个常见 UA
    let referer_header = "Referer: https://www.bilibili.com";
    let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

    // ffmpeg 命令说明：
    // -user_agent 为输入设定 UA
    // -headers 传递自定义头（加上 \r\n 结尾由 ffmpeg 处理；CLI 可不加）
    // 两路输入：第一个为视频，第二个为音频
    // -map 指定映射
    // -c copy 进行流拷贝（零重编码，快速且无损）
    // -movflags +faststart 便于边下边播/网页首播优化
    // -y 覆盖输出
    let status = Command::new("ffmpeg")
        .arg("-loglevel").arg("error")
        .arg("-nostdin")
        .arg("-user_agent").arg(ua)
        .arg("-headers").arg(referer_header)
        .arg("-i").arg(video_url)
        .arg("-user_agent").arg(ua)
        .arg("-headers").arg(referer_header)
        .arg("-i").arg(audio_url)
        .arg("-map").arg("0:v:0")
        .arg("-map").arg("1:a:0")
        .arg("-c").arg("copy")
        .arg("-movflags").arg("+faststart")
        .arg("-y")
        .arg(out_path.as_os_str())
        .status()
        .await?;

    if !status.success() {
        return Err(anyhow!("ffmpeg 合流失败，退出码 {:?}", status.code()));
    }
    Ok(())
}
/// 本地 m4s 合流为 MP4，封面通过 stdin 管道传入（不落盘）
pub async fn mux_files_to_mp4_with_cover_bytes(
    video_path: &Path,
    audio_path: &Path,
    cover_bytes: Option<&[u8]>,
    out_path: &Path,
) -> Result<()> {
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-loglevel").arg("error")
        .arg("-i").arg(video_path.as_os_str())
        .arg("-i").arg(audio_path.as_os_str());

    if cover_bytes.is_some() {
        cmd.arg("-f").arg("image2pipe").arg("-i").arg("pipe:0");
    }

    let mut args: Vec<&str> = vec!["-map", "0:v:0", "-map", "1:a:0", "-c", "copy"]; 
    if cover_bytes.is_some() {
        args.extend(["-map", "2:v:0", "-disposition:v:1", "attached_pic", "-c:v:1", "mjpeg"]);
    }
    args.extend(["-movflags", "+faststart", "-y"]);

    cmd.args(&args)
        .arg(out_path.as_os_str());

    let mut child = if cover_bytes.is_some() {
        cmd.stdin(Stdio::piped()).spawn()?
    } else {
        cmd.spawn()?
    };

    if let (Some(bytes), Some(mut stdin)) = (cover_bytes, child.stdin.take()) {
        stdin.write_all(bytes).await?;
        stdin.shutdown().await?;
    }

    let status = child.wait().await?;
    if !status.success() { return Err(anyhow!("ffmpeg 合流失败，退出码 {:?}", status.code())); }
    Ok(())
}