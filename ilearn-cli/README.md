# ilearn-cli

学在吉大（ilearntec.jlu.edu.cn）命令行客户端：完成登录，导出学期、课程、录播和视频信息。

只做信息查询，不下载视频、不播放视频。视频字段里会直接给出带签名的 MP4 地址。

## 功能

- 密码登录 + 图片验证码 + 企业微信验证码
- 企业微信扫码登录（`--qr`）
- 本地会话缓存，减少重复登录
- 获取学期、课程、周列表、录播列表
- 获取每一条录播的 `videoClassInfo`：教师机位 / HDMI 两路 MP4、大小
- 记录字幕签名 URL（`videoClassInfo.phaseUrl`）和资源中心字幕下载地址
- 可选下载字幕文件
- 输出人类可读摘要或完整 JSON

## 构建

```powershell
cd E:\ilearntec\ilearn-cli
cargo build --release
```

需要 Rust 1.80+。程序只依赖 HTTPS，不需要 GStreamer、MPV 或浏览器。

## 使用

### 1. 扫码登录，交互选择课程和录播

```powershell
cargo run --release -- --qr
```

程序会在缓存目录生成二维码，用企业微信扫码即可。登录后按提示选择学期、课程、录播。

### 2. 密码 + 双验证码登录

```powershell
cargo run --release -- --username 你的学号
```

程序会提示输入密码、图片验证码、企业微信验证码。图片验证码会保存到缓存目录并尝试自动打开。

### 3. 导出某学期全部课程和视频信息

```powershell
cargo run --release -- --term-year 2025-2026 --term 2 --all --out dump.json
```

`--all` 会遍历该学期所有课程、所有录播，并为每条录播请求一次 `videoClassInfo`。

### 4. 只查一门课

```powershell
cargo run --release -- --term-year 2025-2026 --term 2 --course-id <lessonId> --json
```

### 5. 只查一条录播

```powershell
cargo run --release -- --resource-id <resourceId> --json
```

指定 `--resource-id` 且不指定 `--course-id` 时，程序会遍历该学期课程来定位这条录播。

### 6. 只登录并保存会话

```powershell
cargo run --release -- --login-only --qr
```

会话保存在 `.ilearn-cache/session.json`。下次运行会先验证缓存，仍有效则直接复用。

### 7. 忽略缓存，强制重新登录

```powershell
cargo run --release -- --fresh
```

## 输出结构

`--json` 或 `--out` 输出类似：

```json
{
  "generated_at_ms": 0,
  "user": {
    "username": "...",
    "realname": "...",
    "schoolName": "..."
  },
  "term": {
    "id": "...",
    "year": "2025-2026",
    "num": "2",
    "name": "..."
  },
  "weeks": {
    "weekList": [1, 2, 3],
    "currentWeek": "12"
  },
  "courses": [
    {
      "lesson": {
        "id": "...",
        "courseName": "...",
        "teacherName": "...",
        "termId": "..."
      },
      "records": [
        {
          "record": {
            "resourceId": "...",
            "liveRecordName": "...",
            "currentDate": "...",
            "videoClassMap": [
              { "videoClassId": "...", "videoName": "教师机位" },
              { "videoClassId": "...", "videoName": "HDMI" }
            ]
          },
          "video": {
            "resourceName": "...",
            "phaseUrl": "https://...",
            "videoList": [
              {
                "videoCode": "...",
                "videoName": "教师机位",
                "videoPath": "https://jwcmedia...mp4?...",
                "videoSize": "1550863729"
              }
            ]
          },
          "video_error": null,
          "subtitle_url": "https://...（phaseUrl，带签名）",
          "subtitle_file": null,
          "subtitle_error": null
        }
      ]
    }
  ]
}
```

视频地址和字幕 `phaseUrl` 都是预签名 URL，通常 24 小时有效；过期后重新运行即可获取新地址。

`--download-subtitle` 会优先下载 `phaseUrl`；如果失败，再尝试资源中心接口
`/resource-center/word/downLoadCaption?resourceId=...`。两者都是字幕来源，建议以实际文件头（`WEBVTT`）为准。

## 登录链路（实现依据）

```text
CAS 密码/扫码
  → jwcidentity 代理凭据
  → iLearn CAS logout
  → iLearn JSONP get-lt
  → iLearn JSONP 登录（代理用户名 + Base64 密码）
  → iLearn SSO 交换（service=https://ilearntec.jlu.edu.cn/）
  → 第二次 CAS 交换（service=.../coursecenter/main/index）
  → getUserByMid == "1000"
  → 资源中心 /resource-center/user/index 建立独立会话
  → 课程 / 录播 / videoClassInfo
```

关键点：

- `getUserByMid` 的 `code == "1000"` 是唯一可信的登录判据。
- 只做第一次 `service=https://ilearntec.jlu.edu.cn/` 不够，必须再做 `coursecenter/main/index` 的 CAS 交换。
- 视频是 `jwcmedia.jlu.edu.cn` 上的预签名 MP4，不需要额外 Cookie。
- 二次验证页要用 `getWxCode` / `toRecheckLogin` 判断，不能只看 `codeImage`（初始登录页也含 `codeImage`）。

## 目录

```text
src/
  main.rs     CLI 参数、登录编排、导出
  auth.rs     CAS / 扫码 / iLearn SSO / 会话
  api.rs      学期、课程、录播、视频 API
  models.rs   数据模型
  des.rs      JLU 前端 DES 加密
  util.rs     URL 编码、JSONP、HTML 解析、终端输入
```

缓存目录（默认 `./.ilearn-cache`）：

```text
session.json     本地会话 Cookie（脱敏后的 name=value）
captcha.jpg/png  图片验证码
qrcode.png       扫码登录二维码
subtitles/       可选下载的字幕
```

## 安全说明

- 密码只用于当次登录，不写入磁盘。
- `session.json` 含可复用的会话 Cookie，等同登录凭据，请勿分享或提交 Git。
- 抓包/日志不记录密码、`rsa` 密文、验证码值和 Cookie 值。
- 仅限个人学习使用，请遵守吉林大学账号与课程资源使用规定。

## 已知限制

- 视频签名 URL 有有效期，长时间运行后需要重新获取。
- 字幕接口实际返回 VTT 还是 SRT 取决于服务端；当前按 `.vtt` 保存。
- 学校页面/接口变化可能导致登录或解析失败；错误会保留 HTTP 状态和页面特征，便于排查。
- 只导出信息，不下载视频、不播放视频。
