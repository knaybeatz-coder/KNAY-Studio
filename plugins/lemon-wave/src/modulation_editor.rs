// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{assets::WaveParams, controls::*, instrument_view as view, modulation::POINTS};
use lemon_small_fx::view::Hit;
use nih_plug::prelude::*;
use std::sync::{Arc, Mutex, atomic::Ordering};
#[derive(Clone, Copy)]
enum Gesture {
    Envelope { key:usize, second:Option<usize>, x:f32, y:f32, start:f32, other:f32, direction:f32 },
    Draw {
        lfo: usize,
        last: (f32, f32),
        before: [f32; POINTS],
    },
    Route {
        source: usize,
        x: f32,
        y: f32,
    },
}
pub struct ModEditor {
    params: Arc<WaveParams>,
    gesture: Mutex<Option<Gesture>>,
    undo: Mutex<[Option<[f32; POINTS]>; 4]>,
    hits: Mutex<Vec<Hit>>,
    animation_clock: std::time::Instant,
    trail: Mutex<Vec<(f32,f32,std::time::Instant)>>,
    connected: Mutex<Option<(usize,std::time::Instant)>>,
    clipboard: Mutex<Option<[f32; POINTS]>>,
    envelope_context: Mutex<Option<Arc<dyn GuiContext>>>,
}
impl ModEditor {
    pub fn routes_for(&self, id: &str) -> Vec<(usize, &'static str, &'static str)> {
        let Some(target) = SPEC.index(id).and_then(mod_target) else { return Vec::new() };
        ROUTES.into_iter().filter_map(|slot| {
            let source = self.params.base.v(slot) as usize;
            if source == 0 || self.params.base.v(slot + 1) as usize != target { return None; }
            Some((slot, ALL_MOD_SOURCES[source], SETTINGS[slot + 2].id))
        }).collect()
    }

    pub fn remove_route(&self, id: &str, route: usize, context: Option<&Arc<dyn GuiContext>>) {
        if self.routes_for(id).iter().any(|entry| entry.0 == route) {
            // Disable with one host edit, so one Undo can restore the assignment at its previous depth.
            self.edit(&[(route, 0.)], context);
        }
    }

