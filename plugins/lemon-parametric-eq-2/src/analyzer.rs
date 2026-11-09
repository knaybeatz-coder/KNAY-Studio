// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! The spectrum behind the curves (Lemon Studio design; the original's look was not captured): an FFT of the monitored
//! signal mapped to the graph's columns, tilted by the pivot slope and smoothed over time.
//!
// spec: plugins/lemon-parametric-eq-2.md §2 "Spectrum"

use std::f32::consts::PI;

use os_engine::effects::eq::freq_hz;

/// FFT sizes by "Frequency precision" (Low .. Highest).
// Indices 0..3 remain unchanged so existing projects retain their exact FFT size.
pub const SIZES: [usize; 8] = [2048, 4096, 8192, 16384, 1024, 2048, 4096, 8192];
/// Pivot slopes in dB/oct by index.
pub const SLOPES: [f32; 5] = [0.0, 1.5, 3.0, 4.5, 6.0];
/// Display time constants in seconds (Lemon tuning, not FabFilter's unpublished timings).
const ATTACK: [f32; 5] = [0.0, 0.010, 0.025, 0.040, 0.065];
const RELEASE: [f32; 5] = [0.045, 0.120, 0.260, 0.450, 0.850];
pub const SPEEDS: [&str; 5] = ["Fastest", "Fast", "Medium", "Slow", "Slowest"];
pub const FRAME: std::time::Duration = std::time::Duration::from_nanos(1_000_000_000 / 30);

/// Analyzer settings from the options.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub precision: usize,
    pub slope: usize,
    pub smoothing: usize,
    pub average: bool,
}

#[derive(Default)]
pub struct Analyzer {
    size: usize,
    window: Vec<f32>,
    re: Vec<f32>,
    im: Vec<f32>,
    target: Vec<f32>,
    /// Smoothed level per column in dB.
    pub cols: Vec<f32>,
}

/// In-place radix-2 FFT (`re.len()` a power of two).
pub fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

impl Analyzer {
    /// FFT size for a precision index.
    pub fn size_for(look: &Look) -> usize {
        SIZES[look.precision.min(7)]
    }

    /// Analyses the newest samples (`samples.len()` = the FFT size) into `cols` columns of level in dB.
    pub fn update(&mut self, samples: &[f32], rate: f32, cols: usize, look: &Look) {
        self.measure(samples, rate, cols, look);
        self.advance(1.0 / 30.0, look);
    }

    fn measure(&mut self, samples: &[f32], rate: f32, cols: usize, look: &Look) {
        if samples.len() < 2
            || !samples.len().is_power_of_two()
            || cols == 0
            || !rate.is_finite()
            || rate <= 0.0
        {
            return;
        }
        let n = samples.len();
        if n != self.size {
            self.size = n;
            self.window = (0..n)
                .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / n as f32).cos())
                .collect();
        }
        self.re.clear();
        self.re
            .extend(
                samples
                    .iter()
                    .zip(&self.window)
                    .map(|(x, w)| if x.is_finite() { x * w } else { 0.0 }),
            );
        self.im.clear();
        self.im.resize(n, 0.0);
        fft(&mut self.re, &mut self.im);
        // Amplitude of a full-scale sine = 1 (Hann window sums to n / 2).
        let norm = 2.0 / (n as f32 * 0.5);
        let bins = n / 2;
        let mag = |b: usize| -> f32 {
            let b = b.min(bins - 1);
            (self.re[b] * self.re[b] + self.im[b] * self.im[b]).sqrt() * norm
        };
        let slope = SLOPES[look.slope.min(4)];
        let first = self.target.len() != cols;
        self.target.resize(cols, -150.0);
        let per_bin = rate / n as f32;
        for x in 0..cols {
            let f0 = freq_hz(x as f64 / cols as f64) as f32;
            let f1 = freq_hz((x + 1) as f64 / cols as f64) as f32;
            let (b0, b1) = ((f0 / per_bin) as usize, (f1 / per_bin).ceil() as usize);
            // Wide columns take their loudest bin; narrow ones interpolate between two bins.
            let m = if b1 > b0 + 1 {
                (b0..b1).map(mag).fold(0.0f32, f32::max)
            } else {
                let fb = (f0 + f1) * 0.5 / per_bin;
                let (i, t) = (fb.floor() as usize, fb.fract());
                mag(i) * (1.0 - t) + mag(i + 1) * t
            };
            let fc = (f0 * f1).sqrt();
            let db = 20.0 * m.max(1e-9).log10() + slope * (fc / 1000.0).log2();
            self.target[x] = db.clamp(-150.0, 60.0);
        }
        if first {
            self.cols.clone_from(&self.target);
        }
    }

    /// Move toward the latest spectrum using elapsed time, independent of event/paint rate.
    fn advance(&mut self, seconds: f32, look: &Look) -> bool {
        if seconds <= 0.0 {
            return false;
        }
        let speed = look.smoothing.min(4);
        let mut dirty = false;
        for (shown, &target) in self.cols.iter_mut().zip(&self.target) {
            let tau = if look.average {
                RELEASE[speed]
            } else if target > *shown {
                ATTACK[speed]
            } else {
                RELEASE[speed]
            };
            let next = if (target - *shown).abs() < 0.01 || tau == 0.0 {
                target
            } else {
                target + (*shown - target) * (-seconds / tau).exp()
            };
            dirty |= next != *shown;
            *shown = next;
        }
        dirty
    }
}

