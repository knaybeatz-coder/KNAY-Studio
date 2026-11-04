// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! Mixer behaviour: track selection, mute / solo, routing and send levels, the track / options / layout menus,
//! adding / moving / deleting tracks, and the effect slots that host plugins.
//!
// spec: ui/mixer.md (every interaction), flp/events/mixer.md (what is stored), audio/gain-laws.md (fader law)

use std::collections::HashMap;

use os_model::{MixerTrack, Rgb};

use crate::app::{App, State, Action, Win};
use crate::knobs::KnobId;
use crate::playlist::{Cmd, EditTarget, Mark, PItem, Popup};

/// Mixer controls (the view registers them as `Action::Mix`). Controls that are not available yet (title-bar view
/// buttons, input pin, latency clock, disk recording, input / output combos, track equalizer) are greyed out with
/// `Ui::off` instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MHit {
    Input(u16),
    Disk(u16),
    /// Anywhere on a strip that is not a control.
    Strip(u16),
    Led(u16),
    Polarity(u16),
    Swap(u16),
    Effects(u16),
    StripBypass(u16),
    AllEffects,
    Headphones,
    HeadphonesToggle,
    MasterAssistant,
    MasterAssistantToggle,
    Jump(u16, Section),
    Meter(u16),
    CompProfile(u16),
    TuneLearn(u16),
    TunePower(u16),
    DriveType(u16),
    /// The ▲ "route to this track" button.
    Route(u16),
    /// The small green "^" above a send knob (removes that route).
    Unroute(u16),
    /// "+" after the last strip; title-bar "+" (after the selected track).
    Add,
    AddAfter,
    InsertBefore(u16),
    Options,
    Layout,
    View(u8),
    /// Effect slot `n` of the current track: name, ▸ arrow, LED.
    SlotName(u8),
    AddSlot,
    SlotScroll,
    SlotArrow(u8),
    SlotLed(u8),
    /// The level scale column.
    Scale,
    /// The current track's equaliser graph: a press grabs the band nearest the cursor's frequency.
    EqGraph,
    OpenEq,
    EqPosition,
}

/// Navigation only: destinations are resolved afresh so moved tracks keep working.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section { Drums, Instruments, Vocals, Effects, Buses }
impl Section {
    pub const ALL: [Self; 5] = [Self::Drums, Self::Instruments, Self::Vocals, Self::Effects, Self::Buses];
    pub fn label(self) -> &'static str {
        match self { Self::Drums=>"DRUM", Self::Instruments=>"INST", Self::Vocals=>"VOX", Self::Effects=>"FX", Self::Buses=>"BUS" }
    }
    pub fn target(self, s: &State) -> Option<u16> {
        let names: Vec<_> = s.project.mixer.iter().filter(|m| m.index > 0)
            .map(|m| (m.index, m.name.as_deref().unwrap_or("").trim().to_ascii_lowercase())).collect();
        let exact = match self {
            Self::Drums=>&["drums", "drum section"][..],
            Self::Instruments=>&["instruments", "instrument section"][..],
            Self::Vocals=>&["vocals", "vocal section"][..],
            Self::Effects=>&["verb long", "long reverb", "fx sends", "effects sends"][..],
            Self::Buses=>&["mix bus", "master bus", "buses", "busses"][..],
        };
        for name in exact {
            if let Some((n,_))=names.iter().find(|(_,v)|v==name) {return Some(*n);}
        }
        names.iter().filter(|(_,v)| {
            let bus=v.split_whitespace().any(|w| matches!(w,"mix"|"bus"|"buss"|"buses"|"busses"));
            if self==Self::Buses {return bus;}
            if bus {return false;}
            let words:Vec<_>=v.split(|c:char| !c.is_alphanumeric()).collect();
            match self {
                Self::Drums=>words.iter().any(|w| matches!(*w,"kick"|"snare"|"drum"|"drums"|"percussion"|"hh"|"hats"|"clap")),
                Self::Instruments=>words.iter().any(|w| matches!(*w,"synth"|"synths"|"instrument"|"instruments"|"piano"|"keys"|"guitar"|"git"|"pad"|"arp")),
                Self::Vocals=>!words.iter().any(|w|matches!(*w,"verb"|"reverb"|"delay")) && words.iter().any(|w| matches!(*w,"vox"|"vocal"|"vocals"|"backing")),
                Self::Effects=>words.iter().any(|w| matches!(*w,"verb"|"reverb"|"delay"|"slap")),
                Self::Buses=>false,
            }
        }).map(|(n,_)|*n).min()
    }
}

/// Menu commands (`Cmd::Mixer`).
#[derive(Debug, Clone, PartialEq)]
pub enum MCmd {
    CompProfile(u16,u8),
    TuneSetting(u16,u8,u16),
    ChannelLayout(u16,u8),
    Detach,
    DriveType(u16,u8),
    RecordInput(u16,Option<crate::capture::Input>),
    ArmDisk(u16),
    /// KNAY: the input menu's Record point (0 external input only .. 5 post track) and Monitor (0 off, 1 when armed, 2 on).
    RecPoint(u16,u8),
    Monitor(u16,u8),
    Rename(u16),
    Color(u16, Option<Rgb>),
    RandomColor(u16),
    Reset(u16),
    ResetRouting(u16),
    UnrouteTo(u16),
    Solo(u16),
    /// Solo with the tracks routed into it too (spec: ui/mixer-track-commands.md §5).
    AltSolo(u16),
    /// The rack's selected channels to `dst`, or (`spread`) to `dst`, `dst` + 1, … (§2).
    RouteChannels { dst: u16, spread: bool },
    /// Select ▸ (§3): the channels using the tracks, the tracks they send to, the tracks sending to them, all.
    SelectChannels(u16),
    SelectDest(u16),
    SelectSources(u16),
    SelectAll,
    /// Trim unused tracks (§4): delete the inserts no channel uses (`non_empty`: also those holding effects).
    Trim { non_empty: bool },
    MoveLeft(u16),
    MoveRight(u16),
    Delete(u16),
    InsertOne(u16),
    /// Route the selected tracks to `dst`: only = drop their other routes, side = sidechain (level 0).
    Route { dst: u16, only: bool, side: bool },
    QuickSidechain(u16),
    /// Compact or full mixer view.
    Layout(u8),
    InspectorLeft,
    Separator(u16),
    /// Options ▸ Cable physics: open / close the cables' debug panel (Lemon Studio's own).
    CablePhysics,
    /// General settings ▸ Animations: simulate the cables (true) or hang them still; kept for the next start.
    Animations(bool),
    /// Open audio editor: Tesla on this track (Tesla's own, as FL's Edison).
    AudioEditor(u16),
    /// Open audio editor and record: a Tesla on this track set up as an audio logger.
    AudioEditorRecord(u16),
    /// Remove all (audio editor) instances: asks first, then `RemoveTeslasNow` removes every Tesla.
    RemoveTeslas,
    RemoveTeslasNow,
    /// Effect slots of the current track.
    AddEffect(u8, usize),
    SlotDelete(u8),
    SlotMove(u8, i8),
    SlotRename(u8),
    SlotView(u8),
    /// Sidechain input ▸ (Lemon Studio's own): the input of the current track a hosted plugin's sidechain bus hears,
    /// counted from 1, 0 for none (spec: audio/sidechain.md "Hosted plugins' input buses").
    SlotSidechain(u8, usize),
}

/// What the plugin host does next tick for a mixer slot (`App::mixer_tick`).
#[derive(Debug, Clone, PartialEq)]
pub enum FxReq {
    /// `state`: the instance starts from it (a Tesla set up for recording), else from its defaults.
    Create { track: u16, slot: u8, index: usize, state: Option<Vec<u8>> },
    Toggle { track: u16, slot: u8 },
    Show { track: u16, slot: u8 },
    Delete { track: u16, slot: u8 },
    /// Slots moved inside a track or tracks moved: `map[(t, s)] = (t2, s2)`; unlisted instances are dropped.
    Remap(Vec<((u16, u8), (u16, u8))>),
}

/// Full view keeps the existing wide layout identifier.
pub const WIDE: u8 = 3;
pub const COMPACT: u8 = 0;
pub fn set_layout(s: &mut State, layout: u8) {
    let old=crate::views::mixer::strip_width(s);
    s.mixer.layout=if layout==COMPACT {COMPACT} else {WIDE};
    s.mixer_scroll*=crate::views::mixer::strip_width(s)/old;
}

/// Mixer state that is not project data.
#[derive(Debug,Clone,PartialEq)]
pub struct ChannelCopy {pub track:MixerTrack,pub names:HashMap<u8,String>}

pub struct MixerUi {
    pub mini_eq: crate::mini_eq::View,
    pub slot_scroll: HashMap<u16,usize>,
    pub slots_shown: HashMap<u16,usize>,
    pub clipboard: Option<ChannelCopy>,
    pub recording: crate::mixer_recording::Setup,
    /// Session-wide audition bypass; individual track/slot choices remain untouched.
    pub all_fx_bypassed: bool,
    pub peak_view: std::collections::BTreeSet<u16>,
    /// Selected tracks besides the current one (Ctrl+click, §2).
    pub sel: Vec<u16>,
    pub layout: u8,
    /// Slot names typed in the rename box (session only).
    pub slot_names: HashMap<(u16, u8), String>,
    pub pending: Vec<FxReq>,
    /// Meter state per track (`meters::Meter`), fed by `feed_meters`.
    pub meters: Vec<crate::meters::Meter>,
    /// Meter ballistics and look.
    pub meter_cfg: crate::meters::Settings,
    /// When the meters last advanced.
    pub meter_at: Option<std::time::Instant>,
    /// The last solo: the tracks it kept on and the states (track, enabled) from before the first solo, put back by
    /// unsolo when "Restore previous state after solo" is on.
    pub solo: Option<(Vec<u16>, Vec<(u16, bool)>)>,
    /// The track and effect slot the wheel last moved, and where the mouse was then (`slot_wheel`).
    pub wheel_grab: Option<(u16, u8, (f32, f32))>,
}

impl Default for MixerUi {
    fn default() -> Self {
        MixerUi { slot_scroll: HashMap::new(), slots_shown: HashMap::new(), mini_eq: Default::default(), clipboard:None, recording: Default::default(), all_fx_bypassed: false, peak_view: Default::default(), sel: Vec::new(), layout: WIDE, slot_names: HashMap::new(), pending: Vec::new(), meters: Vec::new(),
            meter_cfg: Default::default(), meter_at: None, solo: None, wheel_grab: None }
    }
}

/// Highest insert a channel can target (at least 1).
pub fn max_insert(p: &os_model::Project) -> u16 {
    max_insert_of(&p.mixer)
}

pub fn max_insert_of(mixer: &[MixerTrack]) -> u16 {
    (mixer.len().saturating_sub(2) as u16).max(1)
}

/// Inserts shown: stored tracks minus the master and the last (hidden) one (§1).
pub fn inserts(s: &State) -> u16 {
    s.project.mixer.len().saturating_sub(2) as u16
}

pub fn track(s: &State, n: u16) -> Option<&MixerTrack> {
    s.project.mixer.iter().find(|m| m.index == n)
}

pub(crate) fn track_mut(s: &mut State, n: u16) -> Option<&mut MixerTrack> {
    s.project.mixer.iter_mut().find(|m| m.index == n)
}

/// "Master", "Insert 3", or the track's own name.
pub fn name(s: &State, n: u16) -> String {
    track(s, n).and_then(|m| m.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| if n == 0 { "Master".into() } else { format!("Insert {n}") })
}

/// The current track first, then the other selected ones.
pub fn selected(s: &State) -> Vec<u16> {
    let mut v = vec![s.selected_mixer];
    v.extend(s.mixer.sel.iter().copied().filter(|&t| t != s.selected_mixer));
    v
}

pub fn is_selected(s: &State, n: u16) -> bool {
    s.selected_mixer == n || s.mixer.sel.contains(&n)
}

/// Whether track `src` sends to `dst` (route present), and at what level (fader scale, 0.8 = unity).
pub fn send(s: &State, src: u16, dst: u16) -> Option<f64> {
    let t = track(s, src)?;
    t.routes.contains(&dst).then(|| t.send_levels.iter().find(|(d, _)| *d == dst).map_or(0.8, |&(_, l)| l))
}

/// Whether any track routes into `n`.
pub fn receives(s: &State, n: u16) -> bool {
    s.project.mixer.iter().any(|m| m.index != n && m.routes.contains(&n))
}

/// The slot's shown name (renamed, else the plugin's name).
pub fn slot_name(s: &State, t: u16, slot: u8) -> Option<String> {
    let p = track(s, t)?.slots.get(usize::from(slot))?.as_ref()?;
    Some(s.mixer.slot_names.get(&(t, slot)).cloned().unwrap_or_else(|| crate::project_plugins::slot_title(p)))
}

/// Tesla's "For" index of 5 minutes (Edison's menu: Forever, 1, 3, 5 minutes, ...), the audio logger's length.
const AUDIO_LOG_5_MINUTES: u8 = 3;

/// Whether slot `k` of track `t` holds Tesla (a project's Edison slots too; a name typed for the slot doesn't count).
fn is_tesla(s: &State, t: u16, k: u8) -> bool {
    track(s, t).and_then(|m| m.slots.get(usize::from(k))?.as_ref()).is_some_and(|p| crate::project_plugins::slot_title(p) == crate::plugins::TESLA)
}

/// Track `t`'s first Tesla slot.
fn first_tesla(s: &State, t: u16) -> Option<u8> {
    let n = track(s, t)?.slots.len() as u8;
    (0..n).find(|&k| is_tesla(s, t, k))
}

/// Every Tesla in the mixer, as (track, slot).
pub(crate) fn teslas(s: &State) -> Vec<(u16, u8)> {
    s.project.mixer.iter().flat_map(|m| (0..m.slots.len() as u8).map(move |k| (m.index, k))).filter(|&(t, k)| is_tesla(s, t, k)).collect()
}

/// The question before Remove all (audio editor) instances.
fn remove_question(n: usize) -> String {
    match n {
        1 => "Remove 1 audio editor? Its audio is lost.".into(),
        n => format!("Remove {n} audio editors? Their audio is lost."),
    }
}

/// Empties slot `k` of track `t` and drops its instance (the slot menu's Delete).
fn clear_slot(s: &mut State, t: u16, k: u8) {
    crate::peak_link::remove_slot(s,t,k);
    if let Some(m) = track_mut(s, t) {
        m.slots[usize::from(k)] = None;
        m.slot_on[usize::from(k)] = true;
        m.slot_mix[usize::from(k)] = 1.0;
    }
    s.mixer.slot_names.remove(&(t, k));
    s.mixer.pending.push(FxReq::Delete { track: t, slot: k });
}

/// "Volume: 100%  0.0dB  1.00" (§4); `label` = "Volume" or "To Insert 3".
pub fn level_text(label: &str, v: f64) -> String {
    let g = os_engine::laws::fader_gain(v);
    let db = if g > 0.0 { format!("{:.1}dB", 20.0 * g.log10()) } else { "-INFdB".into() };
    format!("{label}: {:.0}%  {db}  {g:.2}", v / 0.8 * 100.0)
}

/// "Stereo separation: Original" / "40% separated" / "40% merged" (§4). A positive stored value merges towards mono
/// (spec: audio/mixer.md "Fader matrix"); which way FL's knob turns for it is not checked.
pub fn sep_text(sep: f64) -> String {
    let pct = (sep.abs() * 100.0).round();
    if pct == 0.0 {
        "Stereo separation: Original".into()
    } else if sep > 0.0 {
        format!("Stereo separation: {pct:.0}% merged")
    } else {
        format!("Stereo separation: {pct:.0}% separated")
    }
}

/// Would a route `src` -> `dst` make a loop (dst already reaches src)?
pub(crate) fn makes_loop(s: &State, src: u16, dst: u16) -> bool {
    let mut seen = vec![dst];
    let mut i = 0;
    while i < seen.len() {
        let t = seen[i];
        if t == src {
            return true;
        }
        if let Some(m) = track(s, t) {
            for &r in &m.routes {
                if !seen.contains(&r) {
                    seen.push(r);
                }
            }
        }
        i += 1;
    }
    false
}

