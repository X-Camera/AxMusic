/** null/undefined 判定（替代 `== null` 松散比较）。 */
export function notNil<T>(v: T | null | undefined): v is T {
  return v !== null && v !== undefined;
}

export function isNil(v: unknown): v is null | undefined {
  return v === null || v === undefined;
}
