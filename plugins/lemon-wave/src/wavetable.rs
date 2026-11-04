// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded WAV import and periodic Fourier resampling. No file access in render().
use serde::{Deserialize, Serialize};
use std::{f64::consts::TAU, path::Path};
pub const SIZE: usize = 1024;
pub const MIPS: usize = 9;
pub const MAX_FRAMES: usize = 128;
pub const MAX_FILE: usize = 16 * 1024 * 1024;
pub const CYCLES: [usize; 7] = [64, 128, 256, 512, 1024, 2048, 4096];

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub name: String,
    /// Canonical periodic frames, independent of the original file and its sample rate.
    pub samples: Vec<f32>,
}
impl Source {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.len() > 256
            || self.samples.is_empty()
            || self.samples.len() % SIZE != 0
            || self.samples.len() > MAX_FRAMES * SIZE
            || self
                .samples
                .iter()
                .any(|x| !x.is_finite() || x.abs() > 1.001)
        {
            return Err("Invalid embedded wavetable".into());
        }
        Ok(())
    }
    pub fn frames(&self) -> usize {
        self.samples.len() / SIZE
    }
}

pub(crate) fn fft(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    let bits = n.trailing_zeros();
    for i in 0..n {
        let j = i.reverse_bits() >> (usize::BITS - bits);
        if j > i {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let a = TAU * k as f64 / len as f64 * if inverse { 1. } else { -1. };
                let (s, c) = a.sin_cos();
                let (i, j) = (start + k, start + k + len / 2);
                let r = re[j] * c - im[j] * s;
                let t = re[j] * s + im[j] * c;
                re[j] = re[i] - r;
                im[j] = im[i] - t;
                re[i] += r;
                im[i] += t;
            }
        }
        len *= 2;
    }
}

