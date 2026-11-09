// SPDX-License-Identifier: GPL-3.0-or-later
//! Native AppKit parents for mixer strip editors. Editor sizes are device pixels;
//! AppKit geometry is points. The composited editor fits the mixer height.
use std::{cell::RefCell, ffi::c_void, time::Instant};
use objc2::{ClassType,define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly, AnyThread};
use objc2::rc::Retained;
use objc2_app_kit::{NSView, NSResponder, NSScrollView, NSScreen, NSEvent, NSTrackingArea, NSTrackingAreaOptions};
use objc2_foundation::{NSObjectProtocol,NSPoint, NSRect, NSSize};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGMutablePath;
use objc2_quartz_core::{CAShapeLayer, CATransaction};
use crate::strip_geometry::*;

#[derive(Clone, Copy)]
enum PointerAction {
    Down,
    Up,
    RightDown,
    RightUp,
    Move,
    Drag,
    RightDrag,
    Scroll,
    OtherDown,
    OtherUp,
    OtherDrag,
}
struct InputState {
    inner: Retained<NSView>,
    zoom: f64,
    left: f64,
    height: f64,
    width: f64,
    capture: Option<Retained<NSView>>,
}
thread_local!{static CLICKED:std::cell::Cell<Option<u16>>=const{std::cell::Cell::new(None)};}
pub fn contains_editor(view:&NSView)->bool{
    if view.isKindOfClass(StripView::class()){return true;}
    let mut ancestor=unsafe{view.superview()};
    while let Some(parent)=ancestor{
        if parent.isKindOfClass(StripView::class()){return true;}
        ancestor=unsafe{parent.superview()};
    }
    false
}
pub fn clicked_strip()->Option<u16>{CLICKED.with(|t|t.take())}
struct StripData {
    track:std::cell::Cell<Option<u16>>,
    clip: RefCell<Option<Vec<NSRect>>>,
    input: RefCell<Option<InputState>>,
    forwarding: RefCell<Vec<usize>>,
}
define_class!(
    #[unsafe(super(NSView, NSResponder))]
    #[thread_kind = MainThreadOnly]
    #[name = "LemonMixerStripView"]
    #[ivars = StripData]
    struct StripView;
    impl StripView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool { true }
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, p:NSPoint)->Option<Retained<NSView>> {
            let local=match unsafe {self.superview()} {Some(s)=>self.convertPoint_fromView(p,Some(&s)),None=>p};
            if self.isHidden() || self.ivars().clip.borrow().as_ref().is_some_and(|rs|!rs.iter().any(|r|local.x>=r.origin.x&&local.y>=r.origin.y&&local.x<r.origin.x+r.size.width&&local.y<r.origin.y+r.size.height)){None}else if let Some(input)=self.ivars().input.borrow().as_ref(){if local.x>=0.&&local.y>=0.&&local.x<input.width*input.zoom&&local.y<input.height*input.zoom{unsafe{Retained::retain(self as *const Self as *mut NSView)}}else{None}}else{unsafe {msg_send![super(self), hitTest:p]}}
        }
        #[unsafe(method(mouseDown:))] fn down(&self,e:&NSEvent){self.forward(e,PointerAction::Down);}
        #[unsafe(method(mouseUp:))] fn up(&self,e:&NSEvent){self.forward(e,PointerAction::Up);}
        #[unsafe(method(rightMouseDown:))] fn right_down(&self,e:&NSEvent){self.forward(e,PointerAction::RightDown);}
        #[unsafe(method(rightMouseUp:))] fn right_up(&self,e:&NSEvent){self.forward(e,PointerAction::RightUp);}
        #[unsafe(method(mouseMoved:))] fn moved(&self,e:&NSEvent){self.forward(e,PointerAction::Move);}
        #[unsafe(method(mouseDragged:))] fn dragged(&self,e:&NSEvent){self.forward(e,PointerAction::Drag);}
        #[unsafe(method(rightMouseDragged:))] fn right_dragged(&self,e:&NSEvent){self.forward(e,PointerAction::RightDrag);}
        #[unsafe(method(scrollWheel:))] fn wheel(&self,e:&NSEvent){self.forward(e,PointerAction::Scroll);}
        #[unsafe(method(otherMouseDown:))] fn other_down(&self,e:&NSEvent){self.forward(e,PointerAction::OtherDown);}
        #[unsafe(method(otherMouseUp:))] fn other_up(&self,e:&NSEvent){self.forward(e,PointerAction::OtherUp);}
        #[unsafe(method(otherMouseDragged:))] fn other_dragged(&self,e:&NSEvent){self.forward(e,PointerAction::OtherDrag);}

    }
);
impl StripView {
    fn forward(&self, event: &NSEvent, kind: PointerAction) {
        if self
            .ivars()
            .forwarding
            .borrow()
            .contains(&(event as *const NSEvent as usize))
        {
            return;
        }
        let local = self.convertPoint_fromView(event.locationInWindow(), None);
        if !matches!(
            kind,
            PointerAction::Up
                | PointerAction::RightUp
                | PointerAction::Drag
                | PointerAction::RightDrag
                | PointerAction::OtherUp
                | PointerAction::OtherDrag
        ) && self.ivars().clip.borrow().as_ref().is_some_and(|rs| {
            !rs.iter().any(|r| {
                local.x >= r.origin.x
                    && local.y >= r.origin.y
                    && local.x < r.origin.x + r.size.width
                    && local.y < r.origin.y + r.size.height
            })
        }) {
            return;
        }
        if matches!(kind,PointerAction::Down) && let Some(track)=self.ivars().track.get(){
            CLICKED.with(|t|t.set(Some(track)));os_clap::wake();
        }
        let route = {
            let mut state = self.ivars().input.borrow_mut();
            let Some(state) = state.as_mut() else {
                return;
            };
            let native = NSPoint::new(
                local.x / state.zoom + state.left,
                state.height - local.y / state.zoom,
            );
            let hit = || state.inner.hitTest(native);
            let target = if matches!(
                kind,
                PointerAction::Up
                    | PointerAction::RightUp
                    | PointerAction::Drag
                    | PointerAction::RightDrag
                    | PointerAction::OtherUp
                    | PointerAction::OtherDrag
            ) {
                state.capture.clone().or_else(hit)
            } else {
                hit()
            };
            let Some(target) =
                target.filter(|t| Retained::as_ptr(t) != Retained::as_ptr(&state.inner))
            else {
                return;
            };
            if matches!(
                kind,
                PointerAction::Down | PointerAction::RightDown | PointerAction::OtherDown
            ) {
                state.capture = Some(target.clone());
            }
            if matches!(
                kind,
                PointerAction::Up | PointerAction::RightUp | PointerAction::OtherUp
            ) {
                state.capture = None;
            }
            (
                target,
                state.inner.convertPoint_toView(native, None),
                state.zoom,
            )
        };
        let mapped = MappedEvent::new_scaled(
            event,
            route.1,
            if matches!(
                kind,
                PointerAction::Move
                    | PointerAction::Drag
                    | PointerAction::RightDrag
                    | PointerAction::OtherDrag
            ) {
                route.2
            } else {
                1.
            },
        );
        let target = route.0;
        if matches!(
            kind,
            PointerAction::Down | PointerAction::RightDown | PointerAction::OtherDown
        ) {
            if let Some(window) = target.window() {
                window.makeFirstResponder(Some(&target));
            }
        }
        self.ivars().forwarding.borrow_mut().extend([
            event as *const NSEvent as usize,
            Retained::as_ptr(&mapped) as usize,
        ]);
        unsafe {
            match kind {
                PointerAction::Down => {
                    let _: () = msg_send![&*target,mouseDown:&*mapped];
                }
                PointerAction::Up => {
                    let _: () = msg_send![&*target,mouseUp:&*mapped];
                }
                PointerAction::RightDown => {
                    let _: () = msg_send![&*target,rightMouseDown:&*mapped];
                }
                PointerAction::RightUp => {
                    let _: () = msg_send![&*target,rightMouseUp:&*mapped];
                }
                PointerAction::Move => {
                    let _: () = msg_send![&*target,mouseMoved:&*mapped];
                }
                PointerAction::Drag => {
                    let _: () = msg_send![&*target,mouseDragged:&*mapped];
                }
                PointerAction::RightDrag => {
                    let _: () = msg_send![&*target,rightMouseDragged:&*mapped];
                }
                PointerAction::Scroll => {
                    let _: () = msg_send![&*target,scrollWheel:&*mapped];
                }
                PointerAction::OtherDown => {
                    let _: () = msg_send![&*target,otherMouseDown:&*mapped];
                }
                PointerAction::OtherUp => {
                    let _: () = msg_send![&*target,otherMouseUp:&*mapped];
                }
                PointerAction::OtherDrag => {
                    let _: () = msg_send![&*target,otherMouseDragged:&*mapped];
                }
            }
        }
        let mut stack = self.ivars().forwarding.borrow_mut();
        let len = stack.len();
        stack.truncate(len - 2);
    }
}
fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w.max(1.), h.max(1.)))
}
fn view(mtm: MainThreadMarker) -> Retained<StripView> {
    let v = StripView::alloc(mtm).set_ivars(StripData {
        clip: RefCell::new(None),
        track:std::cell::Cell::new(None),
        input: RefCell::new(None),
        forwarding: RefCell::new(Vec::new()),
    });
    unsafe { msg_send![super(v), initWithFrame:rect(0.,0.,1.,1.)] }
}

