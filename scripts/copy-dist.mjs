/**
 * 把 Tauri 构建产物复制到项目根目录 out/(扁平结构,方便取用):
 *   out/AxMusic-v<version>.exe   绿色版(免安装,双击即用)
 * 版本号取自 package.json。Cargo 编译产物仍是 axmusic.exe
 * (Tauri 用 Cargo 包名),版本号只在复制到 out/ 时写入文件名。
 * exe 被占用时先自动强杀 AxMusic* 重试；仍占用再提示输入 y 手动重试（保底）。
 */
import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync } from 'node:fs'
import { readFile } from 'node:fs/promises'
import { execFileSync } from 'node:child_process'
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

/** 强杀 AxMusic* / axmusic*（绿色版进程名带版本号，如 AxMusic-v0.0.2） */
function killAxMusic() {
  try {
    execFileSync(
      'powershell',
      [
        '-NoProfile',
        '-Command',
        "Get-Process -Name 'AxMusic*','axmusic*' -ErrorAction SilentlyContinue | Stop-Process -Force",
      ],
      { stdio: 'inherit' },
    )
  } catch {
    /* none running or already exited */
  }
}

/**
 * 占用时：先自动强杀重试若干次；仍占用则提示输入 y 手动重试（保底），其它放弃。
 */
async function withBusyRetry(label, doFn) {
  for (let attempt = 0; attempt < 2; attempt++) {
    try {
      doFn()
      return true
    } catch (e) {
      if (!isBusy(e)) throw e
      console.warn(`\n⚠ ${label} 失败:文件被占用，正在强杀 AxMusic 进程后重试…`)
      killAxMusic()
    }
  }

  try {
    doFn()
    return true
  } catch (e) {
    if (!isBusy(e)) throw e
  }

  // 保底：自动杀不掉（杀软/其它占用）时再问一次
  const rl = createInterface({ input: process.stdin, output: process.stdout })
  try {
    for (;;) {
      try {
        doFn()
        return true
      } catch (e) {
        if (!isBusy(e)) throw e
        console.warn(`\n⚠ ${label} 失败:文件仍被占用。`)
        console.warn('  请手动关闭占用程序,然后输入 y 重试;直接回车/n 放弃(不中断编译结果,但本次未写入)。')
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

// 清旧版本 exe;被占用同样强杀→手动 y 保底
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
