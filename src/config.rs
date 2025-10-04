use once_cell::sync::Lazy;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Default, Clone)]
pub struct ApiConfig {
    /// 服务器绑定地址（例如 0.0.0.0:8080），默认 127.0.0.1:8080
    #[serde(default = "default_bind")]
    pub bind: String,
    /// 对外暴露的基础 URL，用于拼接返回数据里的 url 字段；默认 http://127.0.0.1:8080
    #[serde(default = "default_public_base")]
    pub public_base: String,
    /// 缓存目录
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
}

fn default_bind() -> String {
    "127.0.0.1:8080".to_string()
}
fn default_public_base() -> String {
    "http://127.0.0.1:8080".to_string()
}
fn default_cache_dir() -> String {
    "cache".to_string()
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
    fn default() -> Self {
        Self {
            path: "cookies.jsonl".to_string(),
        }
    }
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
                if cfg.storage.base_dir.is_empty() {
                    cfg.storage.base_dir = StorageConfig::default().base_dir;
                }
                if cfg.storage.pgc_template.is_empty() {
                    cfg.storage.pgc_template = StorageConfig::default().pgc_template;
                }
                if cfg.storage.ugc_template.is_empty() {
                    cfg.storage.ugc_template = StorageConfig::default().ugc_template;
                }
                if cfg.storage.stream_suffix.is_empty() {
                    cfg.storage.stream_suffix = StorageConfig::default().stream_suffix;
                }
                if cfg.storage.stream_ext.is_empty() {
                    cfg.storage.stream_ext = StorageConfig::default().stream_ext;
                }
                if cfg.cookies.path.is_empty() {
                    cfg.cookies.path = CookiesConfig::default().path;
                }
                cfg
            }
            Err(e) => {
                log::warn!("解析 config.toml 失败，将使用默认配置：{}", e);
                Config::default()
            }
        }
    } else {
        Config::default()
    }
});

pub fn get() -> &'static Config {
    &CONFIG
}
