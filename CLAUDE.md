# CLAUDE.md

## 项目是什么

AxMusic — Windows 本地**音乐播放器 + 管理工具**（绿色版 exe）。默认听歌（任意路径可播）；侧栏「管理」是洗库工作区（标签/封面/歌词/刮削/整理）。格式以 FLAC 为主，兼顾 MP3/M4A/Opus。

**文档即规格**：改功能前先对照 `docs/`（产品需求 / 技术架构 / 界面设计 / 开发计划），实现与文档冲突时先问再改。

## 技术栈

- 壳：**Tauri 2**（Rust），前端 React 18 + TypeScript + Vite + zustand，图标 lucide-react
- 标签读写：**lofty 0.21**；播放：Symphonia + cpal；HTTP：reqwest blocking（刮削，限约 1 req/s）
- 工作库：**SQLite**（rusqlite bundled），见下方数据模型

## 常用命令

```bash
npm run tauri dev        # 开发
npm run build            # = tsc && vite build（前端类型检查+打包）
cd src-tauri && cargo check   # Rust 快速编译检查（改后端后必跑）
build-release.bat        # 绿色版发布构建 → out/AxMusic-v*.exe
```

没有测试套件；验证 = `cargo check` + `npx tsc --noEmit` + 手动跑 `tauri dev` 走主路径。

## 核心数据模型（最重要，别搞反）

**三张表两个世界**，存在 `<库根>/axmusic.db`：

| 表 | 是什么 | 谁来写 |
|---|---|---|
| `tracks` | **文件当前**字段（扫描结果，可缺可错） | scanner |
| `catalog` | **在线元数据的本地子集备份**（被采纳的刮削结果，整张专辑曲目表落库） | scraper → catalog_save |
| `tracks.catalog_id` | 两世界的关联；NULL = 待刮削 | 显式绑定 / 字段自动匹配 |

铁律：

1. **刮削不改音频文件**。采纳候选 = 只写 catalog（整张落库）+ 封面到 `<库>/covers/`；改文件只在管理表选中行 → 右侧对比面板 → 用户勾选字段 →「写入文件」（写前备份到 `data/tag_backups/`，**空值永不覆盖**已有标签）
2. **入库（纳入库管理）只对库外文件**——入口在播放侧（迷你播放条），管理表里全是库内文件，不放此按钮
3. **歌词默认外挂 `.lrc`**（同目录同名，兼容性优先）；内嵌 LYRICS/USLT 是选项；嵌↔挂互转走 `lyrics` 模块，内嵌写回走 tagger（写前备份）
4. **歌词多源聚合**：LRCLIB / 网易云 / QQ音乐 并发搜索，结果经 `lyrics://batch` 事件流式推前端（先回先显示），候选 id 带来源前缀 `lrclib:xx`/`netease:xx`/`qq:xx`；新增源实现 `lyrics/<source>.rs` 的 `search`/`fetch` 并在 `lyrics::fetch` 和 `lyrics_search` 命令注册
3. 播放**不依赖** SQLite；DB 只是管理工作区。应用数据在 `exe_dir/data/`，代码统一走 `paths::data_root()`，禁止写死路径

匹配顺序（`find_catalog_fuzzy`）：MBID（录音→发行+轨号）→ title+artist+album → title+artist，命中即持久化 `catalog_id`。`catalog_save` 后对全库未关联曲目跑 `auto_match_unlinked()`。

## 代码结构

```
src-tauri/src/   commands.rs(IPC 全部在此) · scanner · tagger · scraper(musicbrainz+coverart)
                 lyrics(LRCLIB+嵌/挂互转) · library(SQLite) · player(symphonia+cpal) · settings · paths
src/features/    manage/(ManagePage 表格 · TrackTable · ComparePanel 侧栏 · ScrapeWizard 三栏
                 · LyricsPanel 补歌词) · browse/ · components/(Sidebar/MiniPlayer/TopBar) · state/useApp.ts
src/lib/         api.ts（invoke 封装）· types.ts（与 Rust serde 结构对齐）
```

## 约定

- **界面文案中文**、短、动词开头；错误写清怎么办
- UI 暗色主题，颜色/间距/圆角全部用 `src/styles/tokens.css` 的 CSS 变量，禁止写死颜色；禁止 Win32 灰面板风
- Rust 与 TS 的结构体字段保持一致（serde 直传），加字段两边同步 + `types.ts` 同步
- SQLite 迁移：`CREATE TABLE IF NOT EXISTS` + 逐列 `ALTER TABLE ... ADD COLUMN`（容忍已存在），不支持删列
- MusicBrainz 合规：UA 带联系信息、限速 ≤1 req/s（`scraper::rate_limit_wait`）、不上传音频内容
- Git：主分支 `master`，提交信息中文 conventional 格式（参照 `git log`）
