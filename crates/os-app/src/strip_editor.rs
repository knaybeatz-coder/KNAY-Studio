// SPDX-License-Identifier: GPL-3.0-or-later
//! Original plugin editors with painted native parents and section-aligned paging.
use std::{ffi::c_void,time::{Duration,Instant}};
use windows_sys::Win32::UI::Controls::SetScrollInfo;
use windows_sys::Win32::{Foundation::{HWND,LPARAM,LRESULT,WPARAM,RECT,POINT},Graphics::Gdi::*,System::LibraryLoader::GetModuleHandleW,UI::WindowsAndMessaging::*};
use crate::strip_geometry::*;
fn ease(t:f32)->f32{let t=t.clamp(0.,1.);t*t*(3.-2.*t)}
struct Data {inner:HWND,port:HWND,size:(i32,i32),offset:i32,x:f32,page_width:f32}
pub struct Editor {hidden_since:Option<Instant>,hwnd:HWND,data:Box<Data>,pub owner:isize,pub scale:f32,rect:[i32;4],clip:Vec<[i32;4]>,profile:Profile,pages:Vec<Page>,page:usize,slide:Option<(Instant,f32,f32)>,open_height:i32,pending_height:Option<(i32,Instant)>}
fn wide(s:&str)->Vec<u16>{s.encode_utf16().chain(Some(0)).collect()}
unsafe fn repaint(hwnd:HWND){unsafe{RedrawWindow(hwnd,std::ptr::null(),std::ptr::null_mut(),RDW_INVALIDATE|RDW_ERASE|RDW_ALLCHILDREN);}}
unsafe fn position(hwnd:HWND,d:&mut Data){unsafe{
    let mut r:RECT=std::mem::zeroed();GetClientRect(hwnd,&mut r);
    d.offset=d.offset.clamp(0,(d.size.1-r.bottom).max(0));
    let info=SCROLLINFO{cbSize:std::mem::size_of::<SCROLLINFO>() as u32,fMask:SIF_RANGE|SIF_PAGE|SIF_POS,nMin:0,nMax:d.size.1-1,nPage:r.bottom.max(1) as u32,nPos:d.offset,nTrackPos:0};SetScrollInfo(hwnd,SB_VERT,&info,1);
    GetClientRect(hwnd,&mut r);
    let left=d.x.round() as i32;
    let w=((d.x+d.page_width).round() as i32-left).max(1);
    SetWindowPos(d.port,std::ptr::null_mut(),(r.right-w).max(0)/2,0,w,r.bottom.max(1),SWP_NOZORDER|SWP_NOACTIVATE|SWP_NOCOPYBITS);
    SetWindowPos(d.inner,std::ptr::null_mut(),-left,-d.offset,d.size.0.max(1),d.size.1.max(1),SWP_NOZORDER|SWP_NOACTIVATE|SWP_NOCOPYBITS);
    repaint(hwnd);crate::plugin_window::embedded_exposed();
}}
unsafe extern "system" fn proc(hwnd:HWND,msg:u32,wp:WPARAM,lp:LPARAM)->LRESULT{unsafe{
    let ptr=GetWindowLongPtrW(hwnd,GWLP_USERDATA) as *mut Data;
    if !ptr.is_null()&&msg==WM_VSCROLL{
        let d=&mut *ptr;let mut info:SCROLLINFO=std::mem::zeroed();info.cbSize=std::mem::size_of::<SCROLLINFO>() as u32;info.fMask=SIF_ALL;GetScrollInfo(hwnd,SB_VERT,&mut info);
        d.offset=match (wp&0xffff) as i32{SB_LINEUP=>d.offset-24,SB_LINEDOWN=>d.offset+24,SB_PAGEUP=>d.offset-info.nPage as i32,SB_PAGEDOWN=>d.offset+info.nPage as i32,SB_THUMBTRACK|SB_THUMBPOSITION=>info.nTrackPos,SB_TOP=>0,SB_BOTTOM=>info.nMax,_=>d.offset};position(hwnd,d);return 0;
    }
    DefWindowProcW(hwnd,msg,wp,lp)
}}
impl Editor{
    pub fn new(owner:isize,scale:f32)->Result<Self,String>{unsafe{
        crate::plugin_window::prepare_embedded();
        static INIT:std::sync::Once=std::sync::Once::new();let class=wide("LemonMixerRackViewport");INIT.call_once(||{let mut wc:WNDCLASSW=std::mem::zeroed();wc.lpfnWndProc=Some(proc);wc.style=CS_HREDRAW|CS_VREDRAW;wc.hbrBackground=CreateSolidBrush(0x001d1b17);wc.hInstance=GetModuleHandleW(std::ptr::null());wc.lpszClassName=class.as_ptr();RegisterClassW(&wc);});
        let create=|parent,style|CreateWindowExW(0,class.as_ptr(),std::ptr::null(),WS_CHILD|WS_CLIPCHILDREN|WS_CLIPSIBLINGS|style,0,0,1,1,parent,std::ptr::null_mut(),GetModuleHandleW(std::ptr::null()),std::ptr::null());
        let hwnd=create(owner as HWND,WS_VSCROLL);if hwnd.is_null(){return Err("Could not create mixer editor viewport".into());}
        let port=create(hwnd,WS_VISIBLE);if port.is_null(){DestroyWindow(hwnd);return Err("Could not create section viewport".into());}
        let inner=create(port,WS_VISIBLE);if inner.is_null(){DestroyWindow(hwnd);return Err("Could not create editor parent".into());}
        let mut data=Box::new(Data{inner,port,size:(1,1),offset:0,x:0.,page_width:1.});SetWindowLongPtrW(hwnd,GWLP_USERDATA,(&mut *data as *mut Data) as isize);
        Ok(Self{hidden_since:None,hwnd,data,owner,scale,rect:[0;4],clip:vec![],profile:Profile::Whole,pages:vec![Page{left:0.,right:1.}],page:0,slide:None,open_height:0,pending_height:None})
    }}
    pub fn hide_for_scroll(&mut self)->bool {
        if self.hidden_since.is_none(){unsafe{ShowWindow(self.hwnd,SW_HIDE);}self.hidden_since=Some(Instant::now());}
        self.hidden_since.is_some_and(|t|t.elapsed()<Duration::from_secs(2))
    }
    pub fn refresh(&self){unsafe{repaint(self.hwnd);}crate::plugin_window::embedded_exposed();}
    pub fn area(&self)->*mut c_void{self.data.inner as _}
    pub fn accept_size(&mut self,size:(u32,u32))->bool{
        if !crate::rack_mixer::vertical_shape(size){return false;}
        if self.data.size==(size.0 as i32,size.1 as i32){return true;}
        self.data.size=(size.0 as i32,size.1 as i32);self.pages=pages(self.profile,size);self.page=self.page.min(self.pages.len()-1);self.slide=None;
        self.data.x=self.pages[self.page].left;self.data.page_width=self.pages[self.page].width();unsafe{position(self.hwnd,&mut self.data);}true
    }
    pub fn required_width(&self)->f32{let scrollbar=if self.data.size.1>self.rect[3]-self.rect[1]{(unsafe{GetSystemMetrics(SM_CXVSCROLL)}) as f32}else{0.};(self.data.page_width+scrollbar)/self.scale+6.}
    pub fn minimum_content_height(&self)->Option<f32>{(self.data.size.1>self.open_height+3).then_some(self.data.size.1 as f32/self.scale)}
    pub fn page_info(&self)->(usize,usize){(self.page,self.pages.len())}
    pub fn page_by(&mut self,delta:i8){
        let next=(self.page as isize+delta as isize).clamp(0,self.pages.len() as isize-1) as usize;if next==self.page{return;}
        self.page=next;self.slide=Some((Instant::now(),self.data.x,self.data.page_width));crate::plugin_window::embedded_exposed();
    }
    pub fn animate(&mut self){if let Some((start,x,w))=self.slide{
        let t=(start.elapsed().as_secs_f32()/0.20).min(1.);let f=ease(t);let page=self.pages[self.page];
        self.data.x=x+(page.left-x)*f;self.data.page_width=w+(page.width()-w)*f;
        if t>=1.{self.data.x=page.left;self.data.page_width=page.width();self.slide=None;}
        unsafe{position(self.hwnd,&mut self.data);}
    }}
    fn rescale_due(&mut self,height:i32)->bool{
        if height==self.open_height{self.pending_height=None;return false;}
        match self.pending_height{Some((h,start)) if h==height=>{crate::plugin_window::embedded_exposed();start.elapsed()>=Duration::from_millis(180)},_=>{self.pending_height=Some((height,Instant::now()));crate::plugin_window::embedded_exposed();false}}
    }
    pub fn screen_point(&self,p:(i32,i32))->(i32,i32){unsafe{let mut pt=POINT{x:p.0,y:p.1};ClientToScreen(self.data.inner,&mut pt);(pt.x,pt.y)}}
    pub fn place(&mut self,r:[i32;4],clip:[i32;4],covers:&[[i32;4]]){unsafe{
        if self.hidden_since.take().is_some(){self.clip.clear();}
        let mut key=vec![clip];key.extend_from_slice(covers);if r==self.rect&&key==self.clip{return;}self.clip=key;crate::plugin_window::embedded_exposed();
        let size_changed=r[2]-r[0]!=self.rect[2]-self.rect[0] || r[3]-r[1]!=self.rect[3]-self.rect[1];
        if r!=self.rect{SetWindowPos(self.hwnd,HWND_BOTTOM,r[0],r[1],(r[2]-r[0]).max(1),(r[3]-r[1]).max(1),SWP_NOACTIVATE|SWP_NOCOPYBITS);self.rect=r;if size_changed{position(self.hwnd,&mut self.data);}}
        let region=CreateRectRgn(clip[0]-r[0],clip[1]-r[1],clip[2]-r[0],clip[3]-r[1]);
        for c in covers{let cut=CreateRectRgn(c[0]-r[0],c[1]-r[1],c[2]-r[0],c[3]-r[1]);CombineRgn(region,region,cut,RGN_DIFF);DeleteObject(cut);}
        if SetWindowRgn(self.hwnd,region,1)==0{DeleteObject(region);}ShowWindow(self.hwnd,SW_SHOWNOACTIVATE);repaint(self.hwnd);
    }}
}
impl Drop for Editor{fn drop(&mut self){unsafe{crate::plugin_window::embedded_exposed();SetWindowLongPtrW(self.hwnd,GWLP_USERDATA,0);DestroyWindow(self.hwnd);}}}
impl crate::plugins::Plugins {
    /// User-triggered hidden original-editor fit check before altering a channel.
    pub fn validate_vertical_rack(&mut self,key:usize,owner:isize,scale:f32,area:[i32;4])->Result<(),String>{
        if owner==0{return Ok(());} // Offscreen model tests have no native owner.
        let h=self.hosted.get_mut(&key).ok_or("Plugin unavailable")?;
        if h.window.is_some(){return Err("Close the plugin window before docking this strip.".into());}
        let mut e=Editor::new(owner,scale)?;e.rect=area;
        let mut p=h.plugin.lock().map_err(|_|"Plugin busy")?;
        let d=p.desc().clone();
        prepare(&d,scale,area,|scale|{let result=p.open_editor(e.area(),scale);p.close_editor();result})?;
        Ok(())
    }
    pub fn close_rack(&mut self,key:usize){if let Some(h)=self.hosted.get_mut(&key){if h.rack_editor.is_some(){if let Ok(mut p)=h.plugin.lock(){p.close_editor();}h.rack_editor=None;}h.rack_error=None;}}
    pub fn close_racks(&mut self){let keys:Vec<_>=self.hosted.keys().copied().collect();for key in keys{self.close_rack(key);}}
    pub fn show_rack(&mut self,key:usize,owner:isize,scale:f32,r:[i32;4],clip:[i32;4],covers:&[[i32;4]]){
        let page=self.hosted.get(&key).and_then(|h|h.rack_editor.as_ref()).map_or(0,|e|e.page);
        if self.hosted.get_mut(&key).and_then(|h|h.rack_editor.as_mut()).is_some_and(|e|e.owner!=owner||e.scale!=scale||e.rescale_due(r[3]-r[1])){self.close_rack(key);}
        let Some(h)=self.hosted.get_mut(&key)else{return};
        // A plugin owns one editor. Leave an explicitly opened floating editor alone.
        if h.window.is_some()||h.rack_error.is_some(){return;}
        if h.rack_editor.is_none(){
            let result=(||{
                let mut e=Editor::new(owner,scale)?;e.rect=r;
                unsafe{SetPropW(e.hwnd,wide("LemonStripTrack").as_ptr(),(crate::plugins::fx_of(key).unwrap().0 as usize+1) as _);}
                let mut p=h.plugin.lock().map_err(|_|"Plugin busy".to_string())?;
                let d=p.desc().clone();e.profile=profile(&d);e.open_height=r[3]-r[1];
                let fit=prepare(&d,scale,r,|scale|{let result=p.open_editor(e.area(),scale);p.close_editor();result})?;
                let size=match p.open_editor(e.area(),fit){Ok(size)=>size,Err(error)=>{p.close_editor();return Err(error);}};
                if !e.accept_size(size){p.close_editor();return Err("This plugin reported an invalid editor size.".into());}
                e.page=page.min(e.pages.len()-1);e.data.x=e.pages[e.page].left;e.data.page_width=e.pages[e.page].width();
                e.rect=[0;4];Ok::<_,String>(e)
            })();
            match result {Ok(e)=>h.rack_editor=Some(e),Err(e)=>{h.rack_error=Some(e);return;}}
        }
        if let Some(e)=&mut h.rack_editor{e.place(r,clip,covers);}
    }
}

