//! 窗函数 sinc 重采样器（立体声交错 f32），替换原单抽头线性插值。
//!
//! 设计依据 docs/播放引擎调研.md §4：
//! - 线性插值在整数倍降采样（96k→48k）退化为纯抽取，24kHz 以上能量原幅折叠回可听带；
//!   非整数倍（44.1k→48k）有 sinc² 高频下垂（20kHz 处约 −4.6dB）。
//! - 本实现：Blackman 窗 sinc，截止 = 0.98·min(in,out)/2，每边 Z=512 抽头。
//!
//! 结构 = 经典多相分解（相位主序表）+ 朴素簿记（§9.4 教训：上一版环形缓冲/暂存层错了
//! 而核是对的；这版簿记刻意一眼可验）：
//! - 表 `table[p][k]`：相位 p = round(frac·PHASES)，行内 2Z+1 个抽头按卷积顺序连续存放，
//!   内层循环是分支无关的纯点积（debug 构建也能跑在实时线上方；逐 tap 查表插值的方案
//!   实测 debug 只有 ~1.1× 实时，不够用）；
//! - `buf` = 尚未消费的输入样本队列（交错），`origin` = buf[0] 的全局帧号；
//! - `out_next` = 下一输出帧的全局输入位置（f64 游标，只随 drain 相对化、不回绕）；
//! - 不变量：`origin ≥ floor(out_next) − Z`（drain 保留历史）；
//!   产出上界 = 有完整未来帧（process）或流尾（flush 补 Z 帧零）。
//!
//! 已知边界效应（可接受，与线性插值时代同量级取舍）：
//! - 启动前 Z 帧是滤波器上升沿（零历史激励），seek/切歌 reset 后同理；
//! - EOF 经 flush 补零挤出尾部，总长帧数守恒（±1）；
//! - 相位量化（1/PHASES 输入帧）引入的镜像 ≤ −50dB 量级，埋在阻带底噪下。

const HALF: usize = 512;
/// 每相抽头数
const TAPS: usize = HALF * 2 + 1;
/// 相位数（frac 量化粒度 = 1/PHASES 输入帧）
const PHASES: usize = 256;

pub struct SincResampler {
    step: f64,        // in_rate / out_rate（每输出帧消耗的输入帧）
    table: Vec<f32>,  // PHASES × TAPS，行主序
    buf: Vec<f32>,    // 未消费输入（交错立体声）
    origin: u64,      // buf[0] 对应的全局输入帧号
    in_total: u64,    // 累计喂入的真实输入帧数（不含 flush 补的零）
    out_next: f64,    // 下一输出帧的全局输入位置
}