fn changed(s: &mut State) {
    s.piano.dirty = true;
}

/// Selects `n` as a click with these modifiers does (§2): plain = only it, Ctrl = add it.
pub fn select(s: &mut State, n: u16, ctrl: bool) {
    if ctrl {
        let cur = s.selected_mixer;
        if !s.mixer.sel.contains(&cur) {
            s.mixer.sel.push(cur);
        }
        if !s.mixer.sel.contains(&n) {
            s.mixer.sel.push(n);
        }
    } else {
        s.mixer.sel.clear();
    }
    s.selected_mixer = n;
}

pub(crate) fn fader_group(s:&State,t:u16)->Vec<(u16,f64)> {
    let group=if selected(s).contains(&t){selected(s)}else{vec![t]};
    group.into_iter().filter_map(|n|track(s,n).map(|m|(n,m.volume.unwrap_or(0.8)))).collect()
}
pub(crate) fn group_delta(group:&[(u16,f64)],delta:f64)->f64 {
    let lo=group.iter().map(|(_,v)|-*v).fold(-1.0,f64::max);
    let hi=group.iter().map(|(_,v)|1.0-*v).fold(1.0,f64::min);
    delta.clamp(lo,hi)
}
pub(crate) fn snapshot_faders(s:&mut State,group:&[(u16,f64)]) {
    let snaps=group.iter().filter_map(|(n,_)|track(s,*n).map(|m|crate::history::Snap::Fader(*n,m.volume))).collect();
    crate::history::record(s,"Mixer group volume",crate::history::Snap::Several(snaps));
}

/// Left click (or press) on a mixer control.
pub fn hit(app: &mut App, h: MHit) -> Option<String> {
    if h==MHit::MasterAssistant {crate::master_assistant::open(app,false);return None;}
    if h==MHit::MasterAssistantToggle {crate::master_assistant::open(app,true);return None;}
    if h==MHit::Headphones {crate::headphones::open(app);return None;}
    if h==MHit::HeadphonesToggle {crate::headphones::toggle(app);return None;}
    if let MHit::Jump(_, section) = h {
        if let Some(n) = section.target(&app.state) {
            app.scroll_set(crate::app::Scroll::MixerX, f32::from(n.saturating_sub(1)) * crate::views::mixer::strip_width(&app.state));
            app.invalidate();
        }
        return None;
    }
    if let MHit::Disk(t)=h{return app.arm_mixer_disk(t);}
    let (ctrl, shift) = (app.ctrl, app.shift);
    let at = app.mouse;
    let s = &mut app.state;
    match h {
        MHit::View(mode)=>{set_layout(s,mode);None}
        MHit::Input(t)=>{s.playlist.popup=Some(crate::mixer_recording::input_menu(s,t,at.0,at.1+10.));None}
        MHit::Disk(_) | MHit::Headphones | MHit::HeadphonesToggle | MHit::MasterAssistant | MHit::MasterAssistantToggle=>None,
        MHit::TunePower(n)=>{
            let m=track(s,n)?;if !m.is_vocal(){return None;}let old=m.tune_enabled;
            crate::history::record(s,"Tune on/off",crate::history::Snap::TuneEnabled(n,old));
            track_mut(s,n)?.tune_enabled=!old;changed(s);None
        },
        MHit::TuneLearn(n)=>{
            static REQUEST:std::sync::atomic::AtomicU32=std::sync::atomic::AtomicU32::new(1);
            let m=track_mut(s,n)?;
            if m.is_vocal(){m.tune_learn=if m.tune_learn!=0{0}else{REQUEST.fetch_add(1,std::sync::atomic::Ordering::Relaxed).max(1)};changed(s);}
            None
        },
        MHit::Jump(..)=>None,
        MHit::Strip(n) => {
            select(s, n, ctrl);
            if shift {
                start_rename(s, n, at);
            }
            None
        }
        MHit::Led(n) => {
            select(s, n, ctrl);
            let m = track_mut(s, n)?;
            m.enabled = !m.enabled;
            changed(s);
            None
        }
        MHit::Polarity(n) => {
            select(s, n, ctrl);
            let m = track_mut(s, n)?;
            m.polarity = !m.polarity;
            changed(s);
            None
        }
        MHit::Swap(n) => {
            select(s, n, ctrl);
            let m = track_mut(s, n)?;
            m.swap = !m.swap;
            changed(s);
            None
        }
        MHit::Route(dst) => route(s, dst, ctrl, shift),
        MHit::DriveType(n) => {
            let now=track(s,n)?.strip[6];
            let items=os_dsp::channel_strip::DRIVE_TYPES.iter().enumerate().map(|(i,p)|PItem::new(*p,Cmd::Mixer(MCmd::DriveType(n,i as u8))).mark(Mark::Radio(now==i as u16))).collect();
            s.playlist.popup=Some(Popup{x:at.0,y:at.1,items,sub:None,scroll:[0;2], cascade: Vec::new()});
            None
        }
        MHit::CompProfile(n) => {
            let now=track(s,n)?.strip[5];
            let items=os_dsp::channel_strip::PROFILES.iter().enumerate().map(|(i,p)|PItem::new(p.name,Cmd::Mixer(MCmd::CompProfile(n,i as u8))).mark(Mark::Radio(now==i as u16))).collect();
            s.playlist.popup=Some(Popup {x:at.0,y:at.1,items,sub:None,scroll:[0;2], cascade: Vec::new()});
            None
        }
        MHit::Meter(n) => {
            if !s.mixer.peak_view.remove(&n) { s.mixer.peak_view.insert(n); }
            None
        }
        MHit::AllEffects => {
            if s.project.mixer.iter().all(|m| !m.slots_enabled) {
                let snaps=s.project.mixer.iter().map(|m|crate::history::Snap::Effects(m.index,m.slots_enabled,s.mixer.all_fx_bypassed)).collect();
                crate::history::record(s,"Enable all mixer effects",crate::history::Snap::Several(snaps));
                for m in &mut s.project.mixer {m.slots_enabled=true;}
                s.mixer.all_fx_bypassed=false;
            } else {
                crate::history::record(s, "All mixer effects bypass", crate::history::Snap::AllEffects(s.mixer.all_fx_bypassed));
                s.mixer.all_fx_bypassed = !s.mixer.all_fx_bypassed;
            }
            changed(s);
            None
        }
        MHit::StripBypass(n) => {
            let old=track(s,n)?.strip_bypassed;
            crate::history::record(s,"Mixer knobs bypass",crate::history::Snap::StripBypass(n,old));
            track_mut(s,n)?.strip_bypassed=!old;
            changed(s);
            None
        }
        MHit::Effects(n) => {
            select(s, n, ctrl);
            let enabled = track(s, n)?.slots_enabled;
            crate::history::record(s, "Mixer effects bypass", crate::history::Snap::Effects(n, enabled, s.mixer.all_fx_bypassed));
            let was_global = s.mixer.all_fx_bypassed;
            s.mixer.all_fx_bypassed = false;
            let m = track_mut(s, n)?;
            m.slots_enabled = was_global || !m.slots_enabled;
            changed(s);
            None
        }
        MHit::Unroute(dst) => {
            let cur = s.selected_mixer;
            let m = track_mut(s, cur)?;
            m.routes.retain(|&r| r != dst);
            m.send_levels.retain(|&(d, _)| d != dst);
            changed(s);
            Some(format!("{} no longer sends to {}", name(s, cur), name(s, dst)))
        }
        MHit::Add => {
            let n = add_track(s, None);
            Some(format!("Added {}", name(s, n)))
        }
        MHit::InsertBefore(n) => {
            if s.rec.audio {return Some("Stop the take before inserting mixer tracks".into());}
            let n=add_track(s,Some(n));
            Some(format!("Added {}",name(s,n)))
        }
        MHit::AddAfter => {
            let n = add_track(s, Some(s.selected_mixer + 1));
            Some(format!("Added {}", name(s, n)))
        }
        MHit::Options => {
            s.playlist.popup = Some(options_menu(s, at.0, at.1 + 10.0));
            None
        }
        MHit::Layout => {
            s.playlist.popup = Some(layout_menu(s, at.0, at.1 + 10.0));
            None
        }
        MHit::SlotScroll => None,
        MHit::AddSlot => { let n=s.selected_mixer;let count=slot_count(s,n).saturating_add(1).min(63);s.mixer.slots_shown.insert(n,count);s.mixer.slot_scroll.insert(n,count.saturating_sub(10)); None },
        MHit::SlotName(i) => {
            let t = s.selected_mixer;
            if slot_name(s, t, i).is_some() {
                s.mixer.pending.push(FxReq::Toggle { track: t, slot: i });
            } else {
                s.playlist.popup = Some(effect_menu(s, i, at.0, at.1));
            }
            None
        }
        MHit::SlotArrow(i) => {
            let t = s.selected_mixer;
            s.playlist.popup = Some(if slot_name(s, t, i).is_some() { slot_menu(s, i, at.0, at.1) } else { effect_menu(s, i, at.0, at.1) });
            None
        }
        MHit::SlotLed(i) => {
            let t = s.selected_mixer;
            let m = track_mut(s, t)?;
            let k = usize::from(i);
            m.slot_on[k] = !m.slot_on[k];
            changed(s);
            None
        }
        MHit::Scale | MHit::EqGraph | MHit::OpenEq | MHit::EqPosition => None,
    }
}

// The equaliser graph `g` spans the frequency knob's range left to right (10 Hz .. 16 kHz) and ±18 dB over its
// half-height less 4 px (spec: ui/mixer.md §8).

/// The frequency position 0..1 at `x`.
pub fn eq_graph_pos(g: os_ui::Rect, x: f32) -> f32 {
    ((x - g.x - 1.0) / (g.w - 2.0)).clamp(0.0, 1.0)
}

/// The gain at `y`, in dB.
pub fn eq_graph_db(g: os_ui::Rect, y: f32) -> f32 {
    ((g.cy() - y) / (g.h / 2.0 - 4.0) * 18.0).clamp(-18.0, 18.0)
}

/// The band of the current track a press at `x` tweaks: the one whose frequency is nearest.
pub fn eq_band_at(s: &State, g: os_ui::Rect, x: f32) -> u8 {
    let p = eq_graph_pos(g, x);
    let settings = crate::mini_eq::settings(s, s.selected_mixer);
    (0..7).min_by(|&a, &b| (settings.bands[a].freq-p as f64).abs().total_cmp(&(settings.bands[b].freq-p as f64).abs())).unwrap_or(0) as u8
}

pub fn eq_graph_hint(s: &State, g: os_ui::Rect, x: f32) -> String {
    let hz = os_engine::effects::eq::freq_hz(eq_graph_pos(g,x) as f64);
    format!("{hz:.0}Hz  Tweak band {}",eq_band_at(s,g,x)+1)
}


/// Right click on a mixer control. Returns false when it is not handled here (knob menus).
pub fn right(app: &mut App, h: MHit) -> bool {
    let at = app.mouse;
    let s = &mut app.state;
    match h {
        MHit::TuneLearn(n)=>s.playlist.popup=Some(tune_menu(s,n,at.0,at.1,false)),
        MHit::Input(t)|MHit::Disk(t)=>s.playlist.popup=Some(crate::mixer_recording::input_menu(s,t,at.0,at.1+10.)),
        MHit::Strip(n) => {
            if !is_selected(s, n) {
                select(s, n, false);
            }
            s.playlist.popup = Some(track_menu(s, n, at.0, at.1));
        }
        MHit::Add => s.playlist.popup = Some(track_menu(s, s.selected_mixer, at.0, at.1)),
        MHit::Led(n) => solo(s, n),
        MHit::Route(dst) => {
            let items = vec![
                PItem::new("Route to this track", Cmd::Mixer(MCmd::Route { dst, only: false, side: false })),
                PItem::new("Route to this track only", Cmd::Mixer(MCmd::Route { dst, only: true, side: false })),
                PItem::sep(),
                PItem::new("Quick Sidechain", Cmd::Mixer(MCmd::QuickSidechain(dst))),
                PItem::new("Sidechain to this track", Cmd::Mixer(MCmd::Route { dst, only: false, side: true })),
                PItem::new("Sidechain to this track only", Cmd::Mixer(MCmd::Route { dst, only: true, side: true })),
            ];
            s.playlist.popup = Some(Popup { x: at.0, y: at.1, items, sub: None, scroll: [0; 2], cascade: Vec::new() });
        }
        // The slot's plugin menu, like a left click (spec: ui/plugin-menu.md §1).
        MHit::SlotArrow(i) => {
            let t = s.selected_mixer;
            s.playlist.popup = Some(if slot_name(s, t, i).is_some() { slot_menu(s, i, at.0, at.1) } else { effect_menu(s, i, at.0, at.1) });
        }
        MHit::SlotScroll => {},
        MHit::AddSlot => { let n=s.selected_mixer;let count=slot_count(s,n).saturating_add(1).min(63);s.mixer.slots_shown.insert(n,count);s.mixer.slot_scroll.insert(n,count.saturating_sub(10)); },
        MHit::SlotName(i) => {
            let t = s.selected_mixer;
            if slot_name(s, t, i).is_some() {
                s.mixer.pending.push(FxReq::Show { track: t, slot: i });
            } else {
                s.playlist.popup = Some(effect_menu(s, i, at.0, at.1));
            }
        }
        MHit::SlotLed(i) => {
            // Solo the slot: the track's other slots are bypassed (§7).
            let t = s.selected_mixer;
            if let Some(m) = track_mut(s, t) {
                let only = (0..64).filter(|k|*k!=10).all(|k| m.slot_on[k] == (k == usize::from(i)));
                for k in (0..64).filter(|k|*k!=10) {
                    m.slot_on[k] = only || k == usize::from(i);
                }
                changed(s);
            }
        }
        MHit::Polarity(n) | MHit::Swap(n) => select(s, n, false),
        _ => return false,
    }
    true
}

/// Middle click: rename a filled slot (§7).
pub fn middle(app: &mut App, h: MHit) -> bool {
    if let MHit::SlotName(i) = h {
        let t = app.state.selected_mixer;
        if slot_name(&app.state, t, i).is_some() {
            let at = app.mouse;
            crate::playlist::start_edit(&mut app.state, EditTarget::SlotName(t, i), at);
            return true;
        }
    }
    false
}

/// The wheel over effect slot `slot` of the current track (§7): what the slot holds, an effect or nothing, trades places
/// with its neighbour, one slot per notch, up for `notches` > 0. While the mouse stays still, more notches move the
/// same slot, which has left the cursor.
pub fn slot_wheel(app: &mut App, slot: u8, notches: i32) {
    let at = app.mouse;
    let t = app.state.selected_mixer;
    let mut slot = match app.state.mixer.wheel_grab {
        Some((gt, g, m)) if gt == t && m == at => g,
        _ => slot,
    };
    let d: i8 = if notches > 0 { -1 } else { 1 };
    for _ in 0..notches.unsigned_abs() {
        let to = user_slot_index(slot) as i32 + i32::from(d);
        if !(0..63).contains(&to) {
            break;
        }
        run(app, MCmd::SlotMove(slot, d));
        slot = user_slot(to as usize);
    }
    app.state.mixer.wheel_grab = Some((t, slot, at));
}

fn start_rename(s: &mut State, n: u16, at: (f32, f32)) {
    crate::playlist::start_edit(s, EditTarget::MixerName(n), at);
}

/// Right click on a LED (§3): solo, or back to all on when it already is the only one audible.
pub fn solo(s: &mut State, n: u16) {
    solo_with(s, n, false);
}

