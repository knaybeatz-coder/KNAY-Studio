// SPDX-License-Identifier: GPL-3.0-or-later
//! Bundled oXygen-derived mastering engine. A normal master effect participates in
//! playback, delay compensation, project saving and normal exports alike.
use crate::{app::{App, State}, mixer::{self, FxReq}, plugins::Available};
use os_host::{Descriptor, Format};
use std::path::PathBuf;

pub const AVAILABLE: bool = true;
pub const NAME: &str = "Lemon Master Assistant";
pub const FILE: &str = "Lemon Master Assistant.vst3";

#[path = "master_engine_bundle.rs"]
mod embedded;

pub fn build_flavour() -> &'static str {
    if embedded::is_bundled() { "public-master-assistant-bundled" }
    else { "LEMON_UNBUNDLED_MASTER_ASSISTANT_BUILD" }
}

pub fn bundle_path() -> Option<PathBuf> {
    if embedded::is_bundled() {
        return embedded::bundle_path();
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let local = dir.join("Builtin Plugins").join(FILE);
    if local.exists() { return Some(local); }
    // The private Development launcher uses its own executable beside the main app.
    if exe.file_stem()?.to_string_lossy().eq_ignore_ascii_case("lemonstudio-development") {
        let shared = dir.parent()?.join("Builtin Plugins").join(FILE);
        if shared.exists() { return Some(shared); }
    }
    None
}

pub fn is_bundled(d: &Descriptor) -> bool {
    d.format == Format::Vst3 && d.name == NAME && bundle_path().is_some_and(|p| d.path.starts_with(p))
}

pub fn available() -> Vec<Available> {
    let Some(path) = bundle_path() else { return Vec::new() };
    match crate::plugins::load_file(&path, Format::Vst3) {
        Ok(lib) => lib.plugins().iter().filter(|d| d.name == NAME)
            .map(|desc| Available { lib: lib.clone(), desc: desc.clone() }).collect(),
        Err(e) => { eprintln!("Master Assistant: {e}"); Vec::new() }
    }
}

pub fn slot(s: &State) -> Option<u8> {
    mixer::track(s, 0)?.slots.iter().enumerate().find_map(|(i, p)| {
        let p = p.as_ref()?;
        let ours = p.name == NAME || crate::project_plugins::wrapped_slot(p)
            .is_some_and(|r| r.name == NAME && r.vendor == "Lemon Studio");
        (i != 10 && ours).then_some(i as u8)
    })
}

pub fn enabled(s: &State) -> bool {
    slot(s).zip(mixer::track(s, 0)).is_some_and(|(k, m)| !s.mixer.all_fx_bypassed && m.slots_enabled && m.slot_on[k as usize])
}

/// Never overwrite an existing effect. Append after the last user effect, keeping
/// the reserved EQ slot (10) untouched and the mastering chain last.
fn destination(s: &State) -> Option<u8> {
    let m = mixer::track(s, 0)?;
    // Keep a newly created master processor after user inserts, outside their visible list.
    if m.slots[63].is_none() { return Some(63); }
    let last = m.slots.iter().enumerate().filter(|(i, p)| *i != 10 && p.is_some()).map(|(i, _)| i).max();
    ((last.map_or(0, |i| i + 1))..m.slots.len()).find(|&i| i != 10 && m.slots[i].is_none()).map(|i| i as u8)
}

pub fn open(app: &mut App, toggle: bool) {
    if let Some(k) = slot(&app.state) {
        if toggle {
            let on = !enabled(&app.state);
            if on { app.state.mixer.all_fx_bypassed = false; }
            if let Some(m) = mixer::track_mut(&mut app.state, 0) {
                m.slot_on[k as usize] = on;
                if on { m.slots_enabled = true; }
            }
            app.state.piano.dirty = true;
            app.state.message = Some(if on { "Master Assistant enabled — included in normal song exports" } else { "Master Assistant bypassed — playback and export" }.into());
        } else {
            app.state.mixer.pending.push(FxReq::Show { track: 0, slot: k });
        }
        app.invalidate();
        return;
    }
    let Some(index) = app.plugins.effects.iter().position(|a| is_bundled(&a.desc)) else {
        app.state.message = Some("Master Assistant could not load. Check the application log and reinstall this release if needed.".into());
        app.invalidate(); return;
    };
    let Some(k) = destination(&app.state) else {
        app.state.message = Some("Make room after the last master effect to add Master Assistant. No existing effects were replaced.".into());
        app.invalidate(); return;
    };
    let desc = &app.plugins.effects[index].desc;
    let reference = crate::project_plugins::plugin_ref(desc, Vec::new());
    let m = mixer::track_mut(&mut app.state, 0).unwrap();
    m.slots[k as usize] = Some(os_model::SlotPlugin {
        name: os_flp::wrapper::WRAPPER.into(), state: os_flp::wrapper::encode(None, &reference),
    });
    m.slot_on[k as usize] = toggle; // Open alone never changes the sound.
    m.slot_mix[k as usize] = 1.0;
    if toggle { m.slots_enabled = true; app.state.mixer.all_fx_bypassed = false; }
    app.state.mixer.pending.push(FxReq::Create { track: 0, slot: k, index, state: None });
    app.state.piano.dirty = true;
    app.state.message = Some(if toggle { "Master Assistant enabled. Play your song and run Master Assist." } else { "Master Assistant is OFF. Enable it on the master strip before running the assistant." }.into());
    app.invalidate();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn master_assistant_slot_identity_and_safe_placement() {
        let mut app = App::new(None);
        app.state.project.mixer = vec![os_model::MixerTrack::new(0), os_model::MixerTrack::new(1)];
        app.state.project.arrangements = vec![os_flp::project::default_arrangement()];
        assert_eq!(destination(&app.state), Some(63));
        let m = mixer::track_mut(&mut app.state, 0).unwrap();
        m.slots[9] = Some(os_model::SlotPlugin {name:"Existing limiter".into(),state:vec![42]});
        m.slots[10] = Some(os_model::SlotPlugin {name:"EQ".into(),state:vec![7]});
        assert_eq!(destination(&app.state), Some(63));
        let r = os_model::PluginRef {format:"vst3".into(), name:NAME.into(), vendor:"Lemon Studio".into(), path:FILE.into(), ..Default::default()};
        mixer::track_mut(&mut app.state,0).unwrap().slots[11] = Some(os_model::SlotPlugin {name:os_flp::wrapper::WRAPPER.into(),state:os_flp::wrapper::encode(None,&r)});
        assert_eq!(slot(&app.state),Some(11));
        assert!((0..mixer::user_slot_capacity(&app.state,0)).all(|i|mixer::visible_user_slot(&app.state,0,i)!=11));
        assert_eq!(mixer::slot_count(&app.state,0),11);
        open(&mut app,true); assert!(!enabled(&app.state));
        open(&mut app,true); assert!(enabled(&app.state));
        let bytes=os_flp::project::create(&app.state.project).unwrap().to_bytes().unwrap();
        let loaded=os_flp::project::load(&os_flp::FlpFile::parse(&bytes).unwrap()).unwrap();
        app.state.project=loaded;
        assert_eq!(slot(&app.state),Some(11)); assert!(enabled(&app.state));
        assert_eq!(mixer::track(&app.state,0).unwrap().slots[9].as_ref().unwrap().state,vec![42]);
        open(&mut app,false);
        assert!(matches!(app.state.mixer.pending.last(),Some(FxReq::Show{track:0,slot:11})));
        app.state.project.mixer[0]=os_model::MixerTrack::new(0);
        let master=mixer::track_mut(&mut app.state,0).unwrap();
        master.slots[63]=Some(os_model::SlotPlugin{name:os_flp::wrapper::WRAPPER.into(),state:os_flp::wrapper::encode(None,&r)});
        master.slot_on[63]=false;
        assert_eq!(mixer::slot_count(&app.state,0),10);
        for i in 0..mixer::user_slot_capacity(&app.state,0) {
            let k=mixer::visible_user_slot(&app.state,0,i);
            assert_ne!(k,63);
            assert!(mixer::slot_name(&app.state,0,k).is_none());
        }
        open(&mut app,false);open(&mut app,false);
        assert!(!enabled(&app.state));
        assert!(matches!(app.state.mixer.pending.last(),Some(FxReq::Show{track:0,slot:63})));
    }
    // Requires the separately built bundle; renders synthetic input without an
    // editor, audio device or microphone, through the normal offline engine.
    #[test]
    #[ignore = "requires LEMON_MASTER_TEST_BUNDLE"]
    fn master_assistant_bundled_host_render_and_state() {
        use std::sync::{Arc, Mutex};
        struct Tone(bool);
        impl os_engine::Instrument for Tone {
            fn note_on(&mut self, _:u32, _:u8, _:f32) { self.0=true; }
            fn note_off(&mut self, _:u32, _:u8) { self.0=false; }
            fn process(&mut self,_:u32,l:&mut[f32],r:&mut[f32]) { l.fill(if self.0 {0.1}else{0.});r.copy_from_slice(l); }
            fn reset(&mut self) { self.0=false; }
        }
        let path=PathBuf::from(std::env::var_os("LEMON_MASTER_TEST_BUNDLE").expect("bundle path"));
        let lib=crate::plugins::load_file(&path,Format::Vst3).expect("load real bundle");
        let desc=lib.plugins().iter().find(|d|d.name==NAME).expect("bundled identity").clone();
        let instance=lib.clone().create(&desc.id).expect("instantiate");
        let normal=instance.lock().unwrap().save_state().expect("default state");
        let state={
            let mut p=instance.lock().unwrap();
            let gain=(0..p.param_count()).filter_map(|i|p.param_info(i)).find(|p|p.name=="Gain / Gain").expect("forwarded gain");
            assert!(p.set_param(gain.id,0.0));
            p.save_state().expect("save gain zero")
        };
        let mut project=crate::app::test_project();
        project.channels[0].kind=os_model::ChannelKind::Plugin;
        project.channels[0].mixer_track=0;
        project.patterns[0].notes=vec![os_model::Note::step(0,0)];
        project.mixer[0].slots_enabled=true;
        let render=|on:bool, saved:&[u8]| {
            let mut proj=project.clone();proj.mixer[0].slot_on[63]=on;
            let mut plugins=crate::plugins::Plugins::default();
            plugins.set_rate(48000);
            plugins.effects=vec![Available{lib:lib.clone(),desc:desc.clone()}];
            plugins.create_effect(0,63,0,Some(saved)).expect("restore real engine");
            let shared=plugins.effects().pop().unwrap().2;
            plugins.hosted.get(&crate::plugins::fx_key(0,63)).unwrap().plugin.lock().unwrap().set_offline(true);
            let mut engine=os_engine::Engine::new(&proj,&mut os_engine::SampleLibrary::new(None),48000,os_engine::PlayMode::Pattern(1));
            engine.set_instrument(0,Arc::new(Mutex::new(Tone(false))));
            engine.set_effect(0,63,shared);
            engine.render_all(512)
        };
        let dry=render(false,&state);let wet=render(true,&state);let normal=render(true,&normal);
        let peak=|a:&[f32]|a.iter().map(|x|x.abs()).fold(0f32,f32::max);
        assert!(peak(&normal)>0.01,"enabled default engine must pass non-silent audio");
        assert!(peak(&dry)>0.01,"bypassed export must contain input");
        assert!(peak(&wet)<0.00001,"enabled export must use restored zero-gain engine: {}",peak(&wet));
        println!("Bundled VST3 saved settings, bypass and enabled offline master render passed");
    }

}
