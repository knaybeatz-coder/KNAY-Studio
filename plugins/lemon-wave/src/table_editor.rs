// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    assets::{FIELD, WaveParams},
    wavetable::{self, CYCLES},
};
use lemon_small_fx::{editor::Extension, view::Model};
use nih_plug::prelude::*;
use os_ui::{Canvas, Rect};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

pub struct TableEditor {
    workbench: std::sync::Mutex<crate::wavetable_editor::Workbench>,
    workbench_loaded: std::sync::atomic::AtomicBool,
    hits: std::sync::Mutex<Vec<lemon_small_fx::view::Hit>>,
    browser: std::sync::Mutex<crate::sound_browser::Browser>,
    modulation: crate::modulation_editor::ModEditor,
    pub params: Arc<WaveParams>,
    cycles: [AtomicUsize; 2],
    library:std::sync::Mutex<crate::preset_ui::Library>,
}
impl TableEditor {
    pub fn new(params: Arc<WaveParams>) -> Self {
        Self {
            browser: Default::default(),
            workbench: Default::default(),
            workbench_loaded: Default::default(),
            hits: Default::default(),
            modulation: crate::modulation_editor::ModEditor::new(params.clone()),
            params,
            cycles: [AtomicUsize::new(5), AtomicUsize::new(5)],
            library:Default::default(),
        }
    }
    pub fn buttons(o: usize) -> [Rect; 3] {
        crate::instrument_view::table_buttons(o)
    }
    pub fn factory_state(index: usize) -> PluginState {
        crate::presets::factory(index)
    }
    fn load_program(&self,state:PluginState,context:Option<&Arc<dyn GuiContext>>){
        self.params.audition.stop();self.params.keyboard.cancel();
        if let Some(context)=context{context.set_state(state);}
        else{
            self.params.deserialize_fields(&state.fields);
            let mut values=os_plugkit::vals::Vals::new(&*self.params.base,self.params.base.live.clone(),None);
            for (id,value) in &state.params{
                if let nih_plug::wrapper::state::ParamValue::I32(v)=value{values.edit_plain(id,*v as f32);}
            }
        }
        if !self.workbench.lock().unwrap().dirty{self.workbench_loaded.store(false,Ordering::Relaxed);}
        self.params.fx_scroll.store(0,Ordering::Relaxed);
        self.params.fx_add.store(false,Ordering::Relaxed);
    }
    fn edit_parameter(&self, k: usize, value: i32, context: Option<&Arc<dyn GuiContext>>) {
        let mut v = os_plugkit::vals::Vals::new(
            &*self.params.base,
            self.params.base.live.clone(),
            context.cloned(),
        );
        v.edit_plain(crate::controls::SETTINGS[k].id, value as f32);
    }
    fn set_noise(&self,noise:Option<crate::sample::Sample>,c:Option<&Arc<dyn GuiContext>>) {
        use crate::controls::*;
        let kind=if noise.is_some(){8}else{0};let mut sources=self.params.sources();sources.noise=noise;
        if let Some(c)=c {let mut state=c.get_state();state.fields.insert(crate::assets::FIELD.into(),serde_json::to_string(&sources).unwrap());
            state.params.insert(SETTINGS[NOISE_TYPE].id.into(),nih_plug::wrapper::state::ParamValue::I32(kind));
            let mut raw=self.params.base.fl_state();raw[4+NOISE_TYPE*4..8+NOISE_TYPE*4].copy_from_slice(&kind.to_le_bytes());let hex:String=raw.iter().map(|b|format!("{b:02x}")).collect();state.fields.insert("state".into(),serde_json::to_string(&hex).unwrap());c.set_state(state);
        }else{match self.params.replace(sources){Ok(())=>self.edit_parameter(NOISE_TYPE,kind,None),Err(e)=>self.params.message(e)}}
    }
    fn source_click(&self,x:f32,y:f32,c:Option<&Arc<dyn GuiContext>>)->bool {
        use crate::{controls::*,instrument_view::*};
        if keyboard::press(&self.params.keyboard,x,y){return true;}
        for(k,r,value,_)in quick_choices(&self.params){if r.contains(x,y){if (RACK_START..RACK_END).contains(&k){self.fx_init(c);}self.edit_parameter(k,value,c);return true;}}
        for i in 0..7{if detail_button(i).contains(x,y){self.params.details.fetch_xor(1<<i,Ordering::Relaxed);return true;}}
        for i in 0..2{if filter_tab(i).contains(x,y){self.params.filter_tab.store(i,Ordering::Relaxed);return true;}}
        if NOISE_IMPORT.contains(x,y){if let Some(path)=rfd::FileDialog::new().set_title("Import noise oscillator WAV (up to 12 seconds)").add_filter("WAV",&["wav"]).pick_file(){match crate::sample::Sample::import(&path,60.){Ok(sample)=>{self.set_noise(Some(sample),c);self.params.message("Noise WAV embedded. Use Loop / 1 Shot and Key Track to shape playback.");},Err(e)=>self.params.message(format!("Noise import failed: {e}"))}}return true;}
        if NOISE_CLEAR.contains(x,y){self.set_noise(None,c);return true;}
        let hit=self.hits.lock().unwrap().iter().find(|h|h.r.contains(x,y)).cloned();
        if let Some(k)=hit.and_then(|h|SPEC.index(h.t.id())) {
            let filter_control=matches!(k,FILTER|FILTER_MODEL|CUTOFF|RESONANCE|DRIVE)||(DUAL_FILTER..CHORD_ON).contains(&k);
            if filter_control||matches!(k,SUB_PAN|NOISE_PAN|SUB_DIRECT|NOISE_DIRECT){self.edit_parameter(DUAL_FILTER,1,c);}
            if k>=A_ON&&SETTINGS[k].range==(0,1){self.edit_parameter(k,if self.params.base.v(k)>0{0}else{1},c);return true;}
        }
        false
    }
    fn fx_patch(&self)->crate::dsp::Patch {crate::fxrack::effective(std::array::from_fn(|k|self.params.base.v(k) as f32))}
    fn fx_init(&self,c:Option<&Arc<dyn GuiContext>>) {
        use crate::controls::*;
        if self.params.base.v(RACK_MODE)==0 {let p=self.fx_patch();for k in RACK_START..RACK_END {self.edit_parameter(k,p[k] as i32,c);}self.edit_parameter(RACK_MODE,1,c);}
    }
    fn fx_click(&self,x:f32,y:f32,c:Option<&Arc<dyn GuiContext>>)->bool {
        use crate::{controls::*,instrument_view::*};
        if FX_ADD.contains(x,y){self.params.fx_add.fetch_xor(true,Ordering::Relaxed);return true;}
        if self.params.fx_add.load(Ordering::Relaxed){
            for kind in 1..crate::fxrack::NAMES.len(){if fx_menu_button(kind).contains(x,y){self.fx_init(c);let p=self.fx_patch();let slots=crate::fxrack::order(&p);
                if let Some(slot)=(0..RACK_SLOTS).find(|s|p[crate::fxrack::base(*s)]<0.5) {let b=crate::fxrack::base(slot);
                    for (rank,s) in slots.iter().enumerate(){self.edit_parameter(crate::fxrack::base(*s)+1,rank as i32,c);}
                    for (i,v) in crate::fxrack::initial(kind).iter().enumerate(){self.edit_parameter(b+6+i,*v as i32,c);}
                    for (k,v) in [(b+1,slots.len() as i32),(b+2,1),(b+3,1),(b+4,1),(b+5,0),(b,kind as i32)]{self.edit_parameter(k,v,c);}
                    self.params.fx_scroll.store((slots.len() as f32*FX_CARD_HEIGHT) as usize,Ordering::Relaxed);
                    self.params.fx_add.store(false,Ordering::Relaxed);
                }else{self.params.message("The rack has 16 modules. Remove a module to add another.");}return true;
            }}return FX_AREA.contains(x,y);
        }
        if !(FX_AREA.contains(x,y)||Rect::new(1560.,528.,17.,406.).contains(x,y)){return false;}
        let p=self.fx_patch();let slots=crate::fxrack::order(&p);let scroll=self.params.fx_scroll.load(Ordering::Relaxed).min(fx_max_scroll(slots.len()));
        if x>=1560. {self.params.fx_scroll.store(((y-FX_AREA.y)/FX_AREA.h*fx_max_scroll(slots.len()) as f32) as usize,Ordering::Relaxed);return true;}
        for (position,slot) in slots.iter().enumerate(){let r=fx_card(position,scroll);let b=crate::fxrack::base(*slot);
            if fx_more(r).contains(x,y){self.params.details.fetch_xor(1<<(8+*slot),Ordering::Relaxed);return true;}
            for action in 0..6 {if fx_button(r,action).contains(x,y){self.fx_init(c);match action {
                0..=2=>self.edit_parameter(b+2+action,if p[b+2+action]>0.5{0}else{1},c),
                3|4=>{let next=if action==3{position.checked_sub(1)}else{(position+1<slots.len()).then_some(position+1)};if let Some(next)=next {
                    for (rank,s) in slots.iter().enumerate(){let rank=if rank==position{next}else if rank==next{position}else{rank};self.edit_parameter(crate::fxrack::base(*s)+1,rank as i32,c);}
                }},5=>self.edit_parameter(b,0,c),_=>{}
            }return true;}}
        }
        if self.hits.lock().unwrap().iter().any(|h|h.r.contains(x,y)&&crate::controls::SPEC.index(h.t.id()).is_some_and(|k|(RACK_START..RACK_END).contains(&k))){self.fx_init(c);}
        false
    }
    fn activate_classic(
        &self,
        o: usize,
        bank: Option<usize>,
        context: Option<&Arc<dyn GuiContext>>,
    ) {
        use crate::controls::*;
        let mut v = os_plugkit::vals::Vals::new(
            &*self.params.base,
            self.params.base.live.clone(),
            context.cloned(),
        );
        v.edit_plain(SETTINGS[if o == 0 { A_SOURCE } else { B_SOURCE }].id, 0.);
        if let Some(bank) = bank {
            v.edit_plain(SETTINGS[o * 8].id, bank as f32);
        }
    }
    fn commit(
        &self,
        o: usize,
        bank: Option<usize>,
        sources: crate::assets::Sources,
        context: Option<&Arc<dyn GuiContext>>,
    ) {
        if let Some(context) = context {
            let mut state = context.get_state();
            state
                .fields
                .insert(FIELD.into(), serde_json::to_string(&sources).unwrap());
            use crate::controls::*;
            let source = if o == 0 { A_SOURCE } else { B_SOURCE };
            let mut raw = self.params.base.fl_state();
            for (k, value) in [
                (source, 0),
                (
                    o * 8,
                    bank.map(|b| b as i32).unwrap_or(self.params.base.v(o * 8)),
                ),
            ] {
                state.params.insert(
                    SETTINGS[k].id.into(),
                    nih_plug::wrapper::state::ParamValue::I32(value),
                );
                raw[4 + k * 4..8 + k * 4].copy_from_slice(&value.to_le_bytes());
            }
            let hex: String = raw.iter().map(|b| format!("{b:02x}")).collect();
            state
                .fields
                .insert("state".into(), serde_json::to_string(&hex).unwrap());
            context.set_state(state);
        } else {
            match self.params.replace(sources) {
                Ok(()) => self.activate_classic(o, bank, None),
                Err(e) => self.params.message(e),
            }
        }
    }
}
impl Extension for TableEditor {
    fn modulation_routes(&self, id: &str) -> Vec<(usize, &'static str, &'static str)> {
        if crate::visibility::shown(0) { self.modulation.routes_for(id) } else { Vec::new() }
    }
    fn remove_modulation(&self, id: &str, route: usize, context: Option<&Arc<dyn GuiContext>>) {
        self.modulation.remove_route(id, route, context);
    }
    fn accepts_files(&self)->bool{true}
    fn file_dropped(&self,path:&std::path::Path){
        self.library.lock().unwrap().close();
        let mut work=self.workbench.lock().unwrap();
        if !self.workbench_loaded.swap(true,Ordering::Relaxed){work.load(&self.params,0);}
        work.import_recording(path,&self.params);
        self.browser.lock().unwrap().close();
        self.params.keyboard.cancel();
        self.params.page.store(2,Ordering::Relaxed);
    }
    fn editor_closed(&self) {self.workbench.lock().unwrap().cancel_drag();self.params.audition.stop();self.params.keyboard.cancel();self.params.keyboard.open.store(false,Ordering::Relaxed);}
    fn render(
        &self,
        c: &mut Canvas,
        model: &dyn Model,
        state: &lemon_small_fx::view::ViewState,
        hint: &str,
    ) -> Option<Vec<lemon_small_fx::view::Hit>> {
        self.params.keyboard.open.store(true,Ordering::Relaxed);
        crate::design::refresh();
        let title=self.params.origin.lock().unwrap().as_ref().map(|o|format!("{} / {}",o.category,o.name));
        let hits = crate::instrument_view::paint(
            c,
            model,
            state,
            hint,
            &self.params,
            self.cycles.each_ref().map(|x| x.load(Ordering::Relaxed)),
            title.as_deref(),
        );
        if self.params.page.load(Ordering::Relaxed) == 2 {
            self.workbench.lock().unwrap().paint(c);
            self.hits.lock().unwrap().clear();
            return Some(Vec::new());
        }
        *self.hits.lock().unwrap() = hits.clone();
        self.modulation.observe(&hits, c);
        let library=self.library.lock().unwrap();
        if library.active(){library.paint(c);return Some(Vec::new());}
        let browser = self.browser.lock().unwrap();
        if browser.oscillator.is_some() {
            browser.paint(c);
            Some(Vec::new())
        } else {
            Some(hits)
        }
    }
    fn wheel(&self,x:f32,y:f32,notches:f32)->bool {
        if self.library.lock().unwrap().active(){return true;}
        if self.params.page.load(Ordering::Relaxed)!=0 || !Rect::new(1170.,524.,410.,422.).contains(x,y) || self.browser.lock().unwrap().oscillator.is_some(){return false;}
        if !self.params.fx_add.load(Ordering::Relaxed){let max=crate::instrument_view::fx_max_scroll(crate::fxrack::order(&self.fx_patch()).len());let current=self.params.fx_scroll.load(Ordering::Relaxed) as f32;self.params.fx_scroll.store((current-notches*48.).clamp(0.,max as f32) as usize,Ordering::Relaxed);}true
    }
    fn character(&self, c: char) -> bool {
        if self.library.lock().unwrap().character(c,&self.params){return true;}
        self.params.page.load(Ordering::Relaxed) == 2 && self.workbench.lock().unwrap().character(c)
    }
    fn dragging(&self) -> bool {
        self.params.keyboard.dragging.load(Ordering::Relaxed) || self.workbench.lock().unwrap().dragging() || self.modulation.dragging()
    }
    fn moved(&self, x: f32, y: f32) -> bool {
        if self.params.keyboard.dragging.load(Ordering::Relaxed) {
            crate::instrument_view::keyboard::slide(&self.params.keyboard,x,y);return true;
        }
        if self.workbench.lock().unwrap().move_input(x,y,&self.params) {
            return true;
        }
        self.modulation.moved(x, y)
    }
    fn release(&self, c: Option<&Arc<dyn GuiContext>>) -> bool {
        if self.params.keyboard.release(){return true;}
        if self.workbench.lock().unwrap().release_input(&self.params) {
            return true;
        }
        self.modulation.release(c)
    }
    fn paint(&self, c: &mut Canvas, model: &dyn Model) {
        let _ = self.render(c, model, &Default::default(), "");
    }
    fn click(&self, x: f32, y: f32, context: Option<&Arc<dyn GuiContext>>) -> bool {
        if self.params.page.load(Ordering::Relaxed)!=2{
            let mut library=self.library.lock().unwrap();
            if library.active(){
                let action=library.click(x,y,&self.params);drop(library);
                if let Some(state)=action{self.load_program(state,context);}return true;
            }
            if self.browser.lock().unwrap().oscillator.is_none(){
                let saving=crate::preset_ui::SAVE.contains(x,y);
                if saving||crate::instrument_view::PRESET_FIELD.contains(x,y){
                    library.open(&self.params,saving);return true;
                }
            }
        }
        if crate::instrument_view::page_button(2).contains(x, y) {
            if !self.workbench_loaded.swap(true, Ordering::Relaxed) {
                self.workbench.lock().unwrap().load(&self.params, 0);
            }
            self.params.page.store(2, Ordering::Relaxed);
            self.browser.lock().unwrap().close();
            return true;
        }
        if self.params.page.load(Ordering::Relaxed) == 2 {
            for page in 0..2 {
                if crate::instrument_view::page_button(page).contains(x, y) {
                    self.params.audition.stop();
                    self.params.page.store(page, Ordering::Relaxed);
                    return true;
                }
            }
            let mut work = self.workbench.lock().unwrap();
            if let Some(source) = work.click(x, y, &self.params) {
                let o = work.oscillator;
                let mut sources = self.params.sources();
                sources.samples[o] = None;
                sources.oscillators[o] = Some(source);
                self.commit(o, None, sources, context);
                *self.params.origin.lock().unwrap()=None;
            }
            return true;
        }
        if self.params.page.load(Ordering::Relaxed)==0 && self.browser.lock().unwrap().oscillator.is_none() && self.fx_click(x,y,context) {return true;}
        if self.params.page.load(Ordering::Relaxed)==0 && self.browser.lock().unwrap().oscillator.is_none() && self.source_click(x,y,context) {return true;}
        for o in 0..2 {
            if self.params.page.load(Ordering::Relaxed) == 0
                && crate::instrument_view::edit_table_button(o).contains(x, y)
            {
                let mut work = self.workbench.lock().unwrap();
                if !work.dirty {
                    work.load(&self.params, o);
                    self.workbench_loaded.store(true, Ordering::Relaxed);
                } else if work.oscillator != o {
                    work.status = "Apply or Reload the draft before switching oscillators.".into();
                }
                self.params.page.store(2, Ordering::Relaxed);
                return true;
            }
        }

        {
            let mut browser = self.browser.lock().unwrap();
            if let Some(o) = browser.oscillator {
                if browser.menu.as_ref().is_some_and(|m|m.anchor.contains(x,y)) {
                    browser.close();
                    return true;
                }
                let inside = browser
                    .menu
                    .as_ref()
                    .is_some_and(|m| m.rect().contains(x, y));
                if let Some(choice) = browser.click(x, y) {
                    use crate::sound_browser::Choice;
                    match &choice {
                        Choice::Category(k) => {
                            match k {
                                0 | 1 => browser.sounds(o, *k, ""),
                                6 => browser.categories(o),
                                7 => browser.user_tables(o),
                                2 => browser.groups(o),
                                4 => browser.instrument(o, "Mini-Moog"),
                                5 => browser.instrument(o, "800DV"),
                                _ => {
                                    self.edit_parameter(
                                        if o == 0 {
                                            crate::controls::A_SOURCE
                                        } else {
                                            crate::controls::B_SOURCE
                                        },
                                        1,
                                        context,
                                    );
                                    browser.close();
                                }
                            }
                            return true;
                        }
                        Choice::Group(g) => {
                            browser.sounds(o, 2, g);
                            return true;
                        }
                        Choice::Param(k, v) => {
                            self.edit_parameter(*k, *v, context);
                            browser.close();
                            return true;
                        }
                        _ => {}
                    }
                    let mut sources = self.params.sources();
                    let mut bank = None;
                    let result = match choice {
                        Choice::Basic(i) => {
                            sources.oscillators[o] = Some(crate::sound_browser::basic(i));
                            sources.samples[o] = None;
                            Ok(())
                        }
                        Choice::Bank(i) => {
                            sources.oscillators[o] = None;
                            sources.samples[o] = None;
                            bank = Some(i);
                            Ok(())
                        }
                        Choice::UserTable(path) => crate::table_tools::load(&path).map(|source| {
                            sources.samples[o] = None;
                            sources.oscillators[o] = Some(source);
                        }),
                        Choice::Sample(e) => e
                            .embedded
                            .map(crate::included_library::load)
                            .unwrap_or_else(|| crate::sample::Sample::import(&e.path, e.root))
                            .map(|sample| {
                                sources.samples[o] = Some(sample);
                                sources.oscillators[o] = None;
                            }),
                        _ => unreachable!(),
                    };
                    match result {
                        Ok(()) => {
                            self.commit(o, bank, sources, context);
                            *self.params.origin.lock().unwrap()=None;
                            browser.close();
                            self.params.message("Sound loaded and embedded in this patch. Recorded samples play once; Root adjusts tuning.");
                        }
                        Err(e) => {
                            browser.message = format!("Load failed: {e}. Current sound kept.");
                            self.params.message(browser.message.clone());
                        }
                    }
                }
                if inside {
                    return true;
                }
            }
        }
        if self.params.page.load(Ordering::Relaxed) == 0 {
            for o in 0..2 {
                if crate::sound_browser::button(o).contains(x, y) {
                    self.browser.lock().unwrap().categories(o);
                    return true;
                }

                for (i, delta) in [-12., -1., 1., 12.].iter().enumerate() {
                    if crate::sound_browser::kind(&self.params, o) == 2
                        && crate::sound_browser::root_button(o, i).contains(x, y)
                    {
                        let mut sources = self.params.sources();
                        if let Some(sample) = &mut sources.samples[o] {
                            sample.root = (sample.root + delta).clamp(0., 127.);
                            self.commit(o, None, sources, context);
                            return true;
                        }
                    }
                }
            }
        }
        if let Some(hit) = self
            .hits
            .lock()
            .unwrap()
            .iter()
            .find(|h| {
                h.r.contains(x, y) && matches!(h.t, lemon_small_fx::view::Target::Choice(_, _))
            })
            .cloned()
        {
            if let Some(k) = crate::controls::SPEC.index(hit.t.id()) {
                let setting = &crate::controls::SETTINGS[k];
                let special=if (crate::controls::RACK_START..crate::controls::RACK_END).contains(&k) {let b=crate::fxrack::base((k-crate::controls::RACK_START)/crate::controls::RACK_STRIDE);let kind=self.params.base.v(b) as usize;
                    if k==b+5{Some(crate::fxrack::modes(kind))}else if kind==4&&k==b+10{Some(&crate::fxrack::DIVISIONS[..])}else{None}
                }else{None};
                let range=if let Some(names)=special {0..=names.len() as i32-1}else{setting.range.0..=setting.range.1};
                let items = range.map(|v| {
                        (
                            special.map(|names|names[v as usize].to_string()).unwrap_or_else(||crate::controls::text(k,v)),
                            crate::sound_browser::Choice::Param(k, v),
                        )
                    })
                    .collect();
                self.browser.lock().unwrap().show(0, hit.r, items);
                return true;
            }
        }
        if self.modulation.click(x, y, context) {
            return true;
        }
        use crate::instrument_view::{PRESET_INIT, PRESET_NEXT, PRESET_PREV};
        if [PRESET_PREV, PRESET_NEXT, PRESET_INIT]
            .iter()
            .any(|r| r.contains(x, y))
        {
            let state=if PRESET_INIT.contains(x,y){Some(Self::factory_state(0))}else{self.library.lock().unwrap().adjacent(&self.params,PRESET_NEXT.contains(x,y))};
            if let Some(state)=state{self.load_program(state,context);}
            return true;
        }
        if self.params.page.load(Ordering::Relaxed) == 1 {
            return false;
        }
        for o in 0..2 {
            let Some(action) = Self::buttons(o).iter().position(|r| r.contains(x, y)) else {
                continue;
            };
            match action {
                0 => {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_title("Import periodic wavetable WAV")
                        .add_filter("WAV wavetable", &["wav"])
                        .pick_file()
                    {
                        match wavetable::import(
                            &path,
                            CYCLES[self.cycles[o].load(Ordering::Relaxed)],
                        ) {
                            Ok(source) => {
                                let mut sources = self.params.sources();
                                sources.oscillators[o] = Some(source);
                                sources.samples[o] = None;
                                self.commit(o, None, sources, context);
                                self.params.message("Wavetable imported and active. Position scans frames; Browse Sounds selects another source.");
                            }
                            Err(e) => self
                                .params
                                .message(format!("Import failed: {e}. Current sound kept.")),
                        }
                    }
                }
                1 => {
                    let v = self.cycles[o].load(Ordering::Relaxed);
                    self.cycles[o].store((v + 1) % CYCLES.len(), Ordering::Relaxed);
                    self.params.message("Cycle length applies to the next import. Stereo WAVs are averaged to mono; maximum 128 frames.");
                }
                _ => {
                    let mut sources = self.params.sources();
                    sources.oscillators[o] = None;
                    sources.samples[o] = None;
                    self.commit(o, None, sources, context);
                    self.params
                        .message("Built-in bank restored. Browse Sounds opens the wavetable list.");
                }
            }
            return true;
        }
        false
    }
}

