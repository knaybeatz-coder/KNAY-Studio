// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
//! One-click loop fitting and its explicit correction commands.
use crate::app::App;
use crate::playlist as pl;
use os_model::ClipSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit { Detect, Auto, Half, Double, Bars(u16), Beats(u16), EnterTempo, Tempo(u32), Reset, Edges, Trim }

pub fn from_sampler(code:isize)->Option<Fit>{
    Some(match code{1020000..=1400000=>Fit::Tempo((code-1000000) as u32),0=>Fit::Detect,1=>Fit::Auto,2=>Fit::EnterTempo,3=>Fit::Reset,
        101..=116=>Fit::Beats((code-100) as u16),1001..=1016=>Fit::Bars((code-1000) as u16),_=>return None})
}

/// The sampler and rack share the same commands, independent of Playlist placement.
pub fn channel_items(row:usize)->Vec<pl::PItem>{
    let item=|label:&str,fit|pl::PItem::new(label.to_string(),pl::Cmd::Rack(crate::rack::RCmd::FitSample(row,fit)));
    let mut items=vec![item("Detect tempo",Fit::Detect),item("Make it fit",Fit::Auto),item("Enter source BPM...",Fit::EnterTempo),pl::PItem::sep()];
    let lengths=[1,2,3,4,8,16].into_iter().map(|n|item(&format!("{n} beat(s)"),Fit::Beats(n))).chain(
        [1,2,4,8,16].into_iter().map(|n|item(&format!("{n} bar(s)"),Fit::Bars(n)))).collect();
    items.push(pl::PItem::sub("Fit to length",lengths));
    items.push(item("Reset stretching",Fit::Reset));items
}

