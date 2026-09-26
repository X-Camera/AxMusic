import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

export const VIZ_SETTINGS_WINDOW_LABEL = "viz-settings";

/**
 * 打开独立「背景动效」设置子窗口。已存在则聚焦。
 * 真系统窗口，可拖出主应用、放到其他显示器；
 * 与主窗口经 settings://changed 双向同步（改动即时预览在主窗口歌词区）。
 */
export async function openVizSettingsWindow(): Promise<void> {
  const existing = await WebviewWindow.getByLabel(VIZ_SETTINGS_WINDOW_LABEL);
  if (existing) {
    await existing.setFocus();
    return;
  }

  const win = new WebviewWindow(VIZ_SETTINGS_WINDOW_LABEL, {
    url: "index.html?win=viz-settings",
    title: "背景动效",
    // 固定大小：按参数最多的效果（环线/频谱/粒子 4 项）+ 公共参数全量排版，不出滚动条
    width: 460,
    height: 780,
    center: true,
    resizable: false,
    maximizable: false,
    decorations: false,
    focus: true,
  });

  await new Promise<void>((resolve, reject) => {
    const offCreated = win.once("tauri://created", () => {
      void offErr.then((f) => f());
      resolve();
    });
    const offErr = win.once("tauri://error", (e) => {
      void offCreated.then((f) => f());
      reject(e.payload ?? new Error("创建背景动效窗口失败"));
    });
  });
}
