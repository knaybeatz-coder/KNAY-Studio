// SPDX-License-Identifier: GPL-3.0-or-later
//! An independent, bounded input queue: monitoring never consumes the disk recorder's samples.
use crate::capture::{Input, InputStream, Ring};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

#[derive(Default)]
struct Read {
    owner: Option<u64>,
    last: [f32;2],
    from: [f32;2],
    fade_left: u32,
    fade_len: u32,
    heard: bool,
    starved: bool,
    rate: u32,
    output_rate: u32,
    phase: f64,
    a: Option<[f32; 2]>,
    b: Option<[f32; 2]>,
}
impl Read {
    fn soften(&mut self,rate:u32) {
        if self.heard {self.from=self.last;self.fade_len=(rate/333).max(1);self.fade_left=self.fade_len;}
    }
    fn sample(&mut self,v:Option<[f32;2]>,rate:u32)->[f32;2] {
        let starved=v.is_none();
        if starved!=self.starved {self.soften(rate);self.starved=starved;}
        let mut out=v.unwrap_or([0.;2]);
        if v.is_some() {self.heard=true;}
        if self.fade_left>0 {
            self.fade_left-=1;
            let old=self.fade_left as f32/self.fade_len as f32;
            for k in 0..2 {out[k]=self.from[k]*old+out[k]*(1.-old);}
        }
        self.last=out;out
    }
}
pub struct Monitor {
    on: AtomicBool,
    asio: AtomicBool,
    first: AtomicU32,
    stereo: AtomicBool,
    track: AtomicU32,
    rate: AtomicU32,
    block: AtomicU32,
    dropouts: AtomicU64,
    baseline: AtomicU64,
    ring: Ring,
    read: Mutex<Read>,
}
impl Monitor {
    fn new() -> Self {
        Self {
            on: AtomicBool::new(false),
            asio: AtomicBool::new(true),
            first: AtomicU32::new(0),
            stereo: AtomicBool::new(false),
            track: AtomicU32::new(0),
            rate: AtomicU32::new(48000),
            block: AtomicU32::new(0),
            dropouts: AtomicU64::new(0),
            baseline: AtomicU64::new(0),
            ring: Ring::new(1 << 16),
            read: Mutex::new(Read::default()),
        }
    }
    pub fn on(&self) -> bool {
        self.on.load(Ordering::Acquire)
    }
    pub(crate) fn start(&self, track: u16, input: &Input) {
        self.on.store(false, Ordering::Release);
        // The UI can wait; neither audio callback nor engine waits for this lock.
        let mut read = self.read.lock().unwrap_or_else(|e| e.into_inner());
        self.ring.clear();
        self.block.store(0,Ordering::Relaxed);
        *read = Read::default();
        self.asio.store(input.asio, Ordering::Relaxed);
        self.first.store(input.first.into(), Ordering::Relaxed);
        self.stereo.store(input.stereo, Ordering::Relaxed);
        self.track.store(track.into(), Ordering::Relaxed);
        self.on.store(true, Ordering::Release);
    }
    pub(crate) fn stop(&self) {
        self.on.store(false, Ordering::Release);
    }
    pub fn push(&self, asio: bool, rate: u32, frames: usize, at: impl Fn(usize, usize) -> f32) {
        if !self.on() || self.asio.load(Ordering::Relaxed) != asio {
            return;
        }
        self.rate.store(rate.max(1), Ordering::Relaxed);
        self.block.store(frames.min(u32::MAX as usize) as u32,Ordering::Relaxed);
        let first = self.first.load(Ordering::Relaxed) as usize;
        let second = first + usize::from(self.stereo.load(Ordering::Relaxed));
        self.ring.push_with(frames * 2, |k| {
            let v = at(if k % 2 == 0 { first } else { second }, k / 2);
            if v.is_finite() { v } else { 0.0 }
        });
    }
    #[cfg(test)]
    pub fn render(&self, output_rate: u32, tracks: &mut [Vec<f32>]) { self.render_owned(output_rate,tracks,0); }
    fn render_owned(&self, output_rate: u32, tracks: &mut [Vec<f32>], owner:u64) {
        if !self.on() {
            return;
        }
        // Also excludes a retiring engine during a transport transition. Never blocks an audio thread.
        let Ok(mut r) = self.read.try_lock() else {
            return;
        };
        let Some(out) = tracks.get_mut(self.track.load(Ordering::Relaxed) as usize) else {
            return;
        };
        let rate = self.rate.load(Ordering::Relaxed).max(1);
        let handover=r.owner.is_some_and(|old|old!=owner);
        if r.rate != rate || r.output_rate != output_rate {
            *r = Read {
                rate,
                output_rate,
                ..Read::default()
            };
        }
        if handover {r.soften(output_rate);}
        r.owner=Some(owner);
        // A new playback session takes monitoring from the idle/live engine. Discard
        // input accumulated during session construction once, rather than carrying
        // a permanent FIFO delay. Normal callback-sized input bursts remain intact.
        let needed=((out.len()/2) as u64*rate as u64).div_ceil(output_rate.max(1) as u64) as usize;
        let callback=self.block.load(Ordering::Relaxed) as usize;
        // The output feeder may render a whole tick beyond its 6 ms maximum
        // margin. Input must tolerate that scheduling span, even for 16-frame
        // callbacks. This is a ceiling, not a target: normal reads do not wait
        // for this headroom to fill. Keep a separate hysteresis band so slight
        // clock/tick variation cannot trigger repeated sample shaving.
        let headroom = callback.max(needed + (rate as usize * 6).div_ceil(1000));
        let max = needed + if handover { 0 } else { headroom }
            + usize::from(rate != output_rate) * 2;
        let trim_at = max + if handover { 0 } else { headroom };
        if self.ring.waiting_frames() > trim_at {
            if r.heard && !handover { self.dropouts.fetch_add(1, Ordering::Relaxed); }
            while self.ring.waiting_frames() > max {
                self.ring.pop_frame();
            }
            r.soften(output_rate);
            r.a = None;
            r.b = None;
            r.phase = 0.0;
        }
        // The feeder renders whole engine ticks, often larger than a device callback.
        // Keep partial input queued until a complete tick is available. Consuming it
        // now and padding the rest repeats a hole on every small callback.
        let available = self.ring.waiting_frames() + usize::from(r.a.is_some()) + usize::from(r.b.is_some());
        let required = needed + usize::from(rate != output_rate);
        if available < required {
            // Count the start of an interruption, not every silent render tick.
            if r.heard && !r.starved && !handover { self.dropouts.fetch_add(1, Ordering::Relaxed); }
            for f in out.chunks_exact_mut(2) {
                let v = r.sample(None, output_rate);
                f[0] += v[0];
                f[1] += v[1];
            }
            return;
        }
        if rate == output_rate {
            for f in out.chunks_exact_mut(2) {
                let v=r.sample(self.ring.pop_frame(),output_rate);
                f[0] += v[0];
                f[1] += v[1];
            }
            return;
        }
        let step = f64::from(rate) / f64::from(output_rate.max(1));
        for f in out.chunks_exact_mut(2) {
            if r.a.is_none() {
                r.a = self.ring.pop_frame();
            }
            if r.b.is_none() {
                r.b = self.ring.pop_frame();
            }
            let (Some(a), Some(b)) = (r.a, r.b) else {
                let v=r.sample(None,output_rate);f[0]+=v[0];f[1]+=v[1];continue;
            };
            let p = r.phase as f32;
            let v=r.sample(Some([a[0]+(b[0]-a[0])*p,a[1]+(b[1]-a[1])*p]),output_rate);
            f[0]+=v[0];f[1]+=v[1];
            r.phase += step;
            while r.phase >= 1.0 {
                r.phase -= 1.0;
                r.a = r.b;
                r.b = self.ring.pop_frame();
                if r.b.is_none() {
                    r.phase = 0.0;
                    break;
                }
            }
        }
    }
}
static EXTRA_MONITORS: [OnceLock<Monitor>; 63] = [const { OnceLock::new() }; 63];
fn stop_all() {
    monitor().stop();
    for m in EXTRA_MONITORS.iter().filter_map(OnceLock::get) { m.stop(); }
}
pub fn push_all(asio: bool, rate: u32, frames: usize, at: impl Fn(usize,usize)->f32) {
    monitor().push(asio,rate,frames,&at);
    for m in EXTRA_MONITORS.iter().filter_map(OnceLock::get) { m.push(asio,rate,frames,&at); }
}
pub fn render_all(rate: u32, tracks: &mut [Vec<f32>], owner:u64) {
    monitor().render_owned(rate,tracks,owner);
    for m in EXTRA_MONITORS.iter().filter_map(OnceLock::get) { m.render_owned(rate,tracks,owner); }
}
/// Monitoring interruptions are separate from device/output underruns.
pub fn visible_dropouts() -> u64 {
    std::iter::once(monitor()).chain(EXTRA_MONITORS.iter().filter_map(OnceLock::get))
        .map(|m| m.dropouts.load(Ordering::Relaxed).saturating_sub(m.baseline.load(Ordering::Relaxed))).sum()
}
pub fn reset_dropouts() {
    for m in std::iter::once(monitor()).chain(EXTRA_MONITORS.iter().filter_map(OnceLock::get)) {
        m.baseline.store(m.dropouts.load(Ordering::Relaxed), Ordering::Relaxed);
    }
}
pub fn monitor() -> &'static Monitor {
    static M: OnceLock<Monitor> = OnceLock::new();
    M.get_or_init(Monitor::new)
}

