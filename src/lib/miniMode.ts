import {
  currentMonitor,
  getCurrentWindow,
  LogicalSize,
  PhysicalPosition,
  PhysicalSize,
} from "@tauri-apps/api/window";

/** mini 模式固定逻辑尺寸（封面 + 歌词/进度/控制）
 *  高度 = 封面高 + 边框 1×2；封面 height:100% + aspect-ratio:1 铺满左缘 */
export const MINI_W = 340;
export const MINI_H = 82;
/** 正常模式窗口最小尺寸（= 启动默认尺寸，与 tauri.conf.json 一致） */
const DEFAULT_MIN_W = 1440;
const DEFAULT_MIN_H = 900;
/** 贴桌面角时距工作区边缘 */
const EDGE_PAD = 16;

const win = getCurrentWindow();

interface SavedGeom {
  x: number;
  y: number;
  width: number;
  height: number;
  maximized: boolean;
}

let saved: SavedGeom | null = null;
let busy: Promise<void> = Promise.resolve();

function serialize(job: () => Promise<void>): Promise<void> {
  const run = busy.then(job, job);
  busy = run.catch(() => undefined);
  return run;
}

/** unmaximize 后窗口管理器可能尚未落新几何，等一帧 resized 再读 */
async function waitGeometrySettled(): Promise<void> {
  await new Promise<void>((resolve) => {
    let done = false;
    const finish = () => {
      if (done) return;
      done = true;
      resolve();
    };
    const timer = window.setTimeout(finish, 100);
    void win.onResized(() => {
      window.clearTimeout(timer);
      finish();
    });
  });
}

async function doEnter(): Promise<void> {
  if (saved) return;
  if (await win.isFullscreen().catch(() => false)) {
    await win.setFullscreen(false).catch(() => undefined);
    await waitGeometrySettled();
  }
  const maximized = await win.isMaximized().catch(() => false);
  if (maximized) {
    await win.unmaximize().catch(() => undefined);
    await waitGeometrySettled();
  }

  const pos = await win.outerPosition();
  const size = await win.outerSize();
  const geom: SavedGeom = {
    x: pos.x,
    y: pos.y,
    width: size.width,
    height: size.height,
    maximized,
  };

  // 窗口变更失败必须回滚 saved，否则后续 enter 因 saved 非空早退、卡在半 mini 态
  saved = geom;
  try {
    await win.setAlwaysOnTop(true);
    // 不可调大小：min=max 不够，Windows 边缘仍可拖
    await win.setResizable(false);
    await win.setMinSize(new LogicalSize(MINI_W, MINI_H));
    await win.setMaxSize(new LogicalSize(MINI_W, MINI_H));
    await win.setSize(new LogicalSize(MINI_W, MINI_H));

    // 默认贴当前显示器工作区右下角（桌面角），仍可拖走
    const monitor = await currentMonitor().catch(() => null);
    if (monitor) {
      const s = monitor.scaleFactor;
      const wa = monitor.workArea;
      const x = wa.position.x + wa.size.width - Math.round(MINI_W * s) - Math.round(EDGE_PAD * s);
      const y = wa.position.y + wa.size.height - Math.round(MINI_H * s) - Math.round(EDGE_PAD * s);
      await win.setPosition(new PhysicalPosition(x, y));
    }
  } catch (e) {
    saved = null;
    // 尽力还原，失败也抛给调用方
    await win.setMaxSize(null).catch(() => undefined);
    await win
      .setMinSize(new LogicalSize(DEFAULT_MIN_W, DEFAULT_MIN_H))
      .catch(() => undefined);
    await win.setAlwaysOnTop(false).catch(() => undefined);
    throw e;
  }
}

async function doExit(): Promise<void> {
  // 未进入过 mini：不改写窗口约束
  const geom = saved;
  if (!geom) return;
  saved = null;

  await win.setAlwaysOnTop(false).catch(() => undefined);
  await win.setMaxSize(null).catch(() => undefined);
  await win
    .setMinSize(new LogicalSize(DEFAULT_MIN_W, DEFAULT_MIN_H))
    .catch(() => undefined);
  await win.setResizable(true).catch(() => undefined);

  // outer* 记录的是物理像素，setSize/Position 直接回写
  await win.setSize(new PhysicalSize(geom.width, geom.height)).catch(() => undefined);
  await win.setPosition(new PhysicalPosition(geom.x, geom.y)).catch(() => undefined);
  if (geom.maximized) await win.maximize().catch(() => undefined);
}

export function enterMiniMode(): Promise<void> {
  return serialize(doEnter);
}

export function exitMiniMode(): Promise<void> {
  return serialize(doExit);
}
