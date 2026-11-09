// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! The plugin editor: a child of the host's window (Win32, X11 on Linux, an NSView on macOS) that paints `view` with
//! the os-ui canvas, and the editor logic (platform independent): token / fader / knob drags with the cursor held in
//! place, wheel and reset gestures, mute / solo, A/B compare, the options and menus. Edits show and sound at once
//! (`os_plugkit::vals`).
//!
// spec: plugins/lemon-parametric-eq-2.md (§5 hints, §6 menus, §7 interaction), plugins/host-link.md

use std::any::Any;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::Arc;

use nih_plug::prelude::*;
use os_ui::Rect;
use os_engine::effects::eq::{self, has_level, Band, Settings, BANDS, MASK_BYTE};
use os_plugkit::vals::Vals;
use os_plugkit::MenuDef;
use os_ui::kit::menu::{Mark, MenuItem, MenuStyle};

use crate::params::{self, byte, flag, freq_id, level_id, order_id, set_byte, set_flag, type_id, width_id, EqParams, Opt, MAIN_ID, ORDER_NAMES, TYPE_NAMES};
use crate::view::{self, Hit, Target, ViewState, MAIN_FADER};
use crate::Shared;

/// Bounded input-driven refresh, independent of low-priority OS timer/paint delivery.
#[derive(Default)]
pub struct InputFrames { origin: Option<std::time::Instant>, next: std::time::Duration }
impl InputFrames {
    pub fn due(&mut self) -> bool { let start=*self.origin.get_or_insert_with(std::time::Instant::now);self.due_at(start.elapsed()) }
    pub fn due_at(&mut self,now:std::time::Duration)->bool {
        if now < self.next {return false;}
        self.next=now+crate::analyzer::FRAME;true
    }
}

/// "Lock spare state" (Options menu) is not saved; it lasts while the plugin instance lives.
static LOCK_SPARE: AtomicBool = AtomicBool::new(false);

pub struct EqEditor {
    pub params: Arc<EqParams>,
    pub shared: Arc<Shared>,
    /// DPI scale as f32 bits.
    pub scale: AtomicU32,
    /// The open editor window (0 = closed): parameter changes post a repaint to it.
    #[cfg_attr(not(windows), allow(dead_code))]
    hwnd: Arc<AtomicIsize>,
    /// Linux and macOS: the open editor's thread waker, for the same.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    slot: os_plugkit::gui::Slot,
}

impl EqEditor {
    pub fn new(params: Arc<EqParams>, shared: Arc<Shared>) -> Self {
        EqEditor {
            params,
            shared,
            scale: AtomicU32::new(1f32.to_bits()),
            hwnd: Arc::new(AtomicIsize::new(0)),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            slot: Default::default(),
        }
    }

    fn notify(&self) {
        #[cfg(windows)]
        win32::post_changed(self.hwnd.load(Ordering::Acquire), &self.shared);
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        os_plugkit::gui::notify(&self.slot);
    }
}

impl Editor for EqEditor {
    fn spawn(&self, parent: ParentWindowHandle, context: Arc<dyn GuiContext>) -> Box<dyn Any + Send> {
        let scale = f32::from_bits(self.scale.load(Ordering::Relaxed));
        match parent {
            #[cfg(windows)]
            ParentWindowHandle::Win32Hwnd(hwnd) => {
                Box::new(win32::open(hwnd, scale, self.params.clone(), self.shared.clone(), context, self.hwnd.clone()))
            }
            #[cfg(target_os = "linux")]
            ParentWindowHandle::X11Window(window) => Box::new(gui::open(window, scale, self.params.clone(), self.shared.clone(), context, &self.slot)),
            // macOS hosts set no scale (sizes are points): the editor paints at its window's backing scale.
            #[cfg(target_os = "macos")]
            ParentWindowHandle::AppKitNsView(view) => {
                let scale = os_plugkit::gui::backing_scale(view as usize);
                Box::new(gui::open(view as usize, scale, self.params.clone(), self.shared.clone(), context, &self.slot))
            }
            // Elsewhere no editor opens yet.
            _ => {
                let _ = (context, scale);
                Box::new(())
            }
        }
    }

    fn size(&self) -> (u32, u32) {
        (self.shared.editor_w.load(Ordering::Relaxed),self.shared.editor_h.load(Ordering::Relaxed))
    }

    fn set_scale_factor(&self, factor: f32) -> bool {
        self.scale.store(factor.to_bits(), Ordering::Relaxed);
        true
    }

    fn param_value_changed(&self, _id: &str, _normalized_value: f32) {
        self.notify();
    }
    fn param_modulation_changed(&self, _id: &str, _modulation_offset: f32) {
        self.notify();
    }
    fn param_values_changed(&self) {
        self.notify();
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Laws (spec §7)

/// Pixels of drag for the whole range.
const FREQ_PX: f32 = 1024.0;
const LEVEL_PX: f32 = 640.0;
const WIDTH_PX: f32 = 256.0;
/// Pixels per step of a type / order drag.
const STEP_PX: f32 = 16.0;
const FINE: f32 = 0.25;

/// Note names in the Key menu's order and their semitone above C.
const KEY_NAMES: [(&str, i32); 12] = [("A", 9), ("A#", 10), ("B", 11), ("C", 0), ("C#", 1), ("D", 2), ("D#", 3), ("E", 4), ("F", 5), ("F#", 6), ("G", 7), ("G#", 8)];

/// Frequency of a note (`semitone` above C, scientific octave; A4 = 440 Hz).
pub fn note_hz(semitone: i32, octave: i32) -> f64 {
    440.0 * 2f64.powf(f64::from(12 * (octave + 1) + semitone - 69) / 12.0)
}

// ---------------------------------------------------------------------------------------------------------------
// Menus

/// What a menu entry does.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    None,
    ResetBand(usize),
    DeleteBand(usize),
    Dynamic(usize),
    Spectral(usize),
    AddBand(u8),
    SetType(usize, i32),
    Route(usize,u8),
    SetOrder(usize, i32),
    SetHz(usize, f64),
    Reset(String),
    CopyValue(String),
    PasteValue(String),
    TypeIn(String),
    /// "Create automation clip" (host link).
    Automate(String),
    /// Toggle a flag of the flags byte.
    Flag(u8),
    /// Set an index option.
    Opt(Opt, i32),
    /// Set a state byte.
    Byte(usize, u8),
    /// HQ mode: 3 HQ+, 1 Legacy, 2 Enhanced (stored as 1 + the Enhanced flag).
    Hq(u8),
    /// Toggle "Enhanced frequency" (stored inverted).
    Enhanced,
    LockSpare,
}

/// Which menu is open.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuReq {
    Graph,
    Token(usize),
    Param(String),
    Options,
    Lin,
    Hq,
    Phones,
    Analyzer,
}

fn radio(m: &mut MenuDef<Cmd>, label: &str, on: bool, c: Cmd) {
    m.add(MenuItem::item(label).mark(Mark::Radio(on)), c);
}

fn toggle(b: &mut [u8], f: u8) {
    let on = flag(b, f);
    set_flag(b, f, !on);
}

fn check(m: &mut MenuDef<Cmd>, label: &str, on: bool, c: Cmd) {
    m.add(MenuItem::item(label).mark(Mark::Check(on)), c);
}

/// A check entry for a setting nothing reads yet: greyed out.
fn check_off(m: &mut MenuDef<Cmd>, label: &str, on: bool) {
    m.add(MenuItem::item(label).mark(Mark::Check(on)).enabled(false), Cmd::None);
}

/// `m` with every entry greyed out (settings nothing reads yet).
fn greyed(mut m: MenuDef<Cmd>) -> MenuDef<Cmd> {
    m.items = m.items.into_iter().map(|it| it.enabled(false)).collect();
    m
}

fn opt_menu(extra: &[u8], o: Opt, names: &[&str], first: i32) -> MenuDef<Cmd> {
    let mut m = MenuDef::new(MenuStyle::Context);
    let cur = params::opt(extra, o);
    for (i, n) in names.iter().enumerate() {
        radio(&mut m, n, cur == i as i32 + first, Cmd::Opt(o, i as i32 + first));
    }
    m
}

fn key_menu(k: usize) -> MenuDef<Cmd> {
    let mut m = MenuDef::new(MenuStyle::Context);
    for (name, semi) in KEY_NAMES {
        let mut sub = MenuDef::new(MenuStyle::Context);
        for oct in 0..=10 {
            // Octaves count from C (scientific pitch): A0 = 27.5 Hz.
            let hz = note_hz(semi, oct);
            if (20.0..=20000.0).contains(&hz) {
                sub.add(MenuItem::item(&format!("{name}{oct} - {hz:.1}Hz")), Cmd::SetHz(k, hz));
            }
        }
        m.submenu(name, sub, Cmd::None);
    }
    m
}

// ---------------------------------------------------------------------------------------------------------------
// Editor logic

/// Something the window has to do after an event.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Repaint,
    /// Hide the cursor and hold it in place (drag).
    Lock,
    Menu(MenuReq, f32, f32),
    TypeIn(String, f32, f32),
    /// Ask the host for an automation clip on the parameter at this list position (host link).
    Automate(usize),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Btn {
    Left,
    Right,
    Middle,
}

#[derive(Clone, Debug)]
enum Drag {
    /// A parameter along `px` pixels for its whole (normalised) range, up = more.
    Linear { id: String, px: f32 },
    Linked { ids:Vec<String>,values:Vec<f32>,offset:f32,px:f32 },
    /// Stepped integer parameter: `acc` collects the pixels toward the next step.
    Steps { id: String, acc: f32 },
    /// Hold-to-listen in the detailed controls temporarily overrides the headphone selection.
    Solo { restore: Option<usize> },
    Group { orig:Vec<(usize,Band)>,dx:f32,dy:f32,axis:u8,width:bool },
    Marquee{x0:f32,y0:f32,x:f32,y:f32,create:bool},
}

#[derive(Clone)]
struct Snapshot{settings:Settings,config:eq::dynamic::Config,extra:Vec<u8>,created:u8,selection:u8,selected:usize}
pub struct Logic {
    pub vals: Vals,
    pub params: Arc<EqParams>,
    pub shared: Arc<Shared>,
    pub view: ViewState,
    pub hits: Vec<Hit>,
    drag: Option<Drag>,
    pointer_drag: Option<(f32,f32)>,
    previous_range:[f32;7],
    /// Hint text (a tab before the right-aligned part).
    pub hint: String,
    copied: Option<f32>,
    pub hosted: bool,
    undo:Vec<Snapshot>,redo:Vec<Snapshot>,pending:Option<Snapshot>,
}

impl Logic {
    pub fn new(params: Arc<EqParams>, shared: Arc<Shared>, ctx: Option<Arc<dyn GuiContext>>) -> Self {
        let vals = Vals::new(&*params, params.live.clone(), ctx);
        Logic { vals, params, shared, view: ViewState::default(), hits: Vec::new(), drag: None, hint: String::new(), pointer_drag:None, previous_range:[0.;7], copied: None, hosted: false,undo:vec![],redo:vec![],pending:None }
    }

    fn audition(&mut self, band: Option<usize>) {
        self.view.audition = band;
        self.shared.solo.store(band.map_or(-1, |k| k as i32), Ordering::Relaxed);
    }

    fn read_visibility(&mut self) {
        self.view.hide_band_card = !crate::visibility::band_card();
    }

    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub fn dynamic_values(&self)->eq::dynamic::Config {let mut c=eq::dynamic::Config::default();for i in 0..47{c.set(i,self.vals.plain(&params::dynamic_id(i)));}c}
    pub fn shown_extra(&self)->Vec<u8>{let mut b=self.params.extra_copy();self.dynamic_values().save(&mut b,false);b}
    fn dynamic_meter(&mut self){for (v,m) in self.view.reduction.iter_mut().zip(&self.shared.dynamics){*v=f32::from_bits(m.load(Ordering::Relaxed));}self.view.spectral.resize(256,0.);for(v,m)in self.view.spectral.iter_mut().zip(&self.shared.spectral){*v=f32::from_bits(m.load(Ordering::Relaxed));}for i in 0..7{self.view.trigger[i]=f32::from_bits(self.shared.trigger[i].load(Ordering::Relaxed));self.view.threshold[i]=f32::from_bits(self.shared.threshold[i].load(Ordering::Relaxed));}}
    fn snapshot(&self)->Snapshot{Snapshot{settings:self.shown(),config:self.dynamic_values(),extra:self.params.extra_copy(),created:self.view.created,selection:self.view.selection,selected:self.view.selected}}
    fn remember(&mut self,before:Snapshot){let after=self.snapshot();if before.settings!=after.settings||before.config!=after.config||before.extra!=after.extra||before.created!=after.created{self.undo.push(before);if self.undo.len()>100{self.undo.remove(0);}self.redo.clear();}}
    fn undo_redo(&mut self,redo:bool){self.audition(None);let state=if redo{self.redo.pop()}else{self.undo.pop()};if let Some(state)=state{let current=self.snapshot();for k in 0..7{self.set_band(k,state.settings.bands[k]);}self.vals.edit_plain(MAIN_ID,state.settings.main_db as f32);for i in 0..47{self.vals.edit_plain(&params::dynamic_id(i),state.config.value(i));}self.params.set_extra(state.extra);self.view.created=state.created;self.view.selection=state.selection;self.view.selected=state.selected;self.view.show_dynamics=state.selection!=0;self.view.harsh_open=false;if redo{self.undo.push(current);}else{self.redo.push(current);}}}
    fn linked_ids(&self,id:&str)->Vec<String>{
        for k in 0..7 {
            let field=if id==freq_id(k){Some(0)}else if id==level_id(k){Some(1)}else if id==width_id(k){Some(2)}else{None};
            if let Some(field)=field {if self.view.selection&(1<<k)!=0{return(0..7).filter(|j|self.view.selection&(1<<j)!=0).map(|j|match field{0=>freq_id(j),1=>level_id(j),_=>width_id(j)}).collect();}}
        }
        if let Some(i)=id.strip_prefix("dynamic").and_then(|v|v.parse::<usize>().ok()).filter(|i|*i<28){let k=i/4;if self.view.selection&(1<<k)!=0{return(0..7).filter(|j|self.view.selection&(1<<j)!=0).map(|j|params::dynamic_id(j*4+i%4)).collect();}}
        vec![id.to_string()]
    }
    fn delete_bands(&mut self,mask:u8){if self.view.audition.is_some_and(|k|mask&(1<<k)!=0){self.audition(None);}for k in 0..7{if mask&(1<<k)!=0{self.vals.edit_plain(&type_id(k),0.);self.vals.edit_plain(&params::dynamic_id(k*4),0.);self.vals.edit_plain(&params::dynamic_id(40+k),0.);self.previous_range[k]=0.;}}self.view.selection&=!mask;self.view.created&=!mask;self.view.show_dynamics=self.view.selection!=0;self.view.harsh_open=false;if self.view.selection!=0{self.view.selected=self.view.selection.trailing_zeros()as usize;}}
    fn toggle_dynamics(&mut self,k:usize){let range=self.dynamic_values().bands[k][0];if range.abs()>0.001{self.previous_range[k]=range;self.vals.edit_plain(&params::dynamic_id(k*4),0.);}else{let saved=self.previous_range[k];self.vals.edit_plain(&params::dynamic_id(k*4),if saved.abs()>0.001{saved}else{-6.});if saved.abs()<=0.001{self.vals.edit_plain(&params::dynamic_id(30+k),1.);}}}
    fn toggle_spectral(&mut self,k:usize){let cfg=self.dynamic_values();let active=cfg.bands[k][0].abs()>0.001&&cfg.spectral&(1<<k)!=0;self.vals.edit_plain(&params::dynamic_id(40+k),if active{0.}else{1.});if !active&&cfg.bands[k][0].abs()<=0.001{self.toggle_dynamics(k);}}
    pub fn layout(&self)->view::Layout{view::Layout::of(&self.view)}
    fn visible(&self,k:usize)->bool{let c=self.dynamic_values();view::modern_visible(&self.shown(),&self.view,k)||(self.shown().bands[k].kind!=0&&(c.bands[k][0]!=0.||c.spectral&(1<<k)!=0))}
    fn select(&mut self,k:usize){if self.view.audition.is_some_and(|old|old!=k){self.audition(None);}self.view.selected=k;self.view.selection=1<<k;self.view.show_dynamics=true;self.view.harsh_open=false;}
    fn create_band(&mut self,x:f32,y:f32)->Option<usize>{let s=self.shown();let c=self.dynamic_values();let k=(0..BANDS).find(|&k|s.bands[k].kind==0||(view::modern_available(&s,k)&&self.view.created&(1<<k)==0&&c.bands[k][0]==0.&&c.spectral&(1<<k)==0))?;for i in k*4..k*4+4{self.reset_param(&params::dynamic_id(i));}self.reset_param(&params::dynamic_id(30+k));self.reset_param(&params::dynamic_id(40+k));self.params.edit_extra(|b|{let mut routes=eq::routed::routes(b,false);routes[k]=0;eq::routed::save_routes(b,routes,false);});let l=self.layout();let n=l.n(x);let db=l.db(y).clamp(-18.,18.);let kind=if n<0.04{eq::HIGH_PASS}else if n>0.96{eq::LOW_PASS}else{eq::PEAKING};self.vals.edit_plain(&type_id(k),kind as f32);self.vals.edit_plain(&freq_id(k),n as f32);self.vals.edit_plain(&level_id(k),if has_level(kind){db as f32}else{0.});self.vals.edit_plain(&width_id(k),0.7);self.vals.edit_plain(&order_id(k),0.);self.params.edit_extra(|b|{let mask=params::mask(b)|(1<<k);set_byte(b,MASK_BYTE,mask);});self.view.created|=1<<k;self.select(k);Some(k)}
    fn begin_group(&mut self,width:bool){let shown=self.shown();let selected=if self.view.selection==0{1<<self.view.selected}else{self.view.selection};let orig:Vec<_>=(0..BANDS).filter(|k|selected&(1<<k)!=0).map(|k|(k,shown.bands[k])).collect();for(k,_)in &orig{if width{self.vals.begin(&width_id(*k));}else{self.vals.begin(&freq_id(*k));self.vals.begin(&level_id(*k));}}self.drag=Some(Drag::Group{orig,dx:0.,dy:0.,axis:0,width});}


