use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct NavResp {
    #[allow(dead_code)]
    code: i32,
    data: NavData,
}

#[derive(Debug, Deserialize)]
struct NavData {
    wbi_img: WbiImg,
}

#[derive(Debug, Deserialize)]
struct WbiImg {
    img_url: String,
    sub_url: String,
}

const MIXIN_KEY_ENC_TAB: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49,
    33, 9, 42, 19, 29, 28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40,
    61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25, 54, 21, 56, 59, 6, 63, 57, 62, 11,
    36, 20, 34, 44, 52,
];

fn extract_key(url: &str) -> String {
    let start = url.rfind('/').map(|i| i + 1).unwrap_or(0);
    let end = url.rfind('.').unwrap_or(url.len());
    url[start..end].to_string()
}

fn get_mixin_key(img_key: &str, sub_key: &str) -> String {
    let combined = format!("{}{}", img_key, sub_key);
    let mut result = String::new();
    for &idx in &MIXIN_KEY_ENC_TAB {
        if let Some(ch) = combined.chars().nth(idx) {
            result.push(ch);
        }
    }
    result.chars().take(32).collect()
}

/// 获取 WBI 签名后的查询字符串
pub async fn sign_wbi(client: &Client, params: &[(&str, String)]) -> Result<String> {
    // 获取 img_key 和 sub_key
    let nav_resp: NavResp = client
        .get("https://api.bilibili.com/x/web-interface/nav")
        .send()
        .await?
        .json()
        .await?;
    
    let img_key = extract_key(&nav_resp.data.wbi_img.img_url);
    let sub_key = extract_key(&nav_resp.data.wbi_img.sub_url);
    let mixin_key = get_mixin_key(&img_key, &sub_key);
    
    // 添加 wts 时间戳
    let curr_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    
    let mut all_params: Vec<(String, String)> = params
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    all_params.push(("wts".to_string(), curr_time.to_string()));
    
    // 排序参数
    all_params.sort_by(|a, b| a.0.cmp(&b.0));
    
    // 构建查询字符串
    let query = all_params
        .iter()
        .map(|(k, v)| {
            let filtered_v = v.replace(['!', '\'', '(', ')', '*'], "");
            format!("{}={}", urlencoding::encode(k), urlencoding::encode(&filtered_v))
        })
        .collect::<Vec<_>>()
        .join("&");
    
    // 计算 MD5
    let sign_str = format!("{}{}", query, mixin_key);
    let w_rid = format!("{:x}", md5::compute(sign_str.as_bytes()));
    
    Ok(format!("{}&w_rid={}", query, w_rid))
}
