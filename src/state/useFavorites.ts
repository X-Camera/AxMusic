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
      const keys = new Set(paths.map(favKey));
      const prev = get().keys;
      // 键集不变不 bump rev：避免「拉详情→刷喜爱→rev→再拉详情」打环
      const unchanged = prev.size === keys.size && [...keys].every((k) => prev.has(k));
      set({
        keys,
        loaded: true,
        // 首次加载不 bump，避免歌单页挂载时白刷一轮
        rev: !wasLoaded || unchanged ? get().rev : get().rev + 1,
      });
    } catch (e) {
      // 「尚未初始化库根」是合法空态（库外浏览也会挂心形）：空集即可，避免反复打后端；
      // 其余为真 IO 故障——失败即清空会误伤（瞬时故障被当成「没有喜爱」），
      // 保留旧 keys；首载失败不标 loaded，下次 ensure 自动重试。调用方均为 void 调用，不上抛。
      if (String(e).includes("尚未初始化")) {
        set({ keys: new Set<string>(), loaded: true });
      } else {
        console.error("[favorites] 加载喜爱列表失败：", e);
        if (!wasLoaded) set({ loaded: false });
      }
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
      // 只回退本键：全量 prev 会把并发 toggle 的其它歌曲一并冲掉
      set((s) => {
        const next = new Set(s.keys);
        if (willFav) next.delete(k);
        else next.add(k);
        return { keys: next };
      });
      throw e;
    }
  },
  isFavorite: (path) => get().keys.has(favKey(path)),
}));