    /// The values shown (pending edits included) with the enable mask.
    pub fn shown(&self) -> Settings {
        let v = &self.vals;
        Settings {
            bands: std::array::from_fn(|k| Band {
                db: f64::from(v.plain(&level_id(k))),
                freq: f64::from(v.plain(&freq_id(k))),
                width: f64::from(v.plain(&width_id(k))),
                kind: v.plain(&type_id(k)).round() as i32,
                order: v.plain(&order_id(k)).round() as i32,
            }),
            main_db: f64::from(v.plain(MAIN_ID)),
            mask: self.params.extra.read().map_or(0x7f, |b| params::mask(&b)),
        }
    }

    fn hit(&self, x: f32, y: f32) -> Option<Target> {
        if self.view.details_open && self.view.show_dynamics && !self.view.harsh_open {let cfg=self.dynamic_values();let l=self.layout();let(cx,cy,r)=l.ring(&self.shown(),&self.view,!cfg.auto[self.view.selected.min(6)]&&cfg.bands[self.view.selected.min(6)][0]!=0.);let distance=(x-cx).hypot(y-cy);if (r-3.*l.scale..=r+5.*l.scale).contains(&distance){return Some(Target::Dynamic(self.view.selected.min(6)*4));}}
        self.hits.iter().rev().find(|h| h.r.contains(x, y)).map(|h| h.t)
    }

    fn band_hint(&self, k: usize) -> String {
        let b = self.shown().bands[k];
        format!(
            "{} {} {:.0}%\tBand {} ({})",
            params::db_text(b.db),
            params::hz_text(b.hz()),
            b.width * 100.0,
            k + 1,
            format!("{} / {}",TYPE_NAMES[b.kind.clamp(0, 7) as usize].to_lowercase(),eq::routed::NAMES[eq::routed::routes(&self.params.extra_copy(),false)[k] as usize])
        )
    }

    fn hint_for(&self, t: Option<Target>, x: f32, y: f32) -> String {
        let extra = self.params.extra_copy();
        match t {
            Some(Target::Token(k) | Target::Fader(k) | Target::Freq(k) | Target::Bw(k) | Target::Switch(k)) if k < BANDS => {
                let h = self.band_hint(k);
                if t == Some(Target::Switch(k)) { format!("Mute / solo\t{}", h.split('\t').nth(1).unwrap_or("")) } else { h }
            }
            Some(Target::Fader(_)) => format!("Main level: {}", params::db_text(f64::from(self.vals.plain(MAIN_ID)))),
            Some(Target::TypeTab(k) | Target::OrderMark(k)) => {
                format!("Band {} order: {}", k + 1, ORDER_NAMES[(self.vals.plain(&order_id(k)).round() as i32 + 3).clamp(0, 6) as usize])
            }
            Some(Target::Graph) => format!("{}Hz {:.1}dB - click to create, drag curve to shape, drag empty space to select",eq::freq_hz(self.layout().n(x)).round(),self.layout().db(y)),
            Some(Target::Q(k))=>format!("Q {:.2} - wheel to adjust; double-click for a precise value",view::q_of(self.shown().bands[k].width)),
            Some(Target::Undo)=>"Undo last EQ edit".into(),
            Some(Target::Redo)=>"Redo EQ edit".into(),
            Some(Target::PrevBand|Target::NextBand)=>"Select previous / next band".into(),
            Some(Target::DeleteBand)=>"Delete selected bands (automation IDs stay stable)".into(),
            Some(Target::Solo(_))=>"Hold to audition the selected frequency band".into(),
            Some(Target::Audition(_))=>"Toggle band audition. Drag the node to listen at a new frequency.".into(),
            Some(Target::Details)=>"Show or hide detailed band controls below the graph".into(),
            Some(Target::ExpandDynamics)=>"Expand manual dynamics / return to automatic dynamics".into(),
            Some(Target::AutoDynamics)=>"Automatic threshold, attack and release".into(),
            Some(Target::SpectralBand)=>"Frequency-specific spectral dynamics for the selected band".into(),
            Some(Target::UiScale)=>"Interface scale; window dimensions remain independent".into(),
            Some(Target::DisplayRange)=>"EQ display range".into(),
            Some(Target::BandMenu(_))=>"Band shape, slope and stereo placement".into(),
            Some(Target::HarshPanel)=>"Adaptive de-harsh: click to open compact amount and threshold controls".into(),
            Some(Target::DynamicToggle)=>"Dynamic EQ on/off - the illuminated outer Gain ring adjusts its range".into(),
            Some(Target::Dynamic(i))=>format!("{}: {:.1} - drag vertically, wheel; right-click for value entry and automation",if i<28{params::DYNAMIC_LABELS[i%4]}else{match i{28=>"De-harsh amount",29=>"Threshold",30..=36=>"Automatic dynamics",37=>"Spectral density",38=>"Spectral tilt",39=>"Automatic de-harsh",_=>"Spectral mode"}},self.vals.plain(&params::dynamic_id(i))),
            Some(Target::Options) => "Options".into(),
            Some(Target::Lin) => "Linear phase".into(),
            Some(Target::Hq) => "High quality (oversampling)".into(),
            Some(Target::Tokens) => "View band tokens".into(),
            Some(Target::Phones) => "Band solo on shift".into(),
            Some(Target::Monitor(m)) => format!("Monitor: {}", ["off", "input", "output"][usize::from(m.min(2))]),
            Some(Target::Analyzer)=>"Analyzer speed, resolution, range and tilt; display settings do not change the audio".into(),
            Some(Target::GainMatch)=>"Temporary RMS gain matching for A/B listening (off when editor closes); does not change output knob or saved settings".into(),
            Some(Target::Store) => if LOCK_SPARE.load(Ordering::Relaxed) { "Store in spare state (locked)".into() } else { "Store in spare state".into() },
            Some(Target::Flip) => "Flip with spare state (A/B compare)".into(),
            _ => {
                let _ = extra;
                String::new()
            }
        }
    }

    /// One complete edit of a band to `b` (only the parameters that differ).
    fn set_band(&mut self, k: usize, b: Band) {
        let cur = self.shown().bands[k];
        if cur.db != b.db {
            self.vals.edit_plain(&level_id(k), b.db as f32);
        }
        if cur.freq != b.freq {
            self.vals.edit_plain(&freq_id(k), b.freq as f32);
        }
        if cur.width != b.width {
            self.vals.edit_plain(&width_id(k), b.width as f32);
        }
        if cur.kind != b.kind {
            self.vals.edit_plain(&type_id(k), b.kind as f32);
        }
        if cur.order != b.order {
            self.vals.edit_plain(&order_id(k), b.order as f32);
        }
    }

    /// Reset (spec: level 0, default frequency, the type's default width; type and order stay).
    pub fn reset_band(&mut self, k: usize) {
        let mut b = self.shown().bands[k];
        b.db = 0.0;
        b.freq = eq::default_freq(k);
        b.width = eq::default_width(b.kind);
        self.set_band(k, b);
        for i in k*4..k*4+4{self.reset_param(&params::dynamic_id(i));}
        self.reset_param(&params::dynamic_id(30+k));self.reset_param(&params::dynamic_id(40+k));
    }

    fn reset_param(&mut self, id: &str) {
        let d = self.vals.default_norm(id);
        self.vals.edit(id, d);
    }

    fn set_mask(&mut self, m: u8) {
        self.params.edit_extra(|b| set_byte(b, MASK_BYTE, m & 0x7f));
    }

    fn mask(&self) -> u8 {
        self.params.extra.read().map_or(0x7f, |b| params::mask(&b))
    }

    /// Spare bank: store the current settings in it, or swap them with it.
    pub fn compare(&mut self, flip: bool) {
        if flip { self.audition(None); }
        let cur = self.shown();
        let fields = params::fields_of(&cur);
        let image=self.params.extra_copy();let current_routes=eq::routed::routes(&image,false);let spare_routes=eq::routed::routes(&image,true);
        let current_dynamic=self.dynamic_values();let spare_dynamic=eq::dynamic::Config::load(&self.params.extra_copy(),true);
        if flip || !LOCK_SPARE.load(Ordering::Relaxed){self.params.edit_extra(|b|current_dynamic.save(b,true));}
        if flip {for i in 0..47{self.vals.edit_plain(&params::dynamic_id(i),spare_dynamic.value(i));}}
        if !flip {
            if LOCK_SPARE.load(Ordering::Relaxed) {
                return;
            }
            self.view.compare_b=false;
            self.params.edit_extra(|b| {fields.iter().enumerate().for_each(|(i, &v)| params::set_field(b, 37 + i, v));eq::routed::save_routes(b,current_routes,true);if flip{eq::routed::save_routes(b,spare_routes,false);}});
            return;
        }
        self.view.compare_b=!self.view.compare_b;
        let spare = params::settings_at(&self.params.extra_copy(), 37);
        self.params.edit_extra(|b| {fields.iter().enumerate().for_each(|(i, &v)| params::set_field(b, 37 + i, v));eq::routed::save_routes(b,current_routes,true);if flip{eq::routed::save_routes(b,spare_routes,false);}});
        for k in 0..BANDS {
            self.set_band(k, spare.bands[k]);
        }
        if cur.main_db != spare.main_db {
            self.vals.edit_plain(MAIN_ID, spare.main_db as f32);
        }
    }