/// Solo `n` with the tracks it sends to (down to the master) and, for `alt`, the tracks sending into it; back to
/// all on when exactly that set is already the audible one (spec: ui/mixer-track-commands.md §5), or, with "Restore
/// previous state after solo" on, to the states from before the first solo (a solo moved to another track keeps them;
/// spec: ui/settings-choices.md "Miscellaneous").
pub fn solo_with(s: &mut State, n: u16, alt: bool) {
    let mut keep = reach(s, n, true);
    if alt {
        keep.extend(reach(s, n, false));
    }
    keep.push(0);
    let soloed = audible_set(s, &keep);
    // A solo still in effect (the last solo's set, untouched since) keeps the states saved when it started.
    let in_solo = s.mixer.solo.as_ref().is_some_and(|(k, _)| audible_set(s, k));
    let before = match s.mixer.solo.take() {
        Some((_, b)) if in_solo => b,
        _ => s.project.mixer.iter().map(|m| (m.index, m.enabled)).collect(),
    };
    let back = soloed && in_solo && crate::prefs::check(&crate::prefs::RESTORE_AFTER_SOLO);
    for m in s.project.mixer.iter_mut() {
        m.enabled = if back {
            before.iter().find(|b| b.0 == m.index).is_none_or(|b| b.1)
        } else {
            soloed || keep.contains(&m.index)
        };
    }
    if !soloed {
        s.mixer.solo = Some((keep, before));
    }
    changed(s);
}

/// The tracks on besides the master are exactly `keep`.
fn audible_set(s: &State, keep: &[u16]) -> bool {
    s.project.mixer.iter().all(|m| m.index == 0 || keep.contains(&m.index) == m.enabled)
}

/// `n` and every track it reaches through sends (`down`), or every track reaching it (`!down`).
fn reach(s: &State, n: u16, down: bool) -> Vec<u16> {
    let mut seen = vec![n];
    let mut i = 0;
    while i < seen.len() {
        let t = seen[i];
        let next: Vec<u16> = if down {
            track(s, t).map(|m| m.routes.clone()).unwrap_or_default()
        } else {
            s.project.mixer.iter().filter(|m| m.routes.contains(&t)).map(|m| m.index).collect()
        };
        for r in next {
            if !seen.contains(&r) {
                seen.push(r);
            }
        }
        i += 1;
    }
    seen
}

/// Names track `t` after channel row `r` and gives it the channel's colour (spec: ui/mixer-track-commands.md §1,
/// §2). The master keeps its name.
fn name_after(s: &mut State, t: u16, r: usize) {
    if t == 0 {
        return;
    }
    let nm = crate::rack::channel_name(s, r);
    let col = s.project.channels.get(r).and_then(|c| c.color);
    if let Some(m) = track_mut(s, t) {
        m.name = Some(nm);
        if col.is_some() {
            m.color = col;
        }
    }
}

/// The rack's selected channels (the piano roll's channel when none is selected), in rack order.
fn selected_channels(s: &State) -> Vec<usize> {
    let n = s.project.channels.len();
    let rows: Vec<usize> = s.rack.sel.iter().copied().filter(|&r| r < n).collect();
    if rows.is_empty() && s.selected_channel < n { vec![s.selected_channel] } else { rows }
}

/// Channel routing ▸ (spec: ui/mixer-track-commands.md §2): the selected channels go to `dst` (named after the first),
/// or one per track from `dst` on (`spread`; channels past the last insert stay). `dst` becomes the current track.
pub fn route_channels(s: &mut State, dst: u16, spread: bool) -> Option<String> {
    let rows = selected_channels(s);
    let first = *rows.first()?;
    let max = inserts(s);
    let mut moved = 0;
    for (k, &r) in rows.iter().enumerate() {
        let t = if spread { dst + k as u16 } else { dst };
        if t > max {
            break;
        }
        s.project.channels[r].mixer_track = t;
        if spread || k == 0 {
            name_after(s, t, r);
        }
        moved += 1;
    }
    s.mixer.sel.clear();
    s.selected_mixer = dst;
    changed(s);
    Some(if moved == 1 {
        format!("Routed {} to {}", crate::rack::channel_name(s, first), name(s, dst))
    } else {
        format!("Routed {moved} channels")
    })
}

/// Route to free mixer track (spec: ui/mixer-track-commands.md §1): a channel alone on its insert keeps it, the others
/// move to the lowest insert no channel uses (a new track when none is free); the track is named after the channel
/// and becomes the current one.
pub fn route_free(s: &mut State, rows: &[usize]) -> Option<String> {
    let mut last = None;
    for &r in rows {
        let Some(cur) = s.project.channels.get(r).map(|c| c.mixer_track) else { continue };
        let alone = cur != 0 && cur <= inserts(s) && !s.project.channels.iter().enumerate().any(|(i, c)| i != r && c.mixer_track == cur);
        let t = if alone {
            cur
        } else {
            match (1..=inserts(s)).find(|t| !s.project.channels.iter().any(|c| c.mixer_track == *t)) {
                Some(t) => t,
                None => add_track(s, None),
            }
        };
        s.project.channels[r].mixer_track = t;
        name_after(s, t, r);
        last = Some((r, t));
    }
    let (r, t) = last?;
    s.mixer.sel.clear();
    s.selected_mixer = t;
    changed(s);
    Some(if rows.len() == 1 {
        format!("Routed {} to {}", crate::rack::channel_name(s, r), name(s, t))
    } else {
        format!("Routed {} channels to free tracks", rows.len())
    })
}

/// Select ▸ Tracks this one is routed to / Tracks routed to this one (spec: ui/mixer-track-commands.md §3), for every
/// track the command acts on. The first found becomes the current track.
fn select_linked(s: &mut State, n: u16, down: bool) -> Option<String> {
    let tg = targets(s, n);
    let mut found: Vec<u16> = Vec::new();
    for m in &s.project.mixer {
        let hit = if down { tg.iter().any(|&t| track(s, t).is_some_and(|x| x.routes.contains(&m.index))) } else { m.routes.iter().any(|r| tg.contains(r)) };
        if hit && !tg.contains(&m.index) {
            found.push(m.index);
        }
    }
    let Some(&first) = found.first() else { return Some("No tracks to select".into()) };
    s.selected_mixer = first;
    s.mixer.sel = found;
    None
}

/// The inserts Trim unused tracks deletes (spec: ui/mixer-track-commands.md §4, §6): no channel uses them and (unless
/// `non_empty`) they hold no effect; a track a kept track sends to stays, and so does one insert at least.
pub fn trim_list(s: &State, non_empty: bool) -> Vec<u16> {
    let n = inserts(s);
    let mut gone: Vec<u16> = (1..=n)
        .filter(|&t| !s.project.channels.iter().any(|c| c.mixer_track == t))
        .filter(|&t| non_empty || track(s, t).is_none_or(|m| m.slots.iter().all(Option::is_none)))
        .collect();
    loop {
        let fed: Vec<u16> = gone
            .iter()
            .copied()
            .filter(|&t| s.project.mixer.iter().any(|m| !gone.contains(&m.index) && m.index != t && m.routes.contains(&t)))
            .collect();
        if fed.is_empty() {
            break;
        }
        gone.retain(|t| !fed.contains(t));
    }
    if gone.len() == usize::from(n) {
        gone.remove(0);
    }
    gone
}

/// Deletes the tracks in `gone`: the others close up in order, channels and sends follow them, the mixer gets
/// shorter (spec: ui/mixer-track-commands.md §4).
fn remove_tracks(s: &mut State, gone: &[u16]) -> Option<String> {
    if gone.is_empty() {
        return Some("No unused mixer tracks".into());
    }
    let mut new_of = std::collections::HashMap::new();
    let mut k = 0u16;
    for m in &s.project.mixer {
        if !gone.contains(&m.index) {
            new_of.insert(m.index, k);
            k += 1;
        }
    }
    let m = |t: u16| new_of.get(&t).copied();
    s.mixer.recording.inputs=std::mem::take(&mut s.mixer.recording.inputs).into_iter().filter_map(|(t,i)|Some((m(t)?,i))).collect();
    s.mixer.recording.armed=s.mixer.recording.armed.iter().copied().filter_map(m).collect();
    for &t in gone {
        for slot in 0..64 {
            s.mixer.pending.push(FxReq::Delete { track: t, slot });
        }
    }
    let mut map = Vec::new();
    for (&o, &n) in &new_of {
        if o != n {
            map.extend((0..64u8).map(|slot| ((o, slot), (n, slot))));
        }
    }
    s.mixer.pending.push(FxReq::Remap(map));
    let old = std::mem::take(&mut s.project.mixer);
    s.project.mixer = old
        .into_iter()
        .filter_map(|t| {
            let index = m(t.index)?;
            let routes = t.routes.iter().filter_map(|&r| m(r)).collect();
            let send_levels = t.send_levels.iter().filter_map(|&(d, l)| m(d).map(|d| (d, l))).collect();
            Some(MixerTrack { index, routes, send_levels, ..t })
        })
        .collect();
    s.project.mixer.sort_by_key(|t| t.index);
    for c in s.project.channels.iter_mut() {
        c.mixer_track = m(c.mixer_track).unwrap_or(0);
    }
    s.selected_mixer = m(s.selected_mixer).unwrap_or(1).min(inserts(s));
    s.mixer.sel = s.mixer.sel.iter().filter_map(|&t| m(t)).collect();
    s.mixer.slot_names = std::mem::take(&mut s.mixer.slot_names).into_iter().filter_map(|((t, k), v)| Some(((m(t)?, k), v))).collect();
    changed(s);
    Some(format!("Removed {} unused mixer track(s)", gone.len()))
}

/// Routes the selected tracks to `dst` (§5): a normal route at unity, a sidechain at level 0, `only` drops their
/// other routes.
pub fn route(s: &mut State, dst: u16, only: bool, side: bool) -> Option<String> {
    let mut msg = None;
    for src in selected(s) {
        if src == dst || src == 0 {
            continue;
        }
        if makes_loop(s, src, dst) {
            msg = Some(format!("Can't route {} to {}: it would make a loop", name(s, src), name(s, dst)));
            continue;
        }
        let m = track_mut(s, src)?;
        if only {
            m.routes.clear();
            m.send_levels.clear();
        }
        if !m.routes.contains(&dst) {
            m.routes.push(dst);
            m.routes.sort_unstable();
        }
        m.send_levels.retain(|&(d, _)| d != dst);
        if side {
            m.send_levels.push((dst, 0.0));
        }
    }
    changed(s);
    msg
}

/// Appends a track (`at` = None) or inserts one before `at`, shifting the later ones; selects it. Returns its number.
pub fn add_track(s: &mut State, at: Option<u16>) -> u16 {
    let last = inserts(s) + 1;
    let n = at.unwrap_or(last).clamp(1, last);
    // The new track takes number n; n.. move one up (the hidden last track stays last).
    let len = s.project.mixer.len() as u16;
    s.project.mixer.push(MixerTrack::new(len));
    // The hidden last track moves too, keeping what it had (spec: flp/events/mixer.md "Adding tracks").
    shift_tracks(s, n, len, 1);
    let fresh = MixerTrack::new(n);
    if let Some(m) = track_mut(s, n) {
        *m = fresh;
    }
    s.mixer.sel.clear();
    s.selected_mixer = n;
    changed(s);
    n
}

/// Moves tracks `from..to` (exclusive) by `d` places (+1 = right): contents, routes into them, channel targets and
/// hosted slot instances. The track at the destination end is overwritten; callers make room first.
fn shift_tracks(s: &mut State, from: u16, to: u16, d: i32) {
    let m = |t: u16| if t >= from && t < to { (i32::from(t) + d) as u16 } else { t };
    renumber(s, &m, from, to, d);
}

/// Renumbers tracks with `m` (a permutation on the touched range).
fn renumber(s: &mut State, m: &dyn Fn(u16) -> u16, from: u16, to: u16, d: i32) {
    s.mixer.recording.inputs=std::mem::take(&mut s.mixer.recording.inputs).into_iter().map(|(t,i)|(m(t),i)).collect();
    s.mixer.recording.armed=s.mixer.recording.armed.iter().copied().map(m).collect();
    if let Some(c)=&mut s.mixer.clipboard {
        c.track.index=m(c.track.index);
        for r in &mut c.track.routes {*r=m(*r);}
        for (r,_) in &mut c.track.send_levels {*r=m(*r);}
    }
    let retarget=|id:u32| {
        use os_model::target::{self as t,Target::*};
        match t::decode(id) {
            MixerVolume(n)=>t::mixer(m(n),t::MIXER_VOLUME),MixerPan(n)=>t::mixer(m(n),t::MIXER_PAN),
            MixerSeparation(n)=>t::mixer(m(n),t::MIXER_SEPARATION),
            MixerEq(n,b,k)=>t::mixer(m(n),[t::EQ_GAIN,t::EQ_FREQ,t::EQ_WIDTH][k as usize]+b),
            MixerStrip(n,k)=>t::strip(m(n),k),MixerSend(n,d)=>t::send(m(n),m(d)),
            SlotOn(n,k)=>t::slot(m(n),k,t::SLOT_ON),SlotMix(n,k)=>t::slot(m(n),k,t::SLOT_MIX),
            SlotParam(n,k,p)=>t::slot_param(m(n),k,p),_=>id,
        }
    };
    for a in &mut s.project.automations {for t in &mut a.targets {*t=retarget(*t);}}
    for c in &mut s.project.init_controls {c.target=retarget(c.target);}
    for l in &mut s.project.peak_links {l.source=m(l.source);l.target=retarget(l.target);}
    for p in &mut s.project.patterns {for c in &mut p.controls {c.kind=retarget(c.kind);}}
    let old: Vec<MixerTrack> = s.project.mixer.clone();
    let _ = d;
    for t in &old {
        if t.index >= from && t.index < to {
            let n = m(t.index);
            if let Some(dst) = s.project.mixer.iter_mut().find(|x| x.index == n) {
                *dst = MixerTrack { index: n, ..t.clone() };
            }
        }
    }
    for t in s.project.mixer.iter_mut() {
        for r in t.routes.iter_mut() {
            *r = m(*r);
        }
        for (d, _) in t.send_levels.iter_mut() {
            *d = m(*d);
        }
        t.routes.retain(|&r| r != t.index);
    }
    // Port choices are ordinals of the incoming tracks. Preserve the chosen source after sorting changes.
    for t in &mut s.project.mixer {
        let Some(was)=old.iter().find(|o|m(o.index)==t.index) else {continue};
        let before:Vec<_>=old.iter().filter(|o|o.index!=was.index && o.routes.contains(&was.index)).map(|o|o.index).collect();
        let mut after:Vec<_>=before.iter().copied().map(m).collect();after.sort_unstable();
        for slot in t.slots.iter_mut().flatten() {
            if slot.name==os_flp::wrapper::WRAPPER {
                for (bus,port) in os_flp::wrapper::input_ports(&slot.state).into_iter().enumerate() {
                    if port>0 {if let Some(src)=before.get(port as usize-1) {if let Some(index)=after.iter().position(|t|*t==m(*src)) {
                        if let Some(state)=os_flp::wrapper::set_input_port(&slot.state,bus,index as i32+1){slot.state=state;}
                    }}}
                }
            }
        }
    }
    for c in s.project.channels.iter_mut() {
        c.mixer_track = m(c.mixer_track);
    }
    for a in &mut s.project.arrangements {for t in &mut a.tracks {t.comp=t.comp.map(m);}}
    s.selected_mixer = m(s.selected_mixer);
    s.mixer.sel = s.mixer.sel.iter().map(|&t| m(t)).collect();
    s.mixer.slot_names = std::mem::take(&mut s.mixer.slot_names).into_iter().map(|((t, k), v)| ((m(t), k), v)).collect();
    let mut map = Vec::new();
    for t in &old {
        for k in 0..64u8 {
            map.push(((t.index, k), (m(t.index), k)));
        }
    }
    s.mixer.pending.push(FxReq::Remap(map));
    changed(s);
}