#[derive(Default)]
pub struct Runtime {
    selected: Option<(u16, Input)>,
    monitored: Vec<(u16, Input)>,
    config: Option<crate::audio_cfg::AudioCfg>,
    error: Option<String>,
    stream: Option<InputStream>,
    pub rate: Option<u32>,
}
impl Runtime {
    pub fn channels(&self) -> Option<u16> { self.stream.as_ref().map(|s|s.channels) }
    pub fn retry(&mut self) {
        self.selected = None;
    }
    fn fail(&mut self, message: String) -> Result<(), String> {
        self.error = Some(message.clone());
        Err(message)
    }
    pub fn owns(&self, input: &Input) -> bool {
        self.stream.is_some() && self.same_device(input)
    }
    pub fn same_device(&self, input: &Input) -> bool {
        self.selected.as_ref().is_some_and(|(_, i)| i.device == input.device && i.asio == input.asio)
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        if self.selected.is_some() {
            stop_all();
        }
    }
}
impl crate::app::App {
    pub(crate) fn sync_input_monitor(&mut self) -> Result<(), String> {
        // Headless tests and screenshots must never open a microphone.
        if crate::transport::is_headless() {
            return Ok(());
        }
        let wanted = self.state.mixer.recording.armed.first().copied().and_then(|t| {
            self.state
                .mixer
                .recording
                .inputs
                .get(&t)
                .cloned()
                .map(|i| (t, i))
        });
        let all: Vec<_> = self.state.mixer.recording.armed.iter().filter_map(|&t| self.state.mixer.recording.inputs.get(&t).cloned().map(|i|(t,i))).collect();
        let config = crate::audio_cfg::AudioCfg::current();
        if self.input_monitor.selected == wanted && self.input_monitor.monitored == all
            && self.input_monitor.config.as_ref() == Some(&config)
        {
            return self.input_monitor.error.clone().map_or(Ok(()), Err);
        }
        stop_all();
        self.input_monitor.stream = None;
        self.input_monitor.rate = None;
        self.input_monitor.selected = wanted.clone();
        self.input_monitor.monitored = all.clone();
        self.input_monitor.config = Some(config.clone());
        self.input_monitor.error = None;
        if let Some((track, input)) = wanted {
            if input.asio {
                if config.asio_driver() != Some(input.device.as_str()) {
                    return self.input_monitor.fail("Select the input from the ASIO driver currently chosen in Settings > Audio".into());
                }
                if self.transport.opened().as_ref().is_some_and(|d| {
                    d.host == crate::device::ASIO
                        && d.name == input.device
                        && input.first + u16::from(input.stereo) >= d.inputs
                }) {
                    return self
                        .input_monitor
                        .fail("The ASIO driver did not open the selected input channel".into());
                }
            }
            let opened = if input.asio {
                Ok(None)
            } else {
                crate::capture::open_input(&input.device).map(Some)
            };
            let stream = match opened {
                Ok(s) => s,
                Err(e) => {
                    return self
                        .input_monitor
                        .fail(format!("Audio input: {e}. Reselect the input to try again"));
                }
            };
            if stream
                .as_ref()
                .is_some_and(|s| input.first + u16::from(input.stereo) >= s.channels)
            {
                return self
                    .input_monitor
                    .fail("The selected input channel is unavailable".into());
            }
            self.input_monitor.rate = stream.as_ref().map(|s| s.rate);
            self.input_monitor.stream = stream;
            monitor().start(track, &input);
            let available = self.input_monitor.channels().or_else(|| self.transport.opened().map(|d| d.inputs));
            for (k,(t,i)) in all.iter().skip(1).enumerate() {
                if k >= EXTRA_MONITORS.len() || i.device != input.device || i.asio != input.asio || available.is_some_and(|n| i.first+u16::from(i.stereo)>=n) {
                    stop_all();
                    return self.input_monitor.fail("An armed input is unavailable on the selected interface".into());
                }
                EXTRA_MONITORS[k].get_or_init(Monitor::new).start(*t,i);
            }
            self.sync_live();
        }
        self.transport
            .listen(self.plugins.editor_open() || monitor().on());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(stereo: bool) -> Input {
        Input {
            device: "Synthetic".into(),
            asio: true,
            first: 1,
            stereo,
        }
    }
    #[test]
    fn dropout_counter_excludes_startup_and_counts_recovery_episodes() {
        let m = Monitor::new(); m.start(1, &input(false));
        let mut tracks = vec![vec![0.; 128]; 2];
        m.render(48000, &mut tracks);
        assert_eq!(m.dropouts.load(Ordering::Relaxed), 0);
        m.push(true, 48000, 64, |_,_| 0.2); m.render(48000, &mut tracks);
        for _ in 0..4 { m.render(48000, &mut tracks); }
        assert_eq!(m.dropouts.load(Ordering::Relaxed), 1);
        m.push(true, 48000, 64, |_,_| 0.2); m.render(48000, &mut tracks);
        m.render(48000, &mut tracks);
        assert_eq!(m.dropouts.load(Ordering::Relaxed), 2);
        for _ in 0..80 { m.push(true, 48000, 64, |_,_| 0.3); }
        m.render(48000, &mut tracks);
        assert_eq!(m.dropouts.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn partial_input_waits_for_a_complete_engine_block() {
        let m = Monitor::new(); m.start(1, &input(false));
        let mut tracks = vec![vec![0.; 256 * 2]; 2];
        for batch in 0..4 {
            m.push(true, 48000, 64, |_, f| (batch * 64 + f) as f32);
            m.render(48000, &mut tracks);
            if batch < 3 {
                assert_eq!(m.ring.waiting_frames(), (batch + 1) * 64);
                assert!(tracks[1].iter().all(|&v| v == 0.));
            }
        }
        for f in 0..256 { assert_eq!(tracks[1][2 * f], f as f32); }
        assert_eq!(m.ring.waiting_frames(), 0);
    }

    #[test]
    fn simultaneous_monitor_routes_are_independent() {
        let _lock=crate::capture::TEST_RECORDING.lock().unwrap_or_else(|e|e.into_inner());
        stop_all();
        let input=Input{device:"Synthetic".into(),asio:true,first:0,stereo:false};
        monitor().start(1,&input);
        EXTRA_MONITORS[0].get_or_init(Monitor::new).start(2,&Input{first:1,..input});
        push_all(true,48000,64,|ch,_|if ch==0{0.2}else{-0.4});
        let mut tracks=vec![vec![0.;128];3];
        render_all(48000,&mut tracks,0);
        assert!(tracks[0].iter().all(|&v|v==0.));
        assert!(tracks[1].iter().all(|&v|(v-0.2).abs()<1e-6));
        assert!(tracks[2].iter().all(|&v|(v+0.4).abs()<1e-6));
        stop_all();
    }

    #[test]
    fn selected_input_stereo_mono_resampling_and_disarm() {
        let m = Monitor::new();
        m.start(1, &input(false));
        m.push(true, 48000, 32, |c, _| c as f32 * 0.25);
        let mut tracks = vec![vec![0.; 32]; 3];
        m.render(48000, &mut tracks);
        assert!(tracks[1].iter().all(|v| *v == 0.25));
        assert!(tracks[0].iter().all(|v| *v == 0.));
        m.start(1, &input(true));
        m.push(true, 24000, 20, |c, f| c as f32 * 0.1 + f as f32 * 0.01);
        tracks[1].fill(0.);
        m.render(48000, &mut tracks);
        for (i, v) in tracks[1].chunks_exact(2).enumerate() {
            assert!((v[0] - (0.1 + i as f32 * 0.005)).abs() < 1e-6);
            assert!((v[1] - v[0] - 0.1).abs() < 1e-6);
        }
        m.stop();
        tracks[1].fill(0.);
        m.render(48000, &mut tracks);
        assert!(tracks[1].iter().all(|v| *v == 0.));
    }
    #[test]
    fn monitor_discontinuities_and_starvation_are_declicked_without_queuing_delay() {
        let m=Monitor::new();m.start(1,&input(false));
        let mut tracks=vec![vec![0.;512];2];
        m.push(true,48000,256,|_,_|0.8);m.render_owned(48000,&mut tracks,1);
        assert_eq!(tracks[1][510],0.8);
        for _ in 0..20 {m.push(true,48000,64,|_,_|-0.8);}
        tracks[1].fill(0.);m.render_owned(48000,&mut tracks,2);
        assert!((tracks[1][0]-0.8).abs()<0.02);
        assert!(tracks[1].chunks_exact(2).map(|f|f[0]).collect::<Vec<_>>().windows(2).all(|w|(w[1]-w[0]).abs()<0.02));
        assert!((tracks[1][510]+0.8).abs()<1e-5);assert_eq!(m.ring.waiting_frames(),0);
        tracks[1].fill(0.);m.render_owned(48000,&mut tracks,2);
        assert!((tracks[1][0]+0.8).abs()<0.02);assert_eq!(tracks[1][510],0.);
        m.push(true,48000,256,|_,_|0.8);tracks[1].fill(0.);m.render_owned(48000,&mut tracks,2);
        assert!(tracks[1][0].abs()<0.02);assert!((tracks[1][510]-0.8).abs()<1e-5);
    }
    #[test]
    fn continued_monitoring_bounds_backlog_without_truncating_normal_blocks() {
        for (input_block,output_block) in [(64,250),(512,250),(1024,128)] {
            let m=Monitor::new();m.start(1,&input(false));
            let mut tracks=vec![vec![0.;output_block*2];2];
            m.render_owned(48000,&mut tracks,1);
            // Stall AFTER ownership is established, as can happen while starting a recording.
            for _ in 0..20 {m.push(true,48000,input_block,|_,_|0.1);}
            m.push(true,48000,input_block,|_,_|0.8);
            m.render_owned(48000,&mut tracks,1);
            assert!(m.ring.waiting_frames()<=input_block.max(output_block + 288),"persistent backlog");
        }
        // Ordinary unequal callback sizes preserve the full input, in order.
        let m=Monitor::new();m.start(1,&input(false));
        let mut tracks=vec![vec![0.;128*2];2];
        for batch in 0..4 {
            m.push(true,48000,1024,|_,f|(batch*1024+f) as f32);
            for part in 0..8 {
                tracks[1].fill(0.);m.render_owned(48000,&mut tracks,1);
                for f in 0..128 {assert_eq!(tracks[1][2*f],(batch*1024+part*128+f) as f32);}
            }
        }
    }
    #[test]
    fn recording_transition_cannot_leave_a_persistent_monitor_backlog() {
        for rate in [44100,48000,96000] {
            let m=Monitor::new();m.start(1,&input(false));
            let block=64;
            let mut tracks=vec![vec![0.;block*2];2];
            m.render_owned(rate,&mut tracks,1); // idle monitoring owns input
            // Input accumulates during a transport start/rebuild, then normal callbacks resume.
            for batch in 0..40 {m.push(true,rate,block,|_,_|batch as f32);}
            m.render_owned(rate,&mut tracks,2); // recording session takes over
            assert_eq!(m.ring.waiting_frames(),0);
            for batch in 40..48 {
                m.push(true,rate,block,|_,_|batch as f32);
                tracks[1].fill(0.);m.render_owned(rate,&mut tracks,2);
                assert!(tracks[1][0]>=batch as f32-(rate as f32/200./block as f32).ceil(),"stale audio retained");
            }
            // Rendering a different output rate uses input frames for the bound.
            m.push(true,rate,4096,|_,_|1.);
            m.render_owned(48000,&mut tracks,3);
            assert!(m.ring.waiting_frames()<=2);
        }
    }
    #[test]
    fn monitoring_does_not_take_samples_from_recording_and_drops_old_backlog() {
        let _lock = crate::capture::TEST_RECORDING
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let m = Monitor::new();
        m.start(1, &input(false));
        let c = crate::capture::capture();
        c.start(0, false);
        c.push(5000, |_, _| 0.2);
        for batch in 0..80 {m.push(true,48000,64,|_,_|if batch<40 {0.1}else{0.4});}
        let mut tracks = vec![vec![0.; 128]; 2];
        m.render(48000, &mut tracks);
        assert!(tracks[1].iter().all(|v| *v == 0.4));
        assert_eq!(c.frames(), 5000);
        c.stop();
    }
}