    pub fn down(&mut self, x: f32, y: f32, btn: Btn, m: Mods, dbl: bool) -> Vec<Act> {
        self.view.mouse = Some((x, y));
        self.pointer_drag=Some((x,y));
        let Some(t) = self.hit(x, y) else { return vec![] };
        if matches!(t,Target::Undo|Target::Redo){self.pending=None;self.undo_redo(t==Target::Redo);return vec![Act::Repaint];}
        self.pending=Some(self.snapshot());
        let extra = self.params.extra_copy();
        let mut acts = vec![Act::Repaint];
        let lock = |l: &mut Logic, d: Drag, ids: &[String]| {
            let (d,ids)=if let Drag::Linear{id,px}=&d{let linked=l.linked_ids(id);if linked.len()>1{let values=linked.iter().map(|id|l.vals.norm(id)).collect();(Drag::Linked{ids:linked.clone(),values,offset:0.,px:*px},linked)}else{(d,ids.to_vec())}}else{(d,ids.to_vec())};
            for id in &ids {l.vals.begin(id);}
            l.drag = Some(d);
            l.view.pressed = Some(t);
        };
        if let Target::Fader(k)|Target::Freq(k)|Target::Bw(k)|Target::Q(k)=t{if k<BANDS{if self.view.selection&(1<<k)==0{self.select(k);}self.view.selected=k;}}
        if btn==Btn::Left && dbl {let id=match t{Target::Freq(k)=>Some(freq_id(k)),Target::Fader(k)=>Some(if k==MAIN_FADER{MAIN_ID.into()}else{level_id(k)}),Target::Q(k)=>Some(format!("q{}",k+1)),Target::Dynamic(i)=>Some(params::dynamic_id(i)),_=>None};if let Some(id)=id{return vec![Act::TypeIn(id,x,y)];}}
        if btn==Btn::Left && m.ctrl && !matches!(t,Target::Token(_)|Target::Graph){let id=match t{Target::Freq(k)=>Some(freq_id(k)),Target::Fader(k)=>Some(if k==MAIN_FADER{MAIN_ID.into()}else{level_id(k)}),Target::Q(k)=>Some(width_id(k)),Target::Dynamic(i)=>Some(params::dynamic_id(i)),_=>None};if let Some(id)=id{self.reset_param(&id);return acts;}}

        match (t, btn) {
            (Target::BandMenu(k),Btn::Left|Btn::Right)=>acts.push(Act::Menu(MenuReq::Token(k),x,y)),
            (Target::HarshPanel,Btn::Left)=>{self.audition(None);self.view.harsh_open=!self.view.harsh_open;},
            (Target::Details, Btn::Left) => {
                self.view.details_open = !self.view.details_open;
                self.view.harsh_open = false;
            },
            (Target::DynamicToggle,Btn::Left)=>self.toggle_dynamics(self.view.selected),
            (Target::SpectralBand,Btn::Left)=>self.toggle_spectral(self.view.selected),
            (Target::DynamicToggle|Target::SpectralBand,Btn::Right)=>acts.push(Act::Menu(MenuReq::Token(self.view.selected),x,y)),
            (Target::AutoDynamics|Target::ExpandDynamics,Btn::Left)=>{let k=self.view.selected;let auto=self.dynamic_values().auto[k];self.vals.edit_plain(&params::dynamic_id(30+k),if auto{0.}else{1.});if self.dynamic_values().bands[k][0]==0.{self.vals.edit_plain(&params::dynamic_id(k*4),-6.);}},
            (Target::Dynamic(i),Btn::Left) if i==38||i==39=>{let v=self.vals.plain(&params::dynamic_id(i));self.vals.edit_plain(&params::dynamic_id(i),if v>=0.5{0.}else{1.});},
            (Target::UiScale,Btn::Left)=>{self.view.ui_scale=match self.view.ui_scale{v if v<1.1=>1.25,v if v<1.4=>1.5,_=>1.};},
            (Target::DisplayRange,Btn::Left)=>{self.view.range_db=match self.view.range_db{v if v<=0.||v>=30.=>3.,v if v<6.=>6.,v if v<12.=>12.,v if v<18.=>18.,_=>30.};},
            (Target::DeleteBand,Btn::Left)=>{let mask=if self.view.selection!=0{self.view.selection}else{1<<self.view.selected};self.delete_bands(mask);},
            (Target::PrevBand|Target::NextBand,Btn::Left)=>{let s=self.shown();let mut order:Vec<_>=(0..7).filter(|&k|self.visible(k)).collect();order.sort_by(|&a,&b|s.bands[a].freq.total_cmp(&s.bands[b].freq));if let Some(at)=order.iter().position(|&k|k==self.view.selected){let next=if t==Target::NextBand{(at+1)%order.len()}else{(at+order.len()-1)%order.len()};self.select(order[next]);}},
            (Target::Solo(k), Btn::Left) => {
                let restore = self.view.audition;
                self.audition(Some(k));
                self.view.pressed = Some(t);
                self.drag = Some(Drag::Solo { restore });
            },
            (Target::Audition(k), Btn::Left) => {
                self.audition((self.view.audition != Some(k)).then_some(k));
            },
            (Target::Dynamic(i), Btn::Left) => {
                if i == 29 {
                    self.vals.edit_plain(&params::dynamic_id(39), 0.);
                }
                if i < 28 && i % 4 == 0 && self.dynamic_values().bands[i / 4][0] == 0. {
                    self.vals.edit_plain(&params::dynamic_id(30 + i / 4), 1.);
                }
                let id = params::dynamic_id(i);
                lock(self, Drag::Linear { id: id.clone(), px: 240. }, &[id]);
            }
            (Target::Dynamic(i),Btn::Middle)=>self.reset_param(&params::dynamic_id(i)),
            (Target::Dynamic(i),Btn::Right)=>acts.push(Act::Menu(MenuReq::Param(params::dynamic_id(i)),x,y)),
            (Target::Token(k),Btn::Left) if m.alt=>{self.set_mask(self.mask()^(1<<k));},
            (Target::Token(k), Btn::Left) if m.ctrl => {
                self.view.selection ^= 1 << k;
                if self.view.audition.is_some_and(|old| old != k || self.view.selection & (1 << k) == 0) {
                    self.audition(None);
                }
                self.view.selected = k;
                self.view.show_dynamics = self.view.selection != 0;
            },
            (Target::Token(k),Btn::Left|Btn::Middle)=>{
                if m.shift && self.view.selection!=0 {let s=self.shown();let lo=s.bands[self.view.selected].freq.min(s.bands[k].freq);let hi=s.bands[self.view.selected].freq.max(s.bands[k].freq);for j in 0..7{if self.visible(j)&&s.bands[j].freq>=lo&&s.bands[j].freq<=hi{self.view.selection|=1<<j;}}}else if self.view.selection&(1<<k)==0{self.view.selection=1<<k;}
                if self.view.audition.is_some_and(|old| old != k) { self.audition(None); }
                self.view.selected=k;self.view.show_dynamics=true;self.view.harsh_open=false;self.view.active=Some(k);self.begin_group(btn==Btn::Middle);self.view.pressed=Some(t);if btn==Btn::Middle{acts.push(Act::Lock);}
            }
            (Target::Token(k), Btn::Right) => acts.push(Act::Menu(MenuReq::Token(k), x, y)),
            (Target::Fader(k), Btn::Left) => {
                let id = if k == MAIN_FADER { MAIN_ID.to_string() } else { level_id(k) };
                lock(self, Drag::Linear { id: id.clone(), px: LEVEL_PX }, &[id]);
                acts.push(Act::Lock);
            }
            (Target::Graph,Btn::Right)=>acts.push(Act::Menu(MenuReq::Graph,x,y)),
            (Target::Graph,Btn::Left)=>{
                self.audition(None);
                let selected=self.view.selection!=0;let l=self.layout();let curve=self.shown().response_db(eq::freq_hz(l.n(x)),eq::Options::from_image(&extra).design(f64::from(self.shared.rate.load(Ordering::Relaxed))));let near=(l.y(curve)-y).abs()<8.*l.scale;
                if dbl || m.ctrl || (!selected && near){if self.create_band(x,y).is_some(){if m.alt{let k=self.view.selected;self.vals.edit_plain(&params::dynamic_id(k*4),-6.);self.vals.edit_plain(&params::dynamic_id(30+k),1.);}self.begin_group(false);}else{self.hint="All seven legacy band slots are in use".into();}}
                else{self.view.selection=0;self.view.show_dynamics=false;self.view.harsh_open=false;self.drag=Some(Drag::Marquee{x0:x,y0:y,x,y,create:!selected});}
            }
            (Target::Q(k),Btn::Left)=>{let id=width_id(k);lock(self,Drag::Linear{id:id.clone(),px:300.},&[id]);acts.push(Act::Lock);},
            (Target::Freq(k), Btn::Left) => {
                lock(self, Drag::Linear { id: freq_id(k), px: FREQ_PX }, &[freq_id(k)]);
                acts.push(Act::Lock);
            }
            (Target::Bw(k), Btn::Left) => {
                lock(self, Drag::Linear { id: width_id(k), px: WIDTH_PX }, &[width_id(k)]);
                acts.push(Act::Lock);
            }
            (Target::TypeTab(k), Btn::Left) => {
                lock(self, Drag::Steps { id: type_id(k), acc: 0.0 }, &[type_id(k)]);
                acts.push(Act::Lock);
            }
            (Target::OrderMark(k),Btn::Left)=>acts.push(Act::Menu(MenuReq::Token(k),x,y)),
            (Target::Fader(k), Btn::Middle) => self.reset_param(&if k == MAIN_FADER { MAIN_ID.to_string() } else { level_id(k) }),
            (Target::Freq(k), Btn::Middle) => self.reset_param(&freq_id(k)),
            (Target::Bw(k), Btn::Middle) => self.reset_param(&width_id(k)),
            (Target::Fader(k), Btn::Right) => acts.push(Act::Menu(MenuReq::Param(if k == MAIN_FADER { MAIN_ID.into() } else { level_id(k) }), x, y)),
            (Target::Freq(k), Btn::Right) => acts.push(Act::Menu(MenuReq::Param(freq_id(k)), x, y)),
            (Target::Bw(k), Btn::Right) => acts.push(Act::Menu(MenuReq::Param(width_id(k)), x, y)),
            (Target::TypeTab(k), Btn::Right) => acts.push(Act::Menu(MenuReq::Param(type_id(k)), x, y)),
            (Target::OrderMark(k), Btn::Right) => acts.push(Act::Menu(MenuReq::Param(order_id(k)), x, y)),
            (Target::Switch(k), Btn::Left) => {
                let m = self.mask() ^ (1 << k);
                self.set_mask(m);
            }
            (Target::Switch(k), Btn::Right) => {
                let m = if self.mask() == 1 << k { 0x7f } else { 1 << k };
                self.set_mask(m);
            }
            (Target::Options, Btn::Left) => acts.push(Act::Menu(MenuReq::Options, x, y)),
            (Target::Lin, Btn::Left) => self.params.edit_extra(|b| toggle(b, params::F_LIN)),
            (Target::Lin, Btn::Right) => acts.push(Act::Menu(MenuReq::Lin, x, y)),
            // HQ on / off: bit 0; the mode in the other bits stays for the next time it is switched on.
            (Target::Hq, Btn::Left) => self.params.edit_extra(|b| {
                let cur = byte(b, params::HQ_BYTE);
                set_byte(b, params::HQ_BYTE, cur ^ 1);
            }),
            (Target::Hq, Btn::Right) => acts.push(Act::Menu(MenuReq::Hq, x, y)),
            (Target::Tokens, Btn::Left) => self.params.edit_extra(|b| {
                let v = byte(b, params::TOKENS_BYTE);
                set_byte(b, params::TOKENS_BYTE, u8::from(v == 0));
            }),
            (Target::Phones, Btn::Left) => self.params.edit_extra(|b| toggle(b, params::F_SOLO_SHIFT)),
            (Target::Phones, Btn::Right) => acts.push(Act::Menu(MenuReq::Phones, x, y)),
            (Target::Monitor(i), Btn::Left) => self.params.edit_extra(|b| set_byte(b, params::MONITOR_BYTE, i)),
            (Target::Analyzer,Btn::Left|Btn::Right)=>acts.push(Act::Menu(MenuReq::Analyzer,x,y)),
            (Target::GainMatch,Btn::Left)=>{self.view.gain_match=!self.view.gain_match;self.shared.gain_match.store(self.view.gain_match,Ordering::Relaxed);},
            (Target::Store, Btn::Left) => self.compare(false),
            (Target::Flip, Btn::Left) => self.compare(true),
            _ => {}
        }
        self.hint = self.hint_for(Some(t), x, y);
        acts
    }

    /// Drag movement in logical pixels since the last event.
    pub fn delta(&mut self, dx: f32, dy: f32, m: Mods) -> Vec<Act> {
        let f = if m.shift { FINE } else { 1.0 };
        let Some(d) = self.drag.clone() else { return vec![] };
        match d {
            Drag::Group{orig,dx:oldx,dy:oldy,mut axis,width}=>{let l=self.layout();let mut tx=oldx+dx*f;let mut ty=oldy+dy*f;if m.alt{if axis==0 && tx.abs()+ty.abs()>2.{axis=if tx.abs()>ty.abs(){1}else{2};}if axis==1{ty=0.;}else if axis==2{tx=0.;}}let min=orig.iter().map(|(_,b)|b.freq).fold(1.,f64::min);let max=orig.iter().map(|(_,b)|b.freq).fold(0.,f64::max);let dn=f64::from(tx/l.plot.w).clamp(-min,1.-max);for(k,b)in &orig{if width{self.vals.set(&width_id(*k),(b.width as f32-ty/300.).clamp(0.,1.));}else{self.vals.set(&freq_id(*k),(b.freq+dn) as f32);if has_level(b.kind){let gain=(b.db+f64::from(-ty*l.range*2./l.plot.h)).clamp(-18.,18.);let id=level_id(*k);let n=self.vals.plain_to_norm(&id,gain as f32);self.vals.set(&id,n);}}}self.drag=Some(Drag::Group{orig,dx:tx,dy:ty,axis,width});},
            Drag::Marquee{..}=>return vec![],
            Drag::Linked{ids,values,offset,px}=>{let offset=offset-dy*f/px;let min=values.iter().copied().fold(1.,f32::min);let max=values.iter().copied().fold(0.,f32::max);let applied=offset.clamp(-min,1.-max);for(id,v)in ids.iter().zip(&values){self.vals.set(id,v+applied);}self.drag=Some(Drag::Linked{ids,values,offset,px});},
            Drag::Linear { id, px } => {
                let v = self.vals.norm(&id) - dy * f / px;
                self.vals.set(&id, v);
            }
            Drag::Steps { id, acc } => {
                let acc = acc - dy;
                let steps = (acc / STEP_PX).trunc();
                if steps != 0.0 {
                    let plain = self.vals.plain(&id) + steps;
                    let n = self.vals.plain_to_norm(&id, plain);
                    self.vals.set(&id, n);
                }
                self.drag = Some(Drag::Steps { id, acc: acc - steps * STEP_PX });
            }
            Drag::Solo { .. } => return vec![],
        }
        let (x, y) = self.view.mouse.unwrap_or_default();
        self.hint = self.hint_for(self.view.pressed, x, y);
        vec![Act::Repaint]
    }

    pub fn up(&mut self, _btn: Btn) -> Vec<Act> {
        self.pointer_drag=None;
        let Some(d) = self.drag.take() else {if let Some(before)=self.pending.take(){self.remember(before);}return vec![Act::Repaint]};
        match d {
            Drag::Linked{ids,..}=>{for id in ids{self.vals.end(&id);}},
            Drag::Group{orig,width,..}=>{for(k,_)in orig{if width{self.vals.end(&width_id(k));}else{self.vals.end(&freq_id(k));self.vals.end(&level_id(k));}}},
            Drag::Marquee{x0,y0,x,y,create}=>{self.view.marquee=None;if (x-x0).hypot(y-y0)<3. && create{self.create_band(x,y);}},
            Drag::Linear { id, .. } | Drag::Steps { id, .. } => self.vals.end(&id),
            Drag::Solo { restore } => self.audition(restore),
        }
        if let Some(before)=self.pending.take(){self.remember(before);}
        self.view.pressed = None;
        vec![Act::Repaint]
    }

    pub fn moved(&mut self, x: f32, y: f32, m: Mods) -> Vec<Act> {
        let previous = self.view.mouse.replace((x, y));
        let follows_pointer = matches!(self.drag, Some(Drag::Group { width: false, .. }))
            || matches!(self.drag, Some(Drag::Linear { .. } | Drag::Linked { .. }))
                && matches!(self.view.pressed, Some(Target::Dynamic(_)));
        if follows_pointer {
            if let Some((px, py)) = self.pointer_drag.replace((x, y)) {
                return self.delta(x - px, y - py, m);
            }
            return vec![Act::Repaint];
        }
        if let Some(Drag::Marquee{x0,y0,create,..})=self.drag.clone(){let r=Rect::new(x.min(x0),y.min(y0),(x-x0).abs(),(y-y0).abs());self.view.marquee=Some(r);let s=self.shown();self.view.selection=(0..7).filter(|&k|self.visible(k)&&{let(p,q)=self.layout().node(&s,k);r.contains(p,q)}).fold(0,|m,k|m|(1<<k));if self.view.selection!=0{self.view.selected=self.view.selection.trailing_zeros() as usize;self.view.show_dynamics=true;}self.drag=Some(Drag::Marquee{x0,y0,x,y,create});return vec![Act::Repaint];}
        if self.drag.is_some() {return vec![Act::Repaint];}
        let active=self.view.active;
        let t = self.hit(x, y);
        self.view.active = match t {
            Some(Target::Token(k)) => Some(k),
            _ => None,
        };
        self.hint = self.hint_for(t, x, y);
        // Only a change of hover target in the card needs another frame.
        let card_hit = |x, y| {
            self.hits.iter().any(|h| h.t == Target::BandCard && h.r.contains(x, y))
                .then(|| self.hits.iter().rev().find(|h| h.r.contains(x, y)).map(|h| h.t))
                .flatten()
        };
        let hover_changed = previous.and_then(|(x, y)| card_hit(x, y)) != card_hit(x, y);
        if active != self.view.active || hover_changed {vec![Act::Repaint]} else {vec![]}
    }

    pub fn leave(&mut self) -> Vec<Act> {
        self.view.mouse = None;
        self.view.active = None;
        self.hint.clear();
        vec![Act::Repaint]
    }

    pub fn wheel(&mut self, x: f32, y: f32, notches: f32, m: Mods) -> Vec<Act> {
        let before=self.snapshot();
        let Some(t) = self.hit(x, y) else { return vec![] };
        let notches=notches*if m.shift{0.2}else{1.};
        let step = |l: &mut Logic, id: &str, d: f32| {
            let v = l.vals.norm(id) + d * notches;
            l.vals.edit(id, v);
        };
        if let Target::Token(k)=t {let selected=if self.view.selection&(1<<k)!=0{self.view.selection}else{1<<k};for j in 0..7{if selected&(1<<j)!=0{if m.alt&&m.ctrl{step(self,&params::dynamic_id(j*4),1./36.);step(self,&level_id(j),-1./36.);}else if m.alt{step(self,&params::dynamic_id(j*4),1./36.);}else if m.ctrl{step(self,&level_id(j),1./36.);}else if matches!(self.shown().bands[j].kind,eq::LOW_PASS|eq::HIGH_PASS){let v=self.vals.plain(&order_id(j))+notches.signum();self.vals.edit_plain(&order_id(j),v);}else{step(self,&width_id(j),0.025);}}}self.remember(before);return vec![Act::Repaint];}
        match t {
            Target::Q(k)=>step(self,&width_id(k),0.025),
            Target::Dynamic(i)=>step(self,&params::dynamic_id(i),0.01),
            Target::Token(k) | Target::Bw(k) if k < BANDS => step(self, &width_id(k), 0.05),
            Target::Fader(k) => step(self, &if k == MAIN_FADER { MAIN_ID.to_string() } else { level_id(k) }, 0.5 / 36.0),
            Target::Freq(k) => step(self, &freq_id(k), 1.0 / 128.0),
            Target::TypeTab(k) | Target::OrderMark(k) => {
                let id = if matches!(t, Target::TypeTab(_)) { type_id(k) } else { order_id(k) };
                let plain = self.vals.plain(&id) + notches.signum();
                let n = self.vals.plain_to_norm(&id, plain);
                self.vals.edit(&id, n);
            }
            _ => return vec![],
        }
        self.hint = self.hint_for(Some(t), x, y);
        self.remember(before);
        vec![Act::Repaint]
    }

    /// The parameter's position in the plugin's parameter list (its CLAP / VST3 index).
    pub fn param_index(&self, id: &str) -> Option<usize> {
        nih_plug::params::Params::param_map(&*self.params).iter().position(|(p, _, _)| p == id)
    }

