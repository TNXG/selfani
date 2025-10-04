use anyhow::{Result, anyhow};
use indicatif::{ProgressBar, ProgressStyle};
use qrcode::QrCode;
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct QrGenerateResp {
    code: i32,
    data: QrGenerateData,
}
#[derive(Debug, Deserialize)]
struct QrGenerateData {
    url: String,
    qrcode_key: String,
}

#[derive(Debug, Deserialize)]
struct QrPollResp {
    #[allow(dead_code)]
    code: i32,
    data: QrPollData,
}
#[derive(Debug, Deserialize)]
struct QrPollData {
    code: i32,
    message: String,
}

pub async fn login_qr(client: &Client) -> Result<()> {
    let resp = client
        .get("https://passport.bilibili.com/x/passport-login/web/qrcode/generate")
        .send()
        .await?
        .error_for_status()?;
    let body: QrGenerateResp = resp.json().await?;
    if body.code != 0 {
        return Err(anyhow!("二维码获取失败: code {}", body.code));
    }
    let url = &body.data.url;
    let key = &body.data.qrcode_key;

    let code = QrCode::new(url.as_bytes())?;
    println!("请使用 B 站 App 扫码登录:\n");
    let string = code
        .render::<char>()
        .quiet_zone(true)
        .module_dimensions(2, 1)
        .build();
    println!("{}", string);

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner} 等待确认... ")
            .unwrap(),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(120));
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        let poll = client
            .get("https://passport.bilibili.com/x/passport-login/web/qrcode/poll")
            .query(&[("qrcode_key", key)])
            .send()
            .await?;
        let status: QrPollResp = poll.json().await?;
        match status.data.code {
            0 => {
                pb.finish_with_message("登录成功");
                break;
            }
            86101 => { /* 等待扫码 */ }
            86090 => {
                pb.set_message("已扫码，等待确认...");
            }
            86038 => return Err(anyhow!("二维码已失效，请重试")),
            c => return Err(anyhow!("登录失败，状态码: {} ({})", c, status.data.message)),
        }
    }
    Ok(())
}
