use once_cell::sync::Lazy;
use serde::Deserialize;
use std::path::PathBuf;

fn default_config_toml() -> &'static str {
    r#"# SelfAni 配置文件 (自动生成)
# 首次运行已为你生成默认配置，可按需修改后重启。

[api]
# 服务器监听地址
bind = "127.0.0.1:8080"
# 对外可访问的基础 URL（供接口内返回拼接使用，可改成公网或内网实际地址）
public_base = "http://127.0.0.1:8080"
# 缓存目录（部分接口可能用到）
cache_dir = "cache"

[cookies]
# 登录 cookies 文件路径（程序会在扫码后写入）
path = "cookies.jsonl"
"#
}

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
    // 若文件不存在，生成默认模板
    if !path.exists() {
        if let Err(e) = std::fs::write(&path, default_config_toml()) {
            eprintln!("写入默认 config.toml 失败: {e:#}");
        } else {
            println!("已生成默认配置文件: {}", path.display());
        }
    }
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