    pub fn menu(&self, req: &MenuReq) -> MenuDef<Cmd> {
        let extra = self.params.extra_copy();
        let s = self.shown();
        let mut m = MenuDef::new(MenuStyle::Context);
        match req {
            MenuReq::Graph=>{let available=(0..7).any(|k|{let s=self.shown();let c=self.dynamic_values();s.bands[k].kind==0||(view::modern_available(&s,k)&&self.view.created&(1<<k)==0&&c.bands[k][0]==0.&&c.spectral&(1<<k)==0)});for (label,mode) in [("Add EQ band",0),("Add dynamic band",1),("Add spectral band",2)]{m.add(MenuItem::item(label).enabled(available),Cmd::AddBand(mode));}}
            MenuReq::Token(k) => {
                let k = *k;
                m.head(&format!("Band {}", k + 1), Cmd::None);
                let cfg=self.dynamic_values();let active=cfg.bands[k][0].abs()>0.001;
                check(&mut m,"Dynamic EQ",active,Cmd::Dynamic(k));
                check(&mut m,"Spectral dynamics",active&&cfg.spectral&(1<<k)!=0,Cmd::Spectral(k));
                m.sep(Cmd::None);
                let mut values=MenuDef::new(MenuStyle::Context);
                values.add(MenuItem::item("Frequency..."),Cmd::TypeIn(freq_id(k)));
                values.add(MenuItem::item("Gain...").enabled(has_level(s.bands[k].kind)),Cmd::TypeIn(level_id(k)));
                values.add(MenuItem::item("Q..."),Cmd::TypeIn(format!("q{}",k+1)));
                m.submenu("Type in value",values,Cmd::None);
                m.sep(Cmd::None);
                m.add(MenuItem::item("Reset"), Cmd::ResetBand(k));
                m.add(MenuItem::item("Delete"),Cmd::DeleteBand(k));
                let mut ty = MenuDef::new(MenuStyle::Context);
                for (i, n) in TYPE_NAMES.iter().enumerate() {
                    radio(&mut ty, n, s.bands[k].kind == i as i32, Cmd::SetType(k, i as i32));
                }
                m.submenu("Type", ty, Cmd::None);
                let mut placement=MenuDef::new(MenuStyle::Context);
                let routes=eq::routed::routes(&extra,false);
                for(i,name)in eq::routed::NAMES.iter().enumerate(){radio(&mut placement,name,routes[k]==i as u8,Cmd::Route(k,i as u8));}
                m.submenu("Process",placement,Cmd::None);
                let mut or = MenuDef::new(MenuStyle::Context);
                for (i, n) in ORDER_NAMES.iter().enumerate() {
                    radio(&mut or, n, s.bands[k].order == i as i32 - 3, Cmd::SetOrder(k, i as i32 - 3));
                }
                m.submenu("Order", or, Cmd::None);
                m.submenu("Key", key_menu(k), Cmd::None);
            }
            MenuReq::Param(id) => {
                m.add(MenuItem::item("Reset"), Cmd::Reset(id.clone()));
                if let Some(k) = id.strip_prefix("freq").and_then(|n| n.parse::<usize>().ok()) {
                    m.submenu("Key", key_menu(k - 1), Cmd::None);
                }
                m.sep(Cmd::None);
                m.add(MenuItem::item("Copy value"), Cmd::CopyValue(id.clone()));
                m.add(MenuItem::item("Paste value").enabled(self.copied.is_some()), Cmd::PasteValue(id.clone()));
                m.add(MenuItem::item("Type in value..."), Cmd::TypeIn(id.clone()));
                // Automation belongs to the host: offered when Lemon Studio hosts the editor (host link).
                m.sep(Cmd::None);
                m.add(MenuItem::item("Create automation clip").enabled(self.hosted), Cmd::Automate(id.clone()));
            }
            MenuReq::Options => {
                m.head("Options", Cmd::None);
                check(&mut m, "Lock spare state", LOCK_SPARE.load(Ordering::Relaxed), Cmd::LockSpare);
                m.submenu("Band solo on Shift", self.solo_menu(&extra), Cmd::None);
                // Greyed out: nothing reads the minimum phase mod speed (parameter smoothing), phase rotation or the
                // heatmap's enhanced frequency and high precision yet. The linear phase mod speed is LIN's crossfade time.
                m.submenu("Minimum phase mod speed", greyed(opt_menu(&extra, Opt::MinPhaseSpeed, &["Off", "Medium", "Smooth"], 0)), Cmd::None);
                let mut lin = MenuDef::new(MenuStyle::Context);
                for (i, n) in ["Normal", "Fast", "Faster", "Ultra"].iter().enumerate() {
                    radio(&mut lin, n, byte(&extra, params::LIN_SPEED_BYTE) == i as u8, Cmd::Byte(params::LIN_SPEED_BYTE, i as u8));
                }
                m.submenu("Linear phase mod speed", lin, Cmd::None);
                m.head("Visualization", Cmd::None);
                check_off(&mut m, "Phase rotation", flag(&extra, params::F_PHASE_ROTATION));
                m.submenu("Monitor input", opt_menu(&extra, Opt::MonitorInput, &["Mid", "Side", "Left", "Right"], 1), Cmd::None);
                m.head("Histogram", Cmd::None);
                check(&mut m, "Enabled", flag(&extra, params::F_HISTOGRAM), Cmd::Flag(params::F_HISTOGRAM));
                m.submenu("Range", opt_menu(&extra, Opt::Range, &["-60dB", "-90dB", "-120dB"], 1), Cmd::None);
                m.submenu("Pivot slope", opt_menu(&extra, Opt::PivotSlope, &["0 dB/oct", "1.5 dB/oct", "3 dB/oct", "4.5 dB/oct", "6 dB/oct"], 0), Cmd::None);
                m.submenu("Resolution", self.resolution_menu(&extra), Cmd::None);
                m.submenu("Analyzer speed", opt_menu(&extra, Opt::TimeSmoothing, &crate::analyzer::SPEEDS, 0), Cmd::None);
                check(&mut m, "Average mode", flag(&extra, params::F_AVERAGE), Cmd::Flag(params::F_AVERAGE));
                m.head("Heatmap", Cmd::None);
                check(&mut m, "Enabled", flag(&extra, params::F_HEATMAP), Cmd::Flag(params::F_HEATMAP));
                check_off(&mut m, "Enhanced frequency", byte(&extra, params::ENHANCED_OFF_BYTE) == 0);
                check_off(&mut m, "High precision", byte(&extra, params::HEAT_PRECISION_BYTE) != 0);
                m.submenu("Heatmap position", opt_menu(&extra, Opt::HeatmapPosition, &["Top", "Bottom", "Full"], 0), Cmd::None);
                m.sep(Cmd::None);
                let mut about = MenuDef::new(MenuStyle::Context);
                about.add(MenuItem::item("Lemon Parametric EQ").enabled(false), Cmd::None);
                about.add(MenuItem::item(&format!("Lemon Studio {} - GPLv3", env!("CARGO_PKG_VERSION"))).enabled(false), Cmd::None);
                about.add(MenuItem::item("Filters after the RBJ Audio EQ Cookbook").enabled(false), Cmd::None);
                m.submenu("About", about, Cmd::None);
            }
            MenuReq::Lin => {
                let cur = byte(&extra, params::LIN_PRECISION_BYTE);
                for (i, n) in ["Lowest", "Low", "Medium", "High", "Higher", "Highest"].iter().enumerate() {
                    radio(&mut m, n, cur == i as u8, Cmd::Byte(params::LIN_PRECISION_BYTE, i as u8));
                }
            }
            MenuReq::Hq => {
                // The mode, whether HQ is on or not (the toggle keeps it).
                let mode = byte(&extra, params::HQ_BYTE) >> 1;
                let enh = flag(&extra, params::F_HQ_ENHANCED);
                radio(&mut m, "HQ+", mode == 1, Cmd::Hq(3));
                radio(&mut m, "HQ Legacy", mode == 0 && !enh, Cmd::Hq(1));
                radio(&mut m, "HQ Enhanced", mode == 0 && enh, Cmd::Hq(2));
            }
            MenuReq::Analyzer => {
                m.head("Spectrum analyzer",Cmd::None);
                m.submenu("Speed",opt_menu(&extra,Opt::TimeSmoothing,&crate::analyzer::SPEEDS,0),Cmd::None);
                m.submenu("Resolution",self.resolution_menu(&extra),Cmd::None);
                m.submenu("Range",opt_menu(&extra,Opt::Range,&["60 dB","90 dB","120 dB"],1),Cmd::None);
                m.submenu("Tilt",opt_menu(&extra,Opt::PivotSlope,&["0 dB/oct","1.5 dB/oct","3 dB/oct","4.5 dB/oct","6 dB/oct"],0),Cmd::None);
                check(&mut m,"Average",flag(&extra,params::F_AVERAGE),Cmd::Flag(params::F_AVERAGE));
            }
            MenuReq::Phones => m = self.solo_menu(&extra),
        }
        m
    }

    fn resolution_menu(&self,extra:&[u8])->MenuDef<Cmd>{
        let mut m=MenuDef::new(MenuStyle::Context);
        let cur=params::opt(extra,Opt::FreqPrecision).clamp(0,7) as usize;
        let size=crate::analyzer::SIZES[cur];
        for(i,label)in ["Low - 1024","Medium - 2048","High - 4096","Maximum - 8192"].iter().enumerate(){
            radio(&mut m,label,size==crate::analyzer::SIZES[4+i],Cmd::Opt(Opt::FreqPrecision,(4+i) as i32));
        }
        if cur==3 {radio(&mut m,"Legacy - 16384",true,Cmd::Opt(Opt::FreqPrecision,3));}
        m
    }

    /// Solo mode and "Inherit bandwidth": greyed out, as no solo reads them yet (Shift+click solos the band as it is).
    fn solo_menu(&self, extra: &[u8]) -> MenuDef<Cmd> {
        let mut m = opt_menu(extra, Opt::SoloMode, &["Adaptive", "Bandpass"], 0);
        check(&mut m, "Inherit bandwidth", flag(extra, params::F_INHERIT_BW), Cmd::Flag(params::F_INHERIT_BW));
        greyed(m)
    }

    pub fn pick(&mut self, _req: &MenuReq, cmd: Cmd, x: f32, y: f32) -> Vec<Act> {
        self.pending=None;
        let before=self.snapshot();
        match cmd {
            Cmd::None => {}
            Cmd::ResetBand(k) => self.reset_band(k),
            Cmd::DeleteBand(k)=>{let mask=if self.view.selection&(1<<k)!=0{self.view.selection}else{1<<k};self.delete_bands(mask);},
            Cmd::Dynamic(k)=>{self.select(k);self.toggle_dynamics(k);},
            Cmd::Spectral(k)=>{self.select(k);self.toggle_spectral(k);},
            Cmd::AddBand(mode)=>{if let Some(k)=self.create_band(x,y){if mode==1{self.toggle_dynamics(k);}else if mode==2{self.toggle_spectral(k);}}},
            Cmd::Route(k,v)=>self.params.edit_extra(|b|{let mut r=eq::routed::routes(b,false);r[k]=v.min(4);eq::routed::save_routes(b,r,false);}),
            Cmd::SetType(k,t)=>{let mask=if self.view.selection&(1<<k)!=0{self.view.selection}else{1<<k};for j in 0..7{if mask&(1<<j)!=0{self.vals.edit_plain(&type_id(j),t as f32);}}},
            Cmd::SetOrder(k,o)=>{let mask=if self.view.selection&(1<<k)!=0{self.view.selection}else{1<<k};for j in 0..7{if mask&(1<<j)!=0{self.vals.edit_plain(&order_id(j),o as f32);}}},
            Cmd::SetHz(k, hz) => self.vals.edit_plain(&freq_id(k), eq::freq_pos(hz) as f32),
            Cmd::Reset(id) => self.reset_param(&id),
            Cmd::CopyValue(id) => self.copied = Some(self.vals.norm(&id)),
            Cmd::PasteValue(id) => {
                if let Some(v) = self.copied {
                    self.vals.edit(&id, v);
                }
            }
            Cmd::TypeIn(id) => return vec![Act::TypeIn(id, x, y)],
            Cmd::Automate(id) => return self.param_index(&id).map(Act::Automate).into_iter().collect(),
            Cmd::Flag(f) => self.params.edit_extra(|b| toggle(b, f)),
            Cmd::Opt(o, v) => self.params.edit_extra(|b| params::set_opt(b, o, v)),
            Cmd::Byte(i, v) => self.params.edit_extra(|b| set_byte(b, i, v)),
            Cmd::Hq(mode) => self.params.edit_extra(|b| {
                set_byte(b, params::HQ_BYTE, if mode == 3 { 3 } else { 1 });
                set_flag(b, params::F_HQ_ENHANCED, mode == 2);
            }),
            Cmd::Enhanced => self.params.edit_extra(|b| {
                let off = byte(b, params::ENHANCED_OFF_BYTE);
                set_byte(b, params::ENHANCED_OFF_BYTE, u8::from(off == 0));
            }),
            Cmd::LockSpare => {
                LOCK_SPARE.fetch_xor(true, Ordering::Relaxed);
            }
        }
        self.remember(before);
        vec![Act::Repaint]
    }

    /// The value text a type-in box starts with.
    pub fn value_text(&self, id: &str) -> String {
        if let Some(k)=id.strip_prefix("q").and_then(|v|v.parse::<usize>().ok()){return format!("{:.3}",view::q_of(self.shown().bands[k.saturating_sub(1).min(6)].width));}
        self.vals.text(id)
    }

    /// Applies a typed value.
    pub fn typed(&mut self, id: &str, text: &str) {
        let before=self.snapshot();
        if let Some(k)=id.strip_prefix("q").and_then(|v|v.parse::<usize>().ok()){if let Ok(q)=text.trim().parse::<f64>(){let id=width_id(k.saturating_sub(1).min(6));let value=view::width_of_q(q) as f32;let delta=value-self.vals.plain(&id);for id in self.linked_ids(&id){let v=(self.vals.plain(&id)+delta).clamp(0.,1.);self.vals.edit_plain(&id,v);}}self.remember(before);return;}
        if let Some(n) = self.vals.parse(id, text) {
            let delta=n-self.vals.norm(id);for id in self.linked_ids(id){let value=(self.vals.norm(&id)+delta).clamp(0.,1.);self.vals.edit(&id,value);}
        }
        self.remember(before);
    }

}

// ---------------------------------------------------------------------------------------------------------------
// The window

#[cfg(windows)]
mod win32 {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::sync::Arc;

    use nih_plug::prelude::GuiContext;
    use os_plugkit::win::{self as pw, CursorLock};
    use os_theme::Palette;
    use os_ui::Canvas;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{InvalidateRect,ScreenToClient,UpdateWindow};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    use super::{Act, Btn, Logic, Mods, InputFrames};
    use crate::analyzer::{LiveAnalyzer, Look as ALook};
    use crate::params::{self, EqParams, Opt};
    use crate::view::{self, Look, ViewState};
    use crate::Shared;

    const TIMER: usize = 1;
    const WM_CHANGED: u32 = WM_APP + 1;

    struct Win {
        hwnd: HWND,
        canvas: Canvas,
        buf: Vec<u32>,
        scale: f32,
        dpi:f32,
        ctx:Arc<dyn GuiContext>,
        size: (u32, u32),
        params: Arc<EqParams>,
        shared: Arc<Shared>,
        logic: Logic,
        palette: Palette,
        analyzer: LiveAnalyzer,
        input_frames: InputFrames,
        last_sig: u64,
        last_extra: u64,
        last_view: ViewState,
        last_hint: String,
        lock: CursorLock,
        tracking: bool,
        slot: Arc<AtomicIsize>,
        resize_drag:Option<(i32,i32,u32,u32)>,
    }

    /// Owns the window; dropping it closes the editor.
    pub struct Handle(isize, Arc<AtomicIsize>, Arc<Shared>);

    // SAFETY: the handle is only an HWND value; the wrapper drops it on the GUI thread.
    unsafe impl Send for Handle {}

    impl Drop for Handle {
        fn drop(&mut self) {
            self.1.store(0, Ordering::Release);
            self.2.solo.store(-1,Ordering::Relaxed);
            self.2.editor_open.store(false, Ordering::Relaxed);
            self.2.repaint_pending.store(false, Ordering::Release);
            unsafe { DestroyWindow(self.0 as HWND) };
        }
    }

    pub fn post_changed(hwnd: isize, shared: &Shared) {
        // Parameter automation and grouped edits can emit many notifications per block.
        // Queue one wake-up; painting reads the newest values rather than every intermediate one.
        if hwnd != 0 && !shared.repaint_pending.swap(true, Ordering::AcqRel) {
            if unsafe { PostMessageW(hwnd as HWND, WM_CHANGED, 0, 0) } == 0 {
                shared.repaint_pending.store(false, Ordering::Release);
            }
        }
    }

    fn mods() -> Mods {
        let (ctrl, shift, alt) = pw::mods();
        Mods { ctrl, shift, alt }
    }

