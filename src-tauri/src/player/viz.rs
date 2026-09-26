//! 频谱动效数据管线：cpal 回调侧分接 mono PCM → 无锁 ring → FFT 线程 → `viz://spectrum` 事件。
//!
//! 设计要点：
//! - 回调内只做 `push_slice`（wait-free），满则丢帧——动效丢帧无感；
//! - FFT/平滑/节拍检测都在本线程做，前端直接消费 64 个对数 bin；
//! - 仅在前端订阅（`active`）且播放中（回调有真实采样流入）时计算与发射；
//!   暂停时回调推静音跳过 tap，本线程自然超时停发，前端据此回退模拟动效。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ringbuf::traits::{Consumer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use serde::Serialize;
use tauri::{Emitter, WebviewWindow};

/// FFT 窗口（2048 ≈ 46ms @44.1k），hop 1024（50% 重叠）
const FFT_N: usize = 2048;
const HOP: usize = FFT_N / 2;
/// 对数频带数（与前端 levels 长度一致）
pub const BINS: usize = 64;
/// 发射节拍（≈30Hz）
const TICK: Duration = Duration::from_millis(33);

#[derive(Debug, Clone, Serialize)]
pub struct VizFrame {
    /// 64 个对数频带能量 0..1（30Hz → min(16k, sr/2)，已做时域平滑）
    bins: Vec<f32>,
    /// 低频段（前 6 bin）均值：呼吸/脉动驱动
    bass: f32,
    /// 本 hop 的均方根响度
    rms: f32,
    /// 简易 onset：低频超滑动均值 1.35× 且距上次 >150ms
    beat: bool,
}

/// 音频回调侧的分接句柄（单生产者）；`active=false` 时回调直接跳过收集。
pub struct VizTap {
    pub producer: HeapProd<f32>,
    pub active: Arc<AtomicBool>,
}

pub fn new_tap() -> (VizTap, HeapCons<f32>) {
    // 16384 采样 ≈ 0.34s @48k；溢出丢弃由 Producer::push_slice 天然完成
    let (producer, consumer) = HeapRb::<f32>::new(16384).split();
    (
        VizTap {
            producer,
            active: Arc::new(AtomicBool::new(false)),
        },
        consumer,
    )
}

/// 预计算 64 个对数频带对应的 FFT 下标区间 [lo, hi)
fn log_bands(sample_rate: f32) -> Vec<(usize, usize)> {
    let f_min = 30.0f32;
    let f_max = (sample_rate / 2.0).min(16000.0).max(f_min * 2.0);
    let ratio = f_max / f_min;
    let hz_per_bin = sample_rate / FFT_N as f32;
    (0..BINS)
        .map(|i| {
            let lo_f = f_min * ratio.powf(i as f32 / BINS as f32);
            let hi_f = f_min * ratio.powf((i + 1) as f32 / BINS as f32);
            let lo = ((lo_f / hz_per_bin) as usize).max(1).min(FFT_N / 2 - 1);
            let hi = ((hi_f / hz_per_bin).ceil() as usize)
                .max(lo + 1)
                .min(FFT_N / 2);
            (lo, hi)
        })
        .collect()
}

