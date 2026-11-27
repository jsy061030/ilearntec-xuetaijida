# 学在吉大工具集

![icon](./ilearn-gui/src-tauri/icons/icon.png)

吉林大学“学在吉大”课程录播的非官方桌面工具，包含：

| 目录 | 内容 |
|------|------|
| [`ilearn-cli`](./ilearn-cli) | Rust 核心库 + 命令行：登录、导出课程/录播/视频/字幕信息 |
| [`ilearn-gui`](./ilearn-gui) | Tauri 2 + Svelte 5 桌面客户端：登录、浏览、双路视频 + VTT 字幕 |

## CLI

```powershell
cd ilearn-cli
cargo run --release -- --qr
```

或构建后：

```powershell
cargo build --release
.\target\release\ilearn-cli.exe --qr
```

常用：

```powershell
# 扫码登录，交互选择
.\ilearn-cli.exe --qr

# 密码 + 双验证码
.\ilearn-cli.exe --username 你的学号

# 导出某学期全部课程、录播、视频和字幕 URL
.\ilearn-cli.exe --term-year 2026-2027 --term 1 --all --out dump.json

# 导出并下载字幕
.\ilearn-cli.exe --course-id <lessonId> --all --download-subtitle --out dump.json
```

## GUI

```powershell
cd ilearn-gui
npm install
npm run tauri dev
```

构建：

```powershell
npm run tauri build
```

产物：

```text
ilearn-gui/src-tauri/target/release/ilearn-gui.exe
```

GUI 支持：

- 扫码 / 密码 + 双验证码登录
- 学期、课程、录播浏览
- 可收缩、固定的课程侧边栏
- 教师机位 + HDMI 双路 MP4 播放
- 本地媒体代理：复用登录 Cookie、补 Referer、转发 Range（206）
- VTT 字幕在视频下方滚动展示，当前句高亮并跟随播放
- 字幕签名地址复制
- 已禁用视频下载和视频地址查看
- 缓冲状态提示，HDMI 改为手动对齐
- 顶部版本按钮按语义版本检查 GitHub Release 更新

## 自动构建与发布

每次合并到 `main` 后，GitHub Actions 会自动构建并发布：

- Windows x86_64 / ARM64（NSIS 安装包）
- macOS Universal（DMG，内含 Universal `.app`）
- Linux x86_64 / ARM64（AppImage）

工作流：`.github/workflows/release.yml`

## 数据与安全

- 会话 Cookie 默认保存在各程序自己的缓存目录。
- 视频和字幕是 `jwcmedia.jlu.edu.cn` 的预签名 URL，通常 24 小时有效。
- 不要把 `session.json`、抓包文件、签名 URL 提交到 Git 或公开分享。
- 仅限个人学习使用，请遵守吉林大学账号和课程资源使用规定。

## 许可

BSD-3-Clause