    pub fn open(parent: *mut c_void, scale: f32, params: Arc<EqParams>, shared: Arc<Shared>, ctx: Arc<dyn GuiContext>, slot: Arc<AtomicIsize>) -> Handle {
        let dpi = if scale > 0.0 { scale } else { 1.0 };
        let logical=(shared.editor_w.load(Ordering::Relaxed),shared.editor_h.load(Ordering::Relaxed));
        let scale=dpi;
        let size=((logical.0 as f32*dpi).round() as u32,(logical.1 as f32*dpi).round() as u32);
        unsafe {
            let class = pw::register("LemonParametricEq2View", Some(wndproc));
            let win = Box::new(Win {
                hwnd: std::ptr::null_mut(),
                canvas: Canvas::new(size.0, size.1, scale).expect("canvas"),
                buf: vec![0; (size.0 * size.1) as usize],
                scale,
                dpi,
                ctx:ctx.clone(),
                size,
                logic: Logic::new(params.clone(), shared.clone(), Some(ctx.clone())),
                params,
                shared: shared.clone(),
                palette: os_plugkit::palette(),
                analyzer: LiveAnalyzer::default(),
                input_frames: InputFrames::default(),
                last_sig: 0,
                last_extra: u64::MAX,
                last_view: ViewState::default(),
                last_hint: String::new(),
                lock: CursorLock::default(),
                tracking: false,
                slot: slot.clone(),
                resize_drag:None,
            });
            let title = pw::wide("Lemon Parametric EQ");
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
                0,
                0,
                size.0 as i32,
                size.1 as i32,
                parent as HWND,
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                Box::into_raw(win) as *const c_void,
            );
            slot.store(hwnd as isize, Ordering::Release);
            shared.editor_open.store(true, Ordering::Relaxed);
            SetTimer(hwnd, TIMER, 30, None);
            Handle(hwnd as isize, slot, shared)
        }
    }

    unsafe fn state(hwnd: HWND) -> Option<&'static mut Win> {
        let p = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut Win;
        if p.is_null() { None } else { Some(unsafe { &mut *p }) }
    }

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        unsafe {
            if msg == WM_NCCREATE {
                let cs = lp as *const CREATESTRUCTW;
                let w = (*cs).lpCreateParams as *mut Win;
                (*w).hwnd = hwnd;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, w as isize);
                return DefWindowProcW(hwnd, msg, wp, lp);
            }
            if msg == WM_NCDESTROY {
                let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Win;
                if !p.is_null() {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    (*p).slot.store(0, Ordering::Release);
                    drop(Box::from_raw(p));
                }
                return DefWindowProcW(hwnd, msg, wp, lp);
            }
            let Some(w) = state(hwnd) else { return DefWindowProcW(hwnd, msg, wp, lp) };
            if msg == os_hostlink::message() {
                return w.host_link(wp, lp);
            }
            match msg {
                WM_SETCURSOR=>{let mut p=windows_sys::Win32::Foundation::POINT{x:0,y:0};GetCursorPos(&mut p);ScreenToClient(hwnd,&mut p);if w.resize_drag.is_some() || (p.x as f32>=w.size.0 as f32-12.*w.dpi && p.y as f32>=w.size.1 as f32-12.*w.dpi){SetCursor(LoadCursorW(std::ptr::null_mut(),IDC_SIZENWSE));1}else{DefWindowProcW(hwnd,msg,wp,lp)}},
                WM_PAINT => {
                    w.paint();
                    0
                }
                WM_ERASEBKGND => 1,
                WM_TIMER | WM_CHANGED => {
                    if msg == WM_CHANGED { w.shared.repaint_pending.store(false, Ordering::Release); }
                    w.poll();
                    let flush=w.input_frames.due();let hwnd=w.hwnd;
                    if flush {UpdateWindow(hwnd);}
                    0
                }
                WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_RBUTTONDOWN | WM_RBUTTONDBLCLK | WM_MBUTTONDOWN => {
                    SetFocus(hwnd);
                    let (x, y) = w.mouse(lp);
                    if msg==WM_LBUTTONDOWN && (x>=w.shared.editor_w.load(Ordering::Relaxed) as f32-12. && y>=w.shared.editor_h.load(Ordering::Relaxed) as f32-12.){let mut p=windows_sys::Win32::Foundation::POINT{x:0,y:0};GetCursorPos(&mut p);w.resize_drag=Some((p.x,p.y,w.shared.editor_w.load(Ordering::Relaxed),w.shared.editor_h.load(Ordering::Relaxed)));SetCapture(hwnd);return 0;}
                    let btn = match msg {
                        WM_RBUTTONDOWN | WM_RBUTTONDBLCLK => Btn::Right,
                        WM_MBUTTONDOWN => Btn::Middle,
                        _ => Btn::Left,
                    };
                    let acts = w.logic.down(x, y, btn, mods(), msg == WM_LBUTTONDBLCLK);
                    if w.logic.dragging() && !w.lock.locked() {
                        SetCapture(hwnd);
                    }
                    w.run(acts);
                    0
                }
                WM_MOUSEMOVE => {
                    if let Some((x,y,width,height))=w.resize_drag{let mut p=windows_sys::Win32::Foundation::POINT{x:0,y:0};GetCursorPos(&mut p);let wanted=((width as f32+(p.x-x) as f32/w.dpi).max(1.) as u32,(height as f32+(p.y-y) as f32/w.dpi).max(1.) as u32);w.resize_to(view::resized((width,height),wanted));return 0;}

                    if !w.tracking {
                        let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                        TrackMouseEvent(&mut tme);
                        w.tracking = true;
                    }
                    if let Some((dx, dy)) = w.lock.delta() {
                        if dx != 0 || dy != 0 {
                            let acts = w.logic.delta(dx as f32 / w.scale, dy as f32 / w.scale, mods());
                            w.run(acts);
                        }
                        let flush=w.input_refresh();let hwnd=w.hwnd;
                        if flush {UpdateWindow(hwnd);}
                        return 0;
                    }
                    let (x, y) = w.mouse(lp);
                    let acts = w.logic.moved(x, y, mods());
                    w.run(acts);
                    // WM_TIMER/WM_PAINT can be starved by hardware input. Poll on a paced
                    // input deadline and dispatch pending paint directly, before returning.
                    let flush=w.input_refresh();let hwnd=w.hwnd;
                    if flush {UpdateWindow(hwnd);}
                    0
                }
                WM_MOUSELEAVE => {
                    w.tracking = false;
                    if !w.lock.locked() && !w.logic.dragging() {
                        let acts = w.logic.leave();
                        w.run(acts);
                    }
                    0
                }
                WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => {
                    w.resize_drag=None;
                    w.lock.unlock();
                    ReleaseCapture();
                    let acts = w.logic.up(Btn::Left);
                    w.run(acts);
                    0
                }
                WM_CAPTURECHANGED => {
                    // Another window took the mouse (not this one capturing again): end the drag.
                    if w.logic.dragging() && lp as HWND != hwnd {
                        w.lock.unlock();
                        let acts = w.logic.up(Btn::Left);
                        w.run(acts);
                    }
                    0
                }
                WM_MOUSEWHEEL => {
                    // Wheel positions are screen coordinates.
                    let mut r: RECT = std::mem::zeroed();
                    GetWindowRect(hwnd, &mut r);
                    let (sx, sy) = pw::xy(lp);
                    let (x, y) = ((sx - r.left) as f32 / w.scale, (sy - r.top) as f32 / w.scale);
                    let notches = ((wp >> 16) & 0xffff) as u16 as i16 as f32 / 120.0;
                    let acts = w.logic.wheel(x, y, notches, mods());
                    w.run(acts);
                    0
                }
                WM_DESTROY => {
                    KillTimer(hwnd, TIMER);
                    w.lock.unlock();
                    0
                }
                _ => DefWindowProcW(hwnd, msg, wp, lp),
            }
        }
    }

    impl Win {
        fn resize_to(&mut self,logical:(u32,u32)){
            self.shared.editor_w.store(logical.0,Ordering::Relaxed);self.shared.editor_h.store(logical.1,Ordering::Relaxed);
            self.scale=self.dpi;self.size=((logical.0 as f32*self.dpi).round() as u32,(logical.1 as f32*self.dpi).round() as u32);
            self.canvas.resize(self.size.0,self.size.1,self.scale);
            unsafe{SetWindowPos(self.hwnd,std::ptr::null_mut(),0,0,self.size.0 as i32,self.size.1 as i32,SWP_NOMOVE|SWP_NOZORDER|SWP_NOACTIVATE);}
            self.ctx.request_resize();self.invalidate();
        }
        fn mouse(&self, l: LPARAM) -> (f32, f32) {
            let (x, y) = pw::xy(l);
            (x as f32 / self.scale, y as f32 / self.scale)
        }

        fn screen(&self, x: f32, y: f32) -> (i32, i32) {
            pw::to_screen(self.hwnd, (x * self.scale) as i32, (y * self.scale) as i32)
        }

        fn invalidate(&self) {
            unsafe { InvalidateRect(self.hwnd, std::ptr::null(), 0) };
        }

        fn input_refresh(&mut self)->bool {
            if !self.input_frames.due(){return false;}
            self.poll();
            self.invalidate();
            true
        }

        /// Analyzer settings, or `None` when nothing is monitored.
        fn analyzer_look(&self) -> Option<ALook> {
            let b = self.params.extra_copy();
            let on = params::byte(&b, params::MONITOR_BYTE) != 0 && (params::flag(&b, params::F_HISTOGRAM) || params::flag(&b, params::F_HEATMAP));
            on.then(|| ALook {
                precision: params::opt(&b, Opt::FreqPrecision).clamp(0, 7) as usize,
                slope: params::opt(&b, Opt::PivotSlope).clamp(0, 4) as usize,
                smoothing: params::opt(&b, Opt::TimeSmoothing).clamp(0, 4) as usize,
                average: params::flag(&b, params::F_AVERAGE),
            })
        }

        /// Timer / parameter change: expire confirmed overlay values, analyse new audio, repaint when anything shown
        /// changed.
        fn poll(&mut self) {
            self.logic.dynamic_meter();
            let mut dirty = self.logic.vals.expire();
            let analyzer_look=self.analyzer_look();
            dirty |= self.analyzer.poll(&self.shared,analyzer_look);
            let sig = self.logic.vals.signature();
            let ev = self.params.extra_version.load(Ordering::Acquire);
            if dirty || sig != self.last_sig || ev != self.last_extra || self.logic.view != self.last_view || self.logic.hint != self.last_hint {
                self.invalidate();
            }
            if self.logic.hint != self.last_hint {
                self.last_hint = self.logic.hint.clone();
                if self.logic.hosted {
                    os_hostlink::to_host(self.hwnd as isize, os_hostlink::Cmd::Changed, 0);
                }
            }
        }

        fn run(&mut self, acts: Vec<Act>) {
            let mut repaint = false;
            for a in acts {
                match a {
                    Act::Repaint => repaint = true,
                    Act::Lock => self.lock.lock(self.hwnd),
                    Act::Menu(req, x, y) => {
                        let def = self.logic.menu(&req);
                        let (sx, sy) = self.screen(x, y);
                        if let Some(cmd) = pw::popup(self.hwnd, sx, sy, &def, &self.palette, self.scale) {
                            let acts = self.logic.pick(&req, cmd, x, y);
                            self.run(acts);
                        }
                        repaint = true;
                    }
                    Act::TypeIn(id, x, y) => {
                        let (sx, sy) = self.screen(x, y);
                        let cur = self.logic.value_text(&id);
                        if let Some(t) = pw::type_in(self.hwnd, sx, sy, &cur, self.scale) {
                            self.logic.typed(&id, &t);
                        }
                        repaint = true;
                    }
                    Act::Automate(i) => {
                        os_hostlink::to_host(self.hwnd as isize, os_hostlink::Cmd::Automate, i as isize);
                    }
                }
            }
            if repaint {
                self.invalidate();
            }
        }

        /// Answers the host (spec `plugins/host-link.md`): one page, the hint line. Unknown commands return 0.
        unsafe fn host_link(&mut self, wp: WPARAM, lp: LPARAM) -> LRESULT {
            use os_hostlink::{Cmd, PageIcon};
            let Some(cmd) = Cmd::from_usize(wp) else { return 0 };
            match cmd {
                Cmd::Hello => {
                    self.logic.hosted = true;
                    os_hostlink::VERSION as LRESULT
                }
                Cmd::PageCount => 1,
                Cmd::PageIcon => PageIcon::Plugin as LRESULT,
                Cmd::GetPage | Cmd::SetPage => 0,
                Cmd::Metrics => os_hostlink::pack_size(self.shared.editor_w.load(Ordering::Relaxed),self.shared.editor_h.load(Ordering::Relaxed)),
                Cmd::Resizable=>os_hostlink::pack_size(view::MIN_W,view::MIN_H),
                Cmd::SetSize=>{let wanted=os_hostlink::unpack_size(lp);let old=(self.shared.editor_w.load(Ordering::Relaxed),self.shared.editor_h.load(Ordering::Relaxed));self.resize_to(view::resized(old,wanted));1},
                Cmd::Hint => {
                    if lp != 0 {
                        // SAFETY: the host passes a `HintBuf` of its own process (spec: host-link.md).
                        unsafe { &mut *(lp as *mut os_hostlink::HintBuf) }.set(&self.logic.hint);
                    }
                    1
                }
                Cmd::SampleFit | Cmd::BrowseSample | Cmd::Changed | Cmd::Note | Cmd::Automate | Cmd::Audio => 0,
            }
        }

        fn paint(&mut self) {
            let (hdc, ps) = pw::begin_paint(self.hwnd);
            self.logic.dynamic_meter();
            self.logic.read_visibility();
            let extra = self.logic.shown_extra();
            let histogram = params::flag(&extra, params::F_HISTOGRAM) || params::flag(&extra, params::F_HEATMAP);
            let spectrum = (histogram && params::byte(&extra, params::MONITOR_BYTE) != 0 && !self.analyzer.analyzer.cols.is_empty()).then_some(self.analyzer.analyzer.cols.as_slice());
            let look = Look { s: self.logic.shown(), extra: &extra, spectrum, rate: f64::from(self.shared.rate.load(Ordering::Relaxed)) };
            self.logic.view.size=self.canvas.size();
            self.logic.hits = view::paint(&mut self.canvas, &look, &self.logic.view);
            self.last_sig = self.logic.vals.signature();
            self.last_extra = self.params.extra_version.load(Ordering::Acquire);
            self.last_view = self.logic.view.clone();
            self.buf.resize((self.size.0 * self.size.1) as usize, 0);
            self.canvas.copy_to_xrgb(&mut self.buf);
            pw::blit(hdc, &self.buf, self.size.0, self.size.1);
            pw::end_paint(self.hwnd, &ps);
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod gui {
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    use nih_plug::prelude::GuiContext;
    use os_hostlink::{Cmd, PageIcon, Peer};
    use os_plugkit::gui::{self as px, Button, Input, Window};
    use os_theme::Palette;
    use os_ui::Canvas;

    use super::{Act, Btn, Logic, Mods, InputFrames};
    use crate::analyzer::{LiveAnalyzer, Look as ALook};
    use crate::params::{self, EqParams, Opt};
    use crate::view::{self, Look, ViewState};
    use crate::Shared;

    /// The editor on its GUI thread.
    struct Ed {
        win: Window,
        canvas: Canvas,
        buf: Vec<u32>,
        scale: f32,
        dpi:f32,
        ctx:Arc<dyn GuiContext>,
        size: (u32, u32),
        params: Arc<EqParams>,
        shared: Arc<Shared>,
        logic: Logic,
        palette: Palette,
        analyzer: LiveAnalyzer,
        input_frames: InputFrames,
        last_sig: u64,
        last_extra: u64,
        last_view: ViewState,
        last_hint: String,
        dirty: bool,
        /// The host link: the host finds the editor by the window it gave it (spec: plugins/host-link.md).
        link: Arc<Peer>,
    }

    impl Drop for Ed {
        /// The editor closes: the host no longer finds it.
        fn drop(&mut self) {
            self.link.unregister();
        }
    }

    /// Owns the editor thread; dropping it closes the editor.
    pub struct Handle(Option<px::EditorThread>, Arc<Shared>);

    impl Drop for Handle {
        fn drop(&mut self) {
            self.0.take();
            self.1.editor_open.store(false, Ordering::Relaxed);
        }
    }

    pub fn open(parent: px::Parent, scale: f32, params: Arc<EqParams>, shared: Arc<Shared>, ctx: Arc<dyn GuiContext>, slot: &px::Slot) -> Handle {
        let dpi = if scale > 0.0 { scale } else { 1.0 };
        let logical=(shared.editor_w.load(Ordering::Relaxed),shared.editor_h.load(Ordering::Relaxed));
        let scale=dpi;
        let size=((logical.0 as f32*dpi).round() as u32,(logical.1 as f32*dpi).round() as u32);
        let sh = shared.clone();
        // One page; joined, the editor keeps its size (it has no strip row of its own).
        let wake = slot.clone();
        let link = Peer::register(parent as u64, vec![PageIcon::Plugin], logical, move || px::notify(&wake));
        link.set_resizable((view::MIN_W,view::MIN_H));
        let peer = link.clone();
        let thread = px::spawn(parent, size, slot, move |win| {
            let Ok(canvas) = Canvas::new(size.0, size.1, scale) else { return };
            let logic = Logic::new(params.clone(), shared.clone(), Some(ctx.clone()));
            let ed = Ed {
                win,
                canvas,
                buf: Vec::new(),
                scale,
                dpi,
                ctx:ctx.clone(),
                size,
                params,
                shared,
                logic,
                palette: os_plugkit::palette(),
                analyzer: LiveAnalyzer::default(),
                input_frames: InputFrames::default(),
                last_sig: 0,
                last_extra: u64::MAX,
                last_view: ViewState::default(),
                last_hint: String::new(),
                dirty: true,
                link: peer,
            };
            ed.run();
        });
        if thread.is_none() {
            link.unregister();
        }
        sh.editor_open.store(thread.is_some(), Ordering::Relaxed);
        Handle(thread, sh)
    }

    fn btn(b: Button) -> Btn {
        match b {
            Button::Left => Btn::Left,
            Button::Right => Btn::Right,
            Button::Middle => Btn::Middle,
        }
    }

    impl Ed {
        fn run(mut self) {
            while let Some(input) = self.win.next() {
                match input {
                    Input::Expose => self.dirty = true,
                    Input::Tick => self.poll(),
                    Input::Down { x, y, btn: b, dbl } => {
                        let (x, y) = (x / self.scale, y / self.scale);
                        let acts = self.logic.down(x, y, btn(b), self.mods(), dbl && b == Button::Left);
                        self.act(acts);
                    }
                    Input::Move { x, y } => {
                        let acts = self.logic.moved(x / self.scale, y / self.scale, self.mods());
                        self.act(acts);
                        if self.input_frames.due(){self.poll();if self.dirty{self.paint();}}
                    }
                    Input::Delta { dx, dy } => {
                        let acts = self.logic.delta(dx / self.scale, dy / self.scale, self.mods());
                        self.act(acts);
                        if self.input_frames.due(){self.poll();if self.dirty{self.paint();}}
                    }
                    Input::Up { .. } => {
                        self.win.unlock();
                        let acts = self.logic.up(Btn::Left);
                        self.act(acts);
                    }
                    Input::Leave => {
                        if !self.win.locked() && !self.logic.dragging() {
                            let acts = self.logic.leave();
                            self.act(acts);
                        }
                    }
                    Input::Wheel { x, y, notches } => {
                        let acts = self.logic.wheel(x / self.scale, y / self.scale, notches, self.mods());
                        self.act(acts);
                    }
                    Input::Key { .. } | Input::Drop { .. } => {}
                }
                if self.dirty && self.win.idle() {
                    self.paint();
                }
            }
        }

        fn mods(&self) -> Mods {
            let (ctrl, shift, alt) = self.win.mods();
            Mods { ctrl, shift, alt }
        }

        fn screen(&self, x: f32, y: f32) -> (i32, i32) {
            self.win.to_root((x * self.scale) as i32, (y * self.scale) as i32)
        }

        /// Analyzer settings, or `None` when nothing is monitored.
        fn analyzer_look(&self) -> Option<ALook> {
            let b = self.params.extra_copy();
            let on = params::byte(&b, params::MONITOR_BYTE) != 0 && (params::flag(&b, params::F_HISTOGRAM) || params::flag(&b, params::F_HEATMAP));
            on.then(|| ALook {
                precision: params::opt(&b, Opt::FreqPrecision).clamp(0, 7) as usize,
                slope: params::opt(&b, Opt::PivotSlope).clamp(0, 4) as usize,
                smoothing: params::opt(&b, Opt::TimeSmoothing).clamp(0, 4) as usize,
                average: params::flag(&b, params::F_AVERAGE),
            })
        }

        /// Timer / parameter change: expire confirmed overlay values, analyse new audio, repaint when anything shown
        /// changed.
        fn poll(&mut self) {
            if let Some(wanted)=self.link.take_size_request(){let old=(self.shared.editor_w.load(Ordering::Relaxed),self.shared.editor_h.load(Ordering::Relaxed));let logical=view::resized(old,wanted);self.shared.editor_w.store(logical.0,Ordering::Relaxed);self.shared.editor_h.store(logical.1,Ordering::Relaxed);self.scale=self.dpi;self.size=((logical.0 as f32*self.dpi).round() as u32,(logical.1 as f32*self.dpi).round() as u32);self.canvas.resize(self.size.0,self.size.1,self.scale);self.win.resize(self.size.0,self.size.1);self.link.set_size(logical.0,logical.1);self.ctx.request_resize();self.dirty=true;}

            self.logic.dynamic_meter();
            let mut dirty = self.logic.vals.expire();
            let analyzer_look=self.analyzer_look();
            dirty |= self.analyzer.poll(&self.shared,analyzer_look);
            let sig = self.logic.vals.signature();
            let ev = self.params.extra_version.load(Ordering::Acquire);
            if dirty || sig != self.last_sig || ev != self.last_extra || self.logic.view != self.last_view || self.logic.hint != self.last_hint {
                self.dirty = true;
            }
            if self.logic.hint != self.last_hint {
                self.last_hint.clone_from(&self.logic.hint);
                // The host shows the hint in its title bar (hosted mode).
                self.link.set_hint(&self.logic.hint);
                self.link.to_host(Cmd::Changed, 0);
            }
            // Joined the strip: "Create automation clip" works.
            self.logic.hosted = self.link.hosted();
        }

        fn act(&mut self, acts: Vec<Act>) {
            for a in acts {
                match a {
                    Act::Repaint => self.dirty = true,
                    Act::Lock => self.win.lock(),
                    Act::Menu(req, x, y) => {
                        let def = self.logic.menu(&req);
                        let (sx, sy) = self.screen(x, y);
                        if let Some(cmd) = self.win.popup(sx, sy, &def, &self.palette, self.scale) {
                            let acts = self.logic.pick(&req, cmd, x, y);
                            self.act(acts);
                        }
                        self.dirty = true;
                    }
                    Act::TypeIn(id, x, y) => {
                        let (sx, sy) = self.screen(x, y);
                        let cur = self.logic.value_text(&id);
                        if let Some(t) = self.win.type_in(sx, sy, &cur, self.scale) {
                            self.logic.typed(&id, &t);
                        }
                        self.dirty = true;
                    }
                    // The host makes the clip; the index is the parameter's list position.
                    Act::Automate(i) => {
                        self.link.to_host(Cmd::Automate, i as isize);
                    }
                }
            }
        }

        fn paint(&mut self) {
            self.dirty = false;
            self.logic.dynamic_meter();
            self.logic.read_visibility();
            let extra = self.logic.shown_extra();
            let histogram = params::flag(&extra, params::F_HISTOGRAM) || params::flag(&extra, params::F_HEATMAP);
            let spectrum = (histogram && params::byte(&extra, params::MONITOR_BYTE) != 0 && !self.analyzer.analyzer.cols.is_empty()).then_some(self.analyzer.analyzer.cols.as_slice());
            let look = Look { s: self.logic.shown(), extra: &extra, spectrum, rate: f64::from(self.shared.rate.load(Ordering::Relaxed)) };
            self.logic.view.size=self.canvas.size();
            self.logic.hits = view::paint(&mut self.canvas, &look, &self.logic.view);
            self.last_sig = self.logic.vals.signature();
            self.last_extra = self.params.extra_version.load(Ordering::Acquire);
            self.last_view = self.logic.view.clone();
            self.buf.resize((self.size.0 * self.size.1) as usize, 0);
            self.canvas.copy_to_xrgb(&mut self.buf);
            self.win.blit(&self.buf, self.size.0, self.size.1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logic() -> Logic {
        let mut l = Logic::new(Arc::new(EqParams::default()), Arc::new(Shared::default()), None);
        l.view.created=0x7f;
        paint(&mut l);
        l
    }

    fn paint(l: &mut Logic) {
        let mut c = os_ui::Canvas::new(view::W, view::H, 1.0).unwrap();
        let extra = l.shown_extra();
        let look = view::Look { s: l.shown(), extra: &extra, spectrum: None, rate: 48000.0 };
        l.view.size=c.size();
        l.hits = view::paint(&mut c, &look, &l.view);
    }

    fn centre(l: &Logic, t: Target) -> (f32, f32) {
        let h = l.hits.iter().rev().find(|h| h.t == t).unwrap_or_else(|| panic!("no hit {t:?}"));
        (h.r.cx(), h.r.cy())
    }

    /// A knob's "Create automation clip" works only inside Lemon Studio (host link) and asks for the parameter's list
    /// position: band 1 frequency is parameter 7 (spec: plugins/plugin-parameters.md §3, plugins/host-link.md).
    #[test]
    fn create_automation_clip_asks_the_host_for_the_parameter_index() {
        let mut l = logic();
        let req = MenuReq::Param(freq_id(0));
        let entry = |l: &Logic| {
            let m = l.menu(&req);
            let i = m.items.iter().position(|it| matches!(it, MenuItem::Item { label, .. } if label == "Create automation clip")).unwrap();
            (matches!(&m.items[i], MenuItem::Item { enabled: true, .. }), m.cmds[i].clone())
        };
        assert!(!entry(&l).0, "another host: disabled");
        l.hosted = true;
        let (enabled, cmd) = entry(&l);
        assert!(enabled);
        assert_eq!(l.pick(&req, cmd, 0.0, 0.0), vec![Act::Automate(7)]);
    }

    #[test]
    fn token_drag_tracks_graph_coordinates_and_fine_modifier(){
        let mut l=logic();let(x,y)=centre(&l,Target::Token(3));let layout=l.layout();let before=l.shown().bands[3];l.down(x,y,Btn::Left,Mods::default(),false);l.delta(layout.plot.w*0.1,-layout.plot.h*0.1,Mods::default());l.up(Btn::Left);
        assert!((l.shown().bands[3].freq-before.freq-0.1).abs()<1e-5);assert!((l.shown().bands[3].db-3.6).abs()<0.01);
        paint(&mut l);let(x,y)=centre(&l,Target::Token(3));l.down(x,y,Btn::Left,Mods::default(),false);l.delta(0.,-layout.plot.h*0.1,Mods{shift:true,..Mods::default()});l.up(Btn::Left);assert!((l.shown().bands[3].db-4.5).abs()<0.01);
        paint(&mut l);let(x,y)=centre(&l,Target::Token(3));let mask=l.mask();l.down(x,y,Btn::Left,Mods{alt:true,..Mods::default()},false);assert_eq!(l.mask(),mask^(1<<3));l.down(x,y,Btn::Left,Mods{alt:true,..Mods::default()},false);let w=l.shown().bands[3].width;l.wheel(x,y,1.,Mods::default());assert!((l.shown().bands[3].width-w-0.025).abs()<1e-5);
    }

    #[test]
    fn peak_eq_placement_menu_and_compare(){
        let mut l=logic();let req=MenuReq::Token(0);let menu=l.menu(&req);
        assert_eq!(menu.find("Mid"),Some(Cmd::Route(0,1)));assert_eq!(menu.find("Right"),Some(Cmd::Route(0,4)));
        l.pick(&req,Cmd::Route(0,1),0.,0.);l.compare(false);l.pick(&req,Cmd::Route(0,2),0.,0.);l.compare(true);
        assert_eq!(eq::routed::routes(&l.params.extra_copy(),false)[0],1);l.compare(true);assert_eq!(eq::routed::routes(&l.params.extra_copy(),false)[0],2);
        if let Some(dir)=std::env::var_os("OS_SHOT_DIR"){
            let mut c=os_ui::Canvas::new(view::W*3/2,view::H*3/2,1.5).unwrap();let extra=l.params.extra_copy();
            view::paint(&mut c,&view::Look{s:l.shown(),extra:&extra,spectrum:None,rate:48000.},&l.view);c.pixmap.save_png(std::path::PathBuf::from(dir).join("eq-routing.png")).unwrap();
        }
    }

    #[test]
    fn selected_band_shape_order_and_frequency_menu(){
        let mut l=logic();l.select(1);paint(&mut l);let(x,y)=centre(&l,Target::BandMenu(1));assert!(l.down(x,y,Btn::Left,Mods::default(),false).iter().any(|a|matches!(a,Act::Menu(MenuReq::Token(1),..))));l.pick(&MenuReq::Token(1),Cmd::SetType(1,eq::HIGH_SHELF),0.,0.);assert_eq!(l.shown().bands[1].kind,eq::HIGH_SHELF);
        let m=l.menu(&MenuReq::Token(0));assert_eq!(m.find("Steep 6"),Some(Cmd::SetOrder(0,-2)));assert_eq!(m.find("A4 - 440.0Hz"),Some(Cmd::SetHz(0,440.)));l.pick(&MenuReq::Token(0),Cmd::SetHz(0,440.),0.,0.);assert!((l.shown().bands[0].hz()-440.).abs()<0.05);
    }

    /// Whether the first entry labelled `label` in `m` or its submenus can be picked.
    fn clickable(m: &MenuDef<Cmd>, label: &str) -> bool {
        let it = m.items.iter().chain(m.subs.iter().flat_map(|(_, s)| s.items.iter()));
        it.clone().find(|it| matches!(it, MenuItem::Item { label: l, .. } if l == label)).unwrap_or_else(|| panic!("no {label}")).clickable()
    }

    /// LIN and HQ switch the processing (spec: plugins/lemon-parametric-eq-2.md §4, §5; plugins/fruity-parametric-eq-2.md
    /// "Which processing a state selects").
    #[test]
    fn lin_and_hq_switch_the_processing() {
        let mut l = logic();
        let hq = |l: &Logic| byte(&l.params.extra_copy(), params::HQ_BYTE);
        let opts = |l: &Logic| eq::Options::from_image(&l.params.extra_copy());
        assert_eq!(hq(&l), 1, "a new instance: HQ Legacy");
        assert_eq!(opts(&l).mode(), eq::Mode::HqLegacy);
        for (t, hint) in [(Target::Lin, "Linear phase"), (Target::Hq, "High quality (oversampling)")] {
            let (x, y) = centre(&l, t);
            l.moved(x, y, Mods::default());
            assert_eq!(l.hint, hint);
        }
        // LIN on and off.
        let (x, y) = centre(&l, Target::Lin);
        l.down(x, y, Btn::Left, Mods::default(), false);
        assert_eq!(opts(&l).mode(), eq::Mode::Lin);
        assert_eq!(opts(&l).latency(44100.0), 1536);
        l.pick(&MenuReq::Lin, Cmd::Byte(params::LIN_PRECISION_BYTE, 4), 0.0, 0.0);
        assert_eq!(opts(&l).latency(44100.0), 6144);
        l.down(x, y, Btn::Left, Mods::default(), false);
        assert_eq!(opts(&l).mode(), eq::Mode::HqLegacy);
        // HQ+ from the menu; the toggle switches bit 0 and keeps the mode.
        let (x, y) = centre(&l, Target::Hq);
        assert!(l.down(x, y, Btn::Right, Mods::default(), false).iter().any(|a| matches!(a, Act::Menu(MenuReq::Hq, ..))));
        l.pick(&MenuReq::Hq, Cmd::Hq(3), 0.0, 0.0);
        assert_eq!((hq(&l), opts(&l).mode()), (3, eq::Mode::HqPlus));
        l.down(x, y, Btn::Left, Mods::default(), false);
        assert_eq!((hq(&l), opts(&l).mode()), (2, eq::Mode::Plain));
        assert!(clickable(&l.menu(&MenuReq::Hq), "HQ+"));
        l.down(x, y, Btn::Left, Mods::default(), false);
        assert_eq!((hq(&l), opts(&l).mode()), (3, eq::Mode::HqPlus));
        l.pick(&MenuReq::Hq, Cmd::Hq(2), 0.0, 0.0);
        assert_eq!((hq(&l), opts(&l).mode(), opts(&l).latency(44100.0)), (1, eq::Mode::HqEnhanced, 44));
        // Options nothing reads yet are greyed out; LIN's crossfade ("Linear phase mod speed") and the analyzer's are not.
        let o = l.menu(&MenuReq::Options);
        for label in ["Phase rotation", "Enhanced frequency", "High precision", "Smooth", "Adaptive", "Inherit bandwidth"] {
            assert!(!clickable(&o, label), "{label}");
        }
        for label in ["Lock spare state", "-120dB", "Average mode", "Maximum - 8192", "Ultra"] {
            assert!(clickable(&o, label), "{label}");
        }
        assert!(l.menu(&MenuReq::Phones).items.iter().all(|it| !it.clickable()));
    }

    #[test]
    fn mute_solo_and_compare() {
        let mut l = logic();
        l.select(2);paint(&mut l);
        let (x, y) = centre(&l, Target::Switch(2));
        l.down(x, y, Btn::Left, Mods::default(), false);
        assert_eq!(l.mask(), 0x7f & !(1 << 2));
        l.down(x, y, Btn::Right, Mods::default(), false);
        assert_eq!(l.mask(), 1 << 2);
        l.down(x, y, Btn::Right, Mods::default(), false);
        assert_eq!(l.mask(), 0x7f);
        // A/B: store, change, flip back.
        l.compare(false);
        l.vals.edit_plain(&level_id(4), 6.0);
        l.compare(true);
        assert_eq!(l.vals.plain(&level_id(4)), 0.0);
        l.compare(true);
        assert_eq!(l.vals.plain(&level_id(4)), 6.0);
    }

    #[test]
    fn contextual_dynamic_controls_automation_compare_and_recall(){
        let mut l=logic();l.select(3);l.view.details_open=true;paint(&mut l);let expanded=false;let(cx,cy,r)=l.layout().ring(&l.shown(),&l.view,expanded);l.down(cx+r,cy,Btn::Left,Mods::default(),false);l.moved(cx+r,cy+40.,Mods::default());l.up(Btn::Left);assert!((l.dynamic_values().bands[3][0]+6.).abs()<0.01);
        paint(&mut l);let(x,y)=centre(&l,Target::ExpandDynamics);l.down(x,y,Btn::Left,Mods::default(),false);paint(&mut l);let(x,y)=centre(&l,Target::Dynamic(13));l.down(x,y,Btn::Left,Mods::default(),false);l.moved(x,y-20.,Mods::default());l.up(Btn::Left);assert!(l.dynamic_values().bands[3][1]>-30.);
        l.vals.edit_plain(&params::dynamic_id(28),5.);l.vals.edit_plain(&params::dynamic_id(43),1.);let saved=l.dynamic_values();l.compare(false);l.vals.edit_plain(&params::dynamic_id(12),3.);l.compare(true);assert_eq!(l.dynamic_values(),saved);
        let state=params::to_fl(&l.shown(),&l.shown_extra());let back=params::fl_from_plugin_state(&params::plugin_state_from_fl(&state)).unwrap();assert_eq!(eq::dynamic::Config::load(&back,false),saved);l.hosted=true;assert_eq!(l.pick(&MenuReq::Param(params::dynamic_id(12)),Cmd::Automate(params::dynamic_id(12)),0.,0.),vec![Act::Automate(48)]);
        let(x,y)=centre(&l,Target::HarshPanel);l.down(x,y,Btn::Left,Mods::default(),false);paint(&mut l);assert!(l.view.harsh_open);let(x,y)=centre(&l,Target::Dynamic(28));l.down(x,y,Btn::Left,Mods::default(),false);l.moved(x,y-20.,Mods::default());l.up(Btn::Left);assert!(l.dynamic_values().harsh[0]>5.);
        let point=(l.layout().plot.x+5.,l.layout().plot.y+70.);l.down(point.0,point.1,Btn::Left,Mods::default(),false);l.up(Btn::Left);paint(&mut l);assert!(!l.view.harsh_open && !l.view.show_dynamics);
    }
    #[test]
    fn dynamic_spectral_and_deharsh_drags_keep_the_release_position() {
        for (parameter, spectral, harsh) in [(13, false, false), (12, true, false), (28, false, true)] {
            let mut l = logic();
            l.select(3);
            l.view.details_open = true;
            l.view.harsh_open = harsh;
            l.vals.edit_plain(&params::dynamic_id(12), -6.);
            l.vals.edit_plain(&params::dynamic_id(33), 0.);
            l.vals.edit_plain(&params::dynamic_id(43), if spectral { 1. } else { 0. });
            if parameter == 13 {
                l.view.selection |= 1 << 4;
            }
            paint(&mut l);
            let (x, y) = centre(&l, Target::Dynamic(parameter));
            let target = l.hit(x, y);
            assert_eq!(target, Some(Target::Dynamic(parameter)));
            let id = params::dynamic_id(parameter);
            let before = l.vals.norm(&id);
            let linked = params::dynamic_id(17);
            let linked_before = l.vals.norm(&linked);
            let actions = l.down(x, y, Btn::Left, Mods::default(), false);
            assert!(!actions.contains(&Act::Lock));
            assert!(l.dragging());
            l.moved(x + 3., y - 24., Mods::default());
            assert!((l.vals.norm(&id) - before - 0.1).abs() < 0.002);
            if parameter == 13 {
                assert!((l.vals.norm(&linked) - linked_before - 0.1).abs() < 0.002);
            }
            l.up(Btn::Left);
            assert_eq!(l.view.mouse, Some((x + 3., y - 24.)));
            let after = l.vals.norm(&id);
            l.moved(x + 3., y - 40., Mods::default());
            assert_eq!(l.vals.norm(&id), after);
            l.undo_redo(false);
            assert_eq!(l.vals.norm(&id), before);
        }
    }

    #[test]
    fn free_resize_and_independent_ui_scale_keep_inputs_aligned(){
        for dpi in [1.,1.5,2.] {for wanted in [(640,420),(1400,460),(740,1000)] {let size=view::resized((1000,620),wanted);assert_eq!(size,wanted);let mut l=logic();l.select(3);l.vals.edit_plain(&params::dynamic_id(12),-6.);
            let mut c=os_ui::Canvas::new((size.0 as f32*dpi)as u32,(size.1 as f32*dpi)as u32,dpi).unwrap();l.view.size=c.size();let extra=l.shown_extra();l.hits=view::paint(&mut c,&view::Look{s:l.shown(),extra:&extra,spectrum:None,rate:48000.},&l.view);let h=l.hits.iter().find(|h|h.t==Target::Dynamic(13)).unwrap();let p=(h.r.cx()*dpi,h.r.cy()*dpi);l.down(p.0/dpi,p.1/dpi,Btn::Left,Mods::default(),false);l.delta(0.,-20.,Mods::default());l.up(Btn::Left);assert!(l.dynamic_values().bands[3][1]>-30.);let before=l.layout().plot;l.view.ui_scale=1.25;assert_eq!(l.view.size,(size.0 as f32,size.1 as f32));assert!(l.layout().plot.w<before.w);
        }}
    }

    #[test]
    fn create_select_drag_group_delete_and_history(){
        let mut l=logic();l.view.created=0;paint(&mut l);assert!(!l.hits.iter().any(|h|matches!(h.t,Target::Token(_))));
        let layout=l.layout();let p=(layout.x(eq::freq_pos(1000.)),layout.y(3.));l.down(p.0,p.1,Btn::Left,Mods::default(),false);l.up(Btn::Left);assert_eq!(l.view.selection,1);assert!((l.shown().bands[0].hz()-1000.).abs()<0.1);
        let p=(layout.x(eq::freq_pos(3500.)),layout.y(-3.));l.down(p.0,p.1,Btn::Left,Mods::default(),true);l.up(Btn::Left);assert_eq!(l.view.selection,2);paint(&mut l);
        let p=centre(&l,Target::Token(0));l.down(p.0,p.1,Btn::Left,Mods{ctrl:true,..Mods::default()},false);l.up(Btn::Left);assert_eq!(l.view.selection,3);
        paint(&mut l);let before=l.shown();let p=centre(&l,Target::Token(1));l.down(p.0,p.1,Btn::Left,Mods::default(),false);l.delta(layout.plot.w*0.04,-10.,Mods::default());l.up(Btn::Left);let moved=l.shown();for i in 0..2{assert!((moved.bands[i].freq-before.bands[i].freq-0.04).abs()<1e-5);}l.undo_redo(false);assert_eq!(l.shown(),before);l.undo_redo(true);assert_eq!(l.shown(),moved);
        paint(&mut l);let p=centre(&l,Target::DeleteBand);l.down(p.0,p.1,Btn::Left,Mods::default(),false);l.up(Btn::Left);assert_eq!((l.shown().bands[0].kind,l.shown().bands[1].kind),(0,0));l.undo_redo(false);assert_eq!(l.shown(),moved);
        l.typed("q2","2.0");assert!((view::q_of(l.shown().bands[1].width)-2.).abs()<0.01);
        let mask=l.view.selection;let b=l.shown();paint(&mut l);let p=centre(&l,Target::Fader(1));l.down(p.0,p.1,Btn::Left,Mods::default(),false);l.delta(0.,-20.,Mods::default());l.up(Btn::Left);assert_eq!(l.view.selection,mask);assert!((l.shown().bands[0].db-b.bands[0].db-l.shown().bands[1].db+b.bands[1].db).abs()<0.01);
    }

    #[test]
    fn hints() {
        let mut l = logic();
        let (x, y) = centre(&l, Target::Token(0));
        l.moved(x, y, Mods::default());
        assert_eq!(l.hint, "0.0dB 63Hz 61%\tBand 1 (low shelf / Stereo)");
        let (x, y) = centre(&l, Target::Fader(MAIN_FADER));
        l.moved(x, y, Mods::default());
        assert_eq!(l.hint, "Main level: 0.0dB");
        l.select(0);paint(&mut l);
        let (x, y) = centre(&l, Target::OrderMark(0));
        l.moved(x, y, Mods::default());
        assert_eq!(l.hint, "Band 1 order: 2");
    }
}

#[cfg(test)]mod modern_regression_tests{
 use super::*;
 #[test]fn all_compact_manual_and_harsh_controls_stay_inside_resized_windows(){
  for (w,h) in [(640,420),(1400,480),(740,1000)]{for scale in [1.,1.5]{for mode in 0..3{let mut l=Logic::new(Arc::new(EqParams::default()),Arc::new(Shared::default()),None);l.view.size=(w as f32,h as f32);l.view.ui_scale=scale;l.select(3);l.vals.edit_plain(&params::dynamic_id(12),-6.);l.vals.edit_plain(&params::dynamic_id(33),if mode==0{1.}else{0.});l.view.harsh_open=mode==2;let mut c=os_ui::Canvas::new(w,h,1.).unwrap();let extra=l.shown_extra();l.hits=view::paint(&mut c,&view::Look{s:l.shown(),extra:&extra,spectrum:None,rate:48000.},&l.view);for hit in &l.hits{assert!(hit.r.x>=0.&&hit.r.y>=0.&&hit.r.right()<=w as f32&&hit.r.bottom()<=h as f32,"{w}x{h} scale{scale} mode{mode}: {hit:?}");}let target=if mode==2{Target::Dynamic(28)}else{Target::Freq(3)};let hit=l.hits.iter().find(|h|h.t==target).unwrap();let before=l.vals.plain(&if mode==2{params::dynamic_id(28)}else{freq_id(3)});l.down(hit.r.cx(),hit.r.cy(),Btn::Left,Mods::default(),false);l.delta(0.,-15.,Mods::default());l.up(Btn::Left);assert!(l.vals.plain(&if mode==2{params::dynamic_id(28)}else{freq_id(3)})>before);}}}
 }
 #[test]fn creating_a_band_preserves_existing_flat_dynamic_bands(){let mut l=Logic::new(Arc::new(EqParams::default()),Arc::new(Shared::default()),None);l.vals.edit_plain(&params::dynamic_id(0),-6.);let before=l.shown().bands[0];let layout=l.layout();assert_eq!(l.create_band(layout.x(0.6),layout.y(3.)),Some(1));assert_eq!(l.shown().bands[0],before);assert_eq!(l.dynamic_values().bands[0][0],-6.);assert!(l.visible(0));}
}

// A held audition must never leave the saved band mask changed when its window closes.
impl Drop for Logic {
 fn drop(&mut self){self.shared.solo.store(-1,Ordering::Relaxed);self.shared.gain_match.store(false,Ordering::Relaxed);}
}
#[cfg(test)]mod audition_close_tests{
 use super::*;
 #[test]fn closing_while_auditioning_restores_the_saved_band_mask(){let params=Arc::new(EqParams::default());let shared=Arc::new(Shared::default());{let mut l=Logic::new(params.clone(),shared.clone(),None);l.set_mask(0x35);l.drag=Some(Drag::Solo{restore:None});l.audition(Some(3));}assert_eq!(params::mask(&params.extra_copy()),0x35);assert_eq!(shared.solo.load(Ordering::Relaxed),-1);}
}

#[cfg(test)]mod direct_pointer_tests{
 use super::*;
 #[test]fn point_and_group_drags_follow_visible_pointer_without_lock_or_release_snap(){
  for size in [(640.,420.),(1000.,620.),(1400.,480.)]{let mut l=Logic::new(Arc::new(EqParams::default()),Arc::new(Shared::default()),None);l.view.size=size;let layout=l.layout();let x=layout.x(0.5);let y=layout.y(3.);l.create_band(x,y);l.create_band(layout.x(0.7),layout.y(-2.));l.view.selection=3;l.view.selected=0;l.view.show_dynamics=false;let mut canvas=os_ui::Canvas::new(size.0 as u32,size.1 as u32,1.).unwrap();let extra=l.shown_extra();l.hits=view::paint(&mut canvas,&view::Look{s:l.shown(),extra:&extra,spectrum:None,rate:48000.},&l.view);let before=l.shown();let node=layout.node(&before,0);let actions=l.down(node.0,node.1,Btn::Left,Mods::default(),false);assert!(!actions.contains(&Act::Lock));let dest=(node.0+layout.plot.w*0.08,node.1-layout.plot.h*0.05);l.moved(dest.0,dest.1,Mods::default());let after=l.shown();for k in 0..2{assert!((after.bands[k].freq-before.bands[k].freq-0.08).abs()<1e-5);}let moved=layout.node(&after,0);assert!((moved.0-dest.0).abs()<0.1&&(moved.1-dest.1).abs()<0.1);l.up(Btn::Left);assert_eq!(l.view.mouse,Some(dest));assert_eq!(l.shown(),after);l.moved(dest.0+5.,dest.1+5.,Mods::default());assert_eq!(l.shown(),after);l.undo_redo(false);assert_eq!(l.shown(),before);
   // A new curve-created point also uses normal captured motion.
   let mut l=Logic::new(Arc::new(EqParams::default()),Arc::new(Shared::default()),None);l.view.size=size;let extra=l.shown_extra();l.hits=view::paint(&mut canvas,&view::Look{s:l.shown(),extra:&extra,spectrum:None,rate:48000.},&l.view);let p=(layout.x(0.4),layout.y(0.));let actions=l.down(p.0,p.1,Btn::Left,Mods::default(),false);assert!(!actions.contains(&Act::Lock));l.moved(p.0+20.,p.1-10.,Mods::default());l.up(Btn::Left);let node=layout.node(&l.shown(),0);assert!((node.0-p.0-20.).abs()<0.1&&(node.1-p.1+10.).abs()<0.1);
  }
 }
}