/// Move one insert without changing its identity, routing, effects or automation.
pub fn reorder(s: &mut State, from: u16, to: u16, remember: bool) {
    if from == 0 || to == 0 || from == to || from > inserts(s) || to > inserts(s) { return; }
    if s.rec.audio || s.rec.recording() || !s.mixer.recording.live.is_empty() { s.message=Some("Stop recording before moving mixer tracks".into()); return; }
    if remember {
        crate::history::record(s,"Move mixer track",crate::history::Snap::MixerMove {
            from:to,to:from,arrangements:s.project.arrangements.clone(),lanes:s.project.instrument_lanes.clone(),
        });
    }
    let m=|t:u16| if t==from {to} else if from<to && t>from && t<=to {t-1} else if from>to && t>=to && t<from {t+1} else {t};
    renumber(s,&m,from.min(to),from.max(to)+1,0);
    if remember { sync_playlist_order(s,from.min(to),from.max(to)); }
}

/// Reorder linked rows as whole groups. Mixed-instrument rows and unrelated rows stay in place.
fn sync_playlist_order(s:&mut State,lo:u16,hi:u16) {
    use os_model::ClipSource;
    for arr in &mut s.project.arrangements {
        let mut blocks:Vec<(usize,usize,Option<u16>)>=Vec::new();
        let mut start=0;
        while start<arr.tracks.len() {
            let mut end=start+1;
            while end<arr.tracks.len() && arr.tracks[end].grouped {end+=1;}
            let mut routes=std::collections::BTreeSet::new();
            if let Some(t)=arr.tracks[start].comp {routes.insert(t);}
            for c in arr.clips.iter().filter(|c|usize::from(c.track)>=start && usize::from(c.track)<end) {
                match c.source {
                    ClipSource::Channel(ch)=>{if let Some(ch)=s.project.channels.iter().find(|v|v.index==ch){routes.insert(ch.mixer_track);}},
                    ClipSource::Pattern(n)=>{if let Some(p)=s.project.patterns.iter().find(|p|p.number==n){for note in &p.notes {if let Some(ch)=s.project.channels.iter().find(|c|c.index==note.channel){routes.insert(ch.mixer_track);}}}},
                }
            }
            for lane in s.project.instrument_lanes.iter().filter(|v|v.1==arr.number && usize::from(v.2)==start) {
                if let Some(ch)=s.project.channels.iter().find(|c|c.index==lane.0) {routes.insert(ch.mixer_track);}
            }
            let route=if routes.len()==1 {routes.first().copied().filter(|t|*t>=lo&&*t<=hi)} else {None};
            blocks.push((start,end,route));start=end;
        }
        let mut linked:Vec<_>=blocks.iter().copied().filter(|b|b.2.is_some()).collect();
        linked.sort_by_key(|b|b.2);
        let mut linked=linked.into_iter();
        let mut order=Vec::new();
        for b in &blocks {let b=if b.2.is_some(){linked.next().unwrap()}else{*b};order.extend(b.0..b.1);}
        let old=arr.tracks.clone();let mut map=vec![0u16;old.len()];
        for (new,&old_row) in order.iter().enumerate(){map[old_row]=new as u16;arr.tracks[new]=os_model::PlaylistTrack{number:new as u32+1,..old[old_row].clone()};}
        for c in &mut arr.clips {if let Some(&row)=map.get(c.track as usize){c.track=row;}}
        for lane in s.project.instrument_lanes.iter_mut().filter(|v|v.1==arr.number){if let Some(&row)=map.get(lane.2 as usize){lane.2=row;}}
    }
    s.playlist.sel.clear();
}

/// Swaps track `n` with its neighbour (`d` = -1 left, +1 right), §6.1 Move left / right.
fn move_track(s: &mut State, n: u16, d: i32) -> Option<String> {
    let o = i32::from(n) + d;
    if n == 0 || o < 1 || o > i32::from(inserts(s)) {
        return None;
    }
    reorder(s,n,o as u16,true);
    None
}

/// Deletes track `n`: later tracks move one left, a default track is added at the end (§6.1 Delete).
fn delete_track(s: &mut State, n: u16) -> Option<String> {
    if n == 0 || n > inserts(s) {
        return Some("The master track can't be deleted".into());
    }
    let label = name(s, n);
    let len = s.project.mixer.len() as u16;
    // Drop the track's slots and routes into it, then close the gap.
    for t in s.project.mixer.iter_mut() {
        t.routes.retain(|&r| r != n);
        t.send_levels.retain(|&(d, _)| d != n);
    }
    for c in s.project.channels.iter_mut().filter(|c| c.mixer_track == n) {
        c.mixer_track = 0;
    }
    for k in 0..64 {
        s.mixer.pending.push(FxReq::Delete { track: n, slot: k });
    }
    s.mixer.slot_names.retain(|(t, _), _| *t != n);
    // The hidden last track stays last; the default track comes before it (spec: flp/events/mixer.md "The last track").
    let hidden = len - 1;
    let m = move |t: u16| if t > n && t < hidden { t - 1 } else { t };
    let retained:Vec<_>=s.project.peak_links.iter().copied().filter(|l|l.source!=n && crate::peak_link::mixer_target(l.target)!=Some(n)).collect();
    crate::peak_link::restore_removed(s,&retained);s.project.peak_links=retained;
    for l in &mut s.project.peak_links{l.source=m(l.source);l.target=crate::peak_link::remap_target(l.target,&m);}
    let old = s.project.mixer.clone();
    for t in &old {
        if t.index > n && t.index < hidden {
            if let Some(dst) = s.project.mixer.iter_mut().find(|x| x.index == t.index - 1) {
                *dst = MixerTrack { index: t.index - 1, ..t.clone() };
            }
        }
    }
    if let Some(last) = s.project.mixer.iter_mut().find(|x| x.index == hidden - 1) {
        *last = MixerTrack::new(hidden - 1);
    }
    for t in s.project.mixer.iter_mut() {
        for r in t.routes.iter_mut() {
            *r = m(*r);
        }
        for (d, _) in t.send_levels.iter_mut() {
            *d = m(*d);
        }
    }
    for c in s.project.channels.iter_mut() {
        c.mixer_track = m(c.mixer_track);
    }
    let mut map = Vec::new();
    for t in n + 1..hidden {
        for k in 0..64u8 {
            map.push(((t, k), (t - 1, k)));
        }
    }
    s.mixer.pending.push(FxReq::Remap(map));
    s.mixer.slot_names = std::mem::take(&mut s.mixer.slot_names).into_iter().map(|((t, k), v)| ((m(t), k), v)).collect();
    s.mixer.sel.clear();
    s.selected_mixer = n.min(inserts(s)).max(1);
    changed(s);
    Some(format!("Deleted {label}"))
}

/// Back to a fresh track (name, colour, levels, slots kept out), routed to the master (§6.1 Reset).
fn reset_track(s: &mut State, n: u16) {
    if let Some(m) = track_mut(s, n) {
        let (slots, origin) = (m.slots.clone(), m.origin);
        *m = MixerTrack { slots, origin, ..MixerTrack::new(n) };
    }
    changed(s);
}

// ---------------------------------------------------------------------------------------------------------
// Menus

const COLORS: [(&str, u32); 8] = [
    ("Red", 0xc8504a),
    ("Orange", 0xd0803c),
    ("Yellow", 0xc8b44a),
    ("Green", 0x5a9c4a),
    ("Teal", 0x3c8c88),
    ("Blue", 0x4a78b4),
    ("Purple", 0x8a5aa0),
    ("Grey", 0x7a8288),
];

/// Track menu (right click on a strip), §6.1.
pub fn track_menu(s: &State, n: u16, x: f32, y: f32) -> Popup {
    let mut items = vec![PItem::head(format!("Mixer track ({})", name(s, n)))];
    items.extend(track_items(s, n));
    Popup { x, y, items, sub: None, scroll: [0; 2], cascade: Vec::new() }
}

fn track_items(s: &State, n: u16) -> Vec<PItem> {
    let mx = |c: MCmd| Cmd::Mixer(c);
    let mut colors: Vec<PItem> = vec![PItem::new("Default", mx(MCmd::Color(n, None)))];
    colors.extend(COLORS.iter().map(|&(l, c)| PItem::new(l, mx(MCmd::Color(n, Some(Rgb::new((c >> 16) as u8, (c >> 8) as u8, c as u8)))))));
    vec![
        PItem::new("Rename, color and icon...", mx(MCmd::Rename(n))).key("F2"),
        PItem::sub("Change color...", colors),
        PItem::not_yet("Change icon..."),
        PItem::sep(),
        PItem::new("Reset selected track(s) to default", mx(MCmd::Reset(n))),
        PItem::new("Reset selected track(s) routing", mx(MCmd::ResetRouting(n))),
        PItem::sep(),
        PItem::new("Separator", mx(MCmd::Separator(n))).mark(Mark::Check(has_separator(s, n))),
        PItem::sub("Channel layout",["Standard (FL-style)","Custom strip","Vox (Custom + DS)"].iter().enumerate().map(|(i,label)| {
            let item=PItem::new(*label,mx(MCmd::ChannelLayout(n,i as u8))).mark(Mark::Radio(track(s,n).is_some_and(|m|m.channel_layout==i as u8)));
            if n==0 {item.off()}else{item}
        }).collect()),
        PItem::sub("Dock to", vec![PItem::not_yet("Left"), PItem::not_yet("Middle"), PItem::not_yet("Right")]),
        PItem::sep(),
        PItem::sub("File", vec![PItem::not_yet("Render to wave file...")]),
        PItem::not_yet("Allow threaded processing").mark(Mark::Check(true)),
    ]
}

/// Trim unused tracks ▸ (spec: ui/mixer-track-commands.md §6): each choice with the number of tracks it deletes.
fn trim_items(s: &State) -> Vec<PItem> {
    [(false, "Unlinked tracks only"), (true, "Unlinked and non-empty tracks")]
        .into_iter()
        .map(|(non_empty, label)| {
            let k = trim_list(s, non_empty).len();
            let it = PItem::new(format!("{label} ({k})"), Cmd::Mixer(MCmd::Trim { non_empty }));
            if k == 0 { it.off() } else { it }
        })
        .collect()
}

/// Mixer options (title-bar ▸), §6.2: its own items, then the track menu's.
pub fn options_menu(s: &State, x: f32, y: f32) -> Popup {
    let mut items = vec![
        PItem::head("Options"),
        PItem::new("Inspector on left", Cmd::Mixer(MCmd::InspectorLeft)).mark(Mark::Check(inspector_left())),
        PItem::head("Disk recording automatically creates audio clips"),
        PItem::sub("Plugin delay compensation", vec![PItem::not_yet("Automatic")]),
        PItem::not_yet("Link all parameters..."),
        PItem::sub("View", vec![PItem::not_yet("Waveforms"), PItem::not_yet("Extra properties")]),
        PItem::new("Detached",Cmd::Mixer(MCmd::Detach)).mark(Mark::Check(s.native_detached.contains(&Win::Mixer))),
        PItem::sep(),
        PItem::sub("Trim unused tracks", trim_items(s)),
        PItem::not_yet("Switch smart disable for all plugins"),
        PItem::new("Remove all (audio editor) instances", Cmd::Mixer(MCmd::RemoveTeslas)),
        // The cable physics debug panel, hidden from users; uncomment to tune the cables again.
        // PItem::new("Cable physics (debug)...", Cmd::Mixer(MCmd::CablePhysics)).mark(Mark::Check(s.cables.borrow().debug)),
        PItem::head(format!("Mixer track ({})", name(s, s.selected_mixer))),
    ];
    items.extend(track_items(s, s.selected_mixer));
    Popup { x, y, items, sub: None, scroll: [0; 2], cascade: Vec::new() }
}

/// The compact and full views also available on the mixer toolbar.
pub fn layout_menu(s: &State, x: f32, y: f32) -> Popup {
    let mut items = Vec::new();
    for (mode,label) in [(COMPACT,"Compact"),(WIDE,"Full")] {
        items.push(PItem::new(label,Cmd::Mixer(MCmd::Layout(mode))).mark(Mark::Radio(s.mixer.layout==mode)));
    }
    Popup { x, y, items, sub: None, scroll: [0; 2], cascade: Vec::new() }
}

/// Empty slot: the effect list (§7.1), the plugin menu for this slot (spec: ui/plugin-menu.md).
pub fn effect_menu(s: &State, slot: u8, x: f32, y: f32) -> Popup {
    let mut items = vec![PItem::not_yet("Plugin database").key(crate::shortcuts::menu_key("Plugin database", "Shift+F8")), PItem::sep()];
    items.extend(crate::plugin_menu::items(s, crate::plugin_menu::Target::Fx(Some(slot)), false));
    Popup { x, y, items, sub: None, scroll: [0; 2], cascade: Vec::new() }
}

/// Filled slot ▸ (§7.2).
pub fn slot_menu(s: &State, slot: u8, x: f32, y: f32) -> Popup {
    let mx = |c: MCmd| Cmd::Mixer(c);
    let replace = crate::plugin_menu::items(s, crate::plugin_menu::Target::Fx(Some(slot)), true);
    let items = vec![
        PItem::sub("Replace", replace),
        PItem::new("Delete...", mx(MCmd::SlotDelete(slot))),
        PItem::sep(),
        PItem::not_yet("Patcherize"),
        PItem::not_yet("Patcherize all"),
        PItem::head("Presets"),
        PItem::sub("Presets", vec![PItem::not_yet("(none)")]),
        PItem::not_yet("Browse presets"),
        PItem::not_yet("Save preset as..."),
        PItem::not_yet("Add to plugin database (flag as favorite)"),
        PItem::not_yet("Display preset name in preset selector").mark(Mark::Check(true)),
        PItem::head("Spare state"),
        PItem::not_yet("Store in spare state"),
        PItem::not_yet("Flip with spare state"),
        PItem::head("Parameters"),
        PItem::not_yet("Link all parameters..."),
        PItem::new("Browse parameters", Cmd::Param(crate::params::PCmd::Browse(crate::plugins::fx_key(s.selected_mixer, slot)))),
        PItem::sub("Sidechain input", sidechain_items(s, slot)),
        PItem::head("CPU"),
        PItem::not_yet("Allow threaded processing").mark(Mark::Check(true)),
        PItem::not_yet("Smart disable"),
        PItem::sep(),
        PItem::new("Move up", mx(MCmd::SlotMove(slot, -1))),
        PItem::new("Move down", mx(MCmd::SlotMove(slot, 1))),
        PItem::sep(),
        PItem::new("View editor", mx(MCmd::SlotView(slot))).mark(Mark::Check(false)),
        PItem::new("Rename, color and icon...", mx(MCmd::SlotRename(slot))).key("F2"),
        PItem::not_yet("Change color..."),
        PItem::not_yet("Random color"),
        PItem::not_yet("Change icon..."),
        PItem::sep(),
        PItem::sub("Scaling", vec![PItem::not_yet("100%")]),
        PItem::not_yet("Help"),
    ];
    Popup { x, y, items, sub: None, scroll: [0; 2], cascade: Vec::new() }
}

/// The tracks routed into track `n`, in track order: its inputs, numbered from 1 (spec: audio/sidechain.md "Settings").
pub fn inputs(s: &State, n: u16) -> Vec<u16> {
    let mut ins: Vec<u16> = s.project.mixer.iter().filter(|m| m.index != 0 && m.index != n && m.routes.contains(&n)).map(|m| m.index).collect();
    ins.sort_unstable();
    ins
}

/// Sidechain input ▸: "None" and the current track's inputs by name, the slot's choice marked.
fn sidechain_items(s: &State, slot: u8) -> Vec<PItem> {
    let t = s.selected_mixer;
    let now = track(s, t).and_then(|m| m.slots.get(usize::from(slot))?.as_ref()).filter(|p| p.name == os_flp::wrapper::WRAPPER).map_or(0, |p| {
        os_flp::wrapper::input_ports(&p.state).get(1).map_or(0, |&p| p.max(0) as usize)
    });
    let mut items = vec![PItem::new("None", Cmd::Mixer(MCmd::SlotSidechain(slot, 0))).mark(Mark::Radio(now == 0))];
    for (i, n) in inputs(s, t).into_iter().enumerate() {
        items.push(PItem::new(format!("{}. {}", i + 1, name(s, n)), Cmd::Mixer(MCmd::SlotSidechain(slot, i + 1))).mark(Mark::Radio(now == i + 1)));
    }
    items
}