/// Fit the sample channel itself; do not create clips or change pattern notes.
pub fn run_channel(app:&mut App,row:usize,cmd:Fit)->Option<String>{
    let channel=app.state.project.channels.get(row)?;
    if !channel.kind.is_sampler(){return None;}
    if crate::ara::is(channel){return Some("Change timing in the ARA editor, or remove ARA before using sampler stretch".into());}
    if cmd==Fit::EnterTempo{
        crate::typein::open(&mut app.state,crate::typein::Target::SampleTempo(row),(app.mouse.0,app.mouse.1));
        app.invalidate();return None;
    }
    let mut sound=channel.sound;
    let message=if cmd==Fit::Reset{
        sound.stretch[0]=0;sound.stretch[2]=0;
        "Sample stretching reset".into()
    }else{
        let path=crate::plugins::resolve_sample(channel.sample_path.as_deref()?);
        if std::fs::metadata(&path).is_ok_and(|m|m.len()>256*1024*1024){return Some("Sample is too large for quick tempo analysis".into());}
        let sample=match os_engine::sample::load_shared(std::path::Path::new(&path)){
            Ok(s)=>s,Err(_)=>return Some("Could not read the sample. Check its file path".into())
        };
        let secs=sample.frames() as f64/f64::from(sample.rate.max(1));
        if secs<=0.{return Some("The sample is empty".into());}
        let detected=if matches!(cmd,Fit::Auto|Fit::Detect){source_beats(&sample,&path,app.state.tempo)}else{None};
        if cmd==Fit::Detect{return Some(detected.map_or_else(||"No consistent tempo found. Try a drum loop or enter its source BPM.".into(),|(_,b)|format!("Detected source tempo: {b:.1} BPM")));}
        let beats=match cmd{
            Fit::Auto=>{
                if !has_audio(&sample,None){return Some("The sample is silent; choose a beat/bar length manually".into());}
                detected.map(|v|v.0).unwrap_or_else(||nearest_length(secs*app.state.tempo/60.,f64::from(pl::bar(&app.state))/f64::from(app.state.project.ppq.max(4))))
            },
            Fit::Beats(n)=>f64::from(n),
            Fit::Bars(n)=>f64::from(n)*f64::from(pl::bar(&app.state))/f64::from(app.state.project.ppq.max(4)),
            Fit::Tempo(milli) if (20000..=400000).contains(&milli)=>secs*f64::from(milli)/60000.,
            _=>return None,
        };
        set_fitted_time(&mut sound,beats.clamp(1./192.,4096.));
        let prefix=if cmd==Fit::Auto && detected.is_none(){"No clear beat detected; fitted by duration. "}else{""};
        format!("{prefix}Sample fitted to {beats:.2} beats at {:.2} BPM",app.state.tempo)
    };
    let id=app.state.project.channels[row].index;
    let linked=app.state.project.arrangements.iter().any(|a|a.clips.iter().any(|c|c.source==ClipSource::Channel(id)));
    let snapshot=crate::history::Snap::sample(&app.state,row)?;
    if linked{pl::snapshot_with(&mut app.state,"Fit sample tempo",snapshot);}
    else{crate::history::record(&mut app.state,"Fit sample tempo",snapshot);}
    app.state.project.channels[row].sound=sound;
    app.plugins.set_sampler_stretch(row,&sound);
    if let Some(fits)=pl::audio_fits(&app.state){pl::fit_clips(&mut app.state.project.arrangements,&fits);}
    app.state.piano.dirty=true;app.invalidate();Some(message)
}

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
    if secs<0.4 {return None;}
    if secs>16. {
        // Measure local pulse rather than assuming a whole song contains an exact number of bars.
        let mut votes=Vec::new();
        for fraction in [0.15,0.45,0.75] {
            let start=((n-rate*12) as f64*fraction) as usize;
            let mut env=Vec::new();let mut prev=0.0f64;
            for i in (start..start+rate*12-hop).step_by(hop) {
                let energy=(i..i+hop).step_by(4).map(|j|f64::from(sample.left[j]).powi(2)+f64::from(sample.right[j]).powi(2)).sum::<f64>().sqrt();
                env.push((energy-prev).max(0.));prev=energy;
            }
            // Spread one transient over nearby bins so fractional lags do not artificially favor half-time.
            let env:Vec<f64>=env.windows(5).map(|w|w[0]+2.*w[1]+3.*w[2]+2.*w[3]+w[4]).collect();
            let hz=rate as f64/hop as f64;
            let corr=|lag:usize|{
                let(mut ab,mut aa,mut bb)=(0.,0.,0.);
                for (&a,&b) in env.iter().zip(env.iter().skip(lag)){ab+=a*b;aa+=a*a;bb+=b*b;}
                ab/(aa*bb).sqrt().max(1e-12)
            };
            let mut best=(0.,0.);
            for lag in (hz*60./210.) as usize..=(hz*60./55.) as usize {
                if lag<2 {continue;}
                let (left,mid,right)=(corr(lag-1),corr(lag),corr(lag+1));
                if mid<0.26 || mid<left || mid<right {continue;}
                let adjustment=(0.5*(right-left)/(2.*mid-left-right).max(1e-9)).clamp(-0.5,0.5);
                let bpm=60.*hz/(lag as f64+adjustment);
                let score=mid-0.08*(bpm/near.clamp(55.,210.)).log2().abs();
                if score>best.1 {best=(bpm,score);}
            }
            if best.1>0.2 {votes.push(best);}
        }
        let best=votes.iter().map(|&(b,_)|{
            let matching:Vec<_>=votes.iter().filter(|(v,_)|(v-b).abs()<1.8).collect();
            let weight=matching.iter().map(|v|v.1).sum::<f64>();
            (matching.iter().map(|v|v.0*v.1).sum::<f64>()/weight,weight,matching.len())
        }).filter(|v|v.2>=2).max_by(|a,b|a.1.total_cmp(&b.1))?;
        return Some((secs*best.0/60.,best.0));
    }
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

unsafe extern "C" {
    fn lemon_minibpm(left:*const f32,right:*const f32,count:i32,rate:f32)->f64;
}

fn minibpm(sample:&os_engine::Sample)->Option<f64>{
    let rate=sample.rate;
    if rate<1000 || sample.left.len()!=sample.right.len(){return None;}
    let limit=(u64::from(rate)*120).min(i32::MAX as u64) as usize;
    let count=sample.frames().min(limit);
    if count<rate as usize*2{return None;}
    let start=(sample.frames()-count)/2;
    // Constant tones/DC can produce plausible-looking native candidates from the
    // analysis window alone. Require measurable envelope variation first.
    let hop=(rate as usize/20).max(1);
    let mut sum=0.;let mut squares=0.;let mut windows=0.;
    for offset in (start..start+count-hop).step_by(hop){
        let energy=(offset..offset+hop).step_by(8).map(|j|{
            let l=f64::from(sample.left[j]);let r=f64::from(sample.right[j]);
            if l.is_finite() && r.is_finite(){l*l+r*r}else{0.}
        }).sum::<f64>().sqrt();
        sum+=energy;squares+=energy*energy;windows+=1.;
    }
    let mean=sum/windows;
    if mean<1e-6 || squares/windows-mean*mean<mean*mean*0.0025{return None;}
    // Both slices contain count contiguous frames and remain alive throughout the native call.
    let bpm=unsafe{lemon_minibpm(sample.left[start..].as_ptr(),sample.right[start..].as_ptr(),count as i32,rate as f32)};
    (bpm.is_finite() && (55.0..=210.0).contains(&bpm)).then_some(bpm)
}

