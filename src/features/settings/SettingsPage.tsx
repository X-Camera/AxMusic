import { listen } from "@tauri-apps/api/event";
import { FolderOpen } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { api } from "../../lib/api";
import { COLOR_SCHEMES, THEME_MODES, applyColorScheme, applyThemeMode } from "../../lib/colorScheme";
import { LYRICS_FONTS } from "../../lib/lyricsDisplay";
import type {
  AppSettings,
  CloseBehavior,
  LyricsFont,
  LyricsPrefer,
  LyricsSaveMode,
  PathsInfo,
  RepeatMode,
  SongsView,
  ThemeMode,
} from "../../lib/types";
import { useApp } from "../../state/useApp";
import { TopBar } from "../../components/TopBar";
import { BrandMark } from "../../components/BrandMark";
import "./SettingsPage.css";

const REPEAT_MODES: { id: RepeatMode; label: string }[] = [
  { id: "off", label: "关闭" },
  { id: "all", label: "列表循环" },
  { id: "one", label: "单曲" },
];

const LYRICS_SAVE: { id: LyricsSaveMode; label: string }[] = [
  { id: "sidecar", label: "外挂 .lrc" },
  { id: "embed", label: "内嵌标签" },
];

const LYRICS_PREFER: { id: LyricsPrefer; label: string }[] = [
  { id: "sidecar", label: "外挂优先" },
  { id: "embed", label: "内嵌优先" },
];

const SONGS_VIEWS: { id: SongsView; label: string }[] = [
  { id: "list", label: "列表" },
  { id: "grid", label: "卡片" },
];

const CLOSE_BEHAVIORS: { id: CloseBehavior; label: string }[] = [
  { id: "ask", label: "询问" },
  { id: "tray", label: "缩到托盘" },
  { id: "exit", label: "退出" },
];

const FEATURES = [
  "任意路径本地播放，专辑 · 歌曲 · 歌手一站浏览",
  "满窗歌词滚动，点句跳转；支持外挂 .lrc 与内嵌标签",
  "洗库工作区：标签 / 封面 / 歌词批量维护",
  "MusicBrainz 刮削入 catalog，对比确认后写回文件",
  "m3u8 歌单，绿色便携，数据全部留在本机",
];

