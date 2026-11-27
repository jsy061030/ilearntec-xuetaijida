# 学在吉大桌面客户端（Tauri + Svelte）

基于 `ilearn-core` 的桌面 GUI：登录、浏览学期/课程/录播、双路视频播放、VTT 字幕。

## 功能

- 企业微信扫码登录
- 密码 + 图片验证码 + 企业微信验证码
- 本地会话缓存（重启后自动复用有效会话）
- 学期 → 课程 → 录播浏览
- 教师机位 + HDMI 双路 MP4 播放
- 本地媒体代理：复用登录 Cookie、补 Referer、转发 Range（206）
- VTT 字幕自动加载，在视频下方以可滚动字幕稿展示
- 当前字幕高亮，并随播放自动滚动；可暂停跟随、点击字幕跳转
- 课程侧边栏固定、可收起
- 字幕地址一键复制
- 双路同步播放、暂停、HDMI 手动对齐
- 已禁用视频下载、视频地址查看/复制和视频右键菜单
- 顶部版本按钮可检查 GitHub Release 更新
- 每个视频独立显示缓冲状态；不再每秒强制 seek HDMI，避免短暂卡顿

## 开发

```powershell
cd E:\ilearntec\ilearn-gui
npm install
npm run tauri dev
```

## 构建

```powershell
npm run tauri build
```

产物：

```text
src-tauri/target/release/ilearn-gui.exe
```

如果只需要可执行文件、不生成安装包：

```powershell
npx tauri build --no-bundle
```

## 目录

```text
ilearn-gui/
  src/
    lib/api.ts             # invoke 封装和 TS 类型
    routes/+page.svelte    # 登录 / 浏览 / 播放器
  src-tauri/
    src/lib.rs             # Tauri commands、状态、登录编排
    src/proxy.rs           # 127.0.0.1 媒体代理（Cookie/Referer/Range）
    tauri.conf.json
  package.json
```

## 后端命令

| 命令 | 作用 |
|------|------|
| `load_session` | 加载本地会话并验证 `getUserByMid` |
| `start_qr` | 生成二维码，返回 data URL |
| `poll_qr` | 轮询扫码状态；成功后完成 SSO |
| `start_password` | 提交密码；返回是否需要二次验证和验证码图片 |
| `refresh_captcha` | 刷新图片验证码 |
| `request_wx` | 触发企业微信验证码 |
| `submit_wx` | 提交图片验证码 + 企业微信验证码 |
| `logout` | 清空会话和缓存 |
| `get_terms` | 学期列表 |
| `get_lessons` | 课程列表 |
| `get_records` | 录播列表 |
| `get_proxy_base` | 返回本地媒体代理地址 |
| `get_video` | `videoClassInfo`：两路 MP4 + 字幕 URL |
| `get_subtitle` | 下载 VTT 文本（用于 Blob 字幕） |

## 媒体代理

WebView 播放 `jwcmedia.jlu.edu.cn` 时，部分环境会因 Cookie、Referer 或跨域/PNA 限制失败。后端启动一个只监听 `127.0.0.1` 的媒体代理：

```text
http://127.0.0.1:<port>/media?url=<encodeURIComponent(原始签名 URL)>
```

代理会：

- 复用登录后的 Cookie Jar
- 固定 `Referer: https://ilearntec.jlu.edu.cn/` 和 `Origin`
- 原样转发 `Range` / `If-Range`，支持 206 分段播放
- 把 `application/octet-stream` 修正为 `video/mp4`
- 返回 `Access-Control-Allow-Origin: *`

前端 `<video>` 使用代理地址；复制按钮仍然复制原始签名 URL。

## 登录与播放说明

```text
CAS 密码/扫码
  → jwcidentity 代理凭据
  → iLearn logout + JSONP get-lt + JSONP 登录
  → SSO 交换（service=https://ilearntec.jlu.edu.cn/）
  → 第二次 CAS 交换（service=.../coursecenter/main/index）
  → getUserByMid == "1000"
  → 资源中心 /resource-center/user/index
  → 课程 / 录播 / videoClassInfo
```

视频和字幕都是 `jwcmedia.jlu.edu.cn` 上的预签名 URL，通常 24 小时有效。播放前如果地址过期，重新选择录播即可获取新地址。

字幕不使用 `<track src="https://jwcmedia...">` 直接跨域加载，而是由 Rust 下载 VTT 文本，前端解析成 cue 列表，在视频下方滚动显示；当前 cue 会高亮并随播放自动滚动。这样既避免 CORS，也方便阅读完整字幕稿。

## 安全

- 密码只用于当次登录，不写入磁盘。
- 会话 Cookie 保存在 Tauri 应用缓存目录的 `session.json`，等同登录凭据，请勿分享。
- 视频/字幕 URL 带签名，不要公开传播。
- 仅限个人学习使用，遵守吉林大学账号与课程资源规定。

## 已知限制

- 视频签名 24 小时过期；过期后重新选择录播。
- 双击字幕/播放进度的持久化尚未实现。
- 只做播放，不下载视频。
- 学校页面结构变化可能导致登录失效；错误会显示在界面上。