/// 启动频谱线程（进程级常驻，分离运行；事件定向发 main 窗口，避免 30Hz 打扰歌词搜索等子窗）。
/// `gen` 与回调侧 `audio_gen` 同一计数：切歌/seek 时清 ring 与滑窗，防旧曲残样闪跳。
pub fn spawn_viz_thread(
    win: WebviewWindow,
    mut cons: HeapCons<f32>,
    active: Arc<AtomicBool>,
    gen: Arc<AtomicU64>,
    sample_rate: u32,
) {
    if sample_rate == 0 {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("axmusic-viz".into())
        .spawn(move || {
            let bands = log_bands(sample_rate as f32);
            let hann: Vec<f32> = (0..FFT_N)
                .map(|i| {
                    let x = i as f32 / (FFT_N - 1) as f32;
                    (std::f32::consts::PI * x).sin().powi(2)
                })
                .collect();
            let fft = FftPlanner::<f32>::new().plan_fft_forward(FFT_N);

            let mut window = vec![0.0f32; FFT_N];
            let mut pending: Vec<f32> = Vec::with_capacity(8192);
            let mut fft_buf = vec![Complex::new(0.0f32, 0.0); FFT_N];
            let mut smooth = [0.0f32; BINS];
            let mut bass_avg = 0.0f32;
            let mut last_beat = Instant::now() - Duration::from_secs(1);
            let mut last_data = Instant::now();
            let mut scratch = [0.0f32; 4096];
            let mut last_gen = gen.load(Ordering::Relaxed);

            loop {
                let g = gen.load(Ordering::Relaxed);
                if g != last_gen {
                    last_gen = g;
                    cons.clear();
                    window.fill(0.0);
                    pending.clear();
                    smooth.fill(0.0);
                }
                if !active.load(Ordering::Relaxed) {
                    // 未订阅：清积压与状态，低频空转
                    cons.clear();
                    window.fill(0.0);
                    pending.clear();
                    smooth.fill(0.0);
                    bass_avg = 0.0;
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }

                // 排空 ring 中的新采样
                let mut got = 0usize;
                loop {
                    let n = cons.pop_slice(&mut scratch);
                    if n == 0 {
                        break;
                    }
                    pending.extend_from_slice(&scratch[..n]);
                    got += n;
                    if n < scratch.len() {
                        break;
                    }
                }

                if pending.len() < HOP {
                    if got == 0 && last_data.elapsed() > Duration::from_millis(600) {
                        // 暂停/停止已久：复位，避免恢复播放时旧数据闪跳
                        window.fill(0.0);
                        pending.clear();
                        smooth.fill(0.0);
                    }
                    std::thread::sleep(TICK);
                    continue;
                }
                last_data = Instant::now();

                // 滞后保护：积压超过 ~2 窗说明消费跟不上，丢弃旧数据只留最新
                if pending.len() > FFT_N * 2 {
                    let keep = FFT_N;
                    pending.drain(..pending.len() - keep);
                }

                // 消费所有完整 hop，但每 tick 只发射最后一帧（限 ~30Hz）
                let mut emitted = false;
                while pending.len() >= HOP {
                    window.copy_within(HOP.., 0);
                    window[FFT_N - HOP..].copy_from_slice(&pending[..HOP]);
                    pending.drain(..HOP);

                    for i in 0..FFT_N {
                        fft_buf[i] = Complex::new(window[i] * hann[i], 0.0);
                    }
                    fft.process(&mut fft_buf);

                    // Hann 窗峰值增益 N/4：归一后满幅正弦 ≈ 0 dB
                    let norm = 4.0 / FFT_N as f32;
                    for (i, &(lo, hi)) in bands.iter().enumerate() {
                        let mut sum = 0.0f32;
                        for k in lo..hi {
                            sum += fft_buf[k].norm();
                        }
                        let mag = sum * norm / (hi - lo) as f32;
                        let db = 20.0 * (mag + 1e-9).log10();
                        let v = ((db + 55.0) / 55.0).clamp(0.0, 1.0).powf(0.8);
                        // attack 快 / release 慢，视觉上冲跟手、回落顺滑
                        smooth[i] += (v - smooth[i]) * if v > smooth[i] { 0.55 } else { 0.18 };
                    }

                    emitted = true;
                }

                if emitted {
                    let bass = smooth[..6].iter().sum::<f32>() / 6.0;
                    bass_avg = bass_avg * 0.96 + bass * 0.04;
                    let beat = bass > 0.08
                        && bass > bass_avg * 1.35
                        && last_beat.elapsed() > Duration::from_millis(150);
                    if beat {
                        last_beat = Instant::now();
                    }
                    let rms = {
                        let mut s = 0.0f32;
                        for &x in &window[FFT_N - HOP..] {
                            s += x * x;
                        }
                        (s / HOP as f32).sqrt()
                    };
                    let frame = VizFrame {
                        bins: smooth.to_vec(),
                        bass,
                        rms,
                        beat,
                    };
                    let _ = win.emit("viz://spectrum", &frame);
                }

                std::thread::sleep(TICK);
            }
        });
}
