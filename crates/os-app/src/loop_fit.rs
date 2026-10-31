// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
//! One-click loop fitting and its explicit correction commands.
use crate::app::App;
use crate::playlist as pl;
use os_model::ClipSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit { Auto, Half, Double, Bars(u16), Reset, Edges, Trim }

/// Favor a small timing correction over a half/double-time interpretation of the rhythm.
/// Whole-bar loops stay whole bars; shorter loops use half-beat increments.
fn nearest_length(beats: f64, bar: f64) -> f64 {
    let grid = if beats >= bar * 0.75 { bar } else { 0.5 };
    (beats / grid).round().max(1.0) * grid
}

/// Reject a genuinely silent source window before analysis.
fn has_audio(sample: &os_engine::Sample, window: Option<(f64, f64)>) -> bool {
    let frames = sample.frames();
    let (start, end) = window.map(|(a, b)| {
        let frame = |ms: f64| (ms * f64::from(sample.rate) / 1000.).clamp(0., frames as f64) as usize;
        (frame(a), frame(b))
    }).unwrap_or((0, frames));
    if end <= start { return false; }
    sample.left.get(start..end).into_iter().flatten()
        .chain(sample.right.get(start..end).into_iter().flatten())
        .any(|v| v.is_finite() && v.abs() > 1e-6)
}

/// Explicit pack BPM labels are reliable hints; never treat a loop number as a tempo.
fn tagged_tempo(path:&str)->Option<f64> {
    let name=std::path::Path::new(path).file_stem()?.to_str()?.to_lowercase();
    let at=name.find("bpm")?;
    let before=name[..at].trim_end_matches(|c:char|c.is_whitespace()||c=='_'||c=='-');
    let num:String=before.chars().rev().take_while(|c|c.is_ascii_digit()||*c=='.').collect::<String>().chars().rev().collect();
    num.parse::<f64>().ok().filter(|b|(40.0..=300.0).contains(b))
}

/// Positive energy changes at 200 Hz, normalized periodicity over whole-loop beat-count
/// candidates. Audio is read independently of the clip's current stretched screen length.
fn detected_beats(sample:&os_engine::Sample,near:f64)->Option<(f64,f64)> {
    let rate=sample.rate.max(1) as usize;
    let hop=(rate/200).max(1);
    let n=sample.frames();let secs=n as f64/rate as f64;
    if secs<0.4 || secs>120.0 {return None;}
    let mut env=Vec::new();let mut prev=0.0f64;
    for start in (0..n).step_by(hop) {
        let end=(start+hop).min(n);
        let e=(start..end).map(|i| {let l=sample.left[i] as f64;let r=sample.right[i] as f64;l*l+r*r}).sum::<f64>()/(end-start) as f64;
        let e=e.sqrt();env.push((e-prev).max(0.0));prev=e;
    }
    let energy=env.iter().map(|v|v*v).sum::<f64>();
    if energy<1e-8{return None;}
    let mut best=None;
    for beats in 1..=256 {
        let beats=beats as f64;let bpm=60.0*beats/secs;
        if !(70.0..=180.0).contains(&bpm){continue;}
        let lag=env.len() as f64/beats;
        let corr=|lag:f64| {
            let mut sum=0.0;
            for (i,a) in env.iter().enumerate(){let j=(i as f64+lag)%env.len() as f64;let lo=j.floor() as usize;let f=j-lo as f64;sum+=a*(env[lo]*(1.0-f)+env[(lo+1)%env.len()]*f);}
            sum/energy
        };
        let c=(-2..=2).map(|d|corr(lag+d as f64*0.5)).fold(0.0f64,f64::max);
        let score=c-0.08*(bpm/near.clamp(70.0,180.0)).log2().abs()-if (beats as u32)%4!=0{0.04}else{0.0};
        if c>0.25 && best.is_none_or(|(_,_,s)|score>s){best=Some((beats,bpm,score));}
    }
    best.map(|(beats,bpm,_)|(beats,bpm))
}