// Preserve the original scroll phases, pressure, keyboard modifiers and timing.
// Only the location and pointer motion use native editor coordinates.
struct EventData {
    original: Retained<objc2_app_kit::NSEvent>,
    point: NSPoint,
    delta_scale: f64,
}
define_class!(
 #[unsafe(super(objc2_foundation::NSObject))]
 #[thread_kind = MainThreadOnly]
 #[name = "LemonStripMappedEvent"]
 #[ivars = EventData]
 struct MappedEvent;
 impl MappedEvent {
  #[unsafe(method(locationInWindow))]
  fn location(&self)->NSPoint { self.ivars().point }
  #[unsafe(method(deltaX))] fn dx(&self)->f64{self.ivars().original.deltaX()/self.ivars().delta_scale}
  #[unsafe(method(deltaY))] fn dy(&self)->f64{self.ivars().original.deltaY()/self.ivars().delta_scale}
  #[unsafe(method(forwardingTargetForSelector:))]
  fn forwarding(&self,_sel:objc2::runtime::Sel)->*const objc2::runtime::AnyObject { Retained::as_ptr(&self.ivars().original).cast() }
  #[unsafe(method(respondsToSelector:))]
  fn responds(&self,sel:objc2::runtime::Sel)->bool { unsafe {msg_send![&*self.ivars().original,respondsToSelector:sel]} }
  #[unsafe(method(isKindOfClass:))]
  fn kind(&self,c:&objc2::runtime::AnyClass)->bool { unsafe {msg_send![&*self.ivars().original,isKindOfClass:c]} }
 }
);
impl MappedEvent {
    fn new_scaled(event: &NSEvent, point: NSPoint, delta_scale: f64) -> Retained<Self> {
        let allocated = Self::alloc(MainThreadMarker::new().unwrap()).set_ivars(EventData {
            original: unsafe { Retained::retain(event as *const _ as *mut _).unwrap() },
            point,
            delta_scale,
        });
        unsafe { msg_send![super(allocated), init] }
    }
}