impl SincResampler {
    /// 调用方保证 in_rate != out_rate（同率在外层直通，不经过这里）。
    /// 建表 ~26 万次 sinc 求值，在 worker 线程新开曲时发生一次（与文件 I/O 同路径）。
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        let bandw = 0.98 * (out_rate as f64 / in_rate as f64).min(1.0);
        Self {
            step: in_rate as f64 / out_rate as f64,
            table: build_table(bandw),
            buf: Vec::new(),
            origin: 0,
            in_total: 0,
            out_next: 0.0,
        }
    }

    /// 喂一块源率立体声交错样本，产出设备率样本 push 进 `out`
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let frames = (input.len() / 2) as u64;
        if frames == 0 {
            return;
        }
        self.buf.extend_from_slice(input);
        self.in_total += frames;
        // 只产出"未来 Z 帧齐备"的部分：g + Z < in_total
        let limit = (self.in_total as f64) - HALF as f64;
        self.drive(out, limit);
        self.drain();
    }

    /// EOF：补 Z 帧零把残余尾部挤出。幂等（out_next 越界后自然无产出）。
    pub fn flush(&mut self, out: &mut Vec<f32>) {
        self.buf.resize(self.buf.len() + HALF * 2, 0.0);
        let limit = self.in_total as f64;
        self.drive(out, limit);
        self.drain();
    }

    /// seek/切歌：清空历史与游标（对应旧实现的 `resample_pos = 0.0`）
    pub fn reset(&mut self) {
        self.buf.clear();
        self.origin = 0;
        self.in_total = 0;
        self.out_next = 0.0;
    }

    fn drive(&mut self, out: &mut Vec<f32>, limit: f64) {
        while self.out_next < limit {
            let g = self.out_next;
            let mut c = g.floor() as i64;
            let frac = g - c as f64;
            // frac≈1 四舍五入到下一整帧（否则行号越界）
            let mut p = (frac * PHASES as f64).round() as usize;
            if p == PHASES {
                p = 0;
                c += 1;
            }
            let row = &self.table[p * TAPS..(p + 1) * TAPS];
            // 卷积：y = Σ_k row[k] · x[start + k]，start = c − Z（全局帧号）
            let start = c - HALF as i64;
            // 可用样本区间 [origin, origin + buf_frames)；两头各 clamp 一次，
            // 内层循环就彻底无分支（启动零历史 / flush 零尾垫都靠 clamp 表达）
            let buf_end = self.origin + (self.buf.len() / 2) as u64;
            let k_lo = (self.origin as i64 - start).max(0) as usize;
            let k_hi = (TAPS as i64).min(buf_end as i64 - start).max(k_lo as i64) as usize;
            let base = ((start + k_lo as i64 - self.origin as i64) as usize) * 2;
            let (mut l, mut r) = (0f32, 0f32);
            for k in k_lo..k_hi {
                let h = row[k];
                let i = base + (k - k_lo) * 2;
                l += h * self.buf[i];
                r += h * self.buf[i + 1];
            }
            out.push(l);
            out.push(r);
            self.out_next += self.step;
        }
    }

    /// 丢掉不再需要的输入历史（保留 floor(out_next) − Z 起）
    fn drain(&mut self) {
        if self.out_next < HALF as f64 {
            return;
        }
        let keep_from = (self.out_next.floor() as u64).saturating_sub(HALF as u64);
        if keep_from > self.origin {
            let n = ((keep_from - self.origin) as usize) * 2;
            self.buf.drain(..n.min(self.buf.len()));
            self.origin += (n / 2) as u64;
        }
    }
}

