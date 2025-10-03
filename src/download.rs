use anyhow::{Result, anyhow};
use reqwest::Client;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub fn sanitize_filename(name: &str) -> String {
    let mut s = name.chars().map(|c| match c {
        '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
        c if c.is_control() => '_',
        c => c,
    }).collect::<String>();
    if s.is_empty() { s = "bili".to_string(); }
    s
}

pub async fn download_to_file(client: &Client, url: &str, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).ok(); }
    let resp0 = client
        .get(url)
        .header("Referer", "https://www.bilibili.com")
        .send()
        .await?;
    let status = resp0.status();
    let mut resp = match resp0.error_for_status() {
        Ok(r) => r,
        Err(e) => {
            return Err(anyhow!(format!(
                "HTTP {} for {} ({})",
                status.as_u16(),
                url,
                e
            )));
        }
    };
    let mut file = File::create(path)?;
    while let Some(chunk) = resp.chunk().await? { file.write_all(&chunk)?; }
    Ok(())
}