// ---------------------------------------------------------------------------------------------------------
// Commands

pub fn run(app: &mut App, c: MCmd) -> Option<String> {
    if app.audio_rec.is_some()&&matches!(c,MCmd::MoveLeft(_)|MCmd::MoveRight(_)|MCmd::Delete(_)|MCmd::InsertOne(_)|MCmd::Trim{..}){return Some("Stop the take before moving or removing mixer tracks".into());}
    match c {
        MCmd::QuickSidechain(dst)=>return crate::quick_sidechain::run(app,dst),
        MCmd::Detach=>{app.act(Action::DetachWin(Win::Mixer));return None;},
        MCmd::RecordInput(t,i)=>return app.set_mixer_input(t,i),
        MCmd::ArmDisk(t)=>return app.arm_mixer_disk(t),
        MCmd::RecPoint(t,p)=>{app.state.mixer.recording.rec_point.insert(t,p);app.invalidate();return None;}
        MCmd::Monitor(t,m)=>{
            app.state.mixer.recording.monitor.insert(t,m);
            app.invalidate();
            let what=["Monitoring off","Monitoring when armed","Monitoring on"][usize::from(m.min(2))];
            return Some(app.sync_input_monitor().err().unwrap_or_else(||format!("{}: {what}",name(&app.state,t))));
        }
        _=>{}
    }
    let at = app.mouse;
    let s = &mut app.state;
    let msg = match c {
        MCmd::RecordInput(..)|MCmd::ArmDisk(_)|MCmd::RecPoint(..)|MCmd::Monitor(..)=>None,
        MCmd::TuneSetting(n,k,v)=>{
            if !(1..=3).contains(&k) {return None;}
            let old=track(s,n)?.tune;
            crate::history::record(s,"Vocal tuning",crate::history::Snap::Tune(n,old));
            let m=track_mut(s,n)?;m.tune[k as usize]=v.min(match k{1=>11,2=>12,_=>1});m.tune_learn=0;
            changed(s);None
        },
        MCmd::DriveType(n,p) => {
            let old=track(s,n)?.strip[6];
            crate::history::record(s,"Drive type",crate::history::Snap::DriveType(n,old));
            track_mut(s,n)?.strip[6]=u16::from(p.min(3));
            changed(s);
            None
        }
        MCmd::Detach | MCmd::QuickSidechain(_) => None,
        MCmd::ChannelLayout(n,p) => {
            if n==0 {return None;}
            let old=track(s,n)?.channel_layout;
            crate::history::record(s,"Channel layout",crate::history::Snap::ChannelLayout(n,old));
            track_mut(s,n)?.channel_layout=p.min(2);changed(s);None
        }
        MCmd::CompProfile(n,p) => {
            let old=track(s,n)?.strip[5];
            crate::history::record(s,"Compressor profile",crate::history::Snap::StripProfile(n,old));
            track_mut(s,n)?.strip[5]=u16::from(p.min(5));
            changed(s);
            None
        }
        MCmd::Rename(n) => {
            start_rename(s, n, at);
            None
        }
        MCmd::Color(n, col) => {
            for t in targets(s, n) {
                if let Some(m) = track_mut(s, t) {
                    m.color = col;
                }
            }
            None
        }
        MCmd::RandomColor(n) => {
            for t in targets(s, n) {
                let c = crate::patterns::random_color(u64::from(t).wrapping_mul(0x9e37_79b9) ^ u64::from(rand_seed()));
                if let Some(m) = track_mut(s, t) {
                    m.color = Some(c);
                }
            }
            None
        }
        MCmd::AudioEditor(n) => {
            app.audio_editor(n, false);
            return None;
        }
        MCmd::AudioEditorRecord(n) => {
            app.audio_logger(n);
            return None;
        }
        MCmd::RemoveTeslas => return app.ask_remove_teslas(),
        MCmd::RemoveTeslasNow => {
            let gone = teslas(s);
            for &(t, k) in &gone {
                clear_slot(s, t, k);
            }
            changed(s);
            Some(match gone.len() {
                1 => "Removed 1 audio editor".into(),
                n => format!("Removed {n} audio editors"),
            })
        }
        MCmd::Reset(n) => {
            for t in targets(s, n) {
                reset_track(s, t);
            }
            None
        }
        MCmd::ResetRouting(n) => {
            for t in targets(s, n).into_iter().filter(|&t| t != 0) {
                if let Some(m) = track_mut(s, t) {
                    m.routes = vec![0];
                    m.send_levels.clear();
                }
            }
            changed(s);
            None
        }
        MCmd::UnrouteTo(n) => {
            let tg = targets(s, n);
            for m in s.project.mixer.iter_mut() {
                m.routes.retain(|r| !tg.contains(r));
                m.send_levels.retain(|(d, _)| !tg.contains(d));
            }
            changed(s);
            None
        }
        MCmd::Solo(n) => {
            solo(s, n);
            None
        }
        MCmd::AltSolo(n) => {
            solo_with(s, n, true);
            None
        }
        MCmd::RouteChannels { dst, spread } => route_channels(s, dst, spread),
        MCmd::SelectChannels(n) => {
            let tg = targets(s, n);
            let rows: std::collections::BTreeSet<usize> =
                s.project.channels.iter().enumerate().filter(|(_, c)| tg.contains(&c.mixer_track)).map(|(i, _)| i).collect();
            let k = rows.len();
            s.rack.sel = rows;
            Some(format!("Selected {k} channel(s)"))
        }
        MCmd::SelectDest(n) => select_linked(s, n, true),
        MCmd::SelectSources(n) => select_linked(s, n, false),
        MCmd::SelectAll => {
            s.mixer.sel = s.project.mixer.iter().map(|m| m.index).filter(|&t| t <= inserts(s)).collect();
            None
        }
        MCmd::Trim { non_empty } => {
            let gone = trim_list(s, non_empty);
            remove_tracks(s, &gone)
        }
        MCmd::MoveLeft(n) => move_track(s, n, -1),
        MCmd::MoveRight(n) => move_track(s, n, 1),
        MCmd::Delete(n) => delete_track(s, n),
        MCmd::InsertOne(n) => {
            let n = add_track(s, Some(n.max(1)));
            Some(format!("Inserted {}", name(s, n)))
        }
        MCmd::Route { dst, only, side } => route(s, dst, only, side),
        MCmd::Layout(mode) => {set_layout(s,mode);None},
        MCmd::Separator(n) => {
            let mut names: Vec<String> = crate::prefs::text("mixer_separators").and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default();
            let key = name(s, n);
            let enabled = !has_separator(s, n);
            names.retain(|v| v != &key);
            if let Some(track) = s.project.mixer.get_mut(usize::from(n)) {
                track.separator = enabled;
                s.piano.dirty = true;
            }
            crate::prefs::set_text("mixer_separators", &serde_json::to_string(&names).unwrap_or_default());
            None
        }
        MCmd::InspectorLeft => {
            crate::prefs::set_text("mixer_inspector_left", if inspector_left() { "false" } else { "true" });
            Some("Mixer inspector position saved".into())
        }
        MCmd::CablePhysics => {
            let c = s.cables.get_mut();
            c.debug = !c.debug;
            None
        }
        MCmd::Animations(on) => {
            s.cables.get_mut().animate(on);
            crate::app::save_animations(on);
            None
        }
        MCmd::AddEffect(slot, index) => {
            let t = s.selected_mixer;
            let Some(nm) = app.plugins.effects.get(index).map(|a| a.desc.name.clone()) else { return None };
            crate::peak_link::remove_slot(s,t,slot);
            if let Some(m) = track_mut(s, t) {
                m.slots[usize::from(slot)] = Some(crate::plugins::new_slot(&nm));
                // KNAY: a plugin put in a slot is heard: the track's effects switch comes on with it.
                m.slots_enabled = true;
                m.slot_on[usize::from(slot)] = true;
                m.slot_mix[usize::from(slot)] = 1.0;
            }
            s.mixer.slot_names.remove(&(t, slot));
            if user_slot_index(slot)>=9 {s.mixer.slot_scroll.insert(t,user_slot_index(slot).saturating_sub(8));}
            s.mixer.pending.push(FxReq::Create { track: t, slot, index, state: None });
            changed(s);
            None
        }
        MCmd::SlotDelete(slot) => {
            clear_slot(s, s.selected_mixer, slot);
            changed(s);
            None
        }
        MCmd::SlotMove(slot, d) => {
            let t = s.selected_mixer;
            let o = user_slot_index(slot) as i32 + i32::from(d);
            if !(0..63).contains(&o) {
                return None;
            }
            let (a, b) = (usize::from(slot), user_slot(o as usize) as usize);
            for l in &mut s.project.peak_links {if let os_model::target::Target::SlotParam(tr,k,p)=os_model::target::decode(l.target){if tr==t {let k=if k as usize==a{b as u8}else if k as usize==b{a as u8}else{k};l.target=os_model::target::slot_param(t,k,p);}}}
            if let Some(m) = track_mut(s, t) {
                m.slots.swap(a, b);
                m.slot_on.swap(a, b);
                m.slot_mix.swap(a, b);
            }
            let (na, nb) = (s.mixer.slot_names.remove(&(t, a as u8)), s.mixer.slot_names.remove(&(t, b as u8)));
            if let Some(v) = na {
                s.mixer.slot_names.insert((t, b as u8), v);
            }
            if let Some(v) = nb {
                s.mixer.slot_names.insert((t, a as u8), v);
            }
            s.mixer.pending.push(FxReq::Remap(vec![((t, a as u8), (t, b as u8)), ((t, b as u8), (t, a as u8))]));
            changed(s);
            None
        }
        MCmd::SlotRename(slot) => {
            let t = s.selected_mixer;
            crate::playlist::start_edit(s, EditTarget::SlotName(t, slot), at);
            None
        }
        MCmd::SlotView(slot) => {
            let t = s.selected_mixer;
            s.mixer.pending.push(FxReq::Toggle { track: t, slot });
            None
        }
        MCmd::SlotSidechain(slot, port) => {
            let t = s.selected_mixer;
            if crate::project_plugins::set_slot_sidechain(&mut s.project, &app.plugins, t, slot, port) {
                changed(s);
                None
            } else {
                Some("This effect has no sidechain input".into())
            }
        }
    };
    app.invalidate();
    msg
}

/// Tracks a "selected track(s)" command acts on: the selection when `n` is in it, else `n` alone.
fn targets(s: &State, n: u16) -> Vec<u16> {
    if is_selected(s, n) { selected(s) } else { vec![n] }
}

fn rand_seed() -> u32 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.subsec_nanos())
}

/// Keys while the mixer is in front (§2): F2 rename, Del delete, Alt+Left / Right move, S solo, Alt+S alt solo,
pub(crate) fn channel_copy(s:&State,n:u16)->Option<ChannelCopy> {
    Some(ChannelCopy {track:track(s,n)?.clone(),names:s.mixer.slot_names.iter().filter_map(|(&(t,k),v)|(t==n).then(||(k,v.clone()))).collect()})
}

pub(crate) fn put_channel(s:&mut State,c:ChannelCopy) {
    let n=c.track.index;
    if let Some(dst)=track_mut(s,n) {*dst=c.track;} else {return;}
    s.mixer.slot_names.retain(|(t,_),_|*t!=n);
    for (k,v) in c.names {s.mixer.slot_names.insert((n,k),v);}
    for slot in 0..64 {s.mixer.pending.push(FxReq::Delete{track:n,slot});}
    changed(s);
}

fn copy_channel(app:&mut App,n:u16)->Option<String> {
    app.mixer_tick();
    app.params_tick();
    app.store_builtin_states();
    let mut project=app.state.project.clone();
    crate::project_plugins::store_hosted(&mut project,&app.plugins);
    let mut copied=channel_copy(&app.state,n)?;
    copied.track=project.mixer.iter().find(|t|t.index==n)?.clone();
    app.state.mixer.clipboard=Some(copied);
    Some(format!("Copied {} including effects and routing",name(&app.state,n)))
}

fn paste_channel(app:&mut App,n:u16)->Option<String> {
    if app.state.rec.audio {return Some("Stop the take before pasting a mixer channel".into());}
    let Some(mut c)=app.state.mixer.clipboard.clone() else {return Some("Copy a mixer channel first (Ctrl+C)".into());};
    if n==0 {return Some("Paste onto an insert channel, not the master".into());}
    if c.track.routes.iter().any(|&dst|dst==n || makes_loop(&app.state,n,dst)) {
        return Some("Cannot paste this routing here: it would create a feedback loop".into());
    }
    app.mixer_tick();app.store_builtin_states();
    crate::project_plugins::store_hosted(&mut app.state.project,&app.plugins);
    let old=channel_copy(&app.state,n)?;
    c.track.index=n;c.track.origin=old.track.origin;
    crate::history::record(&mut app.state,"Paste mixer channel",crate::history::Snap::MixerChannel(Box::new(old)));
    put_channel(&mut app.state,c);
    app.mixer_tick();app.sync_effects();app.sync_wrapped();app.sync_native();
    Some(format!("Pasted channel settings, effects and routing into {}",name(&app.state,n)))
}

/// Ctrl+L / Shift+Ctrl+L route the selected channels, Alt+L / Ctrl+Alt+L / Ctrl+A select (spec:
/// ui/mixer-track-commands.md).
pub fn key(app: &mut App, key: &winit::keyboard::Key, ctrl: bool, alt: bool) -> bool {
    use winit::keyboard::{Key, NamedKey};
    let n = app.state.selected_mixer;
    let letter = |c: &str, l: &str| c.eq_ignore_ascii_case(l);
    if ctrl {
        let msg = match key {
            Key::Character(c) if letter(c,"c") && !alt && !app.shift => copy_channel(app,n),
            Key::Character(c) if letter(c,"v") && !alt && !app.shift => paste_channel(app,n),
            Key::Character(c) if letter(c, "l") && alt => run(app, MCmd::SelectSources(n)),
            Key::Character(c) if letter(c, "l") => {
                let spread = app.shift;
                run(app, MCmd::RouteChannels { dst: n, spread })
            }
            Key::Character(c) if letter(c, "a") && !alt => run(app, MCmd::SelectAll),
            _ => return false,
        };
        if msg.is_some() {
            app.state.message = msg;
        }
        app.invalidate();
        return true;
    }
    let msg = match key {
        Key::Character(c) if alt && letter(c, "s") => run(app, MCmd::AltSolo(n)),
        Key::Character(c) if alt && letter(c, "l") => run(app, MCmd::SelectChannels(n)),
        Key::Named(NamedKey::F2) => run(app, MCmd::Rename(n)),
        Key::Named(NamedKey::Delete) => run(app, MCmd::Delete(n)),
        Key::Named(NamedKey::ArrowLeft) if alt => run(app, MCmd::MoveLeft(n)),
        Key::Named(NamedKey::ArrowRight) if alt => run(app, MCmd::MoveRight(n)),
        Key::Named(NamedKey::ArrowLeft) => {
            if n > 0 {
                select(&mut app.state, n - 1, false);
            }
            None
        }
        Key::Named(NamedKey::ArrowRight) => {
            if n < inserts(&app.state) {
                select(&mut app.state, n + 1, false);
            }
            None
        }
        Key::Character(c) if c.eq_ignore_ascii_case("s") && !alt => run(app, MCmd::Solo(n)),
        _ => return false,
    };
    if msg.is_some() {
        app.state.message = msg;
    }
    app.invalidate();
    true
}