/// 多相表：table[p][k] = h(d)，d = (k − Z) − p/PHASES（输入帧为单位的有符号距离）
/// h(d) = bandw·sinc(bandw·d)·blackman(d)：
/// - bandw = 2·fc/in_rate，每相抽头和 ≈ 1（直流增益 1，多乘一次 bandw 会掉到 0.24 级）
/// - Blackman 用偶对称形式（d=0 → 1，|d|=HALF+1 → ≈0），无方向坑
fn build_table(bandw: f64) -> Vec<f32> {
    let mut table = Vec::with_capacity(PHASES * TAPS);
    for p in 0..PHASES {
        let q = p as f64 / PHASES as f64;
        for k in 0..TAPS {
            let d = (k as i64 - HALF as i64) as f64 - q;
            let sinc = if d.abs() < 1e-12 {
                1.0
            } else {
                let x = std::f64::consts::PI * bandw * d;
                x.sin() / x
            };
            let w = 0.42
                + 0.5 * (std::f64::consts::PI * d / (HALF + 1) as f64).cos()
                + 0.08 * (2.0 * std::f64::consts::PI * d / (HALF + 1) as f64).cos();
            table.push((bandw * sinc * w) as f32);
        }
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(frames: usize, rate: u32, freq: f64) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let s = (2.0 * std::f64::consts::PI * freq * i as f64 / rate as f64).sin() as f32;
                [s, s]
            })
            .collect()
    }

    /// 全部 process 完后 flush，返回总输出
    fn run(in_rate: u32, out_rate: u32, input: &[f32], chunk: usize) -> Vec<f32> {
        let mut r = SincResampler::new(in_rate, out_rate);
        let mut out = Vec::new();
        for piece in input.chunks(chunk * 2) {
            r.process(piece, &mut out);
        }
        r.flush(&mut out);
        out
    }

    /// 验证标准①：输出帧数 = 输入帧数 × out/in（±1）
    #[test]
    fn frame_count_conserved() {
        for &(ir, or) in &[(96_000u32, 48_000u32), (44_100, 48_000), (48_000, 96_000), (48_000, 44_100)] {
            let frames = 100_000usize;
            let input = sine(frames, ir, 1000.0);
            let out = run(ir, or, &input, 2048);
            let expect = frames as f64 * or as f64 / ir as f64;
            let got = (out.len() / 2) as f64;
            assert!(
                (got - expect).abs() <= 1.0,
                "{ir}->{or}: 期望 {expect} 帧，实得 {got}"
            );
        }
    }

    /// 验证标准②：1kHz 输入 → 输出上升沿数 = 输出时长 × 1000（频率不变）
    #[test]
    fn frequency_preserved() {
        let (ir, or) = (96_000u32, 48_000u32);
        let secs = 2.0;
        let input = sine((ir as f64 * secs) as usize, ir, 1000.0);
        let out = run(ir, or, &input, 4096);
        let frames: Vec<f32> = out.chunks(2).map(|c| c[0]).collect();
        // 去掉启动/收尾各 100ms 瞬态，数上升过零点
        let skip = (or as f64 * 0.1) as usize;
        let body = &frames[skip..frames.len() - skip];
        let crossings = body
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        let expect = 1000.0 * (body.len() as f64 / or as f64);
        assert!(
            (crossings as f64 - expect).abs() <= 2.0,
            "上升沿 {crossings}，期望约 {expect}"
        );
    }

    /// 验证标准③：直流输入 → 输出恒为同值（跳过启动瞬态）
    #[test]
    fn dc_passthrough() {
        let (ir, or) = (44_100u32, 48_000u32);
        let input: Vec<f32> = (0..50_000).flat_map(|_| [0.7f32, 0.7]).collect();
        let out = run(ir, or, &input, 1024);
        let frames: Vec<f32> = out.chunks(2).map(|c| c[0]).collect();
        // 启动 Z 帧上升沿 + flush 尾部不算；中段必须贴住 0.7
        let skip = HALF * 2;
        let body = &frames[skip..frames.len() - skip];
        let max_dev = body.iter().fold(0f32, |m, &v| m.max((v - 0.7).abs()));
        assert!(max_dev < 1e-3, "直流最大偏差 {max_dev}");
    }

    /// 96k→48k：25kHz（源奈奎斯特之上）必须被压下去，不再原幅折叠
    #[test]
    fn alias_suppressed() {
        let (ir, or) = (96_000u32, 48_000u32);
        let input = sine(ir as usize, ir, 25_000.0);
        let out = run(ir, or, &input, 4096);
        let frames: Vec<f32> = out.chunks(2).map(|c| c[0]).collect();
        let skip = HALF * 2;
        let body = &frames[skip..frames.len() - skip];
        let rms = (body.iter().map(|v| v * v).sum::<f32>() / body.len() as f32).sqrt();
        let full = (0.5f32).sqrt(); // 满幅正弦 RMS
        let db = 20.0 * (rms / full).log10();
        assert!(db < -40.0, "25kHz 折叠残余 {db:.1} dB（原线性插值为 0 dB）");
    }

    /// 44.1k→48k：20kHz 通带基本不掉（原线性插值约 −4.6dB）
    #[test]
    fn passband_flat_at_20k() {
        let (ir, or) = (44_100u32, 48_000u32);
        let input = sine(ir as usize, ir, 20_000.0);
        let out = run(ir, or, &input, 1024);
        let frames: Vec<f32> = out.chunks(2).map(|c| c[0]).collect();
        let skip = HALF * 2;
        let body = &frames[skip..frames.len() - skip];
        let rms = (body.iter().map(|v| v * v).sum::<f32>() / body.len() as f32).sqrt();
        let full = (0.5f32).sqrt();
        let db = 20.0 * (rms / full).log10();
        assert!(db > -0.5, "20kHz 通带 {db:.2} dB（原线性插值约 −4.6 dB）");
    }

    /// reset 后行为等价新实例（seek 场景）
    #[test]
    fn reset_clears_state() {
        let mut r = SincResampler::new(96_000, 48_000);
        let mut sink = Vec::new();
        r.process(&sine(10_000, 96_000, 3000.0), &mut sink);
        r.reset();
        let mut a = Vec::new();
        r.process(&sine(5_000, 96_000, 1000.0), &mut a);
        r.flush(&mut a);
        let mut fresh = SincResampler::new(96_000, 48_000);
        let mut b = Vec::new();
        fresh.process(&sine(5_000, 96_000, 1000.0), &mut b);
        fresh.flush(&mut b);
        assert_eq!(a.len(), b.len());
        let max_diff = a
            .iter()
            .zip(b.iter())
            .fold(0f32, |m, (x, y)| m.max((x - y).abs()));
        assert!(max_diff < 1e-6, "reset 后与新实例不一致：{max_diff}");
    }
}