/// One scheduler for every platform: analyze at most 30 times/sec; never catch up stale frames.
pub struct LiveAnalyzer {
    pub analyzer: Analyzer,
    origin: std::time::Instant,
    next: std::time::Duration,
    presented: Option<std::time::Duration>,
    audio_at: std::time::Duration,
    seen: u64,
    look: Option<Look>,
    rate: u32,
    samples: Vec<f32>,
    pub measurements: u64,
}
impl Default for LiveAnalyzer {
    fn default() -> Self {
        Self {
            analyzer: Analyzer::default(),
            origin: std::time::Instant::now(),
            next: std::time::Duration::ZERO,
            presented: None,
            audio_at: std::time::Duration::ZERO,
            seen: 0,
            look: None,
            rate: 0,
            samples: Vec::with_capacity(16384),
            measurements: 0,
        }
    }
}
impl LiveAnalyzer {
    pub fn poll(&mut self, shared: &crate::Shared, look: Option<Look>) -> bool {
        self.poll_at(shared, look, self.origin.elapsed())
    }
    pub fn poll_at(
        &mut self,
        shared: &crate::Shared,
        look: Option<Look>,
        now: std::time::Duration,
    ) -> bool {
        use std::sync::atomic::Ordering;
        let dt = self
            .presented
            .map_or(0.0, |last| now.saturating_sub(last).as_secs_f32());
        self.presented = Some(now);
        let Some(look) = look else {
            let dirty = !self.analyzer.cols.is_empty();
            self.analyzer.cols.clear();
            self.analyzer.target.clear();
            self.look = None;
            return dirty;
        };
        let mut dirty = false;
        if now >= self.next {
            // Absolute deadlines prevent timer jitter or rapid parameter events changing analysis rate.
            self.next = std::time::Duration::from_nanos(
                ((now.as_nanos() / FRAME.as_nanos() + 1) * FRAME.as_nanos()).min(u64::MAX as u128)
                    as u64,
            );
            let rate = shared.rate.load(Ordering::Relaxed);
            let n = Analyzer::size_for(&look);
            self.samples.resize(n, 0.0); // Allocate only on resolution changes, outside the lock.
            let fresh = if let Ok(scope) = shared.scope.try_lock() {
                if scope.written != self.seen || self.look != Some(look) || self.rate != rate {
                    let size = scope.buf.len();
                    let start = (scope.pos + size - n) % size;
                    let first = n.min(size - start);
                    self.samples[..first].copy_from_slice(&scope.buf[start..start + first]);
                    self.samples[first..].copy_from_slice(&scope.buf[..n - first]);
                    let changed = scope.written != self.seen;
                    self.seen = scope.written;
                    Some(changed)
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(changed) = fresh {
                if changed {
                    self.audio_at = now;
                }
                self.analyzer
                    .measure(&self.samples, rate as f32, crate::view::COLS, &look);
                self.measurements += 1;
                dirty = true;
                self.look = Some(look);
                self.rate = rate;
            }
            // A suspended/stopped host may stop providing blocks: let the last spectrum fade away.
            let stale = (n as f32 / rate.max(1) as f32 * 1.5).max(0.15);
            if now.saturating_sub(self.audio_at).as_secs_f32() > stale {
                self.analyzer.target.fill(-150.0);
            }
        }
        dirty | self.analyzer.advance(dt, &look)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sine_shows_at_its_frequency() {
        let rate = 44100.0;
        let n = 4096;
        let s: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * 1000.0 * i as f32 / rate).sin())
            .collect();
        let mut a = Analyzer::default();
        let look = Look {
            precision: 1,
            slope: 0,
            smoothing: 0,
            average: false,
        };
        a.update(&s, rate, 500, &look);
        let peak = (0..500)
            .max_by(|&x, &y| a.cols[x].total_cmp(&a.cols[y]))
            .unwrap();
        let hz = freq_hz(peak as f64 / 500.0);
        assert!((hz - 1000.0).abs() < 40.0, "{hz}");
        assert!(
            a.cols[peak] > -3.0 && a.cols[peak] < 1.0,
            "{}",
            a.cols[peak]
        );
    }
}

#[cfg(test)]
mod timing_tests {
    use super::*;
    use std::time::Duration;
    fn look() -> Look {
        Look {
            precision: 5,
            slope: 0,
            smoothing: 3,
            average: false,
        }
    }
    #[test]
    fn envelope_speed_is_independent_of_poll_rate_and_average_mode() {
        for average in [false, true] {
            for rising in [false, true] {
                for smoothing in 0..5 {
                    let mut results = Vec::new();
                    for steps in [15, 30, 60, 240] {
                        let mut a = Analyzer::default();
                        a.cols = vec![if rising { -80. } else { -20. }];
                        a.target = vec![if rising { -20. } else { -80. }];
                        let mut look = look();
                        look.average = average;
                        look.smoothing = smoothing;
                        for _ in 0..steps {
                            a.advance(1. / steps as f32, &look);
                        }
                        results.push(a.cols[0]);
                    }
                    for r in &results {
                        assert!((r - results[0]).abs() < 0.02, "{results:?}");
                    }
                }
            }
        }
        let mut a = Analyzer::default();
        a.cols = vec![-80.];
        a.target = vec![-20.];
        a.advance(0.01, &look());
        assert!(a.cols[0] > -80. && a.cols[0] < -60.);
    }
    #[test]
    fn event_storms_and_stalls_do_not_run_extra_fft_or_catch_up() {
        let shared = crate::Shared::default();
        let mut live = LiveAnalyzer::default();
        for ms in 0..1000 {
            shared.scope.lock().unwrap().written += 48;
            live.poll_at(&shared, Some(look()), Duration::from_millis(ms));
        }
        assert_eq!(live.measurements, 30);
        shared.scope.lock().unwrap().written += 48000;
        live.poll_at(&shared, Some(look()), Duration::from_secs(3));
        assert_eq!(live.measurements, 31);
        for _ in 0..100 {
            live.poll_at(&shared, Some(look()), Duration::from_secs(3));
        }
        assert_eq!(live.measurements, 31);
        {
            let _guard = shared.scope.lock().unwrap();
            live.poll_at(&shared, Some(look()), Duration::from_secs(4));
        }
        assert_eq!(live.measurements, 31);
        live.poll_at(&shared, Some(look()), Duration::from_millis(4050));
        assert!(live.analyzer.cols.iter().all(|v| v.is_finite()));
    }
    #[test]
    fn wrapped_snapshot_shows_tone_then_decays_and_stops_when_host_stops() {
        let shared = crate::Shared::default();
        let mut live = LiveAnalyzer::default();
        {
            let mut scope = shared.scope.lock().unwrap();
            scope.pos = 17;
            let len = scope.buf.len();
            scope.written = len as u64;
            for i in 0..len {
                let p = (scope.pos + i) % len;
                scope.buf[p] = (std::f32::consts::TAU * 1000. * i as f32 / 44100.).sin();
            }
        }
        live.poll_at(&shared, Some(look()), Duration::ZERO);
        let peak = live.analyzer.cols.iter().copied().fold(-150., f32::max);
        assert!(peak > -4.);
        for ms in (34..8000).step_by(17) {
            live.poll_at(&shared, Some(look()), Duration::from_millis(ms));
        }
        assert_eq!(live.measurements, 1);
        assert!(live.analyzer.cols.iter().all(|v| *v < -149.));
        assert!(!live.poll_at(&shared, Some(look()), Duration::from_secs(9)));
        assert!(live.poll_at(&shared, None, Duration::from_secs(10)));
        assert!(live.analyzer.cols.is_empty());
    }
}
