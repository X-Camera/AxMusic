import iconUrl from "../assets/axmusic-icon.svg";

/** 应用品牌标（与 exe 图标同一份 SVG） */
export function BrandMark({ size = 32 }: { size?: number }) {
  return (
    <img
      src={iconUrl}
      width={size}
      height={size}
      alt=""
      aria-hidden="true"
      draggable={false}
      style={{ display: "block" }}
    />
  );
}