#[cfg(test)]
mod user_table_tests {
    use super::*;
    #[test]
    fn categorized_preset_save_init_and_user_recall() {
        use crate::{controls::*,instrument_view::{PRESET_FIELD,PRESET_INIT}};
        let dir=std::env::temp_dir().join(format!("wave-full-presets-{}",std::process::id()));
        let params=Arc::new(WaveParams::default());
        let e=TableEditor::new(params.clone());
        *e.library.lock().unwrap()=crate::preset_ui::Library::new(dir.clone());
        let click=|x,y|assert!(e.click(x,y,None));
        click(PRESET_FIELD.cx(),PRESET_FIELD.cy());click(800.,205.);click(1000.,144.);
        assert_eq!(params.base.v(ARP_ON),1);
        let mut sources=params.sources();
        sources.noise=Some(crate::sample::Sample{name:"User noise".into(),rate:8000,root:60.,samples:vec![0.3;256]});
        params.replace(sources).unwrap();params.curves.write().unwrap().samples[2][8]=0.37;
        e.edit_parameter(MACRO1,715,None);
        let before:Vec<_>=(0..COUNT).map(|k|params.base.v(k)).collect();
        let tables=params.sources().oscillators[0].as_ref().unwrap().samples.clone();
        click(crate::preset_ui::SAVE.cx(),crate::preset_ui::SAVE.cy());click(800.,237.);
        for ch in "After Hours".chars(){assert!(e.character(ch));}
        click(990.,308.);let file=crate::presets::path_for(&dir,"Keys","After Hours").unwrap();
        assert!(file.is_file());assert_eq!(params.origin.lock().unwrap().as_ref().unwrap().name,"After Hours");
        click(PRESET_INIT.cx(),PRESET_INIT.cy());assert_eq!(params.base.v(ARP_ON),0);assert_eq!(params.base.v(CHORD_ON),0);assert!(params.sources().oscillators[0].is_none());
        click(PRESET_FIELD.cx(),PRESET_FIELD.cy());click(937.,95.);click(800.,237.);click(1000.,144.);
        assert_eq!(before,(0..COUNT).map(|k|params.base.v(k)).collect::<Vec<_>>());
        assert_eq!(params.sources().oscillators[0].as_ref().unwrap().samples,tables);
        assert_eq!(params.sources().noise.as_ref().unwrap().name,"User noise");
        assert_eq!(params.curves.read().unwrap().samples[2][8],0.37);
        let old=std::fs::read(&file).unwrap();e.edit_parameter(MACRO1,222,None);
        click(crate::preset_ui::SAVE.cx(),crate::preset_ui::SAVE.cy());click(990.,308.);assert_eq!(std::fs::read(&file).unwrap(),old);
        click(990.,308.);assert_ne!(std::fs::read(&file).unwrap(),old);
        if let Ok(out)=std::env::var("LEMON_PRESET_RENDER"){
            click(PRESET_FIELD.cx(),PRESET_FIELD.cy());click(806.,95.);click(800.,173.);
            let mut c=Canvas::new(1584,1056,1.).unwrap();
            struct ModelStub;impl Model for ModelStub{fn plain(&self,id:&str)->f32{SETTINGS[SPEC.index(id).unwrap()].start as f32}fn norm(&self,_:&str)->f32{0.5}fn text(&self,id:&str)->String{let k=SPEC.index(id).unwrap();text(k,SETTINGS[k].start)}}
            e.render(&mut c,&ModelStub,&Default::default(),"");
            c.pixmap.save_png(std::path::Path::new(&out).join("Preset Library.png")).unwrap();
        }
        std::fs::remove_file(file).unwrap();std::fs::remove_dir(dir.join("Keys")).unwrap();std::fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn user_bank_dropdown_loads_and_missing_file_preserves_sound() {
        let root = std::env::temp_dir().join(format!(
            "wave-dropdown-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = crate::sound_browser::basic(3);
        let file = crate::table_tools::save_to(&root, "Test", "User Triangle", &source).unwrap();
        let params = Arc::new(WaveParams::default());
        let e = TableEditor::new(params.clone());
        let r = crate::sound_browser::button(0);
        e.browser.lock().unwrap().show(
            0,
            r,
            vec![(
                "Test / User Triangle".into(),
                crate::sound_browser::Choice::UserTable(file.clone()),
            )],
        );
        e.click(r.x + 10., r.bottom() + 16., None);
        assert_eq!(
            params.sources().oscillators[0].as_ref().unwrap().name,
            "User Triangle"
        );
        assert_eq!(
            params.sources().oscillators[0].as_ref().unwrap().samples,
            source.samples
        );
        std::fs::remove_file(&file).unwrap();
        e.browser.lock().unwrap().show(
            0,
            r,
            vec![(
                "Missing".into(),
                crate::sound_browser::Choice::UserTable(file),
            )],
        );
        e.click(r.x + 10., r.bottom() + 16., None);
        assert_eq!(
            params.sources().oscillators[0].as_ref().unwrap().samples,
            source.samples
        );
        assert!(params.status.lock().unwrap().starts_with("Load failed:"));
        std::fs::remove_dir(root.join("Test")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn modular_rack_add_duplicate_reorder_scroll_remove_and_recall() {
        use crate::{controls::*,instrument_view::*,fxrack};
        let params=Arc::new(WaveParams::default());let editor=TableEditor::new(params.clone());
        for kind in [4,5,4,1,2,3,6,7,8,9,10,11,12,13,1,4] {
            assert!(editor.click(FX_ADD.cx(),FX_ADD.cy(),None));let r=fx_menu_button(kind);assert!(editor.click(r.cx(),r.cy(),None));
        }
        let p=editor.fx_patch();let slots=fxrack::order(&p);assert_eq!(slots.len(),16);assert_eq!(p[fxrack::base(0)],4.);assert_eq!(p[fxrack::base(2)],4.);
        editor.click(FX_ADD.cx(),FX_ADD.cy(),None);let r=fx_menu_button(5);editor.click(r.cx(),r.cy(),None);assert_eq!(fxrack::order(&editor.fx_patch()).len(),16);editor.click(FX_ADD.cx(),FX_ADD.cy(),None);
        params.fx_scroll.store(0,Ordering::Relaxed);let r=fx_button(fx_card(0,0),4);editor.click(r.cx(),r.cy(),None);assert_eq!(fxrack::order(&editor.fx_patch())[0],1);
        let r=fx_button(fx_card(0,0),2);editor.click(r.cx(),r.cy(),None);assert_eq!(params.base.v(fxrack::base(1)+4),0);
        assert!(editor.wheel(1258.,600.,-100.));assert_eq!(params.fx_scroll.load(Ordering::Relaxed),fx_max_scroll(16));
        assert!(!editor.wheel(300.,600.,-1.));
        let scroll=params.fx_scroll.load(Ordering::Relaxed);let r=fx_button(fx_card(15,scroll),5);assert!(FX_AREA.contains(r.cx(),r.cy()));editor.click(r.cx(),r.cy(),None);assert_eq!(fxrack::order(&editor.fx_patch()).len(),15);
        let values:Vec<_>=(0..COUNT).map(|k|params.base.v(k)).collect();let raw=lemon_small_fx::params::to_fl(SPEC,&values,&params.base.raw_copy());let restored=WaveParams::default();restored.base.set_raw(raw.clone());assert_eq!(restored.base.raw_copy(),raw);
        assert_eq!(lemon_small_fx::params::setting(SPEC,&raw,fxrack::base(1)+4),0);
        let mut state=TableEditor::factory_state(0);let hex:String=std::iter::once(1_i32).chain(SETTINGS[..128].iter().map(|s|s.start)).flat_map(i32::to_le_bytes).map(|b|format!("{b:02x}")).collect();state.fields.insert("state".into(),format!("\"{hex}\""));crate::LemonWave::filter_state(&mut state);assert_eq!(state.fields["state"].trim_matches('"').len(),8+COUNT*8);
    }
    #[test]
    fn rack_knobs_drag_and_wheel_scrolls_without_editing_them() {
        use crate::{controls::*,instrument_view::*,fxrack};
        use lemon_small_fx::editor::{Logic,Btn,Mods,Act};
        let params=Arc::new(WaveParams::default());let editor=Arc::new(TableEditor::new(params.clone()));
        for kind in [4,5,1]{editor.click(FX_ADD.cx(),FX_ADD.cy(),None);let r=fx_menu_button(kind);editor.click(r.cx(),r.cy(),None);}
        params.fx_scroll.store(0,Ordering::Relaxed);
        let shared=Arc::new(lemon_small_fx::Shared{extension:Some(editor.clone()),..Default::default()});
        let mut logic=Logic::new(SPEC,params.base.clone(),shared,None);let mut c=Canvas::new(1584,992,1.).unwrap();logic.paint(&mut c,&os_plugkit::palette());
        let k=fxrack::base(0)+7;let h=logic.hits.iter().find(|h|h.t.id()==SETTINGS[k].id).unwrap().r;let before=params.base.v(k);
        assert!(logic.down(h.cx(),h.cy(),Btn::Left,Mods::default(),false).contains(&Act::Lock));logic.delta(0.,-20.,Mods::default());logic.up(Btn::Left);assert!(params.base.v(k)>before);
        let value=params.base.v(k);logic.wheel(h.cx(),h.cy(),-1.,Mods::default());assert_eq!(params.base.v(k),value);assert_eq!(params.fx_scroll.load(Ordering::Relaxed),48);
    }
    #[test]
    fn destination_knob_menu_removes_or_automates_the_selected_route() {
        use crate::controls::*;
        use lemon_small_fx::editor::{Act, Btn, Logic, Mods};
        let params = Arc::new(WaveParams::default());
        let editor = Arc::new(TableEditor::new(params.clone()));
        let mut values = os_plugkit::vals::Vals::new(&*params.base, params.base.live.clone(), None);
        for (k, value) in [(MOD3_SOURCE, 5.), (MOD3_TARGET, 1.), (MOD3_AMOUNT, 250.),
            (MOD4_SOURCE, 2.), (MOD4_TARGET, 1.), (MOD4_AMOUNT, -300.)] {
            values.edit_plain(SETTINGS[k].id, value);
        }
        let shared = Arc::new(lemon_small_fx::Shared { extension: Some(editor), ..Default::default() });
        let mut logic = Logic::new(SPEC, params.base.clone(), shared, None);
        logic.hosted = true;
        let mut canvas = Canvas::new(1584, 1056, 1.).unwrap();
        logic.paint(&mut canvas, &os_plugkit::palette());
        let knob = logic.hits.iter().find(|hit| hit.t.id() == SETTINGS[CUTOFF].id).unwrap().r;
        let events = logic.down(knob.cx(), knob.cy(), Btn::Right, Mods::default(), false);
        let (request, x, y) = events.iter().find_map(|action| match action {
            Act::Menu(req, x, y) => Some((req, *x, *y)), _ => None,
        }).unwrap();
        let menu = logic.menu(request);
        let automation = menu.find("Automate LFO 2 depth").unwrap();
        assert_eq!(logic.pick(request, automation, x, y), vec![Act::Automate(MOD3_AMOUNT)]);
        let remove = menu.find("Remove LFO 2 modulation").unwrap();
        logic.pick(request, remove, x, y);
        assert_eq!(params.base.v(MOD3_SOURCE), 0);
        assert_eq!(params.base.v(MOD3_AMOUNT), 250);
        assert_eq!(params.base.v(MOD4_SOURCE), 2);
        assert!(logic.menu(request).find("Remove LFO 2 modulation").is_none());
        assert!(logic.menu(request).find("Automate Envelope 2 depth").is_some());
    }
    #[test]
    fn dedicated_module_power_filter_tabs_and_noise_embedded_recall() {
        use crate::{controls::*,instrument_view::*};
        use lemon_small_fx::editor::{Logic,Btn,Mods};
        let params=Arc::new(WaveParams::default());let editor=Arc::new(TableEditor::new(params.clone()));let shared=Arc::new(lemon_small_fx::Shared{extension:Some(editor.clone()),..Default::default()});let mut logic=Logic::new(SPEC,params.base.clone(),shared,None);let mut canvas=Canvas::new(1584,992,1.).unwrap();
        for k in [A_ON,B_ON,SUB_ON,NOISE_ON]{logic.paint(&mut canvas,&os_plugkit::palette());let r=logic.hits.iter().find(|h|h.t.id()==SETTINGS[k].id).unwrap().r;logic.down(r.cx(),r.cy(),Btn::Left,Mods::default(),false);assert_eq!(params.base.v(k),0);logic.up(Btn::Left);logic.paint(&mut canvas,&os_plugkit::palette());logic.down(r.cx(),r.cy(),Btn::Left,Mods::default(),false);assert_eq!(params.base.v(k),1);logic.up(Btn::Left);}
        let tab=filter_tab(1);editor.click(tab.cx(),tab.cy(),None);logic.paint(&mut canvas,&os_plugkit::palette());assert!(logic.hits.iter().any(|h|h.t.id()==SETTINGS[F2_CUTOFF].id));let more=detail_button(4);editor.click(more.cx(),more.cy(),None);logic.paint(&mut canvas,&os_plugkit::palette());let input=filter_input(0);editor.click(input.cx(),input.cy(),None);assert_eq!(params.base.v(F2_A),1);assert_eq!(params.base.v(F1_A),1);assert_eq!(params.base.v(DUAL_FILTER),1);
        editor.set_noise(Some(crate::sample::Sample{name:"Noise recall".into(),rate:48000,root:60.,samples:(0..1024).map(|i|(i as f32*0.2).sin()*0.5).collect()}),None);
        let saved=params.serialize_fields();let restored=WaveParams::default();restored.deserialize_fields(&saved);assert_eq!(restored.sources().noise.unwrap().name,"Noise recall");assert_eq!(params.base.v(NOISE_TYPE),8);
        let mut old=TableEditor::factory_state(0);let hex:String=std::iter::once(1_i32).chain(SETTINGS[..321].iter().map(|s|s.start)).flat_map(i32::to_le_bytes).map(|b|format!("{b:02x}")).collect();old.fields.insert("state".into(),format!("\"{hex}\""));crate::LemonWave::filter_state(&mut old);assert_eq!(old.fields["state"].trim_matches('"').len(),8+COUNT*8);
    }
    #[test]
    fn inline_details_and_timing_segments_commit_without_opening_menus() {
        use crate::{controls::*,instrument_view::*};
        use lemon_small_fx::editor::Logic;
        let params=Arc::new(WaveParams::default());let editor=Arc::new(TableEditor::new(params.clone()));
        let shared=Arc::new(lemon_small_fx::Shared{extension:Some(editor.clone()),..Default::default()});let mut logic=Logic::new(SPEC,params.base.clone(),shared,None);let mut canvas=Canvas::new(1584,992,1.).unwrap();
        logic.paint(&mut canvas,&os_plugkit::palette());
        for i in 0..7{let r=detail_button(i);assert!(editor.click(r.cx(),r.cy(),None));assert!(detail_open(&params,i));}
        logic.paint(&mut canvas,&os_plugkit::palette());
        for key in [A_MODE,B_MODE,A_RANDOM,B_RANDOM,A_PHASE_FM,A_FM_RATIO,B_PHASE_FM,B_FM_RATIO,B_RING,SUB_DIRECT,NOISE_ONESHOT,F1_KEY,F1_PAN,AMP_HOLD+1,LFO_TARGET]{assert!(logic.hits.iter().any(|h|h.t.id()==SETTINGS[key].id),"missing inline detail {}",SETTINGS[key].id);}
        if let Some(path) = std::env::var_os("LEMON_HARMONICS_RENDER") { canvas.pixmap.save_png(path).unwrap(); }
        let r=detail_button(6);editor.click(r.cx(),r.cy(),None);
        let (_,r,_,_)=quick_choices(&params).into_iter().find(|(k,_,v,_)|*k==LFO_SYNC&&*v==6).unwrap();editor.click(r.cx(),r.cy(),None);assert_eq!(params.base.v(LFO_SYNC),6);assert!(editor.browser.lock().unwrap().menu.is_none());
        editor.click(FX_ADD.cx(),FX_ADD.cy(),None);let r=fx_menu_button(4);editor.click(r.cx(),r.cy(),None);params.fx_scroll.store(0,Ordering::Relaxed);logic.paint(&mut canvas,&os_plugkit::palette());
        let b=crate::fxrack::base(0);for (key,value) in [(b+5,1),(b+10,3)]{let(_,r,_,_)=quick_choices(&params).into_iter().find(|(k,_,v,_)|*k==key&&*v==value).unwrap();editor.click(r.cx(),r.cy(),None);assert_eq!(params.base.v(key),value);assert!(editor.browser.lock().unwrap().menu.is_none());}
        let r=fx_more(fx_card(0,0));editor.click(r.cx(),r.cy(),None);logic.paint(&mut canvas,&os_plugkit::palette());assert!(logic.hits.iter().any(|h|h.t.id()==SETTINGS[b+8].id));
    }
    #[test]
    fn dropdown_trigger_click_closes_without_reopening() {
        let params=Arc::new(WaveParams::default());
        let editor=TableEditor::new(params.clone());
        for anchor in [crate::sound_browser::button(0),crate::sound_browser::button(1),crate::instrument_view::PRESET_FIELD,Rect::new(142.,324.,70.,16.)] {
            editor.browser.lock().unwrap().show(0,anchor,vec![("Off".into(),crate::sound_browser::Choice::Param(crate::controls::A_MODE,0))]);
            assert!(editor.click(anchor.cx(),anchor.cy(),None));
            assert!(editor.browser.lock().unwrap().menu.is_none());
        }
        let anchor=crate::sound_browser::button(0);
        editor.click(anchor.cx(),anchor.cy(),None);
        assert!(editor.browser.lock().unwrap().menu.is_some());
        editor.click(anchor.cx(),anchor.cy(),None);
        assert!(editor.browser.lock().unwrap().menu.is_none());
        editor.click(anchor.cx(),anchor.cy(),None);
        assert!(editor.browser.lock().unwrap().menu.is_some());
    }

}