fn source_beats(sample:&os_engine::Sample,path:&str,near:f64)->Option<(f64,f64)> {
    let secs=sample.frames() as f64/sample.rate.max(1) as f64;
    if let Some(mut bpm)=tagged_tempo(path) {
        // Packs sometimes label double time: interpret that octave near the project tempo.
        while bpm>190.0 && bpm>near*1.5 {bpm/=2.0;}
        let raw=secs*bpm/60.0;let beats=(raw*2.0).round()/2.0;
        if beats>=0.5 && (raw-beats).abs()<0.08 {return Some((beats,bpm));}
    }
    detected_beats(sample,near)
}

/// Fitting is tempo-locked playback, including samples previously using cached Auto/e3 renders.
pub(crate) fn set_fitted_time(sound:&mut os_model::ChannelSound,beats:f64) {
    sound.set_stretch_time(beats);
    sound.stretch[3]=os_model::stretch_mode::STRETCH_PRO as u32;
}

pub fn run(app: &mut App, i: usize, cmd: Fit) -> Option<String> {
    let clip = *pl::clips(&app.state).get(i)?;
    if pl::source_kind(&app.state, clip.source) != pl::PickKind::Audio { return None; }
    if matches!(cmd, Fit::Edges | Fit::Trim) {
        app.state.playlist.stretch = cmd == Fit::Edges;
        crate::playlist_tools::select(&mut app.state, pl::PlTool::Paint);
        app.invalidate();
        return Some(if cmd == Fit::Edges { "Stretch edges: drag either clip edge; hold Alt for fine adjustment" } else { "Trim edges: dragging changes the visible part of the sample" }.into());
    }
    let ClipSource::Channel(ch) = clip.source else { return None; };
    let row = app.state.project.channels.iter().position(|c| c.index==ch)?;
    let path = crate::plugins::resolve_sample(app.state.project.channels[row].sample_path.as_deref()?);
    if std::fs::metadata(&path).is_ok_and(|m| m.len()>256*1024*1024) { return Some("Sample is too large for quick fitting; choose Stretch clip edges to adjust it manually".into()); }
    let sample = match os_engine::sample::load_shared(std::path::Path::new(&path)) {
        Ok(s)=>s, Err(_)=>return Some("Could not read the sample. Check its file path before stretching".into())
    };
    let secs = sample.frames() as f64/f64::from(sample.rate.max(1));
    if secs<=0.0 { return Some("The sample is empty".into()); }
    let fraction = clip.window.map(|(a,b)| (b-a)/(secs*1000.0)).unwrap_or(1.0).clamp(1e-6,1.0);
    let ppq=f64::from(app.state.project.ppq.max(4));
    let detected=source_beats(&sample,&path,app.state.tempo);
    let visible_beats = match cmd {
        Fit::Auto => {
            if !has_audio(&sample, clip.window) {
                use crate::dialog::{spans,Answer,Kind,Msg,Then};
                app.show_msg(Msg::new(Kind::Info,"Make it fit",spans(&["The visible part of this sample is silent. Choose Stretch clip edges to stretch it manually, or choose a bar length under Stretch / fit options."]),&[Answer::Ok],Then::Nothing));
                app.invalidate();return None;
            }
            detected.map(|(beats,_)|beats*fraction).unwrap_or_else(||nearest_length(secs*app.state.tempo/60.0*fraction, f64::from(pl::bar(&app.state))/ppq))
        }
        Fit::Half => f64::from(clip.length)/ppq/2.,
        Fit::Double => f64::from(clip.length)/ppq*2.,
        Fit::Bars(n) => f64::from(pl::bar(&app.state))*f64::from(n)/ppq,
        Fit::Reset => secs*app.state.project.tick_tempo(app.state.tempo)/60.*fraction,
        _ => return None,
    };
    pl::snapshot_as(&mut app.state,"Fit audio loop");
    let sound=&mut app.state.project.channels[row].sound;
    if cmd==Fit::Reset { sound.stretch[0]=0;sound.stretch[2]=0; }
    else {
        set_fitted_time(sound,(visible_beats/fraction).clamp(1./192.,4096.));
    }
    let sound=*sound;
    app.plugins.set_sampler_stretch(row,&sound);
    if let Some(c)=pl::arr_mut(&mut app.state).clips.get_mut(i) { c.length=(visible_beats*ppq).round().max(1.) as u32; }
    if let Some(fits) = pl::audio_fits(&app.state) { pl::fit_clips(&mut app.state.project.arrangements, &fits); }
    app.state.piano.dirty=true;
    app.invalidate();
    Some(format!("{}Fitted to {visible_beats:.2} beats at {:.2} BPM. Stretch / fit options offers half or double length.",detected.map(|(_,b)|format!("Source tempo {b:.2} BPM. ")).unwrap_or_else(||"No clear beat detected; fitted by source duration. ".into()),app.state.tempo))
}

