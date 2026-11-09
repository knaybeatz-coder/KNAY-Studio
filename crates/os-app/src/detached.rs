//! Native editor windows sharing the main App and audio engine.
use std::rc::Rc;
use std::num::NonZeroU32;
use std::time::Instant;
use winit::{event::{WindowEvent,ElementState,MouseButton,MouseScrollDelta},event_loop::ActiveEventLoop,window::{Window,WindowId},keyboard::ModifiersState,dpi::{LogicalSize,PhysicalPosition,PhysicalSize}};
use crate::{app::{App,Win,Action},layout::{self,MainWin}};
use os_ui::Canvas;
#[derive(Default)]
pub struct Windows { stems:Option<(u16,u32,Vec<std::path::PathBuf>)>, panes:Vec<Pane>, screens:Vec<[i32;4]>, main:Option<crate::app::ViewContext>, changed:bool, layout_generation:u64 }
struct Pane {
    kind:Win, window:Rc<Window>, surface:softbuffer::Surface<Rc<Window>,Rc<Window>>, canvas:Canvas,
    cursor:Option<(crate::app::Cursor,u16,u8)>,
    eq_visible: bool,
    motion:Option<(f32,f32)>, hover:Option<Action>, refresh_editors:bool, frame:MainWin, mouse:(f32,f32), mods:ModifiersState, last:Instant, painted:Instant,
}
fn key(w:Win)-> &'static str {match w {Win::Browser=>"native_browser",Win::Mixer=>"native_mixer",Win::PianoRoll=>"native_piano",_=>"native_playlist"}}
fn title(w:Win)-> &'static str {match w {Win::Browser=>"Lemon Studio - Browser",Win::Mixer=>"Lemon Studio - Mixer",Win::PianoRoll=>"Lemon Studio - Piano roll",_=>"Lemon Studio - Playlist"}}
impl Windows {
    pub fn flush_stems(&mut self,app:&mut App){
        if let Some((track,tick,files))=self.stems.take(){app.import_stems(files,track,tick);self.changed=true;}
    }
    pub fn browser_window(&self) -> Option<&Window> { self.panes.iter().find(|p| p.kind == Win::Browser).map(|p| p.window.as_ref()) }
    fn save_main(&mut self,app:&mut App){if self.main.is_none(){self.main=Some(app.take_view_context());}}
    pub fn restore_main(&mut self,app:&mut App)->bool {
        if let Some(c)=self.main.take(){app.restore_view_context(c);if self.changed {app.invalidate();}self.changed=false;return true;}false
    }

