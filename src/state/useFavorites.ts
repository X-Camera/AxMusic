import { create } from "zustand";

import { api } from "../lib/api";
import type { PlaylistAddItem } from "../lib/types";

/** 喜爱对照键：绝对路径统一正斜杠 + 小写（Windows 不区分大小写）。 */
export function favKey(path: string): string {
  return path.replace(/\\/g, "/").toLowerCase();
}

interface FavState {
  keys: Set<string>;
  loaded: boolean;
  loading: boolean;
  /** 变更序号：toggle/重载成功后 +1，供歌单页等订阅刷新 */
  rev: number;
  /** 首次进入时拉取；已加载则直接返回 */
  ensure: () => Promise<void>;
  /** 强制重读后端喜爱列表 */
  reload: () => Promise<void>;
  /** 切换喜爱；返回切换后是否已喜爱 */
  toggle: (item: PlaylistAddItem) => Promise<boolean>;
  isFavorite: (path: string) => boolean;
}

export const useFavorites = create<FavState>((set, get) => ({
  keys: new Set<string>(),
  loaded: false,
  loading: false,
  rev: 0,
  ensure: async () => {
    if (get().loaded || get().loading) return;
    await get().reload();
  },
  reload: async () => {
    if (get().loading) return;
    const wasLoaded = get().loaded;
    set({ loading: true });
    try {
      const paths = await api.favoritePaths();
      set({
        keys: new Set(paths.map(favKey)),
        loaded: true,
        // 首次加载不 bump，避免歌单页挂载时白刷一轮
        rev: wasLoaded ? get().rev + 1 : get().rev,
      });
    } catch {
      // 尚未初始化库根等：空集即可，避免反复打后端
      set({
        keys: new Set<string>(),
        loaded: true,
        rev: wasLoaded ? get().rev + 1 : get().rev,
      });
    } finally {
      set({ loading: false });
    }
  },
  toggle: async (item) => {
    await get().ensure();
    const k = favKey(item.path);
    const prev = get().keys;
    const next = new Set(prev);
    const willFav = !next.has(k);
    if (willFav) next.add(k);
    else next.delete(k);
    set({ keys: next });
    try {
      const res = await api.favoriteToggle(item);
      const cur = new Set(get().keys);
      if (res.favorited) cur.add(k);
      else cur.delete(k);
      set({ keys: cur, rev: get().rev + 1 });
      return res.favorited;
    } catch (e) {
      set({ keys: prev });
      throw e;
    }
  },
  isFavorite: (path) => get().keys.has(favKey(path)),
}));
