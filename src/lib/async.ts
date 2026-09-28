/**
 * 请求代次：响应回来先比代次，过期直接丢弃。
 * 用于连点/连切时旧响应不覆盖新状态（FIX-13）。
 */
export type SeqGuard = {
  /** 开始一次新请求，返回本次代次 */
  begin: () => number;
  /** 该代次是否仍是最新（未被更新的 begin/invalidate 顶掉） */
  isCurrent: (seq: number) => boolean;
  /** 作废所有在途请求 */
  invalidate: () => void;
};

export function makeSeq(): SeqGuard {
  let current = 0;
  return {
    begin: () => ++current,
    isCurrent: (seq: number) => seq === current,
    invalidate: () => {
      current += 1;
    },
  };
}

/** 全局搜索代次：跨组件重挂/多实例不重复（组件级 ref 重挂会回到 0 撞车） */
let searchSeq = 0;

export function nextSearchId(): number {
  return ++searchSeq;
}
