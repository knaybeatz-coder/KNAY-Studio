// Copyright (C) 2026 KNAY
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Signalsmith Stretch (MIT, vendor/signalsmith) as an offline pitch / formant shifter for VariAudio, set up as
//! Giannitune's `SignalsmithShifter` (21 ms blocks, 5 ms interval, formants kept while the pitch moves).

unsafe extern "C" {
    fn knay_signalsmith_render(input: *const *const f32, output: *const *mut f32, channels: i32, frames: i32, sample_rate: f32, transpose: *const f32, formant: *const f32, steps: i32, hop: i32) -> i32;
}

/// `channels` (one or two, the same length) at `rate`, each `hop` samples transposed by `transpose[k]` and with its
/// formants moved by `formant[k]` (factors; the last value holds to the end). Same length, aligned with the input.
pub fn render(channels: &[&[f32]], rate: u32, transpose: &[f32], formant: &[f32], hop: usize) -> Vec<Vec<f32>> {
    let n = channels.first().map_or(0, |c| c.len());
    let count = channels.len().clamp(0, 2);
    if n == 0 || count == 0 || hop == 0 || channels.iter().take(count).any(|c| c.len() != n) || transpose.is_empty() {
        return channels.iter().map(|c| c.to_vec()).collect();
    }
    let steps = transpose.len().min(formant.len().max(1));
    let formant: Vec<f32> = if formant.len() >= steps { formant[..steps].to_vec() } else { vec![1.0; steps] };
    let mut out: Vec<Vec<f32>> = (0..count).map(|_| vec![0.0f32; n]).collect();
    let ins: Vec<*const f32> = channels.iter().take(count).map(|c| c.as_ptr()).collect();
    let outs: Vec<*mut f32> = out.iter_mut().map(|c| c.as_mut_ptr()).collect();
    // SAFETY: `count` input and output channels of `n` samples each, and `steps` transpose and formant values, all
    // alive for the call; the bridge only reads the inputs and writes `n` samples per output channel.
    let ok = unsafe { knay_signalsmith_render(ins.as_ptr(), outs.as_ptr(), count as i32, n as i32, rate as f32, transpose.as_ptr(), formant.as_ptr(), steps as i32, hop as i32) };
    if ok == 0 {
        return channels.iter().map(|c| c.to_vec()).collect();
    }
    out
}

#[cfg(test)]
mod tests {
    /// The autocorrelation pitch of `x` around its middle.
    fn hz(x: &[f32], rate: f64) -> f64 {
        let a = x.len() / 2 - 4096;
        let r = |lag: usize| (a..a + 8192).map(|i| f64::from(x[i]) * f64::from(x[i + lag])).sum::<f64>();
        let best = (40..400).max_by(|&p, &q| r(p).total_cmp(&r(q))).unwrap();
        let (y0, y1, y2) = (r(best - 1), r(best), r(best + 1));
        rate / (best as f64 + 0.5 * (y0 - y2) / (y0 - 2.0 * y1 + y2))
    }

    #[test]
    fn transposes_and_is_transparent_at_unity() {
        let rate = 44100u32;
        let x: Vec<f32> = (0..rate as usize)
            .map(|i| {
                let p = 2.0 * std::f64::consts::PI * 220.0 * i as f64 / f64::from(rate);
                (0.3 * (p.sin() + 0.5 * (2.0 * p).sin() + 0.3 * (3.0 * p).sin() + 0.2 * (4.0 * p).sin())) as f32
            })
            .collect();
        let steps = x.len() / 256 + 20;
        let up = super::render(&[&x], rate, &vec![2f32.powf(2.0 / 12.0); steps], &vec![1.0; steps], 256);
        let cents = 1200.0 * (hz(&up[0], f64::from(rate)) / 220.0).log2();
        assert!((cents - 200.0).abs() < 15.0, "{cents} cents");
        let same = super::render(&[&x, &x], rate, &vec![1.0; steps], &vec![1.0; steps], 256);
        let err: f64 = x.iter().zip(&same[1]).skip(4410).take(30000).map(|(a, b)| f64::from(a - b).powi(2)).sum();
        let sig: f64 = x.iter().skip(4410).take(30000).map(|a| f64::from(*a).powi(2)).sum();
        assert!(10.0 * (sig / err).log10() > 60.0, "unity is transparent and aligned");
    }
}