#[cfg(test)]mod explicit_modes_tests{
 use super::*;
 fn paint(l:&mut Logic){let mut c=os_ui::Canvas::new(1000,620,1.).unwrap();l.view.size=c.size();let extra=l.shown_extra();l.hits=view::paint(&mut c,&view::Look{s:l.shown(),extra:&extra,spectrum:None,rate:48000.},&l.view);}
 fn press(l:&mut Logic,t:Target){paint(l);let r=l.hits.iter().find(|h|h.t==t).unwrap().r;l.down(r.cx(),r.cy(),Btn::Left,Mods::default(),false);l.up(Btn::Left);}
 fn checked(l:&Logic,label:&str)->bool{l.menu(&MenuReq::Token(3)).items.iter().any(|item|matches!(item,MenuItem::Item{label:name,mark:Mark::Check(true),..}if name==label))}
 #[test]fn labeled_buttons_menus_and_saved_processing_state_agree(){let mut l=Logic::new(Arc::new(EqParams::default()),Arc::new(Shared::default()),None);l.select(3);l.view.details_open=true;paint(&mut l);assert_eq!(l.dynamic_values().bands[3][0],0.);assert!(!checked(&l,"Dynamic EQ"));press(&mut l,Target::DynamicToggle);assert_eq!(l.dynamic_values().bands[3][0],-6.);assert!(l.dynamic_values().auto[3]);assert!(checked(&l,"Dynamic EQ"));l.vals.edit_plain(&params::dynamic_id(12),-9.);press(&mut l,Target::DynamicToggle);assert_eq!(l.dynamic_values().bands[3][0],0.);press(&mut l,Target::DynamicToggle);assert_eq!(l.dynamic_values().bands[3][0],-9.);
  let req=MenuReq::Token(3);assert_eq!(l.menu(&req).find("Dynamic EQ"),Some(Cmd::Dynamic(3)));assert_eq!(l.menu(&req).find("Spectral dynamics"),Some(Cmd::Spectral(3)));l.pick(&req,Cmd::Spectral(3),0.,0.);assert!(checked(&l,"Spectral dynamics"));assert!(checked(&l,"Dynamic EQ"));press(&mut l,Target::DynamicToggle);assert!(!checked(&l,"Dynamic EQ")&&!checked(&l,"Spectral dynamics"));l.pick(&req,Cmd::Spectral(3),0.,0.);assert!(checked(&l,"Spectral dynamics"));press(&mut l,Target::SpectralBand);assert!(checked(&l,"Dynamic EQ")&&!checked(&l,"Spectral dynamics"));let before=l.dynamic_values();l.pick(&req,Cmd::Dynamic(3),0.,0.);l.undo_redo(false);assert_eq!(l.dynamic_values(),before);
  let state=params::to_fl(&l.shown(),&l.shown_extra());let recalled=params::fl_from_plugin_state(&params::plugin_state_from_fl(&state)).unwrap();assert_eq!(eq::dynamic::Config::load(&recalled,false),l.dynamic_values());
 }
 #[test]fn empty_graph_context_creates_regular_dynamic_or_spectral_bands(){for mode in 0..3{let mut l=Logic::new(Arc::new(EqParams::default()),Arc::new(Shared::default()),None);paint(&mut l);let layout=l.layout();let pos=(layout.x(0.5),layout.y(3.));let actions=l.down(pos.0,pos.1,Btn::Right,Mods::default(),false);assert!(actions.contains(&Act::Menu(MenuReq::Graph,pos.0,pos.1)));let label=["Add EQ band","Add dynamic band","Add spectral band"][mode as usize];assert_eq!(l.menu(&MenuReq::Graph).find(label),Some(Cmd::AddBand(mode)));let before=l.shown();l.pick(&MenuReq::Graph,Cmd::AddBand(mode),pos.0,pos.1);l.up(Btn::Right);assert_eq!(l.undo.len(),1);let cfg=l.dynamic_values();assert_eq!(cfg.bands[0][0],if mode==0{0.}else{-6.});assert_eq!(cfg.spectral&1!=0,mode==2);assert!((l.shown().bands[0].freq-0.5).abs()<1e-5);assert!((l.shown().bands[0].db-3.).abs()<0.01);l.undo_redo(false);assert_eq!(l.shown(),before);}}
}