function Segmented<T extends string>({
  value,
  options,
  onChange,
  disabled,
}: {
  value: T;
  options: { id: T; label: string }[];
  onChange: (v: T) => void;
  disabled?: boolean;
}) {
  return (
    <div className="set-seg" role="group">
      {options.map((o) => (
        <button
          key={o.id}
          type="button"
          className={`set-seg-btn${value === o.id ? " active" : ""}`}
          disabled={disabled}
          onClick={() => onChange(o.id)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

function Toggle({
  checked,
  onChange,
  disabled,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      className={`set-toggle${checked ? " on" : ""}`}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    >
      <span className="set-toggle-knob" />
    </button>
  );
}

function Row({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="set-row">
      <div className="set-row-label">
        <div className="set-row-title">{label}</div>
        {hint && <div className="set-row-hint tertiary">{hint}</div>}
      </div>
      <div className="set-row-ctrl">{children}</div>
    </div>
  );
}

/** 滑杆 + 数值（默认音量 / 满窗歌词字号、行距共用） */
function RangeCtrl({
  min,
  max,
  step,
  value,
  label,
  format,
  onChange,
}: {
  min: number;
  max: number;
  step: number;
  value: number;
  label: string;
  format: (v: number) => string;
  onChange: (v: number) => void;
}) {
  const pct = ((value - min) / (max - min)) * 100;
  return (
    <div className="set-vol">
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        aria-label={label}
        onChange={(e) => onChange(Number(e.target.value))}
        style={{ ["--pct" as string]: `${pct}%` }}
      />
      <span className="set-vol-val mono">{format(value)}</span>
    </div>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="set-section">
      <h2 className="set-section-title">{title}</h2>
      <div className="set-card">{children}</div>
    </section>
  );
}

function AboutPanel({
  appInfo,
  paths,
}: {
  appInfo: { name: string; version: string } | null;
  paths: PathsInfo | null;
}) {
  return (
    <div className="set-about-scroll" aria-label="关于 AxMusic">
      <div className="set-about-hero">
        <div className="set-about-mark">
          <BrandMark size={88} />
        </div>
        <h2 className="set-about-name">AxMusic</h2>
        <p className="set-about-ver mono">
          版本 {appInfo?.version ?? "—"}
          {paths?.portable ? " · 绿色便携" : ""}
        </p>
        <p className="set-about-tag">本地音乐播放 · 离线曲库管理</p>
      </div>

      <div className="set-about-body">
        <p>
          AxMusic 是一款 Windows 本地音乐播放器与管理工具。以 FLAC
          为主，兼顾 MP3 / M4A / Opus。打开就能听——任意路径文件皆可播放；
          侧栏「管理」是洗库工作区，用来整理标签、封面、歌词，并支持从
          MusicBrainz 刮削元数据。
        </p>
        <p>不上传音频，不绑账号，绿色单文件解压即用。</p>
      </div>

      <div className="set-about-block">
        <h3>主要功能</h3>
        <ul className="set-about-list">
          {FEATURES.map((f) => (
            <li key={f}>{f}</li>
          ))}
        </ul>
      </div>

      <div className="set-about-block">
        <h3>运行信息</h3>
        <dl className="set-about-meta">
          <div>
            <dt>系统</dt>
            <dd>Windows 10 / 11 x64</dd>
          </div>
          <div>
            <dt>数据目录</dt>
            <dd className="set-about-path">
              <span className="mono" title={paths?.data_root ?? ""}>
                {paths?.data_root ?? "—"}
              </span>
              {paths?.data_root && (
                <button
                  className="link-btn"
                  onClick={() => void api.openPath(paths.data_root)}
                >
                  打开
                </button>
              )}
            </dd>
          </div>
          <div>
            <dt>工作库</dt>
            <dd className="mono set-about-db" title={paths?.db_path ?? ""}>
              {paths?.db_path || "未初始化"}
            </dd>
          </div>
          <div>
            <dt>许可</dt>
            <dd>MIT License</dd>
          </div>
        </dl>
      </div>

      <p className="set-about-copy tertiary">
        © {new Date().getFullYear()} AxMusic · MIT License
      </p>
    </div>
  );
}

export function SettingsPage() {
  const setRoute = useApp((s) => s.setRoute);
  const queuePanelOpen = useApp((s) => s.queuePanelOpen);
  const lyricsPanelOpen = useApp((s) => s.lyricsPanelOpen);
  const setQueuePanelOpen = useApp((s) => s.setQueuePanelOpen);
  const setLyricsPanelOpen = useApp((s) => s.setLyricsPanelOpen);
  const sideOpen = queuePanelOpen || lyricsPanelOpen;
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [paths, setPaths] = useState<PathsInfo | null>(null);
  const [appInfo, setAppInfo] = useState<{ name: string; version: string } | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void Promise.all([api.getSettings(), api.getPaths(), api.getAppInfo()])
      .then(([s, p, a]) => {
        if (cancelled) return;
        setSettings(s);
        setPaths(p);
        setAppInfo(a);
      })
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, []);

  // 顶栏主题开关等外部改设置时，保持本页 Segmented 同步
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<AppSettings>("settings://changed", (e) => setSettings(e.payload)).then(
      (f) => {
        unlisten = f;
      },
    );
    return () => unlisten?.();
  }, []);

  const patch = useCallback(async (p: Parameters<typeof api.updateSettings>[0]) => {
    try {
      const next = await api.updateSettings(p);
      setSettings(next);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  if (!settings) {
    return (
      <>
        <TopBar title="设置" />
        <div className={`page-scroll${sideOpen ? " queue-squeeze-self" : ""}`}>
          <div className="empty-state">
            <p className="muted">{error ?? "加载中…"}</p>
          </div>
        </div>
      </>
    );
  }

  const src = settings.lyrics_sources;

  return (
    <>
      <TopBar title="设置" />
      <div className="set-shell">
        <div
          className="set-main"
          onPointerDown={() => {
            // 点左侧区域：收起播放列表/歌词，恢复本页右边栏
            if (sideOpen) {
              setQueuePanelOpen(false);
              setLyricsPanelOpen(false);
            }
          }}
        >
          {error && <div className="error-line set-error">{error}</div>}

            <Section title="播放">
              <Row label="默认音量" hint="迷你条/满窗共用，调节后即时记住">
                <div className="set-vol">
                  <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.01}
                    value={settings.volume}
                    aria-label="默认音量"
                    onChange={(e) => {
                      const v = Number(e.target.value);
                      setSettings((s) => (s ? { ...s, volume: v } : s));
                      void api
                        .playerSetVolume(v)
                        .catch(() => void patch({ volume: v }));
                    }}
                    style={{ ["--pct" as string]: `${settings.volume * 100}%` }}
                  />
                  <span className="set-vol-val mono">
                    {Math.round(settings.volume * 100)}%
                  </span>
                </div>
              </Row>
              <Row label="随机播放" hint="与循环独立；开=乱序，关=按列表顺序">
                <Toggle
                  checked={settings.shuffle}
                  onChange={(v) => {
                    setSettings((s) => (s ? { ...s, shuffle: v } : s));
                    void useApp.getState().setShuffle(v);
                  }}
                />
              </Row>
              <Row label="循环" hint="关闭=播完停 / 列表循环 / 单曲循环">
                <Segmented
                  value={settings.repeat}
                  options={REPEAT_MODES}
                  onChange={(v) => {
                    setSettings((s) => (s ? { ...s, repeat: v } : s));
                    void useApp.getState().setRepeat(v);
                  }}
                />
              </Row>
              <Row label="启动恢复音量" hint="关闭后每次启动固定 80%">
                <Toggle
                  checked={settings.restore_volume}
                  onChange={(v) => void patch({ restore_volume: v })}
                />
              </Row>
            </Section>

            <Section title="歌词">
              <Row label="默认保存" hint="搜索歌词后写入方式，可逐次改">
                <Segmented
                  value={settings.lyrics_save_mode}
                  options={LYRICS_SAVE}
                  onChange={(v) => void patch({ lyrics_save_mode: v })}
                />
              </Row>
              <Row label="读取优先" hint="外挂与内嵌都有时，播放页听哪份">
                <Segmented
                  value={settings.lyrics_prefer}
                  options={LYRICS_PREFER}
                  onChange={(v) => void patch({ lyrics_prefer: v })}
                />
              </Row>
              <Row label="在线源" hint="至少保留一个；搜索时并发请求已启用源">
                <div className="set-sources">
                  {(
                    [
                      ["lrclib", "LRCLIB"],
                      ["netease", "网易云"],
                      ["qq", "QQ音乐"],
                    ] as const
                  ).map(([key, label]) => {
                    // 最后一个开着的源不可取消（后端也会拒收全关，但前端先拦住更明确）
                    const enabledCount = [src.lrclib, src.netease, src.qq].filter(Boolean).length;
                    const lastOne = src[key] && enabledCount === 1;
                    return (
                      <label
                        key={key}
                        className="set-check"
                        title={lastOne ? "至少保留一个在线源" : undefined}
                      >
                        <input
                          type="checkbox"
                          checked={src[key]}
                          disabled={lastOne}
                          onChange={(e) =>
                            void patch({
                              lyrics_sources: { ...src, [key]: e.target.checked },
                            })
                          }
                        />
                        <span>{label}</span>
                      </label>
                    );
                  })}
                </div>
              </Row>
            </Section>

            <Section title="主界面歌词">
              <Row label="字号" hint="右边栏歌词大小；与满窗歌词分开保存">
                <RangeCtrl
                  min={0.75}
                  max={1.5}
                  step={0.05}
                  value={settings.side_lyrics_font_scale ?? 1}
                  label="主界面歌词字号"
                  format={(v) => `${Math.round(v * 100)}%`}
                  onChange={(v) => void patch({ side_lyrics_font_scale: v })}
                />
              </Row>
              <Row label="字体" hint="主句与译文共用；「默认」跟随应用显示字体">
                <select
                  className="set-select"
                  aria-label="主界面歌词字体"
                  value={settings.side_lyrics_font ?? "display"}
                  onChange={(e) => void patch({ side_lyrics_font: e.target.value as LyricsFont })}
                >
                  {LYRICS_FONTS.map((f) => (
                    <option key={f.id} value={f.id}>
                      {f.label}
                    </option>
                  ))}
                </select>
              </Row>
              <Row label="行距" hint="行与行之间松紧，主句 line-height">
                <RangeCtrl
                  min={1}
                  max={2}
                  step={0.05}
                  value={settings.side_lyrics_line_height ?? 1.25}
                  label="主界面歌词行距"
                  format={(v) => v.toFixed(2)}
                  onChange={(v) => void patch({ side_lyrics_line_height: v })}
                />
              </Row>
            </Section>

            <Section title="满窗歌词">
              <Row label="字号" hint="相对默认大小缩放；满窗右键「歌词样式」可边看边调">
                <RangeCtrl
                  min={0.75}
                  max={1.5}
                  step={0.05}
                  value={settings.lyrics_font_scale ?? 1}
                  label="字号"
                  format={(v) => `${Math.round(v * 100)}%`}
                  onChange={(v) => void patch({ lyrics_font_scale: v })}
                />
              </Row>
              <Row label="字体" hint="主句与译文共用；「默认」跟随应用显示字体">
                <select
                  className="set-select"
                  aria-label="歌词字体"
                  value={settings.lyrics_font ?? "display"}
                  onChange={(e) => void patch({ lyrics_font: e.target.value as LyricsFont })}
                >
                  {LYRICS_FONTS.map((f) => (
                    <option key={f.id} value={f.id}>
                      {f.label}
                    </option>
                  ))}
                </select>
              </Row>
              <Row label="行距" hint="行与行之间松紧，主句 line-height">
                <RangeCtrl
                  min={1}
                  max={2}
                  step={0.05}
                  value={settings.lyrics_line_height ?? 1.25}
                  label="行距"
                  format={(v) => v.toFixed(2)}
                  onChange={(v) => void patch({ lyrics_line_height: v })}
                />
              </Row>
            </Section>

            <Section title="管理">
              <Row label="库根目录" hint="工作库 axmusic.db 与封面都在这里">
                <div className="set-path">
                  <span className="mono set-path-text">
                    {settings.library_root || "未设置"}
                  </span>
                  <button
                    className="btn"
                    onClick={() => setRoute("manage")}
                    title="到管理页更换库根或初始化"
                  >
                    <FolderOpen size={14} /> 管理页
                  </button>
                </div>
              </Row>
            </Section>

            <Section title="界面">
              <Row label="歌曲页视图" hint="默认列表或卡片网格">
                <Segmented
                  value={settings.songs_view}
                  options={SONGS_VIEWS}
                  onChange={(v) => void patch({ songs_view: v })}
                />
              </Row>
              <Row label="关闭主窗口" hint="点 × 时询问 / 缩到托盘 / 退出；托盘可再打开">
                <Segmented
                  value={settings.close_behavior}
                  options={CLOSE_BEHAVIORS}
                  onChange={(v) => void patch({ close_behavior: v })}
                />
              </Row>
              <Row label="外观" hint="深色 / 浅色整套切换">
                <Segmented
                  value={(settings.theme_mode ?? "dark") as ThemeMode}
                  options={THEME_MODES}
                  onChange={(v) => {
                    applyThemeMode(v);
                    void patch({ theme_mode: v });
                  }}
                />
              </Row>
              <Row label="皮肤" hint="强调色（表面中性不偏色）；满窗播放随封面，不跟皮肤">
                <div className="set-swatches" role="radiogroup" aria-label="皮肤">
                  {COLOR_SCHEMES.map((s) => {
                    const mode = settings.theme_mode === "light" ? "light" : "dark";
                    const preview = s[mode];
                    const surface = mode === "light" ? "#f7f8fa" : "#12141a";
                    const card = mode === "light" ? "#ffffff" : "#181b23";
                    return (
                      <button
                        key={s.id}
                        type="button"
                        role="radio"
                        aria-checked={settings.color_scheme === s.id}
                        title={s.label}
                        className={`set-swatch${settings.color_scheme === s.id ? " active" : ""}`}
                        style={{ background: surface }}
                        onClick={() => {
                          applyColorScheme(s.id);
                          void patch({ color_scheme: s.id });
                        }}
                      >
                        <span
                          className="set-swatch-card"
                          style={{ background: card, borderColor: preview.solid }}
                        />
                        <span
                          className="set-swatch-accent"
                          style={{ background: preview.solid }}
                        />
                        <span className="sr-only">{s.label}</span>
                      </button>
                    );
                  })}
                </div>
              </Row>
            </Section>
        </div>

        {/* 右边栏壳与管理页一致；播放列表/歌词激活时让位（App 常驻 dock 覆盖） */}
        <div className="set-about">
          {!sideOpen && <AboutPanel appInfo={appInfo} paths={paths} />}
        </div>
      </div>
    </>
  );
}