pub struct Editor {
    outer:Retained<StripView>, scroll:Retained<NSScrollView>, document:Retained<StripView>, zoom_view:Retained<NSView>, inner:Retained<NSView>,
    hidden_since:Option<Instant>,
    pub owner:isize, pub scale:f32, backing:f64, size:(u32,u32), profile:Profile, pages:Vec<Page>, page:usize,
    rect:[i32;4], clip:Vec<[i32;4]>, page_left:f32, page_width:f32, slide:Option<(Instant,f32,f32)>, layout_changed:bool,
}
impl Editor {
    pub fn new(owner:isize,scale:f32)->Result<Self,String>{
        let mtm=MainThreadMarker::new().ok_or("Channel strips must open on the main thread")?;
        crate::plugin_window::prepare_embedded(mtm);
        let parent=unsafe {(owner as *const NSView).as_ref()}.ok_or("Mixer window unavailable")?;
        let backing=parent.window().map_or(1.,|w|w.backingScaleFactor());
        let outer=view(mtm); outer.setWantsLayer(true); outer.setHidden(true);
        let tracking=unsafe {NSTrackingArea::initWithRect_options_owner_userInfo(NSTrackingArea::alloc(),rect(0.,0.,1.,1.),NSTrackingAreaOptions::MouseMoved|NSTrackingAreaOptions::ActiveInKeyWindow|NSTrackingAreaOptions::InVisibleRect,Some(&outer),None)};
        outer.addTrackingArea(&tracking);
        let scroll=NSScrollView::initWithFrame(NSScrollView::alloc(mtm),rect(0.,0.,1.,1.));
        scroll.setHasVerticalScroller(true); scroll.setAutohidesScrollers(true); scroll.setDrawsBackground(false);
        let document=view(mtm); document.setWantsLayer(true);
        if let Some(layer)=document.layer(){layer.setMasksToBounds(true);}
        let zoom_view=NSView::initWithFrame(NSView::alloc(mtm),rect(0.,0.,560.,700.));
        zoom_view.setAutoresizesSubviews(false);
        let inner=NSView::initWithFrame(NSView::alloc(mtm),rect(0.,0.,560.,700.));
        inner.setAutoresizesSubviews(false);
        zoom_view.addSubview(&inner); document.addSubview(&zoom_view); scroll.setDocumentView(Some(&document)); outer.addSubview(&scroll); parent.addSubview(&outer);
        Ok(Self{hidden_since:None,outer,scroll,document,zoom_view,inner,owner,scale,backing,size:(1,1),profile:Profile::Whole,pages:vec![Page{left:0.,right:1.}],page:0,rect:[0;4],clip:vec![],page_left:0.,page_width:1.,slide:None,layout_changed:false})
    }
    pub fn area(&self)->*mut c_void {Retained::as_ptr(&self.inner) as *mut c_void}
    pub fn accept_size(&mut self,size:(u32,u32))->bool {
        if !crate::rack_mixer::vertical_shape(size){return false;}
        if self.size!=size {self.size=size;self.pages=pages(self.profile,size);self.page=self.page.min(self.pages.len()-1);self.slide=None;self.page_left=self.pages[self.page].left;self.page_width=self.pages[self.page].width();self.layout();} true
    }
    fn zoom(&self)->f64 {mac_strip_zoom(self.size.1,self.rect[3]-self.rect[1])}
    fn layout(&self){
        let k=self.backing;let zoom=self.zoom();
        let width=self.size.0 as f64/k;let height=self.size.1 as f64/k;
        CATransaction::begin();CATransaction::setDisableActions(true);
        self.document.setFrameSize(NSSize::new(self.page_width as f64/k*zoom,height*zoom));
        // Keep AppKit's view/backing geometry native. Bounds-based scaling makes
        // Waves draw into a smaller backing surface and cuts off its controls.
        // A host-owned sublayer transform scales the completed surface instead.
        // StripView maps pointer events back to the native editor coordinates.
        self.zoom_view.setFrame(rect(-self.page_left as f64/k*zoom,0.,width,height));
        self.zoom_view.setBoundsSize(NSSize::new(width,height));
        self.zoom_view.setWantsLayer(true);
        if let Some(layer)=self.zoom_view.layer(){
          let mut t=objc2_quartz_core::CATransform3D::new_scale(zoom,zoom,1.);t.m42=height*(1.-zoom);layer.setSublayerTransform(t);
        }
        self.inner.setFrame(rect(0.,0.,width,height));
        self.inner.setBoundsSize(NSSize::new(width,height));
        let capture=self.outer.ivars().input.borrow_mut().as_mut().and_then(|s|s.capture.take());
        *self.outer.ivars().input.borrow_mut()=Some(InputState{inner:self.inner.clone(),zoom,left:self.page_left as f64/k,height,width:self.page_width as f64/k,capture});
        for child in self.inner.subviews().iter(){let f=child.frame();child.setFrameOrigin(NSPoint::new(0.,height-f.size.height));}
        CATransaction::commit();
    }
    pub fn required_width(&self)->f32 {
        let zoom=self.zoom() as f32;
        let scrollbar=if self.size.1 as f32*zoom>(self.rect[3]-self.rect[1]) as f32+1. {18.*self.backing as f32}else{0.};
        (self.page_width*zoom+scrollbar)/self.scale+6.
    }
    pub fn minimum_content_height(&self)->Option<f32>{None}
    pub fn page_info(&self)->(usize,usize){(self.page,self.pages.len())}
    pub fn page_by(&mut self,delta:i8){
        let next=(self.page as isize+delta as isize).clamp(0,self.pages.len() as isize-1) as usize;
        if next!=self.page {self.page=next;self.slide=Some((Instant::now(),self.page_left,self.page_width));}
    }
    pub fn take_layout_changed(&mut self)->bool {std::mem::take(&mut self.layout_changed)}
    pub fn animate(&mut self) {
        if let Some((start,left,width))=self.slide {
            let t=(start.elapsed().as_secs_f32()/0.20).min(1.);let f=t*t*(3.-2.*t);let page=self.pages[self.page];
            self.page_left=left+(page.left-left)*f;self.page_width=width+(page.width()-width)*f;
            if t>=1. {self.slide=None;}
            self.layout();self.layout_changed=true;
        }
    }
    pub fn screen_point(&self,p:(i32,i32))->(i32,i32){
        let at=self.outer.convertPoint_toView(NSPoint::new((p.0 as f64-self.page_left as f64)/self.backing*self.zoom(),p.1 as f64/self.backing*self.zoom()),None);
        let at=self.inner.window().map_or(at,|w|w.convertPointToScreen(at));
        let height=NSScreen::screens(MainThreadMarker::new().unwrap()).firstObject().map_or(0.,|s|s.frame().size.height);
        ((at.x*self.backing).round() as i32,((height-at.y)*self.backing).round() as i32)
    }
    pub fn canvas_point(&self,x:i32,y:i32)->(f32,f32){
        let owner=unsafe {&*(self.owner as *const NSView)};
        let height=NSScreen::screens(MainThreadMarker::new().unwrap()).firstObject().map_or(0.,|s|s.frame().size.height);
        let p=NSPoint::new(x as f64/self.backing,height-y as f64/self.backing);
        let p=owner.window().map_or(p,|w|w.convertPointFromScreen(p));
        let p=owner.convertPoint_fromView(p,None);
        ((p.x*self.backing) as f32/self.scale,(p.y*self.backing) as f32/self.scale)
    }
    pub fn hide_for_scroll(&mut self)->bool{
        if self.hidden_since.is_none(){self.outer.setHidden(true);self.hidden_since=Some(Instant::now());}
        self.hidden_since.is_some_and(|t|t.elapsed()<std::time::Duration::from_secs(2))
    }
    pub fn place(&mut self,r:[i32;4],clip:[i32;4],covers:&[[i32;4]]) {
        if self.hidden_since.take().is_some(){self.clip.clear();}
        let mut key=vec![clip];key.extend_from_slice(covers);if self.rect==r&&self.clip==key{return;}
        self.rect=r;self.clip=key;let k=self.backing;
        self.outer.setFrame(rect(r[0] as f64/k,r[1] as f64/k,(r[2]-r[0]) as f64/k,(r[3]-r[1]) as f64/k));
        self.scroll.setFrame(rect(0.,0.,(r[2]-r[0]) as f64/k,(r[3]-r[1]) as f64/k));self.layout();
        let visible=crate::plugin_window::visible(r,clip,covers);
        let local:Vec<_>=visible.iter().map(|v|rect((v[0]-r[0]) as f64/k,(v[1]-r[1]) as f64/k,(v[2]-v[0]) as f64/k,(v[3]-v[1]) as f64/k)).collect();
        let path=CGMutablePath::new();
        for v in &local {unsafe {CGMutablePath::add_rect(Some(&path),std::ptr::null(),CGRect::new(CGPoint::new(v.origin.x,v.origin.y),CGSize::new(v.size.width,v.size.height)));}}
        CATransaction::begin();CATransaction::setDisableActions(true);
        if let Some(layer)=self.outer.layer(){let mask=CAShapeLayer::new();mask.setFrame(layer.bounds());mask.setPath(Some(&path));unsafe {layer.setMask(Some(&mask));}}
        CATransaction::commit();*self.outer.ivars().clip.borrow_mut()=Some(local);self.outer.setHidden(visible.is_empty());
    }
}
impl Drop for Editor {fn drop(&mut self){self.outer.removeFromSuperview();}}