#[cfg(test)]
mod band_card_tests {
    use super::*;

    #[test]
    fn band_card_bypass_audition_drag_and_details_are_independent() {
        for (width, height) in [(640, 420), (1000, 620)] {
            let mut l = Logic::new(Arc::new(EqParams::default()), Arc::new(Shared::default()), None);
            let mut c = os_ui::Canvas::new(width, height, 1.).unwrap();
            l.view.size = c.size();
            let initial = l.layout();
            l.create_band(initial.x(0.53), initial.y(-15.));
            let k = l.view.selected;
            let paint = |l: &mut Logic, c: &mut os_ui::Canvas| {
                let extra = l.shown_extra();
                l.hits = view::paint(c, &view::Look { s: l.shown(), extra: &extra, spectrum: None, rate: 48000. }, &l.view);
            };
            let click = |l: &mut Logic, target: Target| {
                let r = l.hits.iter().rev().find(|h| h.t == target).unwrap().r;
                l.down(r.cx(), r.cy(), Btn::Left, Mods::default(), false);
                l.up(Btn::Left);
            };
            paint(&mut l, &mut c);
            let card = l.hits.iter().find(|h| h.t == Target::BandCard).unwrap().r;
            let (x, y) = l.layout().node(&l.shown(), k);
            assert!(!card.outset(8.).contains(x, y), "the deepest part of the cut is clear");
            assert!(card.bottom() < height as f32 - 25.);
            assert!(l.hits.iter().any(|h| h.t == Target::BandMenu(k)));
            assert!(!l.hits.iter().any(|h| h.t == Target::DynamicToggle));
            if width == 1000 {
                if let Some(dir) = std::env::var_os("LEMON_EQ_CARD_PREVIEW") {
                    c.pixmap.save_png(std::path::PathBuf::from(dir).join("lemon-eq-band-card.png")).unwrap();
                }
            }

            let before = l.shown();
            click(&mut l, Target::Switch(k));
            assert!(!l.shown().audible(k));
            assert_eq!(l.shown().bands[k], before.bands[k]);
            l.undo_redo(false);
            assert_eq!(l.shown(), before);

            paint(&mut l, &mut c);
            let mask = l.mask();
            click(&mut l, Target::Audition(k));
            assert_eq!(l.shared.solo.load(Ordering::Relaxed), k as i32);
            assert_eq!(l.mask(), mask);
            let (x, y) = l.layout().node(&l.shown(), k);
            l.down(x, y, Btn::Left, Mods::default(), false);
            l.moved(x + 32., y - 6., Mods::default());
            l.up(Btn::Left);
            assert!(l.shown().bands[k].freq > before.bands[k].freq);
            assert_eq!(l.shared.solo.load(Ordering::Relaxed), k as i32);
            assert_eq!(l.mask(), mask);
            paint(&mut l, &mut c);
            click(&mut l, Target::Audition(k));
            assert_eq!(l.shared.solo.load(Ordering::Relaxed), -1);

            click(&mut l, Target::Details);
            paint(&mut l, &mut c);
            let layout = l.layout();
            assert!(l.hits.iter().any(|h| h.t == Target::Solo(k) && h.r.y > layout.plot.bottom()));
            assert!(l.hits.iter().any(|h| h.t == Target::DynamicToggle && h.r.y > layout.plot.bottom()));
            if width == 1000 {
                if let Some(dir) = std::env::var_os("LEMON_EQ_CARD_PREVIEW") {
                    c.pixmap.save_png(std::path::PathBuf::from(dir).join("lemon-eq-band-details.png")).unwrap();
                }
            }
            l.view.hide_band_card = true;
            paint(&mut l, &mut c);
            assert!(!l.hits.iter().any(|h| matches!(h.t, Target::BandCard | Target::Audition(_))));
            assert!(l.hits.iter().any(|h| h.t == Target::Solo(k)));
        }
    }
}

