//! Native editor windows sharing the main App and audio engine.
use std::rc::Rc;
use std::num::NonZeroU32;
use std::time::Instant;
use winit::{event::{WindowEvent,ElementState,MouseButton,MouseScrollDelta},event_loop::ActiveEventLoop,window::{Window,WindowId},keyboard::ModifiersState,dpi::{LogicalSize,PhysicalPosition,PhysicalSize}};
use crate::{app::{App,Win,Action},layout::{self,MainWin}};
use os_ui::Canvas;
#[derive(Default)]
pub struct Windows { panes:Vec<Pane>, screens:Vec<[i32;4]> }
struct Pane {
    kind:Win, window:Rc<Window>, surface:softbuffer::Surface<Rc<Window>,Rc<Window>>, canvas:Canvas,
    frame:MainWin, mouse:(f32,f32), mods:ModifiersState, last:Instant, painted:Instant,
}
fn key(w:Win)-> &'static str {match w {Win::Mixer=>"native_mixer",Win::PianoRoll=>"native_piano",_=>"native_playlist"}}
fn title(w:Win)-> &'static str {match w {Win::Mixer=>"KNAY Studio - Mixer",Win::PianoRoll=>"KNAY Studio - Piano roll",_=>"KNAY Studio - Playlist"}}
impl Windows {
    pub fn owns(&self,id:WindowId)->bool {self.panes.iter().any(|p|p.window.id()==id)}
    pub fn prepare_drag(&mut self,app:&mut App) {
        if app.drag.is_none(){return;}
        if let Some(p)=self.panes.iter_mut().find(|p|Some(p.kind)==app.state.native_input) {
            app.state.native_view=Some(p.kind);app.mouse=p.mouse;app.paint(&mut p.canvas,None);
        }
    }
    pub fn save(&self) {for p in &self.panes {layout::save_native(key(p.kind),p.frame);}}
    pub fn sync(&mut self,app:&mut App,el:&ActiveEventLoop) {
        let screens=crate::screens(el);let changed=screens!=self.screens;self.screens=screens.clone();
        self.panes.retain(|p|{let keep=app.state.native_detached.contains(&p.kind)&&app.state.shown(p.kind);if !keep {layout::save_native(key(p.kind),p.frame);}keep});
        let mut created=Vec::new();
        for kind in app.state.native_detached.clone() {
            if !app.state.shown(kind)||self.panes.iter().any(|p|p.kind==kind){continue;}
            let initial=layout::native_win(key(kind)).unwrap_or(MainWin{pos:(100,100),size:(1100,800),max:false});
            let frame=layout::on_screens(initial,&screens);
            let attrs=Window::default_attributes().with_title(title(kind)).with_inner_size(PhysicalSize::new(frame.size.0,frame.size.1)).with_position(PhysicalPosition::new(frame.pos.0,frame.pos.1)).with_min_inner_size(LogicalSize::new(kind.min_size().0 as f64,kind.min_size().1 as f64)).with_maximized(frame.max).with_visible(false);
            let made=(||->Result<Pane,String>{
                let window=Rc::new(el.create_window(attrs).map_err(|e|e.to_string())?);
                let context=softbuffer::Context::new(window.clone()).map_err(|e|e.to_string())?;
                let surface=softbuffer::Surface::new(&context,window.clone()).map_err(|e|e.to_string())?;
                let size=window.inner_size();let canvas=Canvas::new(size.width.max(1),size.height.max(1),crate::ui_scale(&window) as f32).map_err(|e|e.to_string())?;
                Ok(Pane{kind,window,surface,canvas,frame,mouse:(-1.,-1.),mods:ModifiersState::empty(),last:Instant::now(),painted:Instant::now()})
            })();
            match made {Ok(p)=>{created.push(p.window.id());self.panes.push(p);},Err(e)=>{app.state.native_detached.retain(|w|*w!=kind);app.state.message=Some(format!("Could not detach window: {e}"));}}
        }
        for p in &mut self.panes {
            if changed {
                let next=layout::on_screens(p.frame,&screens);
                if next!=p.frame {p.window.set_maximized(false);p.window.set_outer_position(PhysicalPosition::new(next.pos.0,next.pos.1));let _=p.window.request_inner_size(PhysicalSize::new(next.size.0,next.size.1));p.frame=next;}
            }
            if p.painted.elapsed().as_millis()>=if app.animating(){16}else{100} {p.window.request_redraw();}
        }
        for id in created {self.event(app,el,id,WindowEvent::RedrawRequested);}
    }
    pub fn event(&mut self,app:&mut App,_el:&ActiveEventLoop,id:WindowId,event:WindowEvent) {
        if app.loading() && matches!(event,WindowEvent::MouseInput{state:ElementState::Pressed,..}|WindowEvent::KeyboardInput{..}|WindowEvent::MouseWheel{..}|WindowEvent::DroppedFile(_)){return;}
        let Some(p)=self.panes.iter_mut().find(|p|p.window.id()==id)else{return};
        let scale=crate::ui_scale(&p.window) as f32;
        if matches!(event,WindowEvent::Moved(_)|WindowEvent::Resized(_)) {
            if !p.window.is_maximized() && p.window.is_minimized()!=Some(true) {if let Some((pos,size))=crate::frame_of(&p.window){p.frame.pos=pos;p.frame.size=size;}}
            p.frame.max=p.window.is_maximized();
        }
        if matches!(event,WindowEvent::CloseRequested) {app.state.hide(p.kind);app.invalidate();return;}
        if matches!(event,WindowEvent::Focused(true)|WindowEvent::MouseInput{state:ElementState::Pressed,..}|WindowEvent::KeyboardInput{..}|WindowEvent::MouseWheel{..}) {if app.state.native_input!=Some(p.kind){app.state.playlist.popup=None;app.state.menu_open=None;}app.state.raise(p.kind);app.state.native_input=Some(p.kind);app.auto_close.main_focused=true;}
        if matches!(event,WindowEvent::Focused(false)) {app.typing_release_all();}
        let size=p.window.inner_size();if size.width==0||size.height==0{return;}
        if p.canvas.pixmap.width()!=size.width||p.canvas.pixmap.height()!=size.height||p.canvas.scale!=scale {p.canvas.resize(size.width,size.height,scale);}
        app.state.native_view=Some(p.kind);app.state.display_scale=p.window.scale_factor() as f32;app.mouse=p.mouse;
        app.paint(&mut p.canvas,None);
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
            WindowEvent::DroppedFile(path)=>{app.state.native_input=Some(p.kind);app.drop_file(path);}
            _=>{}
        }
        app.invalidate();
    }
}
