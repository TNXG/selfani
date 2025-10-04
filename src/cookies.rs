use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use std::fs::{File, create_dir_all};
use std::io::{BufReader, BufWriter, Read};
use std::path::PathBuf;
use std::sync::Arc;

fn cookie_path() -> PathBuf {
    let cfg = crate::config::get();
    let p = &cfg.cookies.path;
    let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let path = PathBuf::from(p);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

/// 对外暴露 cookie 文件是否存在的检查，用于首次启动判断。
pub fn cookie_file_exists() -> bool {
    cookie_path().exists()
}

/// 保存当前 CookieStore 到文件（采用新版 serde json 格式）。
pub fn save_cookie_store(store: &CookieStore) -> Result<()> {
    let path = cookie_path();
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            create_dir_all(parent)?;
        }
    }
    let f = File::create(&path).context("创建 cookies 文件失败")?;
    let mut writer = BufWriter::new(f);
    cookie_store::serde::json::save(store, &mut writer).map_err(|e| anyhow!(e.to_string()))
}

pub fn load_cookie_store() -> Result<CookieStore> {
    let path = cookie_path();
    if path.exists() {
        let f = File::open(&path).context("打开 cookies.json 失败")?;
        let reader = BufReader::new(f);
        match cookie_store::serde::json::load(reader) {
            Ok(store) => Ok(store),
            Err(_e) => {
                // 回退旧格式：尝试使用已废弃的 load_json/load_json_all 解析
                let f2 = File::open(&path).context("重新打开 cookies.json 失败")?;
                let mut r2 = BufReader::new(f2);
                let mut s = String::new();
                r2.read_to_string(&mut s)?;
                #[allow(deprecated)]
                let legacy = CookieStore::load_json(s.as_bytes())
                    .or_else(|_| {
                        #[allow(deprecated)]
                        CookieStore::load_json_all(s.as_bytes())
                    })
                    .map_err(|e| anyhow!(e.to_string()))?;
                Ok(legacy)
            }
        }
    } else {
        Ok(CookieStore::default())
    }
}

pub fn build_client(store: CookieStore) -> Result<(Client, Arc<CookieStoreMutex>)> {
    let cookie_store = Arc::new(CookieStoreMutex::new(store));
    let client = Client::builder()
        .cookie_provider(Arc::clone(&cookie_store))
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36")
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;
    Ok((client, cookie_store))
}