fn source_beats(sample:&os_engine::Sample,path:&str,near:f64)->Option<(f64,f64)> {
    let secs=sample.frames() as f64/sample.rate.max(1) as f64;
    if let Some(mut bpm)=tagged_tempo(path) {
        // Packs sometimes label double time: interpret that octave near the project tempo.
        while bpm>190.0 && bpm>near*1.5 {bpm/=2.0;}
        let raw=secs*bpm/60.0;let beats=(raw*2.0).round()/2.0;
        if beats>=0.5 && (raw-beats).abs()<0.08 {return Some((beats,bpm));}
    }
    let pulse=detected_beats(sample,near);
    if let Some(mut bpm)=minibpm(sample){
        // MiniBPM's bar comb can interpret a steady pulse as a triplet tempo.
        // Corroborate with our independent onset estimate and resolve octave ambiguity.
        if let Some((_,reference))=pulse {
            while bpm<reference*0.75 {bpm*=2.;}
            while bpm>reference*1.5 {bpm/=2.;}
            if (bpm/reference-1.).abs()>0.03 {return pulse;}
        }
        let raw=secs*bpm/60.;
        // Short pack loops have an exact musical length despite estimator rounding.
        let nearest=(raw*2.).round()/2.;
        let beats=if secs<=16. && (raw-nearest).abs()<0.12{nearest}else{raw};
        return Some((beats,bpm));
    }
    pulse
}

/// Fitting is tempo-locked playback, including samples previously using cached Auto/e3 renders.
pub(crate) fn set_fitted_time(sound:&mut os_model::ChannelSound,beats:f64) {
    sound.set_stretch_time(beats);
    sound.stretch[3]=os_model::stretch_mode::STRETCH_PRO as u32;
}

pub fn run(app: &mut App, i: usize, cmd: Fit) -> Option<String> {
    let clip = *pl::clips(&app.state).get(i)?;
    if pl::source_kind(&app.state, clip.source) != pl::PickKind::Audio { return None; }
    if let ClipSource::Channel(id)=clip.source {if app.state.project.channels.iter().any(|c|c.index==id && crate::ara::is(c)) && cmd!=Fit::Trim {
        return Some("Change timing in the ARA editor, or remove ARA before using sampler stretch".into());
    }}
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
    if cmd==Fit::Detect {
        return Some(detected.map_or_else(||"No consistent tempo found. Try a drum stem.".into(),|(_,b)|format!("Detected source tempo: {b:.1} BPM")));
    }
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
    let mut targets=vec![i];
    if cmd==Fit::Auto && app.state.playlist.sel.contains(&i){
        targets.extend(app.state.playlist.sel.iter().copied().filter(|&j|j!=i && pl::clips(&app.state).get(j).is_some_and(|c|pl::source_kind(&app.state,c.source)==pl::PickKind::Audio)));
    }
    let ratio=visible_beats*ppq/f64::from(clip.length.max(1));
    let mut channels=std::collections::HashSet::new();
    for index in &targets{
        let Some(target)=pl::clips(&app.state).get(*index).copied()else{continue;};
        let ClipSource::Channel(id)=target.source else{continue;};
        let Some(channel)=app.state.project.channels.iter().position(|c|c.index==id)else{continue;};
        if channels.insert(channel){
            if cmd==Fit::Reset{let sound=&mut app.state.project.channels[channel].sound;sound.stretch[0]=0;sound.stretch[2]=0;}
            else {
                let beats=if *index==i {visible_beats/fraction}else{
                    let window=pl::window(&app.state,&target);
                    let duration=(window.1-window.0).max(1.)/1000.;
                    let source=app.state.project.channels[channel].sample_path.as_deref().and_then(|p|os_engine::sample::load_shared(std::path::Path::new(&crate::plugins::resolve_sample(p))).ok());
                    let fraction=source.as_ref().map_or(1.,|s|duration/(s.frames() as f64/f64::from(s.rate.max(1))));
                    f64::from(target.length)*ratio/ppq/fraction
                };
                set_fitted_time(&mut app.state.project.channels[channel].sound,beats.clamp(1./192.,4096.));
            }
            let sound=app.state.project.channels[channel].sound;
            app.plugins.set_sampler_stretch(channel,&sound);
        }
        if let Some(c)=pl::arr_mut(&mut app.state).clips.get_mut(*index){c.length=(f64::from(target.length)*ratio).round().max(1.) as u32;}
    }
    if let Some(fits) = pl::audio_fits(&app.state) { pl::fit_clips(&mut app.state.project.arrangements, &fits); }
    app.state.piano.dirty=true;
    app.invalidate();
    Some(format!("{}Fitted to {visible_beats:.2} beats at {:.2} BPM. Stretch / fit options offers half or double length.",detected.map(|(_,b)|format!("Source tempo {b:.2} BPM. ")).unwrap_or_else(||"No clear beat detected; fitted by source duration. ".into()),app.state.tempo))
}

