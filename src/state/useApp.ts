import { create } from "zustand";

import { api } from "../lib/api";
import type { PlayerSnapshot, QueueItem, RepeatMode, RouteId, TrackInfo } from "../lib/types";

interface AppState {
  route: RouteId;
  setRoute: (r: RouteId) => void;
  /** 满窗播放模式（封面点开 / 缩回关闭） */
  fullPlayer: boolean;
  setFullPlayer: (v: boolean) => void;
  /** 播放列表右边栏（迷你条按钮切换；管理/设置点左区自动收起） */
  queuePanelOpen: boolean;
  setQueuePanelOpen: (v: boolean) => void;
  toggleQueuePanel: () => void;
  /** 主页歌词右边栏（与播放列表同逻辑；同开时上下等分） */
  lyricsPanelOpen: boolean;
  setLyricsPanelOpen: (v: boolean) => void;
  toggleLyricsPanel: () => void;
  player: PlayerSnapshot | null;
  refreshing: boolean;
  setPlayer: (p: PlayerSnapshot | null) => void;
  refreshPlayer: () => Promise<void>;
  playPath: (path: string) => Promise<void>;
  playQueue: (items: QueueItem[], start: number) => Promise<void>;
  /** 追加到当前播放队列（排队等播放，不打断当前曲） */
  enqueue: (items: QueueItem[]) => Promise<void>;
  toggle: () => Promise<void>;
  next: () => Promise<void>;
  prev: () => Promise<void>;
  seek: (ms: number) => Promise<void>;
  setVolume: (v: number) => Promise<void>;
  setShuffle: (on: boolean) => Promise<void>;
  setRepeat: (r: RepeatMode) => Promise<void>;
}

const emptyPlayer = (): PlayerSnapshot => ({
  status: "Stopped",
  position_ms: 0,
  duration_ms: 0,
  volume: 0.8,
  track: null,
  queue: [],
  queue_index: null,
  shuffle: false,
  repeat: "off",
});

/** 播放器操作序号：轮询结果不得覆盖更新的点播/控制操作 */
let playerRev = 0;
/** 在途控制写入数：写入过程中轮询直接跳过，避免旧快照回写 */
let writesInFlight = 0;

function beginWrite(): number {
  writesInFlight += 1;
  playerRev += 1;
  return playerRev;
}

function endWrite() {
  writesInFlight = Math.max(0, writesInFlight - 1);
}

function queueItemToTrack(item: QueueItem): TrackInfo {
  return {
    path: item.path,
    title: item.title,
    duration_ms: item.duration_ms,
    sample_rate: 0,
    channels: 2,
  };
}

export const useApp = create<AppState>((set, get) => ({
  route: "albums",
  setRoute: (route) => set({ route }),
  fullPlayer: false,
  setFullPlayer: (fullPlayer) => set({ fullPlayer }),
  queuePanelOpen: false,
  setQueuePanelOpen: (queuePanelOpen) => set({ queuePanelOpen }),
  toggleQueuePanel: () => set((s) => ({ queuePanelOpen: !s.queuePanelOpen })),
  lyricsPanelOpen: false,
  setLyricsPanelOpen: (lyricsPanelOpen) => set({ lyricsPanelOpen }),
  toggleLyricsPanel: () => set((s) => ({ lyricsPanelOpen: !s.lyricsPanelOpen })),
  player: null,
  refreshing: false,
  setPlayer: (p) => set({ player: p }),
  refreshPlayer: async () => {
    if (get().refreshing || writesInFlight > 0) return;
    set({ refreshing: true });
    const rev = playerRev;
    try {
      const p = await api.getPlayerState();
      if (rev === playerRev && writesInFlight === 0) set({ player: p });
    } catch {
      /* keep last */
    } finally {
      set({ refreshing: false });
    }
  },
  playPath: async (path) => {
    const rev = beginWrite();
    try {
      const p = await api.playFile(path);
      if (rev === playerRev) set({ player: p });
    } finally {
      endWrite();
    }
  },
  playQueue: async (items, start) => {
    const rev = beginWrite();
    try {
      const item = items[start];
      if (item) {
        // 先乐观对准本次点击，避免轮询旧快照把界面拨回上一首
        const prev = get().player;
        set({
          player: {
            status: "Playing",
            position_ms: 0,
            duration_ms: item.duration_ms,
            volume: prev?.volume ?? 0.8,
            track: queueItemToTrack(item),
            queue: items,
            queue_index: start,
            shuffle: prev?.shuffle ?? false,
            repeat: prev?.repeat ?? "off",
          },
        });
      }
      const p = await api.playQueue(items, start);
      if (rev === playerRev) set({ player: p });
    } finally {
      endWrite();
    }
  },
  enqueue: async (items) => {
    const rev = beginWrite();
    try {
      const prev = get().player;
      if (prev) {
        set({ player: { ...prev, queue: [...prev.queue, ...items] } });
      }
      const p = await api.playerEnqueue(items);
      if (rev === playerRev) set({ player: p });
    } finally {
      endWrite();
    }
  },
  toggle: async () => {
    const rev = beginWrite();
    try {
      const p = await api.playerToggle();
      if (rev === playerRev) set({ player: p });
    } finally {
      endWrite();
    }
  },
  next: async () => {
    const rev = beginWrite();
    try {
      const p = await api.playerNext();
      if (rev === playerRev) set({ player: p });
    } finally {
      endWrite();
    }
  },
  prev: async () => {
    const rev = beginWrite();
    try {
      const p = await api.playerPrev();
      if (rev === playerRev) set({ player: p });
    } finally {
      endWrite();
    }
  },
  seek: async (ms) => {
    const rev = beginWrite();
    // 乐观对准目标进度；seek 是异步的，返回值也可能仍偏旧，以本次目标为准
    const prev = get().player;
    if (prev) set({ player: { ...prev, position_ms: ms } });
    try {
      const p = await api.playerSeek(ms);
      if (rev === playerRev) set({ player: { ...p, position_ms: ms } });
    } finally {
      endWrite();
    }
  },
  setVolume: async (v) => {
    const rev = beginWrite();
    try {
      const p = await api.playerSetVolume(v);
      if (rev === playerRev) set({ player: p ?? { ...emptyPlayer(), volume: v } });
    } finally {
      endWrite();
    }
  },
  setShuffle: async (on) => {
    const rev = beginWrite();
    try {
      const p = await api.playerSetShuffle(on);
      if (rev === playerRev) {
        set({
          player: p ?? {
            ...(get().player ?? emptyPlayer()),
            shuffle: on,
          },
        });
      }
    } finally {
      endWrite();
    }
  },
  setRepeat: async (r) => {
    const rev = beginWrite();
    try {
      const p = await api.playerSetRepeat(r);
      if (rev === playerRev) {
        set({
          player: p ?? {
            ...(get().player ?? emptyPlayer()),
            repeat: r,
          },
        });
      }
    } finally {
      endWrite();
    }
  },
}));
