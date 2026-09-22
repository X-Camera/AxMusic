/**
 * 把 Tauri 构建产物复制到项目根目录 out/(扁平结构,方便取用):
 *   out/AxMusic-v<version>.exe   绿色版(免安装,双击即用)
 * 版本号取自 package.json。Cargo 编译产物仍是 axmusic.exe
 * (Tauri 用 Cargo 包名),版本号只在复制到 out/ 时写入文件名。
 * exe 被占用时不静默跳过:提示后等你关掉进程,输入 y 手动重试。
 */
import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync } from 'node:fs'
import { readFile } from 'node:fs/promises'
import { createInterface } from 'node:readline/promises'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('..', import.meta.url))
const outDir = join(root, 'out')
const releaseDir = join(root, 'src-tauri', 'target', 'release')

const pkg = JSON.parse(await readFile(join(root, 'package.json'), 'utf-8'))
const version = pkg.version
const outName = `AxMusic-v${version}.exe`

/** 文件锁错误码 */
function isBusy(e) {
  const code = e && typeof e === 'object' && 'code' in e ? e.code : ''
  return code === 'EPERM' || code === 'EBUSY'
}

/**
 * 占用时阻塞询问,直到成功 / 用户放弃。
 * doFn 抛 EPERM/EBUSY → 打印提示 → 读一行:y/Y 重试,其它放弃。
 */
async function withBusyRetry(label, doFn) {
  const rl = createInterface({ input: process.stdin, output: process.stdout })
  try {
    for (;;) {
      try {
        doFn()
        return true
      } catch (e) {
        if (!isBusy(e)) throw e
        console.warn(`\n⚠ ${label} 失败:文件被占用(可能正在运行 out 里旧的 exe)。`)
        console.warn('  请先关闭该 exe,然后输入 y 重试;直接回车/n 放弃(不中断编译结果,但本次未写入)。')
        const ans = (await rl.question('> ')).trim().toLowerCase()
        if (ans !== 'y' && ans !== 'yes') {
          console.warn(`  已放弃:${label}`)
          return false
        }
        console.log('  重试中…')
      }
    }
  } finally {
    rl.close()
  }
}

mkdirSync(outDir, { recursive: true })

const src = join(releaseDir, 'axmusic.exe')
if (!existsSync(src)) {
  console.error('未找到构建产物,请先运行 npm run tauri build')
  process.exit(1)
}

const dest = join(outDir, outName)
const copied = await withBusyRetry(`复制到 out/${outName}`, () => copyFileSync(src, dest))
if (copied) {
  console.log(`✓ out/${outName}(绿色版 v${version})`)
}

// 清旧版本 exe;被占用同样走 y 重试
for (const f of readdirSync(outDir)) {
  if (!/^AxMusic(-v[\d.]+)?\.exe$/i.test(f) || f === outName) continue
  const removed = await withBusyRetry(`删除旧产物 ${f}`, () => rmSync(join(outDir, f)))
  if (removed) console.log(`  已清理旧产物 ${f}`)
}

if (!copied) {
  console.error('\n复制未完成(目标 exe 仍被占用)。关闭程序后可单独重跑:npm run dist:copy')
  process.exit(1)
}
console.log(`\n完成:out/${outName}`)