#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn tempo_analysis_uses_source_not_screen_length() {
    let rate=8000;let frames=(16.0*60.0/126.0*rate as f64) as usize;
    let mut sample=os_engine::Sample{rate,left:vec![0.0;frames],right:vec![0.0;frames]};
    for beat in 0..64 {let start=(beat as f64*60.0/126.0/4.0*rate as f64) as usize;for j in 0..80 {if start+j<frames {sample.left[start+j]=(1.0-j as f32/80.0)*if beat%4==0{0.8}else{0.3};}}}
    let (beats,bpm)=detected_beats(&sample,120.0).unwrap();assert_eq!(beats,16.0);assert!((bpm-126.0).abs()<0.1);
    assert_eq!(source_beats(&sample,"Top Loop 2 - 126 BPM.wav",90.0).unwrap().0,16.0);
    assert_eq!(tagged_tempo("Top Loop 126.wav"),None);
 }
 #[test] fn sanctuary_source_loops_keep_four_bars() {
    let dir=std::path::Path::new(r"E:\PRO Samples\EDM & ELECTRONIC\TOP LOOPS\Top Drum Loops");
    if !dir.exists(){return;}
    for entry in std::fs::read_dir(dir).unwrap(){let path=entry.unwrap().path();if path.extension().and_then(|e|e.to_str())!=Some("wav"){continue;}
        let sample=os_engine::sample::load_shared(&path).unwrap();
        for tempo in [90.0,120.0,130.0,150.0] {
            let (beats,bpm)=source_beats(&sample,&path.to_string_lossy(),tempo).unwrap();assert_eq!(beats,16.0,"{}",path.display());assert_eq!(bpm,126.0);
        }
        let inferred=detected_beats(&sample,120.0);
        eprintln!("{} audio analysis: {:?}",path.file_name().unwrap().to_string_lossy(),inferred);
        assert!(inferred.is_some(),"rhythmic audio should yield a tempo");
    }
 }
 #[test] fn fitting_prefers_a_small_nudge() {
  for (now, want) in [(3.8,4.),(4.2,4.),(7.7,8.),(8.3,8.),(15.3,16.),(2.1,2.),(1.02,1.)] { assert_eq!(nearest_length(now,4.),want); }
  assert_eq!(nearest_length(5.8,3.),6.);
 }
 #[test] fn audio_check_respects_stereo_and_trimmed_windows() {
  let mut sample = os_engine::Sample { rate: 1000, left: vec![0.;2000], right: vec![0.;2000] };
  assert!(!has_audio(&sample,None));
  sample.right[1500]=0.1;
  assert!(has_audio(&sample,None));
  assert!(!has_audio(&sample,Some((0.,1000.))));
  assert!(has_audio(&sample,Some((1000.,2000.))));
  assert!(!has_audio(&sample,Some((3000.,4000.))));
 }
}