/// New peaks from the audio thread: meters rise, fall and hold by elapsed time (`meters::step`). True while any meter
/// is still moving.
pub fn feed_meters(s: &mut State, peaks: &[[f32; 2]], vu: Option<&[[f32; 2]]>) -> bool {
    let now = std::time::Instant::now();
    // Long gaps (the app was idle) count as one frame, so a meter does not vanish in a single step.
    let dt = s.mixer.meter_at.map_or(1.0 / 60.0, |t| now.duration_since(t).as_secs_f32().min(0.1));
    s.mixer.meter_at = Some(now);
    let m = &mut s.mixer.meters;
    if m.len() < peaks.len() {
        m.resize(peaks.len(), Default::default());
    }
    let mut moving = false;
    for (i, v) in m.iter_mut().enumerate() {
        let p = peaks.get(i).copied().unwrap_or([0.0; 2]);
        moving |= crate::meters::step(v, p, dt, &s.mixer.meter_cfg);
        if let Some(level) = vu.and_then(|levels| levels.get(i)) {
            v.vu = *level;
            v.vu_idle = 0.0;
        } else {
            v.vu_idle += dt;
            if v.vu_idle > 0.1 {
                for level in &mut v.vu {
                    *level *= (-dt / 0.065).exp();
                    if *level < 1e-6 { *level = 0.0; }
                }
            }
        }
        moving |= v.vu.iter().any(|&level| level > 0.0);
    }
    if !moving {
        s.mixer.meter_at = None;
    }
    moving
}

impl App {
    /// Mixer slot work on the UI thread (`plugin_tick`): create / open / close / drop hosted effect instances.
    pub(crate) fn mixer_tick(&mut self) {
        let reqs = std::mem::take(&mut self.state.mixer.pending);
        if reqs.is_empty() {
            return;
        }
        let mut relink = false;
        // Creates waiting for a native plugin's first check (`Plugins::effect_ready`); a newer request for the same
        // slot replaces a waiting one.
        let mut waiting: Vec<FxReq> = Vec::new();
        let mut done = false;
        for r in reqs {
            if let FxReq::Create { track, slot, .. } | FxReq::Delete { track, slot } = r {
                waiting.retain(|w| !matches!(*w, FxReq::Create { track: t, slot: s, .. } if (t, s) == (track, slot)));
            }
            if let FxReq::Create { index, .. } = r {
                if !self.plugins.effect_ready(index) {
                    let name = self.plugins.effects.get(index).map(|a| a.desc.name.clone()).unwrap_or_default();
                    let msg = format!("Trying {name} before its first load...");
                    if self.state.message.as_deref() != Some(msg.as_str()) {
                        self.state.message = Some(msg);
                        done = true;
                    }
                    waiting.push(r);
                    continue;
                }
            }
            done = true;
            match r {
                FxReq::Create { track, slot, index, state } => match self.plugins.create_effect(track, slot, index, state.as_deref()) {
                    Ok(_) => {
                        self.plugins.added(crate::plugins::fx_key(track, slot));
                        relink = true;
                        self.open_fx(track, slot);
                    }
                    Err(e) => {
                        if let Some(m) = track_mut(&mut self.state, track) {
                            m.slots[usize::from(slot)] = None;
                        }
                        self.state.message = Some(format!("Effect: {e}"));
                    }
                },
                FxReq::Toggle { track, slot } => {
                    let key = crate::plugins::fx_key(track, slot);
                    if !self.plugins.close_if_open(key) {
                        self.open_fx(track, slot);
                    }
                }
                FxReq::Show { track, slot } => self.open_fx(track, slot),
                FxReq::Delete { track, slot } => {
                    let key = crate::plugins::fx_key(track, slot);
                    if self.plugins.hosted.contains_key(&key) {
                        self.plugins.drop_key(key);
                        relink = true;
                    }
                }
                FxReq::Remap(map) => {
                    self.plugins.remap_fx(&|t, k| Some(map.iter().find(|(a, _)| *a == (t, k)).map_or((t, k), |&(_, b)| b)));
                    relink = true;
                }
            }
        }
        waiting.append(&mut self.state.mixer.pending);
        self.state.mixer.pending = waiting;
        if relink {
            self.transport.set_effects(self.plugins.effects());
            self.state.piano.dirty = true;
        }
        if done {
            self.invalidate();
        }
    }

    /// Tesla on track `t`: the toolbar's audio editor button puts it on the master, as FL opens Edison there (spec:
    /// plugins/tesla/requests.md #5); the track menu's "Open audio editor" on that track. It shows the track's first
    /// Tesla, adding one when it has none; `new` (the button's right click) always adds one, in the first free slot.
    pub(crate) fn audio_editor(&mut self, t: u16, new: bool) {
        if !new {
            if let Some(k) = first_tesla(&self.state, t) {
                self.state.mixer.pending.push(FxReq::Show { track: t, slot: k });
                return self.invalidate();
            }
        }
        self.add_tesla(t, None);
    }

    /// Open audio editor and record: a Tesla on track `t` set up as an audio logger, as FL's quick load sets Edison
    /// (spec: plugins/tesla/README.md "Recording into Tesla from Lemon Studio"): records on the host's play, for 5
    /// minutes, appended, slaved to the transport, armed.
    pub(crate) fn audio_logger(&mut self, t: u16) {
        self.tesla_for_recording(t, |s| {
            s.rec.when = tesla::state::WHEN_ON_PLAY;
            s.rec.max = AUDIO_LOG_5_MINUTES;
            s.rec.append = true;
            s.rec.slave = true;
            s.rec.armed = true;
        });
    }

    /// A Tesla on track `t` to record into (spec: plugins/tesla/README.md "Recording into Tesla from Lemon Studio"):
    /// the track's first Tesla when it holds no sample, else a new one in the first free slot, so no audio or undo
    /// history is touched. `set` changes its state (the empty one is reloaded with it, a new one starts from it) and
    /// its window is shown. False when there is no Tesla to use (the message says why).
    pub(crate) fn tesla_for_recording(&mut self, t: u16, set: impl FnOnce(&mut tesla::state::Saved)) -> bool {
        use tesla::state::{from_plugin_state, plugin_state, Saved};
        let empty = first_tesla(&self.state, t).and_then(|k| {
            let key = crate::plugins::fx_key(t, k);
            let saved = self.plugins.state(key).and_then(|b| from_plugin_state(&b)).filter(|s| s.sample.is_empty())?;
            Some((k, key, saved))
        });
        let saved = match empty {
            Some((k, key, mut saved)) => {
                set(&mut saved);
                if self.plugins.set_state(key, &plugin_state(&saved)) {
                    self.plugins.mark_edited(key);
                    self.state.mixer.pending.push(FxReq::Show { track: t, slot: k });
                    changed(&mut self.state);
                    self.invalidate();
                    return true;
                }
                saved
            }
            None => {
                let mut saved = Saved::default();
                set(&mut saved);
                saved
            }
        };
        self.add_tesla(t, Some(plugin_state(&saved)))
    }

    /// A new Tesla in track `t`'s first free slot, starting from `state` when given; false (with a message) when
    /// Tesla is missing or the track is full.
    fn add_tesla(&mut self, t: u16, state: Option<Vec<u8>>) -> bool {
        use crate::plugins::TESLA;
        let s = &mut self.state;
        let Some(index) = self.plugins.effect_index(TESLA) else {
            s.message = Some(format!("{TESLA} is not available"));
            self.invalidate();
            return false;
        };
        let Some(k) = track(s, t).and_then(|m| m.slots.iter().enumerate().find(|(k,s)|*k!=10&&s.is_none()).map(|(k,_)|k)) else {
            s.message = Some("No free effect slot on this track".into());
            self.invalidate();
            return false;
        };
        let m = track_mut(s, t).expect("the track");
        m.slots[k] = Some(crate::plugins::new_slot(TESLA));
        m.slots_enabled = true;
        m.slot_on[k] = true;
        m.slot_mix[k] = 1.0;
        s.mixer.slot_names.remove(&(t, k as u8));
        s.mixer.pending.push(FxReq::Create { track: t, slot: k as u8, index, state });
        changed(s);
        self.invalidate();
        true
    }

    /// Remove all (audio editor) instances (spec: plugins/tesla/README.md "Recording into Tesla from Lemon Studio"):
    /// asks first, since removed slots can't be brought back yet; with none, says so.
    pub(crate) fn ask_remove_teslas(&mut self) -> Option<String> {
        let n = teslas(&self.state).len();
        if n == 0 {
            return Some("No audio editors in the mixer".into());
        }
        let (x, y) = self.mouse;
        let items = vec![
            PItem::head(remove_question(n)),
            PItem::new("Remove", Cmd::Mixer(MCmd::RemoveTeslasNow)),
            PItem::new("Cancel", Cmd::Nothing),
        ];
        self.state.playlist.popup = Some(Popup { x, y, items, sub: None, scroll: [0; 2], cascade: Vec::new() });
        self.invalidate();
        None
    }

    pub(crate) fn open_fx(&mut self, track: u16, slot: u8) {
        let key = crate::plugins::fx_key(track, slot);
        if !self.plugins.hosted.contains_key(&key) {
            self.state.message = Some("This slot's plugin is not hosted (no KNAY Studio version of it yet)".into());
            return;
        }
        #[cfg(not(plugin_windows))]
        {
            self.state.message = Some(crate::app::off_text(crate::app::PLUGIN_WINDOWS));
        }
        #[cfg(plugin_windows)]
        {
            let info = fx_info(&self.state.project, &self.state.mixer, key);
            match info.map(|i| self.plugins.open_window(key, i)) {
                Some(Ok(())) => {
                    self.state.raise(crate::app::Win::Hosted(key));
                    self.select_window_track(key);
                }
                Some(Err(e)) => {
                    let message = format!("{}: {e}", slot_name(&self.state, track, slot).unwrap_or_default());
                    if let Some(path) = os_paths::user_file("plugin-window-error.txt") {
                        let _ = std::fs::write(path, &message);
                    }
                    self.state.message = Some(format!("Plugin: {message}"));
                }
                None => {}
            }
        }
    }

    /// Sample settings preserve mixer navigation for routing drags. Other linked plugin windows select their
    /// mixer track as clicking that insert would, scrolled into view; a channel on the master selects the master
    /// (spec: ui/settings-choices.md "Miscellaneous: seven more checks, Default template and Startup project").
    #[cfg_attr(not(plugin_windows), allow(dead_code))]
    pub(crate) fn select_window_track(&mut self, key: usize) {
        if crate::plugins::fx_of(key).is_none()
            && self.state.project.channels.get(key).is_some_and(|c| c.kind.is_sampler()) {
            return;
        }
        if !crate::prefs::check(&crate::prefs::AUTO_SELECT_LINKED) {
            return;
        }
        let s = &self.state;
        let track = match crate::plugins::fx_of(key) {
            Some((t, _)) => Some(t),
            None => s.project.channels.get(key).map(|c| c.mixer_track),
        };
        let Some(t) = track.filter(|t| s.project.mixer.iter().any(|m| m.index == *t)) else { return };
        select(&mut self.state, t, false);
        // The master strip is fixed; an insert scrolls into the mixer's last painted view.
        if let Some(bar) = self.bar(crate::app::Scroll::MixerX).filter(|_| t > 0) {
            let w = crate::views::mixer::strip_width(&self.state);
            let left = w * f32::from(t - 1);
            let scroll = self.state.mixer_scroll;
            if left < scroll {
                self.scroll_set(crate::app::Scroll::MixerX, left);
            } else if left + w > scroll + bar.view {
                self.scroll_set(crate::app::Scroll::MixerX, left + w - bar.view);
            }
        }
        self.invalidate();
    }
}

