use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Default, Clone)]
pub struct ApiConfig {
    // 预留字段，暂不使用
    // pub base_url: Option<String>,
    // pub token: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StorageConfig {
    pub base_dir: String,
    pub pgc_template: String,
    pub ugc_template: String,
    /// 当进行流式合流（stream）或输出合成文件名时附加的后缀（放在模板生成的基础名之后，扩展名前）
    #[serde(default)]
    pub stream_suffix: String,
    /// 合流输出文件扩展名（默认 mp4）
    #[serde(default)]
    pub stream_ext: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            base_dir: "downloads".to_string(),
            pgc_template: "{season_title}/[{ep}]{title}".to_string(),
            ugc_template: "{title}".to_string(),
            stream_suffix: "".to_string(),
            stream_ext: "mp4".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct CookiesConfig {
    pub path: String,
}

impl Default for CookiesConfig {
    fn default() -> Self { Self { path: "cookies.jsonl".to_string() } }
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Config {
    #[serde(default)]
    #[allow(dead_code)]
    pub api: ApiConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub cookies: CookiesConfig,
}

pub static CONFIG: Lazy<Config> = Lazy::new(|| {
    let path = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("config.toml");
    if let Ok(s) = std::fs::read_to_string(&path) {
        match toml::from_str::<Config>(&s) {
            Ok(mut cfg) => {
                // 补齐默认值
                if cfg.storage.base_dir.is_empty() { cfg.storage.base_dir = StorageConfig::default().base_dir; }
                if cfg.storage.pgc_template.is_empty() { cfg.storage.pgc_template = StorageConfig::default().pgc_template; }
                if cfg.storage.ugc_template.is_empty() { cfg.storage.ugc_template = StorageConfig::default().ugc_template; }
                if cfg.storage.stream_suffix.is_empty() { cfg.storage.stream_suffix = StorageConfig::default().stream_suffix; }
                if cfg.storage.stream_ext.is_empty() { cfg.storage.stream_ext = StorageConfig::default().stream_ext; }
                if cfg.cookies.path.is_empty() { cfg.cookies.path = CookiesConfig::default().path; }
                cfg
            }
            Err(e) => {
                eprintln!("解析 config.toml 失败，将使用默认配置：{}", e);
                Config::default()
            }
        }
    } else {
        Config::default()
    }
});

pub fn get() -> &'static Config { &CONFIG }

fn replace_vars(mut template: String, vars: &HashMap<&str, String>) -> String {
    for (k, v) in vars.iter() {
        let key = format!("{{{}}}", k);
        template = template.replace(&key, v);
    }
    template
}

fn sanitize_path_segments(s: &str) -> PathBuf {
    let mut pb = PathBuf::new();
    for seg in s.split('/') {
        let clean = crate::download::sanitize_filename(seg);
        if !clean.is_empty() {
            pb.push(clean);
        }
    }
    pb
}

pub fn render_storage_path_for_ugc(title: &str, bvid: &str, aid: u64, cid: u64) -> PathBuf {
    let cfg = get();
    let mut vars = HashMap::new();
    vars.insert("title", crate::download::sanitize_filename(title));
    vars.insert("bvid", crate::download::sanitize_filename(bvid));
    vars.insert("aid", aid.to_string());
    vars.insert("cid", cid.to_string());
    let replaced = replace_vars(cfg.storage.ugc_template.clone(), &vars);
    let rel = sanitize_path_segments(&replaced);
    Path::new(&cfg.storage.base_dir).join(rel)
}

pub fn render_storage_path_for_pgc(
    season_title: &str,
    title: &str,
    ep: usize,
    ep_id: u64,
    aid: u64,
    cid: u64,
) -> PathBuf {
    let cfg = get();
    let mut vars = HashMap::new();
    vars.insert("season_title", crate::download::sanitize_filename(season_title));
    vars.insert("title", crate::download::sanitize_filename(title));
    vars.insert("ep", format!("{}", ep));
    vars.insert("ep_id", ep_id.to_string());
    vars.insert("aid", aid.to_string());
    vars.insert("cid", cid.to_string());
    let replaced = replace_vars(cfg.storage.pgc_template.clone(), &vars);
    let rel = sanitize_path_segments(&replaced);
    Path::new(&cfg.storage.base_dir).join(rel)
}

/// 基于 render_* 返回的“基础路径”与配置，生成合流输出文件最终路径
pub fn render_stream_output_from_base(base: &Path) -> PathBuf {
    let cfg = get();
    let parent = base.parent().unwrap_or_else(|| Path::new("."));
    let stem = base.file_name().and_then(|s| s.to_str()).unwrap_or("bili");
    parent.join(format!("{}{}.{ext}", stem, cfg.storage.stream_suffix, ext = cfg.storage.stream_ext))
}
