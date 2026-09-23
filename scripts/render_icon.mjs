// 从 src/assets/axmusic-icon.svg 生成全套应用图标到 src-tauri/icons/
// 用法: node scripts/render_icon.mjs （依赖 sharp、png-to-ico，见 devDependencies）
import { writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";
import pngToIco from "png-to-ico";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");
const svg = path.join(root, "src", "assets", "axmusic-icon.svg");
const out = path.join(root, "src-tauri", "icons");

// 4x 超采样渲染 1024 主图
const master = await sharp(svg, { density: 288 }).resize(1024, 1024).png().toBuffer();

await sharp(master).png().toFile(path.join(out, "icon.png"));
for (const [name, size] of [["128x128@2x.png", 256], ["128x128.png", 128], ["32x32.png", 32]]) {
  await sharp(master).resize(size, size).png().toFile(path.join(out, name));
}

const sizes = [16, 24, 32, 48, 64, 128, 256];
const bufs = await Promise.all(sizes.map((s) => sharp(master).resize(s, s).png().toBuffer()));
await writeFile(path.join(out, "icon.ico"), await pngToIco(bufs));
console.log("icons ->", out);