/// Probe the original editor invisibly on direct load. Non-scaling portrait editors
/// are accepted at their actual size; layout adapts instead of cropping them.
fn prepare(d:&os_host::Descriptor,scale:f32,r:[i32;4],mut probe:impl FnMut(f32)->Result<(u32,u32),String>)->Result<f32,String>{
    if !crate::rack_mixer::strip_kind(d){return Err("Choose a supported Waves strip or an experimental resizable channel strip.".into());}
    let spec=if let Some(spec)=crate::rack_mixer::vertical_spec(d){spec}else{
        let size=probe(1.)?;crate::rack_mixer::observe_editor(d,size,1.);
        if !crate::rack_mixer::vertical_shape(size){return Err("This plugin reported an invalid editor size.".into());}size
    };
    let experimental=!d.vendor.eq_ignore_ascii_case("Waves");
    if experimental {
        let a=probe(1.)?;let b=probe(0.75)?;
        if a==b{return Err("This unsupported strip does not respond to host resizing. UAD and other non-Waves strips must respond to host resizing.".into());}
    }
    let fit=((r[3]-r[1]).max(1) as f32/spec.1 as f32).clamp(0.25,scale.max(1.)*3.);
    let size=match probe(fit){Ok(size)=>size,Err(_) => {let size=probe(scale)?;if !crate::rack_mixer::vertical_shape(size){return Err("This plugin reported an invalid editor size.".into());}return Ok(scale);}};
    if !crate::rack_mixer::vertical_shape(size){return Err("This plugin reported an invalid editor size.".into());}
    Ok(fit)
}
#[cfg(test)]mod tests{
    fn desc()->os_host::Descriptor{let mut d=crate::plugins::builtin_effects()[0].desc.clone();d.name="Magma Channel Strip Mono".into();d.vendor="Waves".into();d.kind=os_host::Kind::Effect;d.id="direct-strip-fit-test".into();d}
    #[test]fn vertical_direct_loading_measures_and_accepts_unscaled_original(){
        let d=desc();let mut scales=vec![];
        assert!(prepare_test(&d,&mut scales).is_ok());assert_eq!(scales.len(),2);assert_eq!(scales[0],1.);assert!(scales[1]<1.);
    }
    fn prepare_test(d:&os_host::Descriptor,scales:&mut Vec<f32>)->Result<f32,String>{super::prepare(d,1.,[0,0,128,400],|scale|{scales.push(scale);Ok((300,900))})}
    #[test]fn vertical_direct_loading_rejects_invalid_size_before_assignment(){let d=desc();assert!(super::prepare(&d,1.,[0,0,128,400],|_|Ok((0,500))).is_err());}
    #[test]fn vertical_direct_loading_falls_back_when_scale_is_refused(){let d=desc();assert_eq!(super::prepare(&d,1.,[0,0,128,400],|scale|if scale<1.{Err("Unsupported scale".into())}else{Ok((300,900))}),Ok(1.));}
}
