// SPDX-License-Identifier: GPL-3.0-or-later
//! A fine attack pass constrained by the coarse phrase map. Work in seconds so
//! a guide and take can use different sample rates.
use super::Audio;

const HOP: f64 = 0.005;
const SEARCH: f64 = 0.080;
const MAX_ANCHORS: usize = 1024;

#[derive(Clone, Copy)]
pub(super) struct Onset {
    time: f64,
    strength: f32,
    brightness: f32,
    sustain: f32,
}

pub(super) fn detect(audio: &Audio) -> Vec<Onset> {
    let rate = f64::from(audio.sample.rate);
    let step = (rate * HOP).round().max(1.) as usize;
    let mut envelope = Vec::new();
    let mut low = [0.; 2];
    let alpha = 1. - (-std::f32::consts::TAU * 1400. / rate as f32).exp();
    for start in (audio.start..audio.end).step_by(step) {
        let end = (start + step).min(audio.end);
        let (mut full, mut high) = (0., 0.);
        for frame in start..end {
            let stereo = [audio.sample.left[frame], audio.sample.right[frame]];
            for (ch, sample) in stereo.into_iter().enumerate() {
                low[ch] += (sample - low[ch]) * alpha;
                full += sample * sample;
                high += (sample - low[ch]).powi(2);
            }
        }
        envelope.push(((full / (end - start) as f32).sqrt(), (high / (end - start) as f32).sqrt()));
    }
    let peak = envelope.iter().map(|&(a, _)| a).fold(0., f32::max);
    if envelope.len() < 15 || peak < 0.0001 { return Vec::new(); }
    // Compare short windows before and after each potential boundary rather
    // than relying on one sample or on the phase of a voiced waveform.
    let mut scores = vec![0.; envelope.len()];
    let avg = |start: usize, end: usize| {
        let (full, high) = envelope[start..end].iter().fold((0., 0.), |(x, y), &(a, b)| (x + a, y + b));
        let n = (end - start) as f32;
        (full / n / peak, high / n / peak)
    };
    for (i, score) in scores.iter_mut().enumerate().take(envelope.len() - 9).skip(3) {
        let (before, bright_before) = avg(i - 3, i);
        let (after, bright_after) = avg(i, i + 3);
        *score = (after - before).max(0.) + 0.75 * (bright_after - bright_before).max(0.);
    }
    let mut found: Vec<Onset> = Vec::new();
    for i in 3..(scores.len() - 9) {
        let strength = scores[i];
        if strength < 0.09 || scores[i - 3..=i + 3].iter().any(|&other| other > strength) {
            continue;
        }
        let (after, bright) = avg(i, i + 3);
        let (sustain, _) = avg(i + 4, i + 9);
        let onset = Onset {
            time: (i * step) as f64 / rate,
            strength,
            brightness: (bright / after.max(0.001)).min(1.),
            sustain: (sustain / after.max(0.001)).min(1.5),
        };
        if let Some(last) = found.last_mut().filter(|last| onset.time - last.time < 0.035) {
            if onset.strength > last.strength { *last = onset; }
        } else {
            found.push(onset);
        }
    }
    found
}

fn best(source: Onset, expected: f64, candidates: &[Onset]) -> Option<usize> {
    let first = candidates.partition_point(|x| x.time < expected - SEARCH);
    let mut winner = None;
    let mut closest = f64::INFINITY;
    let mut second = f64::INFINITY;
    for (i, &candidate) in candidates.iter().enumerate().skip(first).take_while(|(_, x)| x.time <= expected + SEARCH) {
        let brightness = f64::from((source.brightness - candidate.brightness).abs());
        let sustain = f64::from((source.sustain - candidate.sustain).abs());
        let strength = f64::from((source.strength / candidate.strength).ln().abs());
        let error = (candidate.time - expected).abs() / SEARCH * 0.38
            + brightness * 0.6 + sustain * 0.23 + strength * 0.12;
        if error < closest {
            second = closest;
            closest = error;
            winner = Some(i);
        } else {
            second = second.min(error);
        }
    }
    winner.filter(|_| closest < 0.52 && second - closest > 0.10)
}

fn mapped(points: &[(usize, usize)], value: f64, inverse: bool) -> f64 {
    let axis = |&(x, y): &(usize, usize)| if inverse { (y as f64, x as f64) } else { (x as f64, y as f64) };
    let i = points.partition_point(|p| axis(p).0 <= value).clamp(1, points.len() - 1);
    let (a, x) = axis(&points[i - 1]);
    let (b, y) = axis(&points[i]);
    x + (y - x) * ((value - a) / (b - a)).clamp(0., 1.)
}

