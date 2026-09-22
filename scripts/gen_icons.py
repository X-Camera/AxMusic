# Build Tauri/Windows icons from the user-supplied master PNG.
# Usage: put master at assets/icon-source.png (square, >=512), then run this script.
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
MASTER = ROOT / "assets" / "icon-source.png"
OUT = ROOT / "src-tauri" / "icons"


def load_master() -> Image.Image:
    if not MASTER.is_file():
        raise SystemExit(f"missing master: {MASTER}")
    im = Image.open(MASTER).convert("RGBA")
    w, h = im.size
    if abs(w - h) > max(w, h) * 0.02:
        side = min(w, h)
        left = (w - side) // 2
        top = (h - side) // 2
        im = im.crop((left, top, left + side, top + side))
    return im


def resize(im: Image.Image, size: int) -> Image.Image:
    return im.resize((size, size), Image.Resampling.LANCZOS)


def main() -> None:
    im = load_master()
    OUT.mkdir(parents=True, exist_ok=True)

    resize(im, 32).save(OUT / "32x32.png")
    resize(im, 128).save(OUT / "128x128.png")
    resize(im, 256).save(OUT / "128x128@2x.png")
    resize(im, 512).save(OUT / "icon.png")

    # ICO carries the classic shell sizes
    base = resize(im, 256)
    base.save(
        OUT / "icon.ico",
        format="ICO",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    print("icons ←", MASTER)
    for p in sorted(OUT.glob("*")):
        print(f"  {p.name:16} {p.stat().st_size}")


if __name__ == "__main__":
    main()
