# SelfAni

一个使用 **Rust** 开发的 B 站番剧/视频搜索和流媒体处理工具。

---

> [!WARNING]
> 本项目仅供学习交流使用，禁止用于任何商业用途。所有接口与类型均来源于公开网络文档，项目仅对其整理与聚合。若有侵权请联系删除。  
> 本项目不保证功能的准确性与可靠性，使用本项目所导致的一切后果与作者无关。
> 本项目随时可能因为接口变动、许可协议变更等原因导致无法使用或者停止维护。

## 功能特性

- 🔍 **番剧搜索**: 支持通过关键词搜索 B 站番剧内容
- 📺 **详细信息**: 获取番剧详细信息，包括封面、简介、状态等
- 🎬 **HLS 流媒体**: 提供 HLS 视频流处理和播放支持
- 🍪 **Cookie 管理**: 自动管理登录 Cookie
- 🌐 **Web API**: 提供 RESTful API 接口，支持跨域访问
- ⚡ **高性能**: 基于 Tokio 异步运行时和 Actix-web 框架

## 项目结构

```
selfani/
├── src/
│   ├── main.rs          # 主程序入口，Web 服务器
│   ├── config.rs        # 配置文件管理
│   ├── cookies.rs       # Cookie 处理和管理
│   ├── search.rs        # 番剧搜索功能
│   ├── playurl.rs       # 视频播放地址解析
│   ├── hls.rs           # HLS 流媒体处理
│   └── wbi.rs           # B 站 WBI 签名
├── config.toml          # 配置文件
├── cookies.jsonl        # Cookie 存储文件
└── data/               # 数据存储目录
```

## 快速开始

### 环境要求

- Rust 1.70+
- Tokio 运行时

### 安装

1. 克隆项目
```bash
git clone https://github.com/tnxg/selfani.git
cd selfani
```

2. 构建项目
```bash
cargo build --release
```

3. 配置文件
编辑配置文件 `config.toml`，根据需要修改各项设置。

4. 配置 Cookie
将有效的 B 站 Cookie 信息保存到 `cookies.jsonl` 文件中。

### 运行

```bash
cargo run
```

服务默认运行在 `http://127.0.0.1:8080`

## API 接口

### 搜索番剧

**GET** `/search?q={keyword}`

搜索指定的番剧内容。

**参数:**
- `q`: 搜索关键词

**响应示例:**
```json
{
  "code": 0,
  "success": true,
  "message": "",
  "data": [
    {
      "id": "12345",
      "title": "番剧标题",
      "cover": "https://...",
      "description": "番剧简介",
      "year": "2024",
      "status": "完结",
      "type": "TV",
      "url": "http://127.0.0.1:8080/detail/12345"
    }
  ]
}
```

### 获取详情

**GET** `/detail/{season_id}`

获取指定番剧的详细信息，包括剧集列表。

**参数:**
- `season_id`: 番剧 ID

**响应示例:**
```json
{
  "code": 0,
  "success": true,
  "message": "",
  "data": {
    "id": "12345",
    "title": "番剧标题",
    "cover": "https://...",
    "description": "番剧简介",
    "year": "2024",
    "status": "完结",
    "type": "TV",
    "sources": [
      {
        "name": "第1集 标题",
        "sort": 1,
        "url": "http://127.0.0.1:8080/play/12345/1"
      }
    ]
  }
}
```

### HLS 播放列表

**GET** `/hls/{season_id}/{sort}/index.m3u8`

获取指定剧集的 HLS 播放列表。

### HLS 视频分片

**GET** `/hls/{season_id}/{sort}/{seg}`

获取 HLS 视频分片文件。

## 配置说明

### config.toml

```toml
# API 服务配置
[api]
bind = "127.0.0.1:8080"           # 服务绑定地址
public_base = "http://127.0.0.1:8080"  # 公共访问地址
enable_cache = true               # 启用缓存
cache_dir = "cache"               # 缓存目录

# 存储配置
[storage]
base_dir = "data"                 # 基础保存目录
pgc_template = "{season_title}/[{ep}]{title}"  # 番剧命名模板
ugc_template = "{title}"          # 普通视频命名模板
stream_suffix = ""                # 流文件后缀
stream_ext = "mp4"                # 流文件扩展名

# Cookie 配置
[cookies]
path = "cookies.jsonl"            # Cookie 文件路径
```

## 开发

### 项目依赖

主要依赖库：
- `actix-web`: Web 框架
- `tokio`: 异步运行时
- `reqwest`: HTTP 客户端
- `serde`: 序列化/反序列化
- `anyhow`: 错误处理
- `regex`: 正则表达式

## 注意事项

1. **Cookie 重要性**: 某些功能需要有效的 B 站 Cookie 才能正常工作
2. **使用限制**: 请遵守 B 站的服务条款和使用限制
3. **仅供学习**: 本项目仅用于学习和研究目的

## 许可证

本项目采用 AGPL 3 许可证。

## 贡献

欢迎提交 Issue 和 Pull Request！

## 使用须知

1. **信息安全**

   * 本项目**不收集**任何用户信息，包括用户名、密码、Cookie 等。
   * 使用中的 `csrf_token` 和 `SESSDATA` 等 Cookie 字段均为用户登录后本地获取，**属于高度敏感信息**，请务必妥善保管。
   * `SESSDATA` 的敏感程度等同于“密码 + 验证码”，切勿泄露。

2. **合法使用**

   * 本工具仅限用于技术学习与研究，**禁止用于违反哔哩哔哩用户协议的行为**。
   * 作者不对使用本工具产生的任何封号、风控等后果负责。
   * 严禁将获取的信息用于未授权的多平台转播等违规行为。