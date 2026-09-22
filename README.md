# AxMusic

Windows 本地**音乐播放器 + 管理工具**。默认听歌；侧栏「管理」批量洗库（标签/封面/歌词/刮削/整理）。以 **FLAC** 为主，兼顾 MP3 / M4A / Opus。交付**绿色版 exe**。

## 产品一句话

具备管理功能的音乐播放器：打开就能听任意路径音频；进「管理」设置库目录、扫描建档、把标签和文件收拾干净。

```text
启动 → 播放（任意路径）→ 侧栏「管理」
         │                    │
         │              库目录向导 → 初始化
         │              手动放入 → 刷新扫描
         │              补字段 / 刮削 / 整理
         └─ 底栏迷你播放条（始终在）
         └─ 「纳入库管理」把库外歌收进库
```

## 文档

| 文档 | 内容 |
|------|------|
| [docs/产品需求.md](docs/产品需求.md) | 定位、功能、库目录流程、整理规则、验收 |
| [docs/技术架构.md](docs/技术架构.md) | 技术栈、播放引擎、数据源、数据模型、绿色版打包 |
| [docs/界面设计.md](docs/界面设计.md) | 暗色现代化 UI、布局、Token、关键界面 |
| [docs/开发计划.md](docs/开发计划.md) | 已定决策、里程碑、开工顺序 |

## 目标用户

本地无损收藏者：要标签干净、目录规范、歌词齐，也要天天拿来听歌。

## 开发状态

M0 骨架已可启动：Tauri 2 + React + TS、暗色 UI 壳、`paths::data_root()`、`PlayerEngine`（Symphonia + cpal）、管理库向导/扫描档案表、`scripts/package-portable.ps1`。新会话请先读 `docs/开发计划.md`。

```bash
npm install
npm run tauri dev          # 开发
npm run tauri build        # 构建
./scripts/package-portable.ps1   # 绿色 zip
```

## 许可

待定。