impl crate::plugins::Plugins {
    pub fn validate_vertical_rack(&mut self,key:usize,owner:isize,scale:f32,_area:[i32;4])->Result<(),String>{
        if owner==0{return Ok(());}
        let h=self.hosted.get_mut(&key).ok_or("Plugin unavailable")?;
        if h.window.is_some(){return Err("Close the plugin window before docking this strip.".into());}
        let e=Editor::new(owner,scale)?;
        let mut p=h.plugin.lock().map_err(|_|"Plugin busy")?;
        let result=p.open_editor(e.area(),e.backing as f32);p.close_editor();
        let size=result?;
        if !crate::rack_mixer::vertical_shape(size){return Err("This plugin reported an invalid editor size.".into());}
        Ok(())
    }
    pub fn close_rack(&mut self,key:usize){if let Some(h)=self.hosted.get_mut(&key){if h.rack_editor.is_some(){if let Ok(mut p)=h.plugin.lock(){p.close_editor();}h.rack_editor=None;}h.rack_error=None;}}
    pub fn close_racks(&mut self){let keys:Vec<_>=self.hosted.keys().copied().collect();for key in keys{self.close_rack(key);}}
    pub fn show_rack(&mut self,key:usize,owner:isize,scale:f32,r:[i32;4],clip:[i32;4],covers:&[[i32;4]]){
        let page=self.hosted.get(&key).and_then(|h|h.rack_editor.as_ref()).map_or(0,|e|e.page);
        let backing=unsafe {(owner as *const NSView).as_ref()}.and_then(|v|v.window()).map_or(1.,|w|w.backingScaleFactor());
        if self.hosted.get(&key).and_then(|h|h.rack_editor.as_ref()).is_some_and(|e|e.owner!=owner||e.scale!=scale||e.backing!=backing){self.close_rack(key);}
        let Some(h)=self.hosted.get_mut(&key)else{return};
        if h.window.is_some()||h.rack_error.is_some(){return;}
        if h.rack_editor.is_none(){
            let result=(||{
                let mut e=Editor::new(owner,scale)?;e.rect=r;
                let track=crate::plugins::fx_of(key).map(|(t,_)|t);e.outer.ivars().track.set(track);e.document.ivars().track.set(track);
                let mut p=h.plugin.lock().map_err(|_|"Plugin busy".to_string())?;e.profile=profile(p.desc());
                let result=p.open_editor(e.area(),e.backing as f32);
                let size=match result {Ok(size)=>size,Err(err)=>{p.close_editor();return Err(err);}};
                if !e.accept_size(size){p.close_editor();return Err("This plugin reported an invalid editor size.".into());}
                e.page=page.min(e.pages.len()-1);e.page_left=e.pages[e.page].left;e.page_width=e.pages[e.page].width();
                Ok(e)
            })();
            match result {Ok(e)=>h.rack_editor=Some(e),Err(e)=>{h.rack_error=Some(e);return;}}
        }
        if let Some(e)=&mut h.rack_editor{e.place(r,clip,covers);}
    }
}