pub fn import(path: &Path, cycle: usize) -> Result<Source, String> {
    // Bounded read even if a file grows between metadata and reading.
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take((MAX_FILE + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let name: String = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .take(60)
        .collect();
    decode(&bytes, cycle, name)
}

pub fn decode(bytes: &[u8], cycle: usize, name: String) -> Result<Source, String> {
    let err = |s: &str| Err(s.to_string());
    if bytes.len() > MAX_FILE {
        return err("WAV exceeds 16 MB");
    }
    if !CYCLES.contains(&cycle) {
        return err("Choose a cycle length from 64 to 4096");
    }
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return err("Choose a RIFF WAV file (PCM or 32-bit float)");
    }
    let u16_at = |b: &[u8], i| u16::from_le_bytes(b[i..i + 2].try_into().unwrap());
    let u32_at = |b: &[u8], i| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
    let end = u32_at(bytes, 4) as usize + 8;
    if end > bytes.len() || end < 12 {
        return err("Truncated WAV header");
    }
    let (mut fmt, mut data) = (None, None);
    let mut p = 12;
    while p + 8 <= end {
        let len = u32_at(bytes, p + 4) as usize;
        let stop = p
            .checked_add(8)
            .and_then(|p| p.checked_add(len))
            .ok_or("WAV chunk overflow")?;
        if stop > end {
            return err("Truncated WAV chunk");
        }
        match &bytes[p..p + 4] {
            b"fmt " => fmt = Some(&bytes[p + 8..stop]),
            b"data" => data = Some(&bytes[p + 8..stop]),
            _ => {}
        }
        p = stop + len % 2;
    }
    let fmt = fmt.ok_or("WAV has no format chunk")?;
    let data = data.ok_or("WAV has no audio chunk")?;
    if fmt.len() < 16 {
        return err("Truncated WAV format");
    }
    let (mut kind, channels, align, bits) = (
        u16_at(fmt, 0),
        u16_at(fmt, 2) as usize,
        u16_at(fmt, 12) as usize,
        u16_at(fmt, 14),
    );
    if kind == 0xfffe {
        if fmt.len() < 40
            || u16_at(fmt, 16) < 22
            || u16_at(fmt, 18) != bits
            || fmt[26..40] != [0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]
        {
            return err("Unsupported extensible WAV format");
        }
        kind = u16_at(fmt, 24);
    }
    if !(channels == 1 || channels == 2)
        || !((kind == 1 && [8, 16, 24, 32].contains(&bits)) || (kind == 3 && bits == 32))
        || align != channels * (bits as usize / 8)
        || u32_at(fmt, 4) == 0
    {
        return err("Use mono/stereo PCM 8/16/24/32-bit or float32 WAV");
    }
    if data.len() % align != 0 {
        return err("Incomplete WAV sample");
    }
    let count = data.len() / align;
    if count == 0 || count % cycle != 0 {
        return err("Audio length must be a whole number of selected cycles");
    }
    if count / cycle > MAX_FRAMES {
        return err("Maximum 128 frames; choose a larger cycle length");
    }
    let mut samples = Vec::with_capacity(count / cycle * SIZE);
    for frame in data.chunks_exact(cycle * align) {
        let mut re = Vec::with_capacity(cycle);
        for sample in frame.chunks_exact(align) {
            let mut mono = 0.;
            for b in sample.chunks_exact(bits as usize / 8) {
                let value = match (kind, bits) {
                    (3, 32) => f32::from_le_bytes(b.try_into().unwrap()) as f64,
                    (_, 8) => (b[0] as f64 - 128.) / 128.,
                    (_, 16) => i16::from_le_bytes(b.try_into().unwrap()) as f64 / 32768.,
                    (_, 24) => {
                        ((i32::from_le_bytes([b[0], b[1], b[2], 0]) << 8) >> 8) as f64 / 8388608.
                    }
                    (_, 32) => i32::from_le_bytes(b.try_into().unwrap()) as f64 / 2147483648.,
                    _ => unreachable!(),
                };
                if !value.is_finite() || value.abs() > 16. {
                    return err("WAV contains invalid or excessive sample values");
                }
                mono += value / channels as f64;
            }
            re.push(mono);
        }
        let mut im = vec![0.; cycle];
        fft(&mut re, &mut im, false);
        let mut r = vec![0.; SIZE];
        let mut t = vec![0.; SIZE];
        // Drop DC and source/target Nyquist bins. Preserve periodic phase and avoid downsampling aliases.
        for h in 1..(cycle / 2).min(SIZE / 2) {
            r[h] = re[h] / cycle as f64;
            t[h] = im[h] / cycle as f64;
            r[SIZE - h] = re[cycle - h] / cycle as f64;
            t[SIZE - h] = im[cycle - h] / cycle as f64;
        }
        fft(&mut r, &mut t, true);
        samples.extend(r.into_iter().map(|x| x as f32));
    }
    let peak = samples.iter().fold(0.0_f32, |p, x| p.max(x.abs()));
    if peak < 1e-7 {
        return err("WAV is silent after stereo downmix and DC removal");
    }
    // A single gain for the whole bank preserves differences between frames.
    for x in &mut samples {
        *x /= peak.max(1.);
    }
    let source = Source { name, samples };
    source.validate()?;
    Ok(source)
}

pub struct Table {
    pub source: Source,
    data: Vec<f32>,
}
impl Table {
    pub fn compile(source: Source) -> Result<Self, String> {
        source.validate()?;
        let mut data = Vec::with_capacity(source.frames() * SIZE * MIPS);
        for frame in source.samples.chunks_exact(SIZE) {
            let mut re: Vec<_> = frame.iter().map(|x| *x as f64).collect();
            let mut im = vec![0.; SIZE];
            fft(&mut re, &mut im, false);
            for mip in 0..MIPS {
                let mut r = vec![0.; SIZE];
                let mut t = vec![0.; SIZE];
                for h in 1..=1 << mip {
                    for j in [h, SIZE - h] {
                        r[j] = re[j] / SIZE as f64;
                        t[j] = im[j] / SIZE as f64;
                    }
                }
                fft(&mut r, &mut t, true);
                data.extend(r.into_iter().map(|x| x as f32));
            }
        }
        Ok(Self { source, data })
    }
    pub fn sample(&self, position: f32, phase: f32, freq: f32, rate: f32) -> f32 {
        let safe = (rate * 0.45 / freq.max(1.)).max(1.);
        let mip = safe.log2().floor().clamp(0., (MIPS - 1) as f32) as usize;
        let blend = (safe / (1 << mip) as f32 - 1.).clamp(0., 1.);
        let f = position.clamp(0., 1.) * (self.source.frames() - 1) as f32;
        let a = f.floor() as usize;
        let b = (a + 1).min(self.source.frames() - 1);
        let p = phase.rem_euclid(1.) * SIZE as f32;
        let i = p as usize % SIZE;
        let t = p - p.floor();
        let read = |f, m| {
            let start = (f * MIPS + m) * SIZE;
            self.data[start + i] * (1. - t) + self.data[start + (i + 1) % SIZE] * t
        };
        let frame = |f| {
            if mip == 0 {
                read(f, 0)
            } else {
                read(f, mip - 1) * (1. - blend) + read(f, mip) * blend
            }
        };
        frame(a) * (1. - f.fract()) + frame(b) * f.fract()
    }
}
