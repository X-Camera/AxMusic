<div align="center">
  <img src="src/assets/axmusic-icon.svg" width="104" alt="AxMusic" />
  <h1>AxMusic</h1>
  <p><strong>Windows 本地音乐播放器 + 洗库管理工具</strong></p>
  <p>绿色单文件 · 以 FLAC 为主，兼顾 MP3 / M4A / Opus · 打开就能听，进「管理」收拾干净</p>
  <p>
    <img alt="平台" src="https://img.shields.io/badge/平台-Windows%2010%20%2F%2011%20x64-7c6af2" />
    <img alt="许可" src="https://img.shields.io/badge/许可-MIT-blue" />
    <img alt="Tauri" src="https://img.shields.io/badge/Tauri-2-24c8d8" />
  </p>
</div>

## 📸 截图

| 歌曲网格 | 满窗播放 |
| --- | --- |
| ![歌曲网格](pic/ScreenShot_0.png) | ![满窗播放](pic/ScreenShot_1.png) |

## ✨ 特性

**🎵 播放**
- 任意路径音频打开即播，不依赖曲库；FLAC / MP3 / M4A / Opus
- 顺序 / 随机 / 单曲循环；迷你播放条常驻，满窗播放页封面 + 滚动歌词
- 外挂 `.lrc` 歌词优先，支持内嵌歌词读写、嵌↔挂互转
- 托盘驻留，关闭可询问；库外歌曲可一键「纳入库管理」

**🖼️ 浏览**
- 歌曲网格 / 列表、专辑墙与专辑详情、歌手、目录树（递归 / 多选 / 批量入歌单）
- 歌单管理；系统「喜爱」歌单置顶，全列表心形切换

**🛠️ 管理（洗库工作区）**
- 库目录向导 → 扫描建档（SQLite 仅作工作区，播放不依赖）
- 标签 / 封面对比编辑：右侧对比面板勾选字段后写入，**空值永不覆盖**已有标签
- 歌词三源聚合搜索：LRCLIB / 网易云 / QQ 音乐并发请求，流式回显，先回先显示
- MusicBrainz 刮削：候选整张先入本地 catalog，确认采纳才写文件；限速 ≤1 req/s 合规访问

**📦 绿色便携**
- 单文件 exe，解压即用；应用数据全部放在 `exe 同目录/data/`，不写注册表、不污染系统

## 🔨 从源码构建

环境要求：Node.js ≥ 18、Rust stable、[Tauri 2 前置依赖](https://tauri.app/start/prerequisites/)（MSVC Build Tools + WebView2）。

```bash
npm install
npm run tauri dev      # 开发调试
build-release.bat      # 发布构建，绿色版输出到 out/AxMusic-v*.exe
```

## 🧰 技术栈

| 层 | 选型 |
| --- | --- |
| 应用壳 | Tauri 2（Rust） |
| 前端 | React 18 · TypeScript · Vite · zustand |
| 标签读写 | lofty |
| 音频解码 / 输出 | Symphonia / cpal |
| 工作库 | rusqlite（bundled SQLite，存于 `<库根>/axmusic.db`） |
| 刮削网络 | reqwest（blocking，限速 ≤1 req/s） |

## 📚 文档

| 文档 | 内容 |
| --- | --- |
| [docs/产品需求.md](docs/产品需求.md) | 定位、功能、库目录流程、整理规则、验收 |
| [docs/技术架构.md](docs/技术架构.md) | 技术栈、播放引擎、数据源、数据模型、绿色版打包 |
| [docs/界面设计.md](docs/界面设计.md) | 暗色 UI、布局、Token、关键界面 |
| [docs/开发计划.md](docs/开发计划.md) | 已定决策、里程碑、开工顺序 |

## 🙏 致谢

- [MusicBrainz](https://musicbrainz.org/) 开放音乐元数据库
- [LRCLIB](https://lrclib.net/) 免费同步歌词库，以及网易云音乐、QQ 音乐的公开歌词接口
- [Tauri](https://tauri.app/)、[lofty](https://github.com/Serial-ATA/lofty-rs)、[Symphonia](https://github.com/pdeljanov/Symphonia) 等开源项目

## 📄 许可

[MIT](LICENSE) © AxMusic contributors