/// The plugin frame's title for a mixer slot: "Name (Insert N)" / "(Master)" (§7).
#[cfg(plugin_windows)]
pub fn fx_info(p: &os_model::Project, ui: &MixerUi, key: usize) -> Option<crate::plugin_window::ChannelInfo> {
    let (t, k) = crate::plugins::fx_of(key)?;
    let m = p.mixer.iter().find(|m| m.index == t)?;
    let name = ui.slot_names.get(&(t, k)).cloned().or_else(|| m.slots.get(usize::from(k))?.as_ref().map(crate::project_plugins::slot_title))?;
    Some(crate::plugin_window::ChannelInfo {
        misc: None,
        name,
        target: m.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| if t == 0 { "Master".into() } else { format!("Insert {t}") }),
        enabled: m.slot_on[usize::from(k)],
        volume: 0.8,
        pan: 0.0,
        pitch_cents: 0,
        range: 2,
        track: t,
        max_track: t,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new(None);
        app.state.project = crate::app::test_project();
        app
    }

    #[test]
    fn mixer_insert_copy_paste_routes_and_effects() {
        use os_model::target;
        use winit::keyboard::Key;
        let mut a=app();a.reset_hosted();
        a.plugins.effects=crate::plugins::builtin_effects();
        let src=&mut a.state.project.mixer[2];
        src.name=Some("Hi-hat".into());src.color=Some(Rgb::new(40,160,190));
        src.routes=vec![5,6];src.send_levels=vec![(5,0.65),(6,0.)];
        src.volume=Some(0.63);src.pan=Some(-0.25);src.strip=[15000,22000,12000,23000,34000,4,2];
        src.slots[1]=Some(os_model::SlotPlugin{name:crate::plugins::EQ_FILE_NAME.into(),state:lemon_parametric_eq_2::params::default_state()});
        src.slot_mix[1]=0.72;
        a.sync_effects();
        let oldkey=crate::plugins::fx_key(2,1);
        let plugin=a.plugins.hosted[&oldkey].plugin.clone();
        let id=plugin.lock().unwrap().param_info(0).unwrap().id;
        {
            let mut p=plugin.lock().unwrap();assert!(p.activate(48000));assert!(p.set_param(id,0.7));
            p.process_fx(48000,&mut [0.;64],&mut [0.;64]);p.idle();
        }
        let edited_value=plugin.lock().unwrap().param_value(id).unwrap();
        assert!((edited_value-0.7).abs()<0.01,"edit {edited_value}");
        a.state.selected_mixer=2;
        assert!(key(&mut a,&Key::Character("c".into()),true,false));
        let clipboard=a.state.mixer.clipboard.clone().unwrap();
        a.state.project.automations.push(os_flp::project::new_automation(100,target::mixer(5,target::MIXER_VOLUME),0.5,4.));
        let count=inserts(&a.state);
        hit(&mut a,MHit::InsertBefore(3));
        assert_eq!(inserts(&a.state),count+1);
        assert_eq!(a.state.selected_mixer,3);
        assert_eq!(*track(&a.state,3).unwrap(),MixerTrack::new(3));
        assert_eq!(track(&a.state,2).unwrap().routes,vec![6,7]);
        assert_eq!(a.state.project.automations[0].target(),Some(target::mixer(6,target::MIXER_VOLUME)));
        assert!(key(&mut a,&Key::Character("v".into()),true,false));
        let dst=track(&a.state,3).unwrap();
        assert_eq!(dst.name,clipboard.track.name);assert_eq!(dst.strip,clipboard.track.strip);
        assert_eq!(dst.routes,vec![6,7]);assert_eq!(dst.send_levels,vec![(6,0.65),(7,0.)]);
        assert_eq!(dst.slots,clipboard.track.slots);assert_eq!(dst.slot_mix,clipboard.track.slot_mix);
        assert_eq!(a.state.project.automations.len(),1);
        let destplugin=&a.plugins.hosted[&crate::plugins::fx_key(3,1)].plugin;
        assert!(!std::sync::Arc::ptr_eq(&plugin,destplugin));
        assert!((destplugin.lock().unwrap().param_value(id).unwrap()-edited_value).abs()<0.01);
        crate::playlist::arr_mut(&mut a.state);
        let saved=os_flp::project::create(&a.state.project).unwrap();
        let back=os_flp::project::load(&os_flp::FlpFile::parse(&saved.to_bytes().unwrap()).unwrap()).unwrap();
        assert_eq!(back.mixer[3].routes,vec![6,7]);
        crate::history::undo(&mut a.state);assert_eq!(*track(&a.state,3).unwrap(),MixerTrack::new(3));
        crate::history::redo(&mut a.state);assert_eq!(track(&a.state,3).unwrap().routes,vec![6,7]);
        a.state.selected_mixer=6;
        let before=a.state.project.clone();
        assert!(paste_channel(&mut a,6).unwrap().contains("feedback"));assert_eq!(a.state.project,before);
    }

    #[test]
    fn knobs_bypass_preserves_values_and_undo() {
        let mut a=app();a.state.project.mixer[1].strip=[20000;7];
        let before=a.state.project.mixer[1].clone();
        hit(&mut a,MHit::StripBypass(1));assert!(a.state.project.mixer[1].strip_bypassed);
        let mut expected=before.clone();expected.strip_bypassed=true;assert_eq!(a.state.project.mixer[1],expected);
        crate::history::undo(&mut a.state);assert_eq!(a.state.project.mixer[1],before);
        crate::history::redo(&mut a.state);assert_eq!(a.state.project.mixer[1],expected);
        hit(&mut a,MHit::StripBypass(1));assert_eq!(a.state.project.mixer[1],before);
    }
    #[test]
    fn select_mute_solo() {
        let mut a = app();
        let n = inserts(&a.state);
        assert_eq!(n, 16, "default mixer shows Insert 1-16");
        hit(&mut a, MHit::Strip(3));
        assert_eq!(selected(&a.state), vec![3]);
        a.ctrl = true;
        hit(&mut a, MHit::Strip(5));
        a.ctrl = false;
        assert_eq!(selected(&a.state), vec![5, 3]);
        hit(&mut a, MHit::Strip(2));
        assert_eq!(selected(&a.state), vec![2]);
        hit(&mut a, MHit::Led(4));
        assert!(!track(&a.state, 4).unwrap().enabled);
        hit(&mut a, MHit::Led(4));
        assert!(track(&a.state, 4).unwrap().enabled);
        right(&mut a, MHit::Led(6));
        assert!(a.state.project.mixer.iter().all(|m| m.enabled == (m.index == 0 || m.index == 6)), "solo");
        right(&mut a, MHit::Led(6));
        assert!(a.state.project.mixer.iter().all(|m| m.enabled), "unsolo");
        a.shift = true;
        hit(&mut a, MHit::Strip(7));
        a.shift = false;
        assert_eq!(a.state.playlist.edit.as_ref().map(|e| e.target), Some(EditTarget::MixerName(7)));
        a.state.playlist.edit.as_mut().unwrap().text = "Drums".into();
        crate::playlist::commit_edit(&mut a.state);
        assert_eq!(name(&a.state, 7), "Drums");
    }

    /// Auto select linked modules (spec: ui/settings-choices.md "Miscellaneous: seven more checks, Default template and
    /// Startup project"): a plugin window selects its channel's insert (scrolled into view) or its slot's track.
    #[test]
    fn plugin_windows_select_their_mixer_track() {
        use crate::app::{Scroll, Win};
        let mut a = app();
        assert!(crate::prefs::check(&crate::prefs::AUTO_SELECT_LINKED), "on by default");
        // Instrument plugins still follow Auto select linked modules.
        for c in &mut a.state.project.channels {c.kind=os_model::ChannelKind::Plugin;}
        // Snare (channel 3) is on Insert 4, as in the test FL was checked with.
        select(&mut a.state, 2, true);
        a.select_window_track(3);
        assert_eq!((a.state.selected_mixer, a.state.mixer.sel.clone()), (4, vec![]), "as a plain click on the insert");
        a.state.project.channels[0].mixer_track = 0;
        a.select_window_track(0);
        assert_eq!(a.state.selected_mixer, 0, "a channel on the master selects the master");
        a.select_window_track(crate::plugins::fx_key(5, 2));
        assert_eq!(a.state.selected_mixer, 5, "an effect selects its slot's track");
        a.state.project.channels[1].mixer_track = 40;
        a.select_window_track(1);
        assert_eq!(a.state.selected_mixer, 5, "no such track: nothing changes");
        // Scrolled into the mixer's view.
        let Ok(mut c) = os_ui::Canvas::new(1920, 1080, 1.0) else { return };
        let ws = &mut a.state.wins[Win::Mixer.slot().unwrap()];
        (ws.maxed, ws.rect) = (false, os_ui::Rect::new(100.0, 100.0, 560.0, 400.0));
        a.state.z = vec![Win::Mixer];
        a.paint(&mut c, None);
        let view = a.bar(Scroll::MixerX).expect("mixer scroll bar").view;
        let w = crate::views::mixer::STRIP_W;
        assert!(view < 16.0 * w, "the mixer shows part of the inserts");
        a.state.project.channels[4].mixer_track = 16;
        a.select_window_track(4);
        assert_eq!(a.state.selected_mixer, 16);
        assert!((a.state.mixer_scroll + view - 16.0 * w).abs() < 0.5, "Insert 16 at the right edge: {}", a.state.mixer_scroll);
        a.select_window_track(2);
        assert_eq!((a.state.selected_mixer, a.state.mixer_scroll), (3, 2.0 * w), "Insert 3 at the left edge");
        a.select_window_track(0);
        assert_eq!((a.state.selected_mixer, a.state.mixer_scroll), (0, 2.0 * w), "the master does not scroll");
        // Off: the selection is left alone.
        crate::prefs::set_check(&crate::prefs::AUTO_SELECT_LINKED, false);
        a.select_window_track(3);
        assert_eq!(a.state.selected_mixer, 0);
    }

    #[test]
    fn sample_import_and_settings_preserve_mixer_navigation() {
        let mut a=app();
        crate::prefs::set_check(&crate::prefs::AUTO_SELECT_LINKED,true);
        let path=std::env::temp_dir().join(format!("lemon-sample-routing-{}.wav",std::process::id()));
        os_engine::wav::write_f32_stereo(&mut std::fs::File::create(&path).unwrap(),48000,&[0.1;960]).unwrap();
        a.state.selected_mixer=12;
        a.state.mixer_scroll=800.;
        let before=a.state.project.channels.len();
        crate::playlist::add_audio_clip(&mut a.state,&path.to_string_lossy(),0,0).unwrap();
        assert_eq!(a.state.project.channels[before].mixer_track,0);
        for kind in [os_model::ChannelKind::AudioClip,os_model::ChannelKind::Sampler] {
            a.state.project.channels[before].kind=kind;
            for route in [0,2,15] {
                a.state.project.channels[before].mixer_track=route;
                a.select_window_track(before);
                assert_eq!((a.state.selected_mixer,a.state.mixer_scroll),(12,800.));
                assert_eq!(a.state.project.channels[before].mixer_track,route);
            }
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn routing() {
        let mut a = app();
        select(&mut a.state, 1, false);
        hit(&mut a, MHit::Route(5));
        assert_eq!(track(&a.state, 1).unwrap().routes, vec![0, 5]);
        assert_eq!(send(&a.state, 1, 5), Some(0.8));
        a.ctrl = true;
        hit(&mut a, MHit::Route(6));
        a.ctrl = false;
        assert_eq!(track(&a.state, 1).unwrap().routes, vec![6], "route only");
        a.shift = true;
        hit(&mut a, MHit::Route(7));
        a.shift = false;
        assert_eq!(send(&a.state, 1, 7), Some(0.0), "sidechain");
        hit(&mut a, MHit::Unroute(7));
        assert_eq!(send(&a.state, 1, 7), None);
        assert!(receives(&a.state, 6));
        // 6 -> 1 would loop.
        select(&mut a.state, 6, false);
        assert!(hit(&mut a, MHit::Route(1)).is_some_and(|m| m.contains("loop")));
        assert!(!track(&a.state, 6).unwrap().routes.contains(&1));
        assert_eq!(level_text("Volume", 0.8), "Volume: 100%  0.0dB  1.00");
        assert_eq!(level_text("To Insert 11", 0.0), "To Insert 11: 0%  -INFdB  0.00");
    }

    #[test]
    fn add_move_delete_tracks() {
        let mut a = app();
        let len = a.state.project.mixer.len();
        hit(&mut a, MHit::Add);
        assert_eq!(a.state.project.mixer.len(), len + 1);
        assert_eq!(a.state.selected_mixer, 17);
        track_mut(&mut a.state, 2).unwrap().name = Some("Two".into());
        a.state.project.channels[0].mixer_track = 2;
        run(&mut a, MCmd::MoveRight(2));
        assert_eq!(name(&a.state, 3), "Two");
        assert_eq!(a.state.project.channels[0].mixer_track, 3, "channels follow their track");
        run(&mut a, MCmd::Delete(1));
        assert_eq!(name(&a.state, 2), "Two", "later tracks move left");
        assert_eq!(a.state.project.channels[0].mixer_track, 2);
        assert_eq!(a.state.project.mixer.len(), len + 1, "a fresh track fills the end");
        run(&mut a, MCmd::InsertOne(2));
        assert_eq!(name(&a.state, 3), "Two");
        assert!(a.state.project.mixer.iter().enumerate().all(|(i, m)| usize::from(m.index) == i), "indices stay in order");
    }

    #[test]
    fn slot_commands_without_plugins() {
        let mut a = app();
        a.plugins.effects = vec![crate::plugin_db::fake("Test FX", os_host::Kind::Effect)];
        select(&mut a.state, 2, false);
        run(&mut a, MCmd::AddEffect(0, 0));
        assert_eq!(slot_name(&a.state, 2, 0).as_deref(), Some("Test FX"));
        run(&mut a, MCmd::SlotMove(0, 1));
        assert_eq!(slot_name(&a.state, 2, 1).as_deref(), Some("Test FX"));
        hit(&mut a, MHit::SlotLed(1));
        assert!(!track(&a.state, 2).unwrap().slot_on[1]);
        run(&mut a, MCmd::SlotDelete(1));
        assert_eq!(slot_name(&a.state, 2, 1), None);
        assert!(a.state.mixer.pending.iter().any(|r| matches!(r, FxReq::Create { track: 2, slot: 0, index: 0, .. })));
    }

    /// spec: audio/sidechain.md "Hosted plugins' input buses": Sidechain input ▸ lists None and the tracks routed into
    /// the track, in track order, and writes the pick into the slot's wrapper state as FL does; an effect that is not
    /// a wrapped plugin says so.
    #[test]
    fn reorder_preserves_sidechain_source_and_automation() {
        use os_flp::wrapper;
        let mut a=app();
        track_mut(&mut a.state,2).unwrap().routes=vec![5];track_mut(&mut a.state,3).unwrap().routes=vec![5];
        let reference=os_model::PluginRef{format:"clap".into(),name:"Comp".into(),path:"Comp.clap".into(),..Default::default()};
        let state=wrapper::set_input_port(&wrapper::encode(None,&reference),1,1).unwrap();
        track_mut(&mut a.state,5).unwrap().slots[0]=Some(os_model::SlotPlugin{name:wrapper::WRAPPER.into(),state});
        a.state.project.init_controls.push(os_model::InitControl{target:os_model::target::mixer(2,os_model::target::MIXER_VOLUME),value:123,head:Default::default()});
        reorder(&mut a.state,2,4,true);
        assert_eq!(inputs(&a.state,5),[2,4]);
        assert_eq!(crate::project_plugins::slot_ports(&a.state.project,5,0),(0,2));
        assert_eq!(a.state.project.init_controls.last().unwrap().target,os_model::target::mixer(4,os_model::target::MIXER_VOLUME));
        crate::history::undo(&mut a.state);
        assert_eq!(crate::project_plugins::slot_ports(&a.state.project,5,0),(0,1));
    }

    #[test]
    fn sidechain_input_submenu() {
        use os_flp::wrapper;
        let mut a = app();
        select(&mut a.state, 1, false);
        track_mut(&mut a.state, 3).unwrap().routes = vec![1];
        track_mut(&mut a.state, 2).unwrap().routes = vec![1, 0];
        track_mut(&mut a.state, 2).unwrap().name = Some("Kick".into());
        assert_eq!(inputs(&a.state, 1), [2, 3]);
        let plugin = os_model::PluginRef { format: "clap".into(), name: "Comp".into(), path: "C:/Comp.clap".into(), ..Default::default() };
        track_mut(&mut a.state, 1).unwrap().slots[0] = Some(os_model::SlotPlugin { name: wrapper::WRAPPER.into(), state: wrapper::encode(None, &plugin) });
        let labels = |a: &App| sidechain_items(&a.state, 0).iter().map(|i| (i.label.to_string(), i.mark == Mark::Radio(true))).collect::<Vec<_>>();
        assert_eq!(labels(&a), [("None".into(), true), ("1. Kick".into(), false), ("2. Insert 3".into(), false)]);
        assert_eq!(run(&mut a, MCmd::SlotSidechain(0, 2)), None);
        let state = |a: &App| track(&a.state, 1).unwrap().slots[0].as_ref().unwrap().state.clone();
        assert_eq!(wrapper::input_ports(&state(&a)), [0, 2]);
        assert_eq!(wrapper::decode(&state(&a)), Some(plugin));
        assert_eq!(labels(&a)[2].1, true);
        assert_eq!(crate::project_plugins::slot_ports(&a.state.project, 1, 0), (0, 2));
        track_mut(&mut a.state, 1).unwrap().slots[1] = Some(os_model::SlotPlugin { name: "Fruity Limiter".into(), state: vec![] });
        assert!(run(&mut a, MCmd::SlotSidechain(1, 1)).is_some());
    }

    // spec: ui/mixer-track-commands.md §1
    #[test]
    fn route_to_free_track() {
        let mut a = app();
        // Kick 1, Clap 2, Hat 3, Snare 4, Pluck 5; Snare joins Kick on 1, Pluck uses the master.
        a.state.project.channels[3].mixer_track = 1;
        a.state.project.channels[4].mixer_track = 0;
        a.state.project.channels[2].color = Some(Rgb::new(10, 20, 30));
        crate::rack::run(&mut a, crate::rack::RCmd::AssignFree(2));
        assert_eq!(a.state.project.channels[2].mixer_track, 3, "alone on its track: stays");
        assert_eq!(name(&a.state, 3), "Hat");
        assert_eq!(track(&a.state, 3).unwrap().color, Some(Rgb::new(10, 20, 30)));
        assert_eq!(a.state.selected_mixer, 3);
        crate::rack::run(&mut a, crate::rack::RCmd::AssignFree(3));
        assert_eq!(a.state.project.channels[3].mixer_track, 4, "shared: the lowest free insert");
        assert_eq!(name(&a.state, 4), "Snare");
        crate::rack::run(&mut a, crate::rack::RCmd::AssignFree(4));
        assert_eq!(a.state.project.channels[4].mixer_track, 5, "on the master: a free insert");
        assert_eq!(a.state.selected_mixer, 5);
        // No free insert left: a new track at the end.
        for (i, c) in a.state.project.channels.iter_mut().enumerate() {
            c.mixer_track = if i == 0 { 1 } else { 1 + i as u16 };
        }
        // Channels on every insert, one more sharing Insert 1.
        let more: Vec<_> = (6..=16).chain([1]).map(|t| os_model::Channel { mixer_track: t, ..a.state.project.channels[0].clone() }).collect();
        a.state.project.channels.extend(more);
        let len = a.state.project.mixer.len();
        crate::rack::run(&mut a, crate::rack::RCmd::AssignFree(16));
        assert_eq!(a.state.project.mixer.len(), len + 1);
        assert_eq!(a.state.project.channels[16].mixer_track, 17);
    }

    // spec: ui/mixer-track-commands.md §2
    #[test]
    fn channel_routing() {
        let mut a = app();
        a.state.rack.sel = [0, 2].into_iter().collect();
        run(&mut a, MCmd::RouteChannels { dst: 14, spread: false });
        assert_eq!(a.state.project.channels[0].mixer_track, 14);
        assert_eq!(a.state.project.channels[2].mixer_track, 14);
        assert_eq!(name(&a.state, 14), "Kick", "named after the first channel");
        assert_eq!(name(&a.state, 3), "Insert 3", "the track left keeps its name");
        assert_eq!(a.state.selected_mixer, 14);
        a.state.rack.sel = [1, 3, 4].into_iter().collect();
        run(&mut a, MCmd::RouteChannels { dst: 15, spread: true });
        let tracks: Vec<u16> = a.state.project.channels.iter().map(|c| c.mixer_track).collect();
        assert_eq!(tracks, vec![14, 15, 14, 16, 5], "past the last insert the channel stays");
        assert_eq!((name(&a.state, 15), name(&a.state, 16)), ("Clap".to_string(), "Snare".to_string()));
        // Nothing selected in the rack: the piano roll's channel.
        a.state.rack.sel.clear();
        a.state.selected_channel = 4;
        run(&mut a, MCmd::RouteChannels { dst: 7, spread: false });
        assert_eq!(a.state.project.channels[4].mixer_track, 7);
    }

    // spec: ui/mixer-track-commands.md §3
    #[test]
    fn select_by_routing() {
        let mut a = app();
        a.state.project.channels[3].mixer_track = 2;
        run(&mut a, MCmd::SelectChannels(2));
        assert_eq!(a.state.rack.sel.iter().copied().collect::<Vec<_>>(), vec![1, 3]);
        track_mut(&mut a.state, 5).unwrap().routes = vec![3];
        track_mut(&mut a.state, 6).unwrap().routes = vec![3];
        run(&mut a, MCmd::SelectSources(3));
        assert_eq!(a.state.mixer.sel, vec![5, 6]);
        assert_eq!(a.state.selected_mixer, 5);
        run(&mut a, MCmd::SelectDest(15));
        assert_eq!(a.state.mixer.sel, vec![0]);
        assert!(run(&mut a, MCmd::SelectSources(9)).is_some_and(|m| m.contains("No tracks")));
        run(&mut a, MCmd::SelectAll);
        assert_eq!(a.state.mixer.sel, (0..=16).collect::<Vec<u16>>());
    }

    // spec: ui/mixer-track-commands.md §4, §6
    #[test]
    fn trim_unused_tracks() {
        let mut a = app();
        a.state.project.channels[3].mixer_track = 5;
        a.state.project.channels[4].mixer_track = 9;
        track_mut(&mut a.state, 9).unwrap().name = Some("Nine".into());
        // 12 holds an effect, 11 feeds nothing but 9 sends to 13.
        track_mut(&mut a.state, 12).unwrap().slots[0] = Some(os_model::SlotPlugin { name: "FX".into(), state: Vec::new() });
        track_mut(&mut a.state, 9).unwrap().routes = vec![13];
        assert_eq!(trim_list(&a.state, false), vec![4, 6, 7, 8, 10, 11, 14, 15, 16]);
        assert_eq!(trim_list(&a.state, true), vec![4, 6, 7, 8, 10, 11, 12, 14, 15, 16]);
        let len = a.state.project.mixer.len();
        run(&mut a, MCmd::Trim { non_empty: true });
        assert_eq!(a.state.project.mixer.len(), len - 10);
        assert!(a.state.project.mixer.iter().enumerate().all(|(i, m)| usize::from(m.index) == i));
        let tracks: Vec<u16> = a.state.project.channels.iter().map(|c| c.mixer_track).collect();
        assert_eq!(tracks, vec![1, 2, 3, 4, 5]);
        assert_eq!(name(&a.state, 5), "Nine");
        assert_eq!(track(&a.state, 5).unwrap().routes, vec![6], "its send follows 13 to 6");
        assert!(a.state.project.mixer.last().unwrap().routes.is_empty(), "the hidden last track stays");
        assert!(trim_list(&a.state, true).is_empty());
        // Nothing uses any insert: one stays.
        for c in a.state.project.channels.iter_mut() {
            c.mixer_track = 0;
        }
        track_mut(&mut a.state, 5).unwrap().routes = vec![0];
        assert_eq!(trim_list(&a.state, true), vec![2, 3, 4, 5, 6]);
    }

    // spec: ui/mixer-track-commands.md §5
    #[test]
    fn solo_and_alt_solo() {
        let mut a = app();
        track_mut(&mut a.state, 5).unwrap().routes = vec![3];
        let on = |a: &App| a.state.project.mixer.iter().filter(|m| m.enabled).map(|m| m.index).collect::<Vec<_>>();
        run(&mut a, MCmd::Solo(3));
        assert_eq!(on(&a), vec![0, 3], "Solo mutes the track sending into 3");
        run(&mut a, MCmd::Solo(3));
        assert_eq!(on(&a).len(), a.state.project.mixer.len());
        run(&mut a, MCmd::AltSolo(3));
        assert_eq!(on(&a), vec![0, 3, 5]);
        run(&mut a, MCmd::AltSolo(3));
        assert_eq!(on(&a).len(), a.state.project.mixer.len());
        run(&mut a, MCmd::Solo(5));
        assert_eq!(on(&a), vec![0, 3, 5], "Solo keeps where 5 sends");
    }

    #[test]
    fn mixer_keys() {
        use winit::keyboard::Key;
        let mut a = app();
        select(&mut a.state, 10, false);
        a.state.rack.sel = [1].into_iter().collect();
        assert!(key(&mut a, &Key::Character("l".into()), true, false));
        assert_eq!(a.state.project.channels[1].mixer_track, 10, "Ctrl+L");
        assert!(key(&mut a, &Key::Character("a".into()), true, false));
        assert_eq!(a.state.mixer.sel.len(), 17, "Ctrl+A");
        select(&mut a.state, 4, false);
        assert!(key(&mut a, &Key::Character("s".into()), false, true));
        assert!(!track(&a.state, 7).unwrap().enabled, "Alt+S");
        assert!(!key(&mut a, &Key::Character("q".into()), true, false));
    }

    /// Moved and deleted tracks take their effects to their new numbers through saves (spec: flp/events/mixer.md
    /// "Removing tracks"): each slot's window record (event 212), marked here in the saved file with the track it was
    /// saved on, follows its track, and the slots read back where the tracks went.
    #[test]
    fn moved_tracks_keep_their_effects_in_the_file() {
        use os_flp::{ChunkBody, FlpFile, Value};
        use os_model::SlotPlugin;
        let mut a = app();
        (a.state.file, a.state.path) = (None, None);
        let fx = |name: &str, s: u8| Some(SlotPlugin { name: name.into(), state: vec![s; 4] });
        track_mut(&mut a.state, 2).unwrap().slots[0] = fx("Test FX", 2);
        track_mut(&mut a.state, 3).unwrap().slots[0] = fx("Test FX", 3);
        track_mut(&mut a.state, 4).unwrap().slots[1] = fx("Other FX", 4);
        let path = std::env::temp_dir().join(format!("lemon-mixer-moves-{}.flp", std::process::id()));
        let saved = |a: &mut App| {
            a.save_to(path.clone());
            assert!(a.state.message.as_deref().is_some_and(|m| m.starts_with("Saved")), "{:?}", a.state.message);
        };
        // The mixer's slot records (after the first track record, event 236): track, slot and mark (byte 40, which
        // Lemon Studio does not read).
        let records = || {
            let file = FlpFile::parse(&std::fs::read(&path).unwrap()).unwrap();
            let ev = file.chunks.into_iter().find_map(|c| match c.body {
                ChunkBody::Events(e) => Some(e),
                _ => None,
            });
            let ev = ev.unwrap();
            let first = ev.iter().position(|e| e.id == 236).unwrap();
            let le = |d: &[u8], at: usize| u32::from_le_bytes(d[at..at + 4].try_into().unwrap());
            ev[first..].iter().filter(|e| e.id == 212).filter_map(|e| e.data()).map(|d| (le(d, 0), le(d, 4), d[40])).collect::<Vec<_>>()
        };
        saved(&mut a);
        let mut file = FlpFile::parse(&std::fs::read(&path).unwrap()).unwrap();
        for c in &mut file.chunks {
            if let ChunkBody::Events(ev) = &mut c.body {
                for e in ev.iter_mut().filter(|e| e.id == 212) {
                    if let Value::Var { data, .. } = &mut e.value {
                        data[40] = data[0];
                    }
                }
            }
        }
        std::fs::write(&path, file.to_bytes().unwrap()).unwrap();
        assert_eq!(records(), vec![(2, 0, 2), (3, 0, 3), (4, 1, 4)]);
        let mut a = App::new(None);
        a.open(path.clone());
        run(&mut a, MCmd::MoveRight(2));
        saved(&mut a);
        assert_eq!(records(), vec![(2, 0, 3), (3, 0, 2), (4, 1, 4)], "2 and 3 swap places with their effects");
        // Saved over again (the file holds the tracks in their new order now): deleting 2 moves the later ones left.
        run(&mut a, MCmd::Delete(2));
        saved(&mut a);
        assert_eq!(records(), vec![(2, 0, 2), (3, 1, 4)]);
        let mut opened = App::new(None);
        opened.open(path.clone());
        let _ = std::fs::remove_file(&path);
        let slots = |t: u16| track(&opened.state, t).unwrap().slots.clone();
        assert_eq!((slots(2)[0].clone(), slots(3)[1].clone()), (fx("Test FX", 2), fx("Other FX", 4)));
        assert!(slots(4).iter().all(Option::is_none));
    }

    /// Open audio editor and record: a Tesla set up as an audio logger (On play, 5 minutes, Append, slaved, armed). An
    /// empty Tesla on the track is used; one holding a sample is left alone for a new one (spec:
    /// plugins/tesla/README.md "Recording into Tesla from Lemon Studio").
    #[test]
    fn open_audio_editor_and_record_sets_up_a_logger() {
        use tesla::state::{from_plugin_state, plugin_state, Saved, WHEN_ON_PLAY};
        let mut a = app();
        a.plugins.effects = crate::plugins::builtin_effects();
        let logger = |b: &[u8]| from_plugin_state(b).map(|s| (s.rec.when, s.rec.max, s.rec.append, s.rec.slave, s.rec.armed));
        let want = Some((WHEN_ON_PLAY, 3, true, true, true));
        assert!(track_items(&App::new(None).state, 2).iter().any(|i| i.label == "Open audio editor and record" && i.enabled));
        run(&mut a, MCmd::AudioEditorRecord(2));
        let Some(FxReq::Create { track: 2, slot: 0, index, state: Some(st) }) = a.state.mixer.pending.first().cloned() else {
            panic!("{:?}", a.state.mixer.pending)
        };
        assert_eq!(logger(&st), want);
        // An empty Tesla already there (hosted with its defaults) is set up in place and shown.
        a.state.mixer.pending.clear();
        a.plugins.create_effect(2, 0, index, None).unwrap();
        let key = crate::plugins::fx_key(2, 0);
        assert_eq!(logger(&a.plugins.state(key).unwrap()).map(|r| r.4), Some(false));
        run(&mut a, MCmd::AudioEditorRecord(2));
        assert_eq!(a.state.mixer.pending, vec![FxReq::Show { track: 2, slot: 0 }]);
        assert_eq!(logger(&a.plugins.state(key).unwrap()), want);
        // One holding a sample: a new Tesla in the next free slot.
        a.state.mixer.pending.clear();
        let sample = tesla::sample::Sample::from_channels(44100, vec![vec![0.5; 100]], tesla::sample::Format::Int16);
        assert!(a.plugins.set_state(key, &plugin_state(&Saved { sample, ..Saved::default() })));
        run(&mut a, MCmd::AudioEditorRecord(2));
        assert!(matches!(a.state.mixer.pending.as_slice(), [FxReq::Create { track: 2, slot: 1, state: Some(_), .. }]));
        assert_eq!(from_plugin_state(&a.plugins.state(key).unwrap()).map(|s| s.sample.frames()), Some(100), "left alone");
    }

    /// Remove all (audio editor) instances, in Mixer options and TOOLS > Macros: every Tesla (a project's Edison slots
    /// too) goes after a question; with none, a message (spec: plugins/tesla/README.md "Recording into Tesla from
    /// Lemon Studio").
    #[test]
    fn remove_all_audio_editors_asks_first() {
        let mut a = app();
        a.plugins.effects = crate::plugins::builtin_effects();
        let label = "Remove all (audio editor) instances";
        assert!(options_menu(&a.state, 0.0, 0.0).items.iter().any(|i| i.label == label && i.enabled));
        assert_eq!(run(&mut a, MCmd::RemoveTeslas).as_deref(), Some("No audio editors in the mixer"));
        a.audio_editor(0, false);
        a.audio_editor(3, false);
        track_mut(&mut a.state, 4).unwrap().slots[2] = Some(os_model::SlotPlugin { name: crate::plugins::EDISON_FILE_NAME.into(), state: Vec::new() });
        a.state.mixer.pending.clear();
        assert_eq!(run(&mut a, MCmd::RemoveTeslas), None);
        let Some(Popup { items, .. }) = a.state.playlist.popup.clone() else { panic!("no question") };
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_ref()).collect();
        assert_eq!(labels, ["Remove 3 audio editors? Their audio is lost.", "Remove", "Cancel"]);
        assert_eq!(teslas(&a.state).len(), 3, "nothing goes before the answer");
        assert_eq!(run(&mut a, MCmd::RemoveTeslasNow).as_deref(), Some("Removed 3 audio editors"));
        assert!(teslas(&a.state).is_empty());
        assert_eq!(a.state.mixer.pending.iter().filter(|r| matches!(r, FxReq::Delete { .. })).count(), 3);
        use crate::views::menus::{item_at, REMOVE_TESLAS_LABEL, TOOLS_MENU};
        let i = (0..200).find(|&i| item_at(TOOLS_MENU, i).is_some_and(|it| it.label == REMOVE_TESLAS_LABEL)).expect("in TOOLS");
        a.menu_item(TOOLS_MENU, i);
        assert_eq!(a.state.message.as_deref(), Some("No audio editors in the mixer"));
    }
}

pub(crate) fn inspector_left() -> bool {
    crate::prefs::text("mixer_inspector_left").is_none_or(|v| v == "true")
}

pub fn has_separator(s: &State, n: u16) -> bool {
 let names: Vec<String> = crate::prefs::text("mixer_separators").and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default();
 s.project.mixer.get(usize::from(n)).is_some_and(|m| m.separator) || names.contains(&name(s, n))
}

/// Learn-button menu owns musical choices; the knob menu owns voice range.
pub fn tune_menu(s:&State,n:u16,x:f32,y:f32,voice:bool)->Popup {
 let t=track(s,n).map_or([0;4],|t|t.tune);
 let choice=|label:&str,k:u8,v:u16|PItem::new(label.to_owned(),Cmd::Mixer(MCmd::TuneSetting(n,k,v))).mark(Mark::Radio(t[k as usize]==v));
 let items=if voice {vec![choice("Male (lower range)",3,0),choice("Female (higher range)",3,1),PItem::sep(),PItem::new("Reset strength to 0%",Cmd::Reset(crate::knobs::Control::Knob(KnobId::Strip(n,8))))]}else{
  vec![PItem::sub("Key",os_dsp::snaptune::KEYS.iter().enumerate().map(|(i,l)|choice(l,1,i as u16)).collect()),
       PItem::sub("Scale",os_dsp::snaptune::SCALES.iter().enumerate().map(|(i,l)|choice(l,2,i as u16)).collect())]
 };
 Popup{x,y,items,sub:None,scroll:[0;2], cascade: Vec::new()}
}

/// Logical user slot numbers skip the reserved, independently bypassable EQ.
pub fn user_slot(i:usize)->u8 { if i<10 {i as u8}else{(i+1) as u8} }
pub fn user_slot_index(k:u8)->usize {if k<10 {k as usize}else{(k-1) as usize}}
pub fn slot_count(s:&State,t:u16)->usize {
    let filled=track(s,t).and_then(|m|(0..63).rfind(|i|m.slots[user_slot(*i) as usize].is_some())).map_or(10,|i|i+2);
    filled.max(s.mixer.slots_shown.get(&t).copied().unwrap_or(10)).clamp(10,63)
}
pub fn scroll_slots(s:&mut State,delta:f32) {
    let t=s.selected_mixer;let max=slot_count(s,t).saturating_sub(10);
    let old=s.mixer.slot_scroll.get(&t).copied().unwrap_or(0).min(max);
    s.mixer.slot_scroll.insert(t,(old as i32-delta.signum() as i32).clamp(0,max as i32) as usize);
}
