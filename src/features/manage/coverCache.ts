import { api } from "../../lib/api";

/** 封面缩略图内存缓存 + 懒加载调度（模块级单例，全表共享一个 IntersectionObserver）。 */

const resolved = new Map<string, string | null>();
const pending = new Map<string, Promise<string | null>>();

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

/** 已完成的缓存取值；undefined = 尚未加载。 */
export function peekCover(key: string): string | null | undefined {
  return resolved.get(key);
}

/**
 * 加载封面（内存缓存 + 并发去重，扛 StrictMode 双挂载）。
 * key 用 `${path}:${mtime}`：写回文件后 mtime 变 → key 变 → 自动重新提取。
 */
export function loadCover(key: string, path: string): Promise<string | null> {
  const hit = resolved.get(key);
  if (hit !== undefined) return Promise.resolve(hit);
  const inflight = pending.get(key);
  if (inflight) return inflight;
  const p = api
    .trackCoverThumb(path)
    .then((url) => {
      resolved.set(key, url);
      pending.delete(key);
      return url;
    })
    .catch(() => {
      resolved.set(key, null);
      pending.delete(key);
      return null;
    });
  pending.set(key, p);
  return p;
}