/// Refine clear reciprocal matches only. Keep the coarse result on disagreement.
/// Review marks are aligned output times in seconds.
pub(super) fn refine(
    coarse: &[(usize, usize)], take: &[Onset], guide: &[Onset], rate: u32,
    guide_seconds: f64, amount: u8,
) -> (Vec<(usize, usize)>, Vec<f64>) {
    let mut points = coarse.to_vec();
    let mut review = Vec::<f64>::new();
    let rate = f64::from(rate);
    let output = coarse.last().unwrap().1 as f64;
    let scale = output / guide_seconds;
    let gap = (rate * 0.023).round().max(1.) as usize;
    let mut protected_until = 0;
    for (i, &onset) in take.iter().enumerate() {
        let x = (onset.time * rate).round() as usize;
        let predicted = mapped(coarse, x as f64, false);
        let matched = best(onset, predicted / scale, guide).filter(|&j| {
            let expected = mapped(coarse, guide[j].time * scale, true) / rate;
            best(guide[j], expected, take) == Some(i)
        });
        let mut refined = false;
        if let Some(j) = matched {
            let target = guide[j].time * scale;
            let y = (predicted + (target - predicted) * f64::from(amount) / 100.).round() as usize;
            let at = points.partition_point(|&(s, _)| s < x);
            if at > 0 && at < points.len() && x > protected_until {
                // Replace a nearby coarse anchor, which otherwise fights the new correction.
                let replace = if at + 1 < points.len() && points[at].0 - x < gap { Some(at) }
                    else if at > 1 && x - points[at - 1].0 < gap && points[at - 1].0 > protected_until { Some(at - 1) }
                    else { None };
                let (before, after) = match replace {
                    Some(k) => (points[k - 1], points[k + 1]),
                    None => (points[at - 1], points[at]),
                };
                let valid = |a: (usize, usize), b: (usize, usize)| {
                    if b.0 <= a.0 || b.1 <= a.1 { return false; }
                    (0.48..=2.1).contains(&((b.1 - a.1) as f64 / (b.0 - a.0) as f64))
                };
                if x >= before.0 + gap && x + gap <= after.0
                    && valid(before, (x, y)) && valid((x, y), after) {
                    let at = replace.unwrap_or(at);
                    let radius = (rate * 0.010).round() as usize;
                    let can_protect = points.len() + 2 <= MAX_ANCHORS && radius > 0 && y >= radius
                        && valid(before, (x - radius, y - radius))
                        && valid((x + radius, y + radius), after);
                    if can_protect {
                        let island = [(x - radius, y - radius), (x + radius, y + radius)];
                        points.splice(at..at + usize::from(replace.is_some()), island);
                        protected_until = x + radius;
                        refined = true;
                    } else if replace.is_some() || points.len() < MAX_ANCHORS {
                        points.splice(at..at + usize::from(replace.is_some()), [(x, y)]);
                        protected_until = x;
                        refined = true;
                    }
                }
            }
        }
        if !refined && onset.strength > 0.22 && (matched.is_none()
            || matched.is_some_and(|j| (guide[j].time * scale - predicted).abs() > rate * 0.025)) {
            let time = mapped(&points, x as f64, false) / rate;
            if review.len() < 32 && review.last().is_none_or(|&last| time - last >= 0.16) {
                review.push(time);
            }
        }
    }
    (points, review)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocal_align_refines_a_clear_attack_and_flags_an_ambiguous_one() {
        let onset = |time| Onset { time, strength: 0.7, brightness: 0.3, sustain: 0.7 };
        let coarse = [(0, 0), (800, 800), (1200, 1200), (1700, 1700), (2000, 2000)];
        let take = [onset(1.), onset(1.4)];
        let guide = [onset(1.055), onset(1.36), onset(1.44)];
        let (tight, review) = refine(&coarse, &take, &guide, 1000, 2., 100);
        assert!(tight.windows(2).any(|w| w == [(990, 1045), (1010, 1065)]));
        assert_eq!(review, vec![1.4]);
        assert_eq!(mapped(&tight, 1000., false), 1055.);
        let (natural, _) = refine(&coarse, &take, &guide, 1000, 2., 45);
        assert_eq!(mapped(&natural, 1000., false), 1025.);
        assert!(tight.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1));
    }
}
