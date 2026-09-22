import { create } from "zustand";

import { api } from "../lib/api";
import type { PlayerSnapshot, QueueItem, RouteId } from "../lib/types";

interface AppState {
  route: RouteId;
  setRoute: (r: RouteId) => void;
  /** 满窗播放模式（封面点开 / 缩回关闭） */
  fullPlayer: boolean;
  setFullPlayer: (v: boolean) => void;
  player: PlayerSnapshot | null;
  refreshing: boolean;
  setPlayer: (p: PlayerSnapshot | null) => void;
  refreshPlayer: () => Promise<void>;
  playPath: (path: string) => Promise<void>;
  playQueue: (items: QueueItem[], start: number) => Promise<void>;
  toggle: () => Promise<void>;
  next: () => Promise<void>;
  prev: () => Promise<void>;
  seek: (ms: number) => Promise<void>;
  setVolume: (v: number) => Promise<void>;
}

const emptyPlayer = (): PlayerSnapshot => ({
  status: "Stopped",
  position_ms: 0,
  duration_ms: 0,
  volume: 0.8,
  track: null,
  queue: [],
  queue_index: null,
});

export const useApp = create<AppState>((set, get) => ({
  route: "albums",
  setRoute: (route) => set({ route }),
  fullPlayer: false,
  setFullPlayer: (fullPlayer) => set({ fullPlayer }),
  player: null,
  refreshing: false,
  setPlayer: (p) => set({ player: p }),
  refreshPlayer: async () => {
    if (get().refreshing) return;
    set({ refreshing: true });
    try {
      const p = await api.getPlayerState();
      set({ player: p });
    } catch {
      /* keep last */
    } finally {
      set({ refreshing: false });
    }
  },
  playPath: async (path) => {
    const p = await api.playFile(path);
    set({ player: p });
  },
  playQueue: async (items, start) => {
    const p = await api.playQueue(items, start);
    set({ player: p });
  },
  toggle: async () => {
    const p = await api.playerToggle();
    set({ player: p });
  },
  next: async () => {
    const p = await api.playerNext();
    set({ player: p });
  },
  prev: async () => {
    const p = await api.playerPrev();
    set({ player: p });
  },
  seek: async (ms) => {
    const p = await api.playerSeek(ms);
    set({ player: p });
  },
  setVolume: async (v) => {
    const p = await api.playerSetVolume(v);
    set({ player: p ?? { ...emptyPlayer(), volume: v } });
  },
}));