    pub fn new(params: Arc<WaveParams>) -> Self {
        Self {
            params,
            gesture: Mutex::new(None),
            undo: Mutex::new([None; 4]),
            hits: Mutex::new(Vec::new()),
            animation_clock: std::time::Instant::now(),
            trail: Mutex::new(Vec::new()),
            connected: Mutex::new(None),
            clipboard: Mutex::new(None),
            envelope_context: Mutex::new(None),
        }
    }
    fn edit(&self, edits: &[(usize, f32)], context: Option<&Arc<dyn GuiContext>>) {
        let mut v = os_plugkit::vals::Vals::new(
            &*self.params.base,
            self.params.base.live.clone(),
            context.cloned(),
        );
        for (k, x) in edits {
            v.edit_plain(SETTINGS[*k].id, *x);
        }
    }
    fn commit(&self, context: Option<&Arc<dyn GuiContext>>) {
        if let Some(c) = context {
            // The standalone already owns these persistent curves, and process()
            // picks them up directly. Reloading the entire plugin state here
            // unnecessarily reinitializes the engine on its audio thread.
            if c.plugin_api() == PluginApi::Standalone {
                return;
            }
            let mut state = c.get_state();
            state.fields.insert(
                crate::modulation::FIELD.into(),
                self.params.curves.read().unwrap().encode(),
            );
            c.set_state(state);
        }
    }
    fn point(&self, i: usize, x: f32, y: f32) -> (f32, f32) {
        let r = if self.params.page.load(Ordering::Relaxed)==0 {view::synth_lfo_graph()} else {view::lfo_graph(i)};
        (
            ((x - r.x) / r.w).clamp(0., 1.),
            (1. - 2. * (y - r.y) / r.h).clamp(-1., 1.),
        )
    }
    pub fn observe(&self, hits: &[Hit], canvas: &mut os_ui::Canvas) {
        *self.hits.lock().unwrap() = hits.to_vec();
        let color=crate::design::color(0xbda4fb);
        let now=std::time::Instant::now();
        if let Some(Gesture::Route { x, y, .. }) = *self.gesture.lock().unwrap() {
            // No labels or drag instructions: motion carries the feedback.
            canvas.fill_rect(os_ui::Rect::new(16.,962.,1548.,24.),crate::design::color(0x101319));
            let beat=(self.animation_clock.elapsed().as_secs_f32()*7.).sin()*0.5+0.5;
            let mut trail=self.trail.lock().unwrap();
            trail.retain(|(_,_,at)|now.duration_since(*at).as_secs_f32()<0.16);
            for (tx,ty,at) in trail.iter() {
                let fade=1.-now.duration_since(*at).as_secs_f32()/0.16;
                canvas.fill_circle(*tx,*ty,2.8*fade,color.with_alpha((75.*fade) as u8));
            }
            canvas.fill_circle(x,y,11.+beat*2.,color.with_alpha(15));
            canvas.stroke_circle(x,y,6.5+beat,1.2,color.with_alpha(180));
            canvas.fill_circle(x,y,2.4,crate::design::color(0xf1eaff));
            if let Some(h)=hits.iter().find(|h|h.r.contains(x,y)&&SPEC.index(h.t.id()).and_then(mod_target).is_some()) {
                canvas.stroke_rrect(h.r.inset(-1.),7.,1.5,color.with_alpha((135.+70.*beat) as u8));
                canvas.fill_rrect(h.r,7.,color.with_alpha(12));
            }
        }
        let mut connected=self.connected.lock().unwrap();
        if let Some((target,at))=*connected {
            let t=now.duration_since(at).as_secs_f32()/0.45;
            if t>=1. { *connected=None; }
            else if let Some(h)=hits.iter().find(|h|SPEC.index(h.t.id()).and_then(mod_target)==Some(target)) {
                let cx=h.r.cx();
                let cy=h.r.cy();
                let success=crate::design::color(0x73dacc);
                canvas.stroke_circle(cx,cy,23.+10.*t,2.*(1.-t)+0.3,success.with_alpha((210.*(1.-t)) as u8));
                canvas.fill_circle(cx,cy,22.,success.with_alpha((28.*(1.-t)) as u8));
            }
        }
    }
    pub fn click(&self, x: f32, y: f32, context: Option<&Arc<dyn GuiContext>>) -> bool {
        for i in 0..2 {
            if view::page_button(i).contains(x, y) {
                self.params.page.store(i, Ordering::Relaxed);
                return true;
            }
        }
        let page = self.params.page.load(Ordering::Relaxed);
        for (source, r, _) in view::source_buttons(page,&self.params) {
            if r.contains(x, y) {
                self.trail.lock().unwrap().clear();
                *self.gesture.lock().unwrap() = Some(Gesture::Route { source, x, y });
                return true;
            }
        }
        if page==0 {
            for i in 0..2 {if view::envelope_tab(i).contains(x,y) {self.params.env_tab.store(i,Ordering::Relaxed);return true;}}
            for i in 0..4 {if view::lfo_tab(i).contains(x,y) {self.params.lfo_tab.store(i,Ordering::Relaxed);return true;}}
            let env=self.params.env_tab.load(Ordering::Relaxed).min(1);
            let nodes=view::envelope_nodes(env,|k|self.params.base.v(k) as f32);
            if !view::detail_open(&self.params,5) {if let Some((key,second,_,_))=nodes.into_iter().find(|(_,_,px,py)|(x-px).hypot(y-py)<9.) {
                let release=if env==0 {RELEASE} else {ENV_RELEASE};
                *self.envelope_context.lock().unwrap()=context.cloned();
                *self.gesture.lock().unwrap()=Some(Gesture::Envelope {key,second,x,y,start:self.params.base.v(key) as f32,other:second.map(|k|self.params.base.v(k) as f32).unwrap_or(0.),direction:if key==release {-1.} else {1.}});
                return true;
            }
        }
        }
        let selected=self.params.lfo_tab.load(Ordering::Relaxed).min(3);
        for i in 0..4 {
            if page==0 && i!=selected {continue;}
            let graph=if page==0 {view::synth_lfo_graph()} else {view::lfo_graph(i)};
            if graph.contains(x, y) && !(page==0 && view::detail_open(&self.params,6)) {
                let before = self.params.curves.read().unwrap().samples[i];
                let point = self.point(i, x, y);
                self.params.curves.write().unwrap().draw(i, point, point);
                *self.gesture.lock().unwrap() = Some(Gesture::Draw {
                    lfo: i,
                    last: point,
                    before,
                });
                if i == 0 {
                    self.edit(&[(LFO_DRAW, 1.)], context);
                }
                return true;
            }
            for j in 0..if page==0 {7} else {5} {
                let button=if page==0 {view::synth_shape_button(j)} else {view::shape_button(i,j)};
                if button.contains(x, y) {
                    if j==5 {
                        let mut curves=*self.params.curves.read().unwrap();
                        if i==0 && self.params.base.v(LFO_DRAW)==0 {curves.preset(0,self.params.base.v(LFO_SHAPE) as usize);}
                        *self.clipboard.lock().unwrap()=Some(curves.samples[i]);return true;
                    }
                    if j==6 {
                        if let Some(copied)=*self.clipboard.lock().unwrap() {
                            self.undo.lock().unwrap()[i]=Some(self.params.curves.read().unwrap().samples[i]);
                            self.params.curves.write().unwrap().samples[i]=copied;
                            if i==0 {self.edit(&[(LFO_DRAW,1.)],context);}
                            self.commit(context);
                        }
                        return true;
                    }
                    let before = self.params.curves.read().unwrap().samples[i];
                    if j == 4 {
                        if let Some(old) = self.undo.lock().unwrap()[i] {
                            self.params.curves.write().unwrap().samples[i] = old;
                        }
                    } else {
                        self.params.curves.write().unwrap().preset(i, j);
                    }
                    self.undo.lock().unwrap()[i] = Some(before);
                    if i == 0 {
                        self.edit(&[(LFO_DRAW, 1.)], context);
                    }
                    self.commit(context);
                    return true;
                }
            }
        }
        if page==0 {return false;}
        for (i, slot) in ROUTES.into_iter().enumerate() {
            if view::clear_route(i).contains(x, y) {
                self.edit(&[(slot, 0.), (slot + 2, 0.)], context);
                return true;
            }
        }
        false
    }
    pub fn dragging(&self) -> bool {
        self.gesture.lock().unwrap().is_some()
    }
    pub fn moved(&self, x: f32, y: f32) -> bool {
        let mut g = self.gesture.lock().unwrap();
        match *g {
            Some(Gesture::Envelope {key,second,x:ox,y:oy,start,other,direction}) => {
                let range=SETTINGS[key].range;
                let value=(start+(x-ox)*direction*(range.1-range.0) as f32/240.).clamp(range.0 as f32,range.1 as f32);
                let context=self.envelope_context.lock().unwrap().clone();
                self.edit(&[(key,value)],context.as_ref());
                if let Some(k)=second {self.edit(&[(k,(other-(y-oy)*1000./view::synth_env_graph().h).clamp(0.,1000.))],context.as_ref());}
                true
            }
            Some(Gesture::Draw { lfo, last, before }) => {
                let point = self.point(lfo, x, y);
                self.params.curves.write().unwrap().draw(lfo, last, point);
                *g = Some(Gesture::Draw {
                    lfo,
                    last: point,
                    before,
                });
                true
            }
            Some(Gesture::Route { source, .. }) => {
                let mut trail=self.trail.lock().unwrap();
                if trail.len()>=16 {trail.remove(0);}
                trail.push((x,y,std::time::Instant::now()));
                drop(trail);
                for i in 0..2 {
                    if view::page_button(i).contains(x, y) {
                        self.params.page.store(i, Ordering::Relaxed);
                    }
                }
                *g = Some(Gesture::Route { source, x, y });
                true
            }
            None => false,
        }
    }
    pub fn release(&self, context: Option<&Arc<dyn GuiContext>>) -> bool {
        let g = self.gesture.lock().unwrap().take();
        match g {
            Some(Gesture::Envelope {key,second,..}) => {
                self.edit(&[(key,self.params.base.v(key) as f32)],context);
                if let Some(k)=second {self.edit(&[(k,self.params.base.v(k) as f32)],context);}
                self.envelope_context.lock().unwrap().take();
                true
            }
            Some(Gesture::Draw { lfo, before, .. }) => {
                self.undo.lock().unwrap()[lfo] = Some(before);
                self.commit(context);
                self.params.message(
                    "Shape saved with the patch. Undo restores the last stroke for this LFO.",
                );
                true
            }
            Some(Gesture::Route { source, x, y }) => {
                let target = self
                    .hits
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|h| h.r.contains(x, y))
                    .and_then(|h| SPEC.index(h.t.id()))
                    .and_then(mod_target);
                if let Some(t) = target {
                    self.assign(source, t, context);
                } else {
                    self.params
                        .message("Drop cancelled: choose an oscillator, filter or output knob.");
                }
                true
            }
            None => false,
        }
    }
    fn assign(&self, source: usize, target: usize, context: Option<&Arc<dyn GuiContext>>) {
        if ROUTES.iter().any(|s| {
            self.params.base.v(*s) == source as i32 && self.params.base.v(*s + 1) == target as i32
        }) {
            self.params
                .message("Route already exists. Adjust its Depth in Modulation.");
            return;
        }
        let slot = ROUTES[2..].iter().chain(&ROUTES[..2]).copied().find(|s| {
            (*s >= MOD3_SOURCE || (source <= 4 && target < 8)) && self.params.base.v(*s) == 0
        });
        if let Some(s) = slot {
            self.edit(
                &[(s + 1, target as f32), (s + 2, 250.), (s, source as f32)],
                context,
            );
            *self.connected.lock().unwrap()=Some((target,std::time::Instant::now()));
            self.params.message("");
        } else {
            self.params
                .message("No compatible empty route. Clear a route in Modulation first.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lemon_small_fx::view::Model;
    struct ModelOf(Arc<WaveParams>);
    impl Model for ModelOf {
        fn plain(&self, id: &str) -> f32 {
            self.0.base.v(SPEC.index(id).unwrap()) as f32
        }
        fn norm(&self, id: &str) -> f32 {
            let s = &SETTINGS[SPEC.index(id).unwrap()];
            (self.plain(id) - s.range.0 as f32) / (s.range.1 - s.range.0) as f32
        }
        fn text(&self, id: &str) -> String {
            crate::controls::text(SPEC.index(id).unwrap(), self.plain(id) as i32)
        }
    }
    #[test]
    fn draw_undo_cross_page_drop_and_state_recall() {
        let params = Arc::new(WaveParams::default());
        let editor = ModEditor::new(params.clone());
        params.page.store(1, Ordering::Relaxed);
        let before = *params.curves.read().unwrap();
        let graph = view::lfo_graph(2);
        assert!(editor.click(graph.x + 1., graph.y + 1., None));
        assert!(editor.dragging());
        editor.moved(graph.right() - 1., graph.bottom() - 1.);
        editor.release(None);
        assert_ne!(*params.curves.read().unwrap(), before);
        let saved = params.serialize_fields();
        let restored = WaveParams::default();
        restored.deserialize_fields(&saved);
        assert_eq!(
            *params.curves.read().unwrap(),
            *restored.curves.read().unwrap()
        );
        let undo = view::shape_button(2, 4);
        editor.click(undo.x + 2., undo.y + 2., None);
        assert_eq!(*params.curves.read().unwrap(), before);
        let source = view::source_buttons(1,&params)
            .into_iter()
            .find(|(s, _, _)| *s == 6)
            .unwrap()
            .1;
        editor.click(source.x + 2., source.y + 2., None);
        let tab = view::page_button(0);
        editor.moved(tab.x + 2., tab.y + 2.);
        assert_eq!(params.page.load(Ordering::Relaxed), 0);
        let mut canvas = os_ui::Canvas::new(1180, 820, 1.).unwrap();
        let hits = view::paint(
            &mut canvas,
            &ModelOf(params.clone()),
            &Default::default(),
            "",
            &params,
            [5; 2],
            None,
        );
        let target = hits.iter().find(|h| h.t.id() == "cutoff").unwrap().r;
        editor.observe(&hits, &mut canvas);
        editor.moved(target.x + 10., target.y + 10.);
        editor.release(None);
        assert_eq!(params.base.v(MOD3_SOURCE), 6);
        assert_eq!(params.base.v(MOD3_TARGET), 1);
        assert_eq!(params.base.v(MOD3_AMOUNT), 250);
        params.page.store(1, Ordering::Relaxed);
        let clear = view::clear_route(2);
        editor.click(clear.x + 2., clear.y + 2., None);
        assert_eq!(params.base.v(MOD3_SOURCE), 0);
        restored.deserialize_fields(&std::collections::BTreeMap::new());
        assert_eq!(
            *restored.curves.read().unwrap(),
            crate::modulation::Curves::default()
        );
    }
    #[test]
    fn header_handles_route_each_visible_source() {
        for (source,handle,_) in view::source_buttons(0,&WaveParams::default()) {
            let params=Arc::new(WaveParams::default());
            let editor=ModEditor::new(params.clone());
            let mut canvas=os_ui::Canvas::new(1180,820,1.).unwrap();
            let hits=view::paint(&mut canvas,&ModelOf(params.clone()),&Default::default(),"",&params,[5;2],None);
            let target=hits.iter().find(|h|h.t.id()=="cutoff").unwrap().r;
            editor.observe(&hits,&mut canvas);
            assert!(editor.click(handle.cx(),handle.cy(),None));
            assert!(editor.dragging());
            editor.moved(target.cx(),target.cy());
            editor.observe(&hits,&mut canvas);
            assert!(editor.connected.lock().unwrap().is_none());
            if let Ok(dir)=std::env::var("LEMON_WAVE_PREVIEW") {
                std::fs::create_dir_all(&dir).unwrap();
                canvas.pixmap.save_png(format!("{dir}/drag-{source}.png")).unwrap();
            }
            editor.release(None);
            assert_eq!(editor.connected.lock().unwrap().as_ref().unwrap().0,1);
            let hits=view::paint(&mut canvas,&ModelOf(params.clone()),&Default::default(),"",&params,[5;2],None);
            editor.observe(&hits,&mut canvas);
            if let Ok(dir)=std::env::var("LEMON_WAVE_PREVIEW") {
                canvas.pixmap.save_png(format!("{dir}/drop-{source}.png")).unwrap();
            }
            assert_eq!(params.base.v(MOD3_SOURCE),source as i32);
            assert_eq!(params.base.v(MOD3_TARGET),1);
            assert_eq!(params.base.v(MOD3_AMOUNT),250);
        }
    }

    #[test]
    fn tabs_draw_copy_undo_and_envelope_nodes_are_independent() {
        let params=Arc::new(WaveParams::default());
        let editor=ModEditor::new(params.clone());
        for lfo in 0..4 {
            let tab=view::lfo_tab(lfo);assert!(editor.click(tab.cx(),tab.cy(),None));
            assert_eq!(params.lfo_tab.load(Ordering::Relaxed),lfo);
            let before=*params.curves.read().unwrap();
            let r=view::synth_lfo_graph();
            editor.click(r.x+1.,r.y+1.,None);editor.moved(r.right()-1.,r.bottom()-1.);editor.release(None);
            let after=*params.curves.read().unwrap();
            assert_ne!(after.samples[lfo],before.samples[lfo]);
            for other in 0..4 {if other!=lfo {assert_eq!(before.samples[other],after.samples[other]);}}
            let copy=view::synth_shape_button(5);editor.click(copy.cx(),copy.cy(),None);
            let next=(lfo+1)%4;let tab=view::lfo_tab(next);editor.click(tab.cx(),tab.cy(),None);
            let old=params.curves.read().unwrap().samples[next];
            let paste=view::synth_shape_button(6);editor.click(paste.cx(),paste.cy(),None);
            assert_eq!(params.curves.read().unwrap().samples[next],after.samples[lfo]);
            let undo=view::synth_shape_button(4);editor.click(undo.cx(),undo.cy(),None);
            assert_eq!(params.curves.read().unwrap().samples[next],old);
        }
        for env in 0..2 {
            let tab=view::envelope_tab(env);editor.click(tab.cx(),tab.cy(),None);
            assert_eq!(params.env_tab.load(Ordering::Relaxed),env);
            let (key,_,x,y)=view::envelope_nodes(env,|k|params.base.v(k) as f32)[0];
            let before=params.base.v(key);
            editor.click(x,y,None);editor.moved(x+20.,y);editor.release(None);
            assert!(params.base.v(key)>before);
            assert_eq!(view::source_buttons(0,&params)[0].0,[12,2][env]);
        }
        // Curves are custom serialized fields; numeric parameters are persisted by
        // the host separately after confirming the GUI edit gestures.
        let saved=params.serialize_fields();let restored=WaveParams::default();restored.deserialize_fields(&saved);
        assert_eq!(*params.curves.read().unwrap(),*restored.curves.read().unwrap());

    }

}
