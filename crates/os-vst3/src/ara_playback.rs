// SPDX-License-Identifier: GPL-3.0-or-later
//! Clip voices on an ARA source-relative renderer. Storage is allocated before activation.
use os_engine::{instrument::VOICE_GAIN_FRAMES, NoteLevels};

#[derive(Clone, Copy)]
pub(crate) enum Action {
    Start {
        key: u8,
        id: u32,
        start: f32,
        velocity: f32,
        levels: NoteLevels,
    },
    Gain(u32, f32),
    Stop(u32),
    KeyOff(u8),
}
#[derive(Clone, Copy)]
struct Event {
    time: u32,
    order: usize,
    action: Action,
}
#[derive(Clone, Copy)]
struct Voice {
    key: u8,
    id: u32,
    seconds: f64,
    velocity: f32,
    pan: f32,
    gain: f32,
    target: f32,
    step: f32,
    ramp: u32,
    first_gain: bool,
}
pub(crate) struct Playback {
    duration: f64,
    voices: Vec<Voice>,
    events: Vec<Event>,
    left: Vec<f32>,
    right: Vec<f32>,
    overflow: bool,
}
impl Playback {
    pub fn new(duration: f64) -> Self {
        Self {
            duration,
            voices: Vec::with_capacity(64),
            events: Vec::with_capacity(8192),
            left: vec![0.; crate::MAX_FRAMES as usize],
            right: vec![0.; crate::MAX_FRAMES as usize],
            overflow: false,
        }
    }
    pub fn idle(&self) -> bool {
        self.voices.is_empty() && self.events.is_empty()
    }
    pub fn count(&self) -> usize {
        self.voices.len()
    }
    pub fn clear(&mut self) {
        self.voices.clear();
        self.events.clear();
        self.overflow = false;
    }
    pub fn event(&mut self, time: u32, action: Action) {
        if self.events.len() == self.events.capacity() {
            self.overflow = true;
            return;
        }
        self.events.push(Event {
            time,
            order: self.events.len(),
            action,
        });
    }
    fn apply(&mut self, action: Action) {
        match action {
            Action::Start {
                key,
                id,
                start,
                velocity,
                levels,
            } => {
                self.voices.retain(|v| v.id != id);
                if self.voices.len() == self.voices.capacity() {
                    return;
                }
                self.voices.push(Voice {
                    key,
                    id,
                    seconds: f64::from(start.clamp(0., 1.)) * self.duration,
                    velocity: os_engine::laws::velocity_gain(
                        (velocity.clamp(0., 1.) * 128.).round() as u8,
                    ) as f32,
                    pan: levels.pan.clamp(-1., 1.),
                    gain: 1.,
                    target: 1.,
                    step: 0.,
                    ramp: 0,
                    first_gain: true,
                });
            }
            Action::Gain(id, gain) => {
                if let Some(v) = self.voices.iter_mut().find(|v| v.id == id) {
                    let gain = if gain.is_finite() { gain.max(0.) } else { 0. };
                    v.target = gain;
                    if v.first_gain {
                        v.gain = gain;
                        v.first_gain = false;
                    } else {
                        v.ramp = VOICE_GAIN_FRAMES;
                        v.step = (gain - v.gain) / VOICE_GAIN_FRAMES as f32;
                    }
                }
            }
            Action::Stop(id) => self.voices.retain(|v| v.id != id),
            Action::KeyOff(key) => self.voices.retain(|v| v.key != key),
        }
    }
    pub fn process(
        &mut self,
        rate: u32,
        left: &mut [f32],
        right: &mut [f32],
        mut render: impl FnMut(u64, &mut [f32], &mut [f32]),
    ) {
        left.fill(0.);
        right.fill(0.);
        // Fail silent rather than lose a queued stop on pathological event overflow.
        if self.overflow {
            self.clear();
            return;
        }
        self.events.sort_unstable_by_key(|e| (e.time, e.order));
        let mut at = 0;
        let mut ei = 0;
        while at < left.len() {
            while ei < self.events.len() && self.events[ei].time as usize <= at {
                self.apply(self.events[ei].action);
                ei += 1;
            }
            let end = self
                .events
                .get(ei)
                .map_or(left.len(), |e| (e.time as usize).min(left.len()))
                .min(at + self.left.len());
            let n = end - at;
            for v in &mut self.voices {
                let remaining =
                    ((self.duration - v.seconds).max(0.) * rate as f64).round() as usize;
                let count = n.min(remaining);
                if count == 0 {
                    continue;
                }
                render(
                    (v.seconds * rate as f64).round() as u64,
                    &mut self.left[..count],
                    &mut self.right[..count],
                );
                for j in 0..count {
                    if v.ramp > 0 {
                        v.gain += v.step;
                        v.ramp -= 1;
                        if v.ramp == 0 {
                            v.gain = v.target;
                        }
                    }
                    let gain = v.velocity * v.gain;
                    left[at + j] += self.left[j] * gain * (1. - v.pan.max(0.));
                    right[at + j] += self.right[j] * gain * (1. + v.pan.min(0.));
                }
                v.seconds += n as f64 / rate as f64;
            }
            self.voices
                .retain(|v| v.seconds < self.duration - 0.5 / rate as f64);
            at = end;
        }
        self.events.drain(..ei);
        for e in &mut self.events {
            e.time = e.time.saturating_sub(left.len() as u32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ara_clip_offsets_overlap_gain_stop_and_rate() {
        let mut p = Playback::new(2.);
        let start = |id, start| Action::Start {
            key: 60,
            id,
            start,
            velocity: 100. / 128.,
            levels: NoteLevels::default(),
        };
        p.event(4, start(1, 0.25));
        p.event(4, Action::Gain(1, 0.5));
        p.event(8, start(2, 0.5));
        p.event(12, Action::Stop(1));
        p.event(16, Action::Stop(2));
        let mut l = [0.; 20];
        let mut r = l;
        p.process(100, &mut l, &mut r, |frame, l, r| {
            l.fill(frame as f32);
            r.fill(frame as f32);
        });
        assert_eq!(&l[..4], &[0.; 4]);
        assert_eq!(l[4], 25.);
        assert_eq!(l[8], 127.);
        assert_eq!(l[12], 104.);
        assert_eq!(&l[16..], &[0.; 4]);
        assert_eq!(p.count(), 0);
        assert_eq!(l, r);
        p.event(0, start(3, 0.5));
        p.process(200, &mut l, &mut r, |f, l, r| {
            assert_eq!(f, 200);
            l.fill(1.);
            r.fill(1.);
        });
        p.clear();
        assert_eq!(p.count(), 0);
    }
}