#[cfg(test)]
mod tests {
 use super::*;
 #[test]
 fn rack_sample_fit_without_playlist_supports_bpm_lengths_reset_and_undo(){
    let mut app=App::new(None);app.state.project=crate::app::test_project();app.state.tempo=120.;
    let path=std::env::temp_dir().join(format!("lemon-rack-fit-{}-90bpm.wav",std::process::id()));
    os_engine::wav::write_f32_stereo(&mut std::fs::File::create(&path).unwrap(),8000,&vec![0.2;128000]).unwrap();
    let row=0;app.state.project.channels[row].sample_path=Some(path.to_string_lossy().into_owned());
    app.tick();
    let before=app.state.project.channels[row].sound;
    let patterns=app.state.project.patterns.clone();let arrangements=app.state.project.arrangements.clone();
    let menu=crate::rack::channel_menu(&app.state,row,10.,10.);
    let fitting=menu.items.iter().find(|i|i.label=="Tempo / stretching").unwrap();assert!(fitting.enabled);
    let detect=&fitting.children.iter().find(|i|i.label=="Detect tempo").unwrap().cmd;
    assert!(pl::run(&mut app,detect.clone()).unwrap().contains("90.0"));
    assert_eq!(app.state.project.channels[row].sound,before);
    let fit=fitting.children.iter().find(|i|i.label=="Make it fit").unwrap().cmd.clone();
    pl::run(&mut app,fit);
    assert_eq!(app.state.project.channels[row].sound.stretch_time(),Some(12.));
    assert_eq!(app.state.project.channels[row].sound.stretch_mode(),os_model::stretch_mode::STRETCH_PRO);
    crate::history::undo(&mut app.state);app.tick();
    assert_eq!(app.state.project.channels[row].sound,before);
    run_channel(&mut app,row,Fit::EnterTempo);
    let target=crate::typein::Target::SampleTempo(row);
    assert_eq!(app.state.playlist.edit.as_ref().unwrap().target,pl::EditTarget::TypeIn(target,true));
    crate::typein::accept(&mut app.state,target,true,"NaN");assert!(app.state.sample_tempo_requests.is_empty());
    crate::typein::accept(&mut app.state,target,true,"120");app.state.playlist.edit=None;app.tick();
    assert_eq!(app.state.project.channels[row].sound.stretch_time(),Some(16.));
    for (code,beats) in [(103,3.),(1002,8.)]{
        run_channel(&mut app,row,from_sampler(code).unwrap());
        assert_eq!(app.state.project.channels[row].sound.stretch_time(),Some(beats));
    }
    run_channel(&mut app,row,Fit::Reset);assert_eq!(app.state.project.channels[row].sound.stretch_time(),None);
    assert_eq!(app.state.project.patterns,patterns);assert_eq!(app.state.project.arrangements,arrangements);
    std::fs::remove_file(path).unwrap();
 }