#[cfg(test)]mod point_menu_tests{
 use super::*;
 fn paint(l:&mut Logic){let mut c=os_ui::Canvas::new(1000,620,1.).unwrap();l.view.size=c.size();let extra=l.shown_extra();l.hits=view::paint(&mut c,&view::Look{s:l.shown(),extra:&extra,spectrum:None,rate:48000.},&l.view);}
 #[test]fn fast_second_click_on_new_point_drags_without_opening_value_entry(){let mut l=Logic::new(Arc::new(EqParams::default()),Arc::new(Shared::default()),None);paint(&mut l);let layout=l.layout();let p=(layout.x(0.5),layout.y(0.));l.down(p.0,p.1,Btn::Left,Mods::default(),false);l.up(Btn::Left);paint(&mut l);let before=l.shown();let node=layout.node(&before,0);let actions=l.down(node.0,node.1,Btn::Left,Mods::default(),true);assert!(!actions.iter().any(|a|matches!(a,Act::TypeIn(..)|Act::Lock)));assert!(l.dragging());let dest=(node.0+70.,node.1-25.);l.moved(dest.0,dest.1,Mods::default());l.up(Btn::Left);let actual=layout.node(&l.shown(),0);assert!((actual.0-dest.0).abs()<0.1&&(actual.1-dest.1).abs()<0.1);assert_eq!(l.view.mouse,Some(dest));l.undo_redo(false);assert_eq!(l.shown(),before);
  paint(&mut l);let node=layout.node(&l.shown(),0);let actions=l.down(node.0,node.1,Btn::Right,Mods::default(),false);assert!(actions.contains(&Act::Menu(MenuReq::Token(0),node.0,node.1)));let req=MenuReq::Token(0);let menu=l.menu(&req);assert_eq!(menu.find("Frequency..."),Some(Cmd::TypeIn(freq_id(0))));assert_eq!(menu.find("Gain..."),Some(Cmd::TypeIn(level_id(0))));assert_eq!(menu.find("Q..."),Some(Cmd::TypeIn("q1".into())));assert_eq!(l.pick(&req,Cmd::TypeIn(freq_id(0)),node.0,node.1),vec![Act::TypeIn(freq_id(0),node.0,node.1)]);l.typed(&freq_id(0),"1500");assert!((l.shown().bands[0].hz()-1500.).abs()<0.2);
 }
 #[test]fn right_click_delete_is_undoable_and_preserves_other_band_ids(){let mut l=Logic::new(Arc::new(EqParams::default()),Arc::new(Shared::default()),None);let layout=l.layout();l.create_band(layout.x(0.4),layout.y(3.));l.create_band(layout.x(0.7),layout.y(-2.));l.select(0);l.toggle_spectral(0);let before=l.shown();let cfg=l.dynamic_values();let menu=l.menu(&MenuReq::Token(0));assert_eq!(menu.find("Delete"),Some(Cmd::DeleteBand(0)));l.pick(&MenuReq::Token(0),Cmd::DeleteBand(0),0.,0.);assert_eq!(l.shown().bands[0].kind,0);assert_eq!(l.shown().bands[1],before.bands[1]);assert_eq!(l.dynamic_values().bands[0][0],0.);assert_eq!(l.dynamic_values().spectral&1,0);l.undo_redo(false);assert_eq!(l.shown(),before);assert_eq!(l.dynamic_values(),cfg);l.undo_redo(true);assert_eq!(l.shown().bands[0].kind,0);l.undo_redo(false);l.view.selection=3;l.pick(&MenuReq::Token(0),Cmd::DeleteBand(0),0.,0.);assert_eq!((l.shown().bands[0].kind,l.shown().bands[1].kind),(0,0));l.undo_redo(false);assert_eq!(l.shown(),before);}
}

#[cfg(test)]mod upgrade_workflow_tests {
 use super::*;
 #[test]fn audition_match_and_compare_use_real_panel_hits_without_saved_level_changes(){
  let params=Arc::new(EqParams::default());let shared=Arc::new(Shared::default());
  {let mut l=Logic::new(params.clone(),shared.clone(),None);l.select(3);l.view.details_open=true;l.vals.edit_plain(&params::dynamic_id(12),-9.);
   let mut c=os_ui::Canvas::new(1000,620,1.).unwrap();l.view.size=c.size();let extra=l.shown_extra();l.hits=view::paint(&mut c,&view::Look{s:l.shown(),extra:&extra,spectrum:None,rate:48000.},&l.view);
   let click=|l:&mut Logic,t|{let r=l.hits.iter().find(|h|h.t==t).unwrap().r;l.down(r.cx(),r.cy(),Btn::Left,Mods::default(),false);l.up(Btn::Left);};
   let before=l.shown_extra();let output=l.vals.plain(MAIN_ID);click(&mut l,Target::GainMatch);assert!(shared.gain_match.load(Ordering::Relaxed));assert_eq!(l.shown_extra(),before);assert_eq!(l.vals.plain(MAIN_ID),output);
   let mask=l.mask();let r=l.hits.iter().find(|h|h.t==Target::Solo(3)).unwrap().r;l.down(r.cx(),r.cy(),Btn::Left,Mods::default(),false);assert_eq!(shared.solo.load(Ordering::Relaxed),3);assert_eq!(l.view.pressed,Some(Target::Solo(3)));l.up(Btn::Left);assert_eq!(l.mask(),mask);assert_eq!(shared.solo.load(Ordering::Relaxed),-1);
   click(&mut l,Target::Store);l.vals.edit_plain(&level_id(3),6.);l.vals.edit_plain(&params::dynamic_id(12),-3.);click(&mut l,Target::Flip);assert!(l.view.compare_b);assert_eq!(l.vals.plain(&level_id(3)),0.);assert_eq!(l.dynamic_values().bands[3][0],-9.);click(&mut l,Target::Flip);assert!(!l.view.compare_b);assert_eq!(l.vals.plain(&level_id(3)),6.);assert_eq!(l.dynamic_values().bands[3][0],-3.);
  }
  assert!(!shared.gain_match.load(Ordering::Relaxed));
 }
}

#[cfg(test)]mod analyzer_menu_tests {
 use super::*;
 #[test]fn compact_analyzer_menu_preserves_audio_and_legacy_resolution_on_recall(){
  let mut l=Logic::new(Arc::new(EqParams::default()),Arc::new(Shared::default()),None);
  assert_eq!(params::opt(&l.params.extra_copy(),Opt::FreqPrecision),5);
  assert_eq!(params::opt(&l.params.extra_copy(),Opt::TimeSmoothing),3);
  let mut c=os_ui::Canvas::new(640,420,1.).unwrap();l.view.size=c.size();let extra=l.shown_extra();
  l.hits=view::paint(&mut c,&view::Look{s:l.shown(),extra:&extra,spectrum:None,rate:48000.},&l.view);
  let hit=l.hits.iter().find(|h|h.t==Target::Analyzer).unwrap();
  let acts=l.down(hit.r.cx(),hit.r.cy(),Btn::Left,Mods::default(),false);l.up(Btn::Left);
  assert!(acts.iter().any(|a|matches!(a,Act::Menu(MenuReq::Analyzer,..))));
  let settings=l.shown();let config=l.dynamic_values();let m=l.menu(&MenuReq::Analyzer);
  assert_eq!(m.find("Slowest"),Some(Cmd::Opt(Opt::TimeSmoothing,4)));
  assert_eq!(m.find("Low - 1024"),Some(Cmd::Opt(Opt::FreqPrecision,4)));
  l.pick(&MenuReq::Analyzer,Cmd::Opt(Opt::TimeSmoothing,4),0.,0.);
  l.pick(&MenuReq::Analyzer,Cmd::Opt(Opt::FreqPrecision,4),0.,0.);
  assert_eq!(l.shown(),settings);assert_eq!(l.dynamic_values(),config);
  for (index,size) in [(0,2048),(1,4096),(2,8192),(3,16384),(4,1024),(5,2048),(6,4096),(7,8192)] {
   let mut image=l.shown_extra();params::set_opt(&mut image,Opt::FreqPrecision,index);
   let recalled=params::fl_from_plugin_state(&params::plugin_state_from_fl(&image)).unwrap();
   assert_eq!(params::opt(&recalled,Opt::FreqPrecision),index);
   assert_eq!(crate::analyzer::Analyzer::size_for(&crate::analyzer::Look{precision:index as usize,smoothing:4,slope:3,average:false}),size);
   let mut a=eq::routed::RoutedEq::from_state(&params::to_fl(&settings,&extra),48000.).unwrap();
   let mut b=eq::routed::RoutedEq::from_state(&params::to_fl(&settings,&recalled),48000.).unwrap();
   let input:Vec<_>=(0..4096).map(|i|0.2*(i as f32*0.17).sin()).collect();let(mut x,mut y,mut z,mut w)=(input.clone(),input.clone(),input.clone(),input);a.process(&mut x,&mut y);b.process(&mut z,&mut w);assert_eq!(x,z);assert_eq!(y,w);assert_eq!(a.latency(),b.latency());
  }
 }
}

#[cfg(test)]mod mouse_refresh_tests {
 use super::*;
 use crate::analyzer::{LiveAnalyzer,Look as AnalyzerLook};
 use std::time::Duration;
 #[test]fn continuous_mouse_only_stream_keeps_analyzer_and_frames_alive(){
  let shared=Arc::new(Shared::default());let mut l=Logic::new(Arc::new(EqParams::default()),shared.clone(),None);
  let mut c=os_ui::Canvas::new(640,420,1.).unwrap();l.view.size=c.size();let extra=l.shown_extra();l.hits=view::paint(&mut c,&view::Look{s:l.shown(),extra:&extra,spectrum:None,rate:44100.},&l.view);
  let mut cadence=InputFrames::default();let mut analyzer=LiveAnalyzer::default();let(mut frames,mut unsolicited,mut last,mut sample)=(0,0,0u64,0usize);
  // Deliberately no Tick, idle or queued Paint events: 2000 consecutive mouse moves.
  for ms in 0..2000u64{
   {let mut scope=shared.scope.lock().unwrap();for _ in 0..44{let pos=scope.pos;scope.buf[pos]=0.1*(sample as f32*std::f32::consts::TAU*440./44100.).sin();scope.pos=(pos+1)%scope.buf.len();scope.written+=1;sample+=1;}}
   let plot=l.layout().plot;let acts=l.moved(plot.x+10.+(ms%400)as f32,plot.y+60.,Mods::default());unsolicited+=usize::from(acts.contains(&Act::Repaint));
   let now=Duration::from_millis(ms);
   if cadence.due_at(now){assert!(ms-last<=34);last=ms;frames+=1;analyzer.poll_at(&shared,Some(AnalyzerLook{precision:5,smoothing:3,slope:3,average:false}),now);let extra=l.shown_extra();l.hits=view::paint(&mut c,&view::Look{s:l.shown(),extra:&extra,spectrum:Some(&analyzer.analyzer.cols),rate:44100.},&l.view);}
  }
  assert_eq!(frames,59);assert!(analyzer.measurements>=58);assert!(analyzer.analyzer.cols.iter().any(|v|*v> -60.));assert_eq!(unsolicited,0,"plain hover must not request2000full redraws");
  // A backlog is skipped instead of rendered in a burst.
  assert!(cadence.due_at(Duration::from_secs(10)));for _ in 0..1000{assert!(!cadence.due_at(Duration::from_secs(10)));}
 }
}
