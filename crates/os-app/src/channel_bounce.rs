//! Channel-rack consolidation through the existing hosted offline renderer.
use crate::{app::{App, State}, playlist as pl, pattern_render as pr, history::{self, Snap, Replaced}};
use os_model::{Project, ChannelKind, Clip, ClipSource};
use std::path::PathBuf;
#[derive(Debug, Clone)]
pub struct Pending { row: usize, source: Project, host: Option<(usize, Vec<u8>)>, archived: os_model::Channel, song: bool, end: u32, wet: bool }
fn channel_level(t: u32, id: u16) -> bool {
    matches!(os_model::target::decode(t), os_model::target::Target::ChannelVolume(c) | os_model::target::Target::ChannelPan(c) if c == id)
}
fn mixer_target(t:u32)->Option<u16> {
    use os_model::target::Target::*;
    match os_model::target::decode(t) {MixerVolume(t)|MixerPan(t)|MixerSeparation(t)|MixerEq(t,..)|MixerStrip(t,..)|MixerSend(t,..)|SlotOn(t,..)|SlotMix(t,..)|SlotParam(t,..)=>Some(t),_=>None}
}
fn prepare(s: &State, row: usize, wet: bool) -> Result<(Project, crate::export::Job, bool, u32), String> {
    let ch = s.project.channels.get(row).ok_or("Channel no longer exists")?;
    if ch.kind == ChannelKind::Automation { return Err("Choose an instrument or sample channel.".into()); }
    let id = ch.index;
    let mut p = crate::rack::play_project(s);
    p.varispeed_cents = 0; // The resulting audio follows the project's live varispeed once.
    let song = s.song_mode;
    let end = if song { pl::clips(s).iter().filter(|c| !c.muted).map(Clip::end).max().unwrap_or(0) } else { crate::piano::pattern_length(s,s.pattern) };
    if end == 0 { return Err("There is no music to consolidate in this playback mode.".into()); }
    for pat in &mut p.patterns {
        pat.notes.retain(|n| n.channel == id);
        pat.controls.retain(|c| wet || !channel_level(c.kind,id));
    }
    let autos: Vec<_> = p.channels.iter().filter(|c| c.kind == ChannelKind::Automation).map(|c|c.index).collect();
    if let Some(a) = p.arrangement_mut() {
        a.clips.retain(|c| matches!(c.source,ClipSource::Pattern(_)) || matches!(c.source,ClipSource::Channel(n) if n==id || autos.contains(&n)));
        a.markers.clear();
    }
    for c in &mut p.channels { if c.index != id && c.kind != ChannelKind::Automation {c.enabled=false;} }
    p.channels[row].enabled=true;
    if wet {
        let mut used=std::collections::BTreeSet::from([ch.mixer_track]);
        loop {let n=used.len();for m in &p.mixer {if used.contains(&m.index){used.extend(m.routes.iter().copied().filter(|dst|m.send_levels.iter().find(|(t,_)|t==dst).is_none_or(|(_,v)|*v>0.0)));}}if used.len()==n{break;}}
        for m in p.mixer.iter().filter(|m|m.index!=0 && used.contains(&m.index) && m.enabled && m.slots_enabled) {
            let mut inputs:Vec<_>=p.mixer.iter().filter(|src|src.index!=m.index && src.routes.contains(&m.index)).map(|src|src.index).collect();inputs.sort_unstable();
            for (slot,fx) in m.slots.iter().enumerate().filter_map(|(i,f)|f.as_ref().map(|f|(i,f))) {
                if !m.slot_on[slot] || m.slot_mix[slot]<=0.0 {continue;}
                let ports=if fx.name==os_flp::wrapper::WRAPPER {os_flp::wrapper::input_ports(&fx.state).into_iter().filter(|p|*p>0).map(|p|p as usize).collect::<Vec<_>>()}else{os_engine::effects::from_slot(&fx.name,&fx.state,44100).map(|f|vec![f.key_input()]).unwrap_or_default()};
                if ports.into_iter().filter_map(|port|port.checked_sub(1).and_then(|i|inputs.get(i))).any(|src|!used.contains(src)) {return Err("This channel uses an external sidechain. Choose Consolidate (keep mixer live) to preserve it.".into());}
            }
        }
        if p.peak_links.iter().any(|l|!used.contains(&l.source)){return Err("Audio-level links need their source tracks. Choose Consolidate (keep mixer live).".into());}
        if let Some(m)=p.mixer.iter_mut().find(|m|m.index==0){*m=os_model::MixerTrack::new(0);m.strip_bypassed=true;}
    } else {
    // Audio-clip playback normalizes its default level; render at that same reference.
    p.channels[row].volume=0.78125;
    p.channels[row].pan=0.0;
    // Capture before the shared mixer. Inserts, sends, automation and external
    // sidechains remain live on the same routing, so they are never baked twice.
    p.channels[row].mixer_track=0;
    p.mixer=vec![os_model::MixerTrack::new(0)];
    p.mixer[0].strip_bypassed=true;
    p.peak_links.clear();
    let keep = |t| !channel_level(t,id) && !matches!(os_model::target::decode(t), os_model::target::Target::MixerVolume(_) | os_model::target::Target::MixerPan(_) | os_model::target::Target::MixerSeparation(_) | os_model::target::Target::SlotParam(..) | os_model::target::Target::SlotOn(..) | os_model::target::Target::SlotMix(..));
    for a in &mut p.automations {a.targets.retain(|t|keep(*t));}
    p.init_controls.retain(|c|keep(c.target));
    }
    let keep=|t| if wet {mixer_target(t)!=Some(0)} else {mixer_target(t).is_none() && !channel_level(t,id)};
    for a in &mut p.automations {a.targets.retain(|t|keep(*t));}
    for pat in &mut p.patterns {pat.controls.retain(|v|keep(v.kind));}
    p.init_controls.retain(|v|keep(v.target));
    let dir=pr::folder(s);
    let stem=pr::file_stem(&dir,&format!("{} - Consolidated",ch.name.as_deref().unwrap_or("Channel")));
    let mut job=pr::job(s,&dir,&stem);
    job.instrument_rows=Some(vec![row]);
    job.span=Some((0,end as u64));
    job.options.insert_fx=wet;
    Ok((p,job,song,end))
}
impl App {
    pub(crate) fn consolidate_channel(&mut self,row:usize) { self.bounce_channel(row,false); }
    pub(crate) fn bounce_channel(&mut self,row:usize,wet:bool) {
        if let Err(e)=crate::clip_effects::normalize(self){self.state.message=Some(e);self.invalidate();return;}
        if self.export.is_some() {return;}
        match prepare(&self.state,row,wet) {
            Err(e)=>{self.state.message=Some(e);self.invalidate();}
            Ok((p,job,song,end))=>{
                if let Err(e)=std::fs::create_dir_all(&job.folder){self.state.message=Some(e.to_string());return;}
                let mut source=self.state.project.clone();
                if let Err(e)=crate::project_plugins::store_hosted(&mut source,&self.plugins){self.state.message=Some(format!("Cannot preserve original source: {e}"));return;}
                // Permanent recovery copy, including every original MIDI pattern and plugin state.
                let backup=job.folder.join(format!("{} - Source.flp",job.name));
                let saved=os_flp::project::create(&source).map_err(|e|e.to_string()).and_then(|f|f.to_bytes().map_err(|e|e.to_string())).and_then(|b|std::fs::write(&backup,b).map_err(|e|e.to_string()));
                if let Err(e)=saved {self.state.message=Some(format!("Cannot preserve original source: {e}"));return;}
                self.channel_bounce=Some(Pending{row,source:self.state.project.clone(),host:self.plugin_copy(row),archived:source.channels[row].clone(),song,end,wet});
                self.start_project_export(p,job,song);
            }
        }
    }
    pub(crate) fn channel_bounce_done(&mut self,p:Pending,r:Result<(f64,Vec<PathBuf>),String>){
        let result=r.and_then(|(_,files)|{
            if self.state.project!=p.source {return Err("Project changed during rendering; WAV and original source were saved, but the channel was not replaced.".into());}
            let file=files.first().ok_or("Render produced no WAV")?;
            let text=file.to_string_lossy().into_owned();
            let wave=self.state.playlist.wave(&text).ok_or("Cannot read rendered audio")?;
            let length=(wave.secs()*self.state.project.tick_tempo(self.state.tempo)*self.state.project.ppq as f64/60.).ceil() as u32;
            let s=&mut self.state;
            let mut changes=vec![Snap::Replace(Box::new(Replaced{row:p.row,ch:s.project.channels[p.row].clone(),host:p.host.map(|(i,b)|(i,Some(b))),other:None})),Snap::Patterns{patterns:s.project.patterns.clone(),current:s.pattern}, Snap::Tail(Box::new(history::Tail::now(s,s.project.channels.len())))];
            let id=s.project.channels[p.row].index;
            let archived_id=s.project.channels.iter().map(|c|c.index).max().unwrap_or(0).checked_add(1).ok_or("Channel limit reached")?;
            for v in &s.project.init_controls {
                if (p.wet && channel_level(v.target,id)) || matches!(os_model::target::decode(v.target),os_model::target::Target::ChannelPitch(n) | os_model::target::Target::ChannelParam(n,_) if n==id) {
                    changes.push(Snap::Init(v.target,Some(*v)));
                    changes.push(Snap::Init((u32::from(archived_id)<<16)|(v.target&0xffff),None));
                }
            }
            let before=Snap::Several(changes);
            pl::snapshot_with(s,"Consolidate channel",before);
            let archived_id=s.project.channels.iter().map(|c|c.index).max().unwrap_or(0).checked_add(1).ok_or("Channel limit reached")?;
            let mut archived=p.archived.clone();
            archived.index=archived_id;archived.enabled=false;
            archived.name=Some(format!("{} - Original (muted)",archived.name.as_deref().unwrap_or("Channel")));
            s.project.channels.push(archived);
            let c=&mut s.project.channels[p.row];
            let id=c.index;
            c.kind=ChannelKind::AudioClip;c.plugin=None;c.plugin_state=None;c.sample_path=Some(text);
            c.pitch_cents=0;c.misc=os_model::ChannelMisc{pitch_range:c.misc.pitch_range,main_pitch:false,..Default::default()};
            if p.wet {c.volume=0.78125;c.pan=0.0;c.mixer_track=0;}
            c.envelopes=Default::default();c.sound=Default::default();c.sound.sampler_flags=Some(os_model::ChannelSound::AUDIO_CLIP_FLAGS);
            for pat in &mut s.project.patterns {
                for n in &mut pat.notes {if n.channel==id {n.channel=archived_id;}}
                for v in &mut pat.controls {if (p.wet && channel_level(v.kind,id)) || matches!(os_model::target::decode(v.kind),os_model::target::Target::ChannelPitch(n) | os_model::target::Target::ChannelParam(n,_) if n==id) {v.kind=(u32::from(archived_id)<<16)|(v.kind&0xffff);}}
            }
            for a in &mut s.project.automations {for t in &mut a.targets {if (p.wet && channel_level(*t,id)) || matches!(os_model::target::decode(*t),os_model::target::Target::ChannelPitch(n) | os_model::target::Target::ChannelParam(n,_) if n==id){*t=(u32::from(archived_id)<<16)|(*t&0xffff);}}}
            for v in &mut s.project.init_controls {if (p.wet && channel_level(v.target,id)) || matches!(os_model::target::decode(v.target),os_model::target::Target::ChannelPitch(n) | os_model::target::Target::ChannelParam(n,_) if n==id){v.target=(u32::from(archived_id)<<16)|(v.target&0xffff);}}
            if p.song {
                let track=pl::free_track(s,pl::track_after_used(s),0,length);
                let a=pl::arr_mut(s);a.clips.retain(|c|c.source!=ClipSource::Channel(id));a.clips.push(Clip::new(0,length,track,ClipSource::Channel(id)));
            } else {
                let pat=crate::piano::pattern_mut(s);
                pat.notes.push(os_model::Note{channel:id,key:60,position:0,length:p.end,velocity:100,..os_model::Note::step(id,0)});
            }
            s.hosted_rows.retain(|r|*r!=p.row);
            self.plugins.drop_key(p.row);
            self.transport.set_instruments(self.plugins.instruments());
            s.piano.dirty=true;
            Ok(if p.wet {"Channel bounced with mixer effects and tails; audio plays through the master once. Original retained muted; Undo restores it."} else {"Channel consolidated. Mixer effects stay live; Undo restores the instrument and MIDI. Original source saved beside the WAV."}.to_string())
        });
        self.state.message=Some(result.unwrap_or_else(|e|format!("Consolidate: {e}")));self.invalidate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forum_save_preserves_disabled_pitch_follow_and_undo() {
        let mut a=App::new(None);a.state.project=crate::app::test_project();
        a.plugins.available=crate::plugins::builtin();a.sync_samplers(None);a.params_tick();
        let row=*a.state.hosted_rows.iter().next().expect("hosted sampler");
        a.state.project.main_pitch=Some(300);
        a.state.project.channels[row].pitch_cents=600;
        crate::rack::run(&mut a,crate::rack::RCmd::PitchFollow(row));
        let expected=a.state.project.channels[row].clone();
        assert!(!expected.misc.main_pitch);assert_eq!(expected.misc.pitch_follow_offset,300);
        let dir=std::env::temp_dir().join(format!("lemon-forum-save-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();
        let path=dir.join("pitch.flp");
        for _ in 0..2 {
            a.save_to(path.clone());assert!(a.state.message.as_deref().unwrap().starts_with("Saved"),"{:?}",a.state.message);
            let ch=&a.state.project.channels[row];assert_eq!((ch.misc.main_pitch,ch.misc.pitch_follow_offset,ch.pitch_cents),(false,300,600));
            let file=os_flp::FlpFile::parse(&std::fs::read(&path).unwrap()).unwrap();let p=os_flp::project::load(&file).unwrap();
            let ch=&p.channels[row];assert_eq!((ch.misc.main_pitch,ch.misc.pitch_follow_offset,ch.pitch_cents),(false,300,600));
        }
        crate::history::undo(&mut a.state);assert!(a.state.project.channels[row].misc.main_pitch);assert_eq!(a.state.project.channels[row].misc.pitch_follow_offset,0);
        crate::history::redo(&mut a.state);assert!(!a.state.project.channels[row].misc.main_pitch);
        a.save_to(path);assert!(!a.state.project.channels[row].misc.main_pitch);
    }
    #[test]
    fn rack_features_compact_controls_and_filtered_piano_layout() {
        use crate::{app::{Action,Win},knobs::KnobId};
        let mut a=App::new(None);a.state.project=crate::app::test_project();
        a.state.z=vec![Win::ChannelRack];a.state.rack_size=crate::views::rack::DEFAULT_SIZE;
        let mut c=os_ui::Canvas::new(1280,900,1.).unwrap();a.paint(&mut c,None);
        let find=|a:&App,action:Action|a.hits.iter().find(|h|h.action==action).unwrap().rect;
        let pitch=find(&a,Action::Knob(KnobId::ChanPitch(0)));
        let follow=find(&a,Action::RackPitchFollow(0));let name=find(&a,Action::ChannelSelect(0));
        assert!(pitch.w<=22. && pitch.right()<=follow.x && follow.right()<=name.x);
        assert!(find(&a,Action::Knob(KnobId::MasterPitch)).w<=24.);
        assert!(find(&a,Action::Knob(KnobId::Varispeed)).w<=40.);
        a.state.set_knob(KnobId::ChanPitch(0),0.9);
        a.mouse_move(pitch.cx(),pitch.cy());a.mouse_down();a.mouse_up();a.mouse_down();a.mouse_up();
        assert_eq!(a.state.project.channels[0].pitch_cents,0);
        if let Some(dir)=std::env::var_os("OS_SHOT_DIR"){std::fs::create_dir_all(&dir).unwrap();c.pixmap.save_png(std::path::PathBuf::from(dir).join("compact-rack.png")).unwrap();}
        a.state.piano.harmony.shade=true;a.state.piano.harmony.scale_only=true;
        a.state.z=vec![Win::PianoRoll];
        let h=crate::views::piano::grid_rect(&a.state).h;crate::piano::center_keys(&mut a.state,h);
        a.paint(&mut c,None);
        assert!(a.hits.iter().any(|h|h.action==Action::Harmony(crate::harmony::HCmd::ScaleOnly)));
        if let Some(dir)=std::env::var_os("OS_SHOT_DIR"){c.pixmap.save_png(std::path::PathBuf::from(dir).join("scale-only-piano.png")).unwrap();}
    }
    #[test]
    fn rack_features_pitch_follow_persistence_and_scale_geometry() {
        use crate::{knobs::KnobId,rack::{self,RCmd},piano::Geom};
        let mut a=App::new(None);a.state.project=crate::app::test_project();
        assert_eq!(os_model::ChannelMisc::default().pitch_range,12);
        a.state.set_knob(KnobId::ChanPitch(0),0.75);
        assert_eq!(a.state.project.channels[0].pitch_cents,600);
        a.state.project.main_pitch=Some(300);
        let other=a.state.project.channels.get(1).map(|c|c.misc.main_pitch);
        rack::run(&mut a,RCmd::PitchFollow(0));
        assert!(!a.state.project.channels[0].misc.main_pitch);
        assert_eq!(a.state.project.channels[0].misc.pitch_follow_offset,300);
        a.state.project.main_pitch=Some(700);
        rack::run(&mut a,RCmd::PitchFollow(0));
        assert_eq!(a.state.project.channels[0].misc.pitch_follow_offset,-400);
        assert_eq!(a.state.project.channels.get(1).map(|c|c.misc.main_pitch),other);
        a.state.project.varispeed_cents=1200;
        let file=os_flp::project::create(&a.state.project).unwrap();
        let loaded=os_flp::project::load(&file).unwrap();
        assert_eq!(loaded.varispeed_cents,1200);
        assert_eq!(loaded.channels[0].misc.pitch_range,12);
        assert_eq!(loaded.channels[0].misc.pitch_follow_offset,-400);
        a.state.project.patterns[0].notes.push(os_model::Note{key:61,..os_model::Note::step(0,0)});
        let before=a.state.project.patterns.clone();
        a.state.piano.harmony.shade=true;a.state.piano.harmony.scale_only=true;
        for scale in 67..=crate::chords::ENTRIES.len() {
            a.state.piano.harmony.scale=scale;
            for root in 0..12 {
                a.state.piano.harmony.root=root;
                let g=Geom::new(&a.state,os_ui::Rect::new(0.,0.,600.,400.));
                for &k in &g.keys { assert!(a.state.piano.harmony.allows(k));assert_eq!(g.key_at(g.y_of(k)+g.row/2.),k); }
            }
        }
        assert_eq!(a.state.project.patterns,before);
        a.state.piano.harmony.scale_only=false;
        crate::harmony::run(&mut a,crate::harmony::HCmd::ScaleOnly);
        let saved=os_flp::project::load(&os_flp::project::create(&a.state.project).unwrap()).unwrap();
        assert_eq!(saved.harmony.unwrap().2&16,16);
        a.state.piano.harmony.scale_only=false;
        assert_eq!(Geom::new(&a.state,os_ui::Rect::new(0.,0.,600.,400.)).keys.len(),132);
    }
    #[test]
    fn rack_features_bounce_renders_real_audio_and_undo_restores_source() {
        for wet in [false,true] {
        let mut app=App::new(None);app.state.project=crate::app::test_project();app.state.tempo=120.;app.state.song_mode=true;
        app.state.project.mixer[1].volume=Some(0.65);
        app.state.project.mixer[1].eq[0][0]=600;
        let dir=std::env::temp_dir().join(format!("lemon-rack-bounce-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();
        let input=dir.join("source.wav");
        let audio:Vec<f32>=(0..22050).flat_map(|i|{let v=(i as f32*0.073).sin()*0.15;[v,v]}).collect();
        os_engine::wav::write_f32_stereo(&mut std::fs::File::create(&input).unwrap(),44100,&audio).unwrap();
        app.state.project.data_folder=Some(os_model::DataFolder{choice:os_model::DataFolder::SPECIFIED,folder:crate::data_folder::folder_text(&dir)});
        let c=&mut app.state.project.channels[0];c.sample_path=Some(input.to_string_lossy().into_owned());c.kind=ChannelKind::Sampler;c.plugin=None;c.plugin_state=None;
        let id=c.index;
        let pat=crate::piano::pattern_mut(&mut app.state);pat.notes=vec![os_model::Note{length:96,..os_model::Note::step(id,0)}];
        let pattern=app.state.pattern;
        pl::arr_mut(&mut app.state).clips.push(Clip::new(96,192,0,ClipSource::Pattern(pattern)));
        app.state.project.tempo=Some(app.state.tempo);
        let original=app.state.project.clone();
        fn signal(p:&Project)->Vec<f32>{let mut lib=os_engine::SampleLibrary::new(None);let mut e=os_engine::Engine::new(p,&mut lib,44100,os_engine::PlayMode::Song);e.set_span(0,288);e.render_all(64)}
        let expected=signal(&original);
        let mut fast=original.clone();fast.varispeed_cents=1200;
        let sped=signal(&fast);
        assert!((sped.len() as i64*2-expected.len() as i64).abs()<8,"master output must change duration");
        app.bounce_channel(0,wet);
        assert!(app.channel_bounce.is_some(),"{:?}",app.state.message);
        let now=std::time::Instant::now();while app.export.is_some(){assert!(now.elapsed().as_secs()<60);app.export_tick();std::thread::sleep(std::time::Duration::from_millis(5));}
        assert_eq!(app.state.project.channels[0].kind,ChannelKind::AudioClip,"{:?}",app.state.message);
        let output=app.state.project.channels[0].sample_path.as_deref().unwrap();let wave=app.state.playlist.wave(output).unwrap();
        assert!(wave.peaks.iter().any(|(lo,hi)|lo.abs().max(hi.abs())>0.01));
        assert_eq!(app.state.project.channels.len(),original.channels.len()+1);
        assert!(!app.state.project.channels.last().unwrap().enabled);
        let actual=signal(&app.state.project);
        let err=expected.iter().zip(&actual).map(|(a,b)|(a-b).abs()).fold(0f32,f32::max);
        assert!(err<0.002,"consolidation signal mismatch: {err}; expected {:?}, actual {:?}; rate {}, main {:?}, vol {}", (expected.len(), expected.iter().position(|x|x.abs()>0.001),expected.iter().copied().fold(0f32,|a,b|a.max(b.abs()))),(actual.len(),actual.iter().position(|x|x.abs()>0.001),actual.iter().copied().fold(0f32,|a,b|a.max(b.abs()))),wave.rate,original.main_volume,original.channels[0].volume);
        app.undo_key(false);
        assert_eq!(app.state.project.channels,original.channels);
        assert_eq!(app.state.project.patterns,original.patterns);
        assert_eq!(app.state.project.arrangements,original.arrangements);
        }
    }
}