 #[test]
 fn fitting_selected_stems_uses_one_ratio_and_undo(){
    let mut app=App::new(None);app.state.project=crate::app::test_project();app.state.tempo=120.;
    let dir=std::env::temp_dir().join(format!("lemon-minibpm-fit-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();
    let a=dir.join("drum-90bpm.wav");let b=dir.join("vocal.wav");
    for path in [&a,&b]{os_engine::wav::write_f32_stereo(&mut std::fs::File::create(path).unwrap(),8000,&vec![0.2;128000]).unwrap();}
    let x=pl::add_audio_clip(&mut app.state,&a.to_string_lossy(),0,384).unwrap();
    let y=pl::add_audio_clip(&mut app.state,&b.to_string_lossy(),1,384).unwrap();
    app.state.playlist.sel=vec![x,y];
    let before=pl::capture(&app.state);
    run(&mut app,x,Fit::Auto);
    let clips=pl::clips(&app.state);
    assert_eq!(clips[x].position,384);assert_eq!(clips[y].position,384);
    assert_eq!(clips[x].length,clips[y].length);assert_eq!(clips[x].length,12*u32::from(app.state.project.ppq));
    crate::history::undo(&mut app.state);assert_eq!(pl::capture(&app.state),before);
    for path in [a,b]{std::fs::remove_file(path).unwrap();}std::fs::remove_dir(dir).unwrap();
 }

 #[test]
 fn hybrid_detector_handles_minibpm_metric_ambiguity_and_silence(){
    for bpm in [90.,126.,150.]{
        let rate=44100u32;let frames=(32.*60./bpm*f64::from(rate)).round() as usize;
        let mut sample=os_engine::Sample{rate,left:vec![0.;frames],right:vec![0.;frames]};
        for beat in 0..128{
            let start=(beat as f64*60./bpm/4.*f64::from(rate)).round() as usize;
            for j in 0..2205 {if start+j<frames{
                let frequency=if beat%4==0{85.}else{9500.};
                let v=(1.-j as f32/2205.)*(j as f32*std::f32::consts::TAU*frequency/rate as f32).sin()*if beat%4==0{1.}else{0.25};
                sample.left[start+j]=v;sample.right[start+j]=v;
            }}
        }
        let native=minibpm(&sample).expect("native detector must return a tempo");
        let (_,detected)=source_beats(&sample,"untagged.wav",120.).unwrap();
        eprintln!("Expected {bpm}, MiniBPM {native}, corroborated {detected}");
        assert!((detected-bpm).abs()<1. || (detected*2.-bpm).abs()<1. || (detected/2.-bpm).abs()<1.);
    }
    let silent=os_engine::Sample{rate:16000,left:vec![0.;160000],right:vec![0.;160000]};
    assert_eq!(minibpm(&silent),None);
    assert_eq!(source_beats(&silent,"untagged.wav",120.),None);
    let steady=os_engine::Sample{rate:16000,left:vec![0.2;160000],right:vec![0.2;160000]};
    assert_eq!(minibpm(&steady),None);
 }

 #[test] fn tempo_analysis_uses_source_not_screen_length() {
    let rate=8000;let frames=(16.0*60.0/126.0*rate as f64) as usize;
    let mut sample=os_engine::Sample{rate,left:vec![0.0;frames],right:vec![0.0;frames]};
    for beat in 0..64 {let start=(beat as f64*60.0/126.0/4.0*rate as f64) as usize;for j in 0..80 {if start+j<frames {sample.left[start+j]=(1.0-j as f32/80.0)*if beat%4==0{0.8}else{0.3};}}}
    let (beats,bpm)=detected_beats(&sample,120.0).unwrap();assert_eq!(beats,16.0);assert!((bpm-126.0).abs()<0.1);
    assert_eq!(source_beats(&sample,"Top Loop 2 - 126 BPM.wav",90.0).unwrap().0,16.0);
    assert_eq!(tagged_tempo("Top Loop 126.wav"),None);
 }
 #[test] fn detects_full_stem_with_leading_silence_and_partial_bar(){
    let rate=8000;let frames=55*rate;let mut sample=os_engine::Sample{rate:rate as u32,left:vec![0.;frames],right:vec![0.;frames]};
    for beat in 0..104 {let i=rate*2+(beat as f64*rate as f64*60./127.) as usize;for j in 0..80 {sample.left[i+j]=1.-j as f32/80.;}}
    assert!((detected_beats(&sample,120.).unwrap().1-127.).abs()<1.);
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
        let (beats,bpm)=source_beats(&sample,"untagged.wav",120.).unwrap();
        assert_eq!(beats,16.0,"untagged {} at {bpm}",path.display());
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
