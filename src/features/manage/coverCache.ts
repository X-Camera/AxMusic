import { api } from "../../lib/api";

/** 封面缩略图内存缓存 + 懒加载调度（模块级单例，全表共享一个 IntersectionObserver）。 */

/**
 * LRU 容量：data URL 约 8–25KB/条，300 条约 3–8MB。
 * 超限踢最久未用；磁盘 `.thumbs/` 仍在，再看时重新取。
 */
const MAX_CACHE = 300;

/** Map 插入序 = 使用序：命中后挪到队尾，淘汰时删队头。 */
const resolved = new Map<string, string | null>();
const pending = new Map<string, Promise<string | null>>();

function cacheGet(key: string): string | null | undefined {
  if (!resolved.has(key)) return undefined;
  const v = resolved.get(key) ?? null;
  resolved.delete(key);
  resolved.set(key, v);
  return v;
}

function cachePut(key: string, value: string | null) {
  resolved.delete(key);
  resolved.set(key, value);
  while (resolved.size > MAX_CACHE) {
    const oldest = resolved.keys().next().value;
    if (oldest === undefined) break;
    resolved.delete(oldest);
  }
}

/** element → 回调；进入视口（外扩 200px 预取）触发一次后自动注销。 */
const callbacks = new Map<Element, () => void>();
const observer = new IntersectionObserver(
  (entries) => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      const cb = callbacks.get(e.target);
      if (cb) {
        observer.unobserve(e.target);
        callbacks.delete(e.target);
        cb();
      }
    }
  },
  { root: null, rootMargin: "200px" },
);

export function observeCover(el: Element, cb: () => void) {
  callbacks.set(el, cb);
  observer.observe(el);
}

export function unobserveCover(el: Element) {
  callbacks.delete(el);
  observer.unobserve(el);
}

/** 纯读：可在 render 阶段（如 useState 初始化器）安全调用；不改 LRU 位次。 */
export function peekCover(key: string): string | null | undefined {
  return resolved.get(key);
}

/**
 * 加载封面（内存缓存 + 并发去重，扛 StrictMode 双挂载）。
 * key 用 `${path}:${mtime}`：写回文件后 mtime 变 → key 变 → 自动重新提取。
 * 命中会刷新 LRU 位次（只应在此 effect 链路调用，勿在 render 中 touch）。
 */
export function loadCover(key: string, path: string): Promise<string | null> {
  const hit = cacheGet(key);
  if (hit !== undefined) return Promise.resolve(hit);
  const inflight = pending.get(key);
  if (inflight) return inflight;
  const p = api
    .trackCoverThumb(path)
    .then((url) => {
      cachePut(key, url);
      pending.delete(key);
      return url;
    })
    .catch(() => {
      cachePut(key, null);
      pending.delete(key);
      return null;
    });
  pending.set(key, p);
  return p;
}