    pub fn owns(&self,id:WindowId)->bool {self.panes.iter().any(|p|p.window.id()==id)}
    pub fn prepare_drag(&mut self,app:&mut App) {
        let external=app.state.sample_drop.external&&app.state.sample_drop.surface.is_some();
        let target=if external{app.state.sample_drop.surface}else{app.state.native_input};
        if (!external&&app.drag.is_none())||!self.panes.iter().any(|p|Some(p.kind)==target){return;}
        self.save_main(app);
        if let Some(p)=self.panes.iter_mut().find(|p|Some(p.kind)==target) {
            if external {if let (Some((at,_)),Ok(pos))=(crate::screen_state(&p.window),p.window.inner_position()){let scale=crate::ui_scale(&p.window) as f32;p.mouse=((at.0-pos.x) as f32/scale,(at.1-pos.y) as f32/scale);}}
            app.state.native_view=Some(p.kind);app.mouse=p.mouse;set_rack_parent(app,&p.window);app.hover=p.hover.clone();app.paint(&mut p.canvas,None);
            if external{crate::drop_preview::poll(app);p.window.request_redraw();}
        }
    }
    pub fn save(&self) {for p in &self.panes {layout::save_native(key(p.kind),p.frame);}}
    pub fn sync(&mut self,app:&mut App,el:&ActiveEventLoop,cache:&mut crate::CursorCache) {
        let screens=crate::screens(el);let changed=screens!=self.screens;self.screens=screens.clone();
        if self.layout_generation!=app.state.native_layout_generation {
            self.layout_generation=app.state.native_layout_generation;
            for p in &mut self.panes {if let Some((_,f))=app.state.native_frames.iter().find(|(w,_)|*w==p.kind){
                let f=layout::on_screens(*f,&screens);p.window.set_maximized(false);p.window.set_outer_position(PhysicalPosition::new(f.pos.0,f.pos.1));let _=p.window.request_inner_size(PhysicalSize::new(f.size.0,f.size.1));p.window.set_maximized(f.max);p.frame=f;
            }}
        }
        self.panes.retain(|p|{let keep=app.state.native_detached.contains(&p.kind)&&app.state.shown(p.kind);if !keep {#[cfg(windows)] if p.kind==Win::Mixer {app.plugins.close_racks();}layout::save_native(key(p.kind),p.frame);}keep});
        let mut created=Vec::new();
        for kind in app.state.native_detached.clone() {
            if !app.state.shown(kind)||self.panes.iter().any(|p|p.kind==kind){continue;}
            let initial=app.state.native_frames.iter().find(|(w,_)|*w==kind).map(|(_,f)|*f).or_else(||layout::native_win(key(kind))).unwrap_or(MainWin{pos:(100,100),size:if kind==Win::Browser{(380,680)}else{(1100,800)},max:false});
            let frame=layout::on_screens(initial,&screens);
            let attrs=Window::default_attributes().with_title(title(kind)).with_inner_size(PhysicalSize::new(frame.size.0,frame.size.1)).with_position(PhysicalPosition::new(frame.pos.0,frame.pos.1)).with_min_inner_size(LogicalSize::new(kind.min_size().0 as f64,kind.min_size().1 as f64)).with_maximized(frame.max).with_visible(false);
            let made=(||->Result<Pane,String>{
                let window=Rc::new(el.create_window(attrs).map_err(|e|e.to_string())?);
                let context=softbuffer::Context::new(window.clone()).map_err(|e|e.to_string())?;
                let surface=softbuffer::Surface::new(&context,window.clone()).map_err(|e|e.to_string())?;
                let size=window.inner_size();let canvas=Canvas::new(size.width.max(1),size.height.max(1),crate::ui_scale(&window) as f32).map_err(|e|e.to_string())?;
                Ok(Pane{kind,window,surface,canvas,cursor:None,eq_visible:false,motion:None,hover:None,refresh_editors:false,frame,mouse:(-1.,-1.),mods:ModifiersState::empty(),last:Instant::now(),painted:Instant::now()})
            })();
            match made {Ok(p)=>{created.push(p.window.id());self.panes.push(p);},Err(e)=>{app.state.native_detached.retain(|w|*w!=kind);app.state.message=Some(format!("Could not detach window: {e}"));}}
        }
        for p in &mut self.panes {
            app.state.native_frames.retain(|(w,_)|*w!=p.kind);app.state.native_frames.push((p.kind,p.frame));
            if changed {
                let next=layout::on_screens(p.frame,&screens);
                if next!=p.frame {p.window.set_maximized(false);p.window.set_outer_position(PhysicalPosition::new(next.pos.0,next.pos.1));let _=p.window.request_inner_size(PhysicalSize::new(next.size.0,next.size.1));p.frame=next;}
            }
            if p.painted.elapsed().as_millis()>=if app.animating()||p.motion.is_some(){16}else{100} {p.window.request_redraw();}
        }
        for id in created {self.event(app,el,id,WindowEvent::RedrawRequested,cache);}
        app.detached_eq_visible = self.panes.iter().any(|p| {
            p.kind == Win::Mixer && p.eq_visible && p.window.is_minimized() != Some(true)
        });
    }
    pub fn event(&mut self,app:&mut App,_el:&ActiveEventLoop,id:WindowId,event:WindowEvent,cache:&mut crate::CursorCache) {
        if app.loading() && matches!(event,WindowEvent::MouseInput{state:ElementState::Pressed,..}|WindowEvent::KeyboardInput{..}|WindowEvent::MouseWheel{..}|WindowEvent::DroppedFile(_)){return;}
        // Mouse input is consumed once per frame, not at the mouse polling rate.
        if let WindowEvent::CursorMoved{position,..}=&event {
            if let Some(p)=self.panes.iter_mut().find(|p|p.window.id()==id){let scale=crate::ui_scale(&p.window) as f32;p.motion=Some((position.x as f32/scale,position.y as f32/scale));}
            return;
        }
        self.save_main(app);
        let Some(p)=self.panes.iter_mut().find(|p|p.window.id()==id)else{return};
        let scale=crate::ui_scale(&p.window) as f32;
        if matches!(event,WindowEvent::Moved(_)|WindowEvent::Resized(_)) {
            if !p.window.is_maximized() && p.window.is_minimized()!=Some(true) {if let Some((pos,size))=crate::frame_of(&p.window){p.frame.pos=pos;p.frame.size=size;}}
            p.frame.max=p.window.is_maximized();
            app.state.native_frames.retain(|(w,_)|*w!=p.kind);app.state.native_frames.push((p.kind,p.frame));
        }
        if matches!(event,WindowEvent::CloseRequested) {app.state.hide(p.kind);self.changed=true;return;}
        if matches!(event,WindowEvent::Focused(true)|WindowEvent::MouseInput{state:ElementState::Pressed,..}|WindowEvent::KeyboardInput{..}|WindowEvent::MouseWheel{..}) {if app.state.native_input!=Some(p.kind){self.changed|=app.state.playlist.popup.is_some()||app.state.menu_open.is_some();app.state.playlist.popup=None;app.state.menu_open=None;}app.state.raise(p.kind);app.state.native_input=Some(p.kind);app.auto_close.main_focused=true;}
        if matches!(event,WindowEvent::Focused(false)) {p.motion=None;app.typing_release_all();}
        if !needs_layout(&event,app.drag.is_some()||app.pressed.is_some()) {
            if matches!(event,WindowEvent::Moved(_)|WindowEvent::ScaleFactorChanged{..}) {p.refresh_editors=true;}
            if matches!(event,WindowEvent::Resized(_)|WindowEvent::ScaleFactorChanged{..}) {p.window.request_redraw();}
            return;
        }
        let size=p.window.inner_size();if size.width==0||size.height==0{return;}
        if p.canvas.pixmap.width()!=size.width||p.canvas.pixmap.height()!=size.height||p.canvas.scale!=scale {p.canvas.resize(size.width,size.height,scale);}
        app.state.native_view=Some(p.kind);app.state.display_scale=p.window.scale_factor() as f32;app.mouse=p.mouse;
        set_rack_parent(app,&p.window);app.hover=p.hover.clone();app.paint(&mut p.canvas,None);
        if p.kind == Win::Mixer {
            p.eq_visible = app.mixer_eq_region().is_some();
        }
        let redraw=matches!(event,WindowEvent::RedrawRequested);
        if let Some(pos)=p.motion.take(){p.mouse=pos;app.mouse_move(pos.0,pos.1);if app.drag.is_some(){self.changed=true;}}
        p.refresh_editors|=matches!(event,WindowEvent::Moved(_)|WindowEvent::ScaleFactorChanged{..});
        self.changed|=matches!(event,WindowEvent::MouseInput{..}|WindowEvent::MouseWheel{..}|WindowEvent::KeyboardInput{..}|WindowEvent::DroppedFile(_)|WindowEvent::Focused(_));
        match event {
            WindowEvent::RedrawRequested=>{
                p.painted=Instant::now();
                if let (Some(w),Some(h))=(NonZeroU32::new(size.width),NonZeroU32::new(size.height)) {
                    if p.surface.resize(w,h).is_ok() {if let Ok(mut buf)=p.surface.buffer_mut(){
                        for (dst,src) in buf.iter_mut().zip(p.canvas.pixmap.data().chunks_exact(4)){*dst=(u32::from(src[0])<<16)|(u32::from(src[1])<<8)|u32::from(src[2]);}
                        let _=buf.present();p.window.set_visible(true);
                    }}
                }
            }
            WindowEvent::Focused(false)=>app.mouse_up(),
            WindowEvent::CursorMoved{position,..}=>{p.mouse=(position.x as f32/scale,position.y as f32/scale);app.mouse_move(p.mouse.0,p.mouse.1);}
            WindowEvent::ModifiersChanged(m)=>{p.mods=m.state();app.ctrl=crate::ctrl(p.mods);app.shift=p.mods.shift_key();app.alt=p.mods.alt_key();}
            WindowEvent::MouseInput{state,button,..}=>{
                app.state.native_input=Some(p.kind);
                if state==ElementState::Released {app.mouse_up();}else {
                    let hit=app.hits.iter().rev().find(|h|h.rect.contains(p.mouse.0,p.mouse.1)).map(|h|h.action.clone());
                    match (button,hit) {
                        (MouseButton::Left,Some(Action::WinDrag(_)))=>{if p.last.elapsed().as_millis()<350{p.window.set_maximized(!p.window.is_maximized());}else{let _=p.window.drag_window();}p.last=Instant::now();}
                        (MouseButton::Left,Some(Action::MaxWin(_)))=>p.window.set_maximized(!p.window.is_maximized()),
                        (MouseButton::Left,Some(Action::MinWin(_)))=>p.window.set_minimized(true),
                        (MouseButton::Left,_)=>app.mouse_down(),(MouseButton::Right,_)=>app.mouse_down_right(),(MouseButton::Middle,_)=>app.mouse_down_middle(),_=>{}
                    }
                }
            }
            WindowEvent::MouseWheel{delta,..}=>{app.state.native_input=Some(p.kind);let (x,y)=match delta{MouseScrollDelta::LineDelta(x,y)=>(x,y),MouseScrollDelta::PixelDelta(p)=>(p.x as f32/40.,p.y as f32/40.)};app.wheel(x,y,p.mods.shift_key(),crate::ctrl(p.mods));}
            WindowEvent::KeyboardInput{event,..}=>{
                app.state.native_input=Some(p.kind);let pressed=event.state==ElementState::Pressed;
                if event.physical_key==winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ShiftRight){app.rshift=pressed;}
                let used=if let winit::keyboard::PhysicalKey::Code(code)=event.physical_key {app.typing_key(code,pressed,crate::ctrl(p.mods),p.mods.alt_key())}else{false};
                if pressed&&!used {app.repeat=event.repeat;app.key(&event.logical_key,crate::ctrl(p.mods),p.mods.alt_key());app.repeat=false;}
            }
            WindowEvent::HoveredFile(path)=>{crate::drop_preview::hover_file(app,path,Some(p.kind));p.window.request_redraw();},
            WindowEvent::HoveredFileCancelled=>{crate::drop_preview::clear(app);p.window.request_redraw();},
            WindowEvent::DroppedFile(path)=>{
                crate::drop_preview::clear(app);
                app.state.native_input=Some(p.kind);
                let at=crate::screen_state(&p.window).map(|v|v.0);
                #[cfg(target_os="linux")] let at=at.or_else(crate::plugin_window::pointer);
                if let (Some((cx,cy)),Ok(origin))=(at,p.window.inner_position()){
                    app.mouse=((cx-origin.x) as f32/scale,(cy-origin.y) as f32/scale);
                }
                if p.kind==Win::Playlist && !app.modal_open() && crate::views::browser::file_icon(&path.to_string_lossy())==crate::icons::Icon::FileAudio
                    && let Some((track,tick))=app.playlist_drop_position(){
                    self.stems.get_or_insert_with(||(track,tick,Vec::new())).2.push(path);
                }else{app.drop_file_at_plugin(path,None);}
            }
            _=>{}
        }
        #[cfg(windows)] if p.kind==Win::Mixer && redraw && p.refresh_editors {for h in app.plugins.hosted.values(){if let Some(e)=&h.rack_editor{e.refresh();}}}
        if matches!(p.kind,Win::PianoRoll|Win::Playlist){
            let key=(app.cursor(),(scale*100.).round().clamp(50.,400.) as u16,app.theme.colors.appearance.icon_pack);
            if p.cursor!=Some(key){crate::apply_window_cursor(&p.window,_el,key.0,key.1,key.2,cache);p.cursor=Some(key);}
        }
        p.hover=app.hover.clone();
        if redraw {p.refresh_editors=false;}
    }
}

fn set_rack_parent(app:&mut App,window:&Window){
    #[cfg(windows)] {use winit::raw_window_handle::{HasWindowHandle,RawWindowHandle};app.rack_parent=match window.window_handle().map(|h|h.as_raw()){Ok(RawWindowHandle::Win32(h))=>h.hwnd.get(),_=>0};}
    #[cfg(target_os="macos")] {use winit::raw_window_handle::{HasWindowHandle,RawWindowHandle};app.rack_parent=match window.window_handle().map(|h|h.as_raw()){Ok(RawWindowHandle::AppKit(h))=>h.ns_view.as_ptr() as isize,_=>0};}
    #[cfg(not(any(windows,target_os="macos")))] let _=(app,window);
}

// Focus/position/monitor notifications need no hit testing or rasterization.
fn needs_layout(event:&WindowEvent,drag:bool)->bool {
    matches!(event,WindowEvent::RedrawRequested|WindowEvent::MouseInput{..}|WindowEvent::MouseWheel{..}|WindowEvent::KeyboardInput{..}|WindowEvent::DroppedFile(_)|WindowEvent::HoveredFile(_)|WindowEvent::HoveredFileCancelled|WindowEvent::ModifiersChanged(_))
        ||(drag&&matches!(event,WindowEvent::Focused(false)))
}
#[cfg(test)] mod monitor_tests {
    use super::*;
    #[test] fn monitor_switch_notifications_do_not_paint() {
        for _ in 0..1000 {
            assert!(!needs_layout(&WindowEvent::Focused(true),false));
            assert!(!needs_layout(&WindowEvent::Focused(false),false));
            assert!(!needs_layout(&WindowEvent::Moved(PhysicalPosition::new(2000,100)),false));
        }
        assert!(needs_layout(&WindowEvent::RedrawRequested,false));
        assert!(needs_layout(&WindowEvent::Focused(false),true),"finish an active drag in its own view");
    }
}
