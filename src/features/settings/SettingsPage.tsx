import { FolderOpen, ListVideo } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { api } from "../../lib/api";
import type {
  AppSettings,
  LyricsPrefer,
  LyricsSaveMode,
  PathsInfo,
  PlayMode,
  SongsView,
} from "../../lib/types";
import { useApp } from "../../state/useApp";
import { TopBar } from "../../components/TopBar";
import "./SettingsPage.css";

const PLAY_MODES: { id: PlayMode; label: string }[] = [
  { id: "sequential", label: "顺序" },
  { id: "shuffle", label: "随机" },
  { id: "repeat_one", label: "单曲" },
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

const FEATURES = [
  "任意路径本地播放，专辑墙 · 歌曲 · 歌手一站浏览",
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
    <aside className="set-about" aria-label="关于 AxMusic">
      <div className="set-about-hero">
        <div className="set-about-mark">
          <ListVideo size={36} strokeWidth={2} />
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
        </dl>
      </div>

      <p className="set-about-copy tertiary">
        © {new Date().getFullYear()} AxMusic · 本地优先的听歌与洗库工具
      </p>
    </aside>
  );
}

export function SettingsPage() {
  const setRoute = useApp((s) => s.setRoute);
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
        <div className="page-scroll">
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
        <div className="set-main">
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
              <Row label="播放模式" hint="顺序播完 / 随机 / 单曲循环">
                <Segmented
                  value={settings.play_mode}
                  options={PLAY_MODES}
                  onChange={(v) => {
                    void patch({ play_mode: v });
                    void useApp.getState().setPlayMode(v);
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
                  ).map(([key, label]) => (
                    <label key={key} className="set-check">
                      <input
                        type="checkbox"
                        checked={src[key]}
                        onChange={(e) =>
                          void patch({
                            lyrics_sources: { ...src, [key]: e.target.checked },
                          })
                        }
                      />
                      <span>{label}</span>
                    </label>
                  ))}
                </div>
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
              <Row label="主题" hint="浅色主题规划中">
                <Segmented
                  value={"dark" as "dark"}
                  options={[{ id: "dark" as const, label: "暗色" }]}
                  onChange={() => {}}
                  disabled
                />
              </Row>
            </Section>
        </div>

        <AboutPanel appInfo={appInfo} paths={paths} />
      </div>
    </>
  );
}
