// SPDX-License-Identifier: GPL-3.0-or-later
//! Persistent, audio-device-free renderer using the actual Wave editor.
use std::{io::{self,BufRead,Write},sync::{Arc,atomic::Ordering}};
use lemon_small_fx::editor::{Logic,Extension};
use serde::Deserialize;
#[derive(Deserialize)]
struct Request { design:serde_json::Value, page:usize, output:String, #[serde(default)] fx_scroll:usize, #[serde(default)] fx_add:bool, #[serde(default)] values:std::collections::BTreeMap<String,i32> }
fn main() {
    let params=Arc::new(lemon_wave::assets::WaveParams::default());
    let editor=Arc::new(lemon_wave::table_editor::TableEditor::new(params.clone()));
    let shared=Arc::new(lemon_small_fx::Shared{extension:Some(editor.clone()),..Default::default()});
    let mut logic=Logic::new(lemon_wave::controls::SPEC,params.base.clone(),shared,None);
    let mut canvas=os_ui::Canvas::new(1180,820,1.).expect("software canvas");
    for line in io::stdin().lock().lines() {
        let result=(||->Result<serde_json::Value,String>{
            let request:Request=serde_json::from_str(&line.map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
            let d=lemon_wave::design::Design::parse(&request.design.to_string())?;
            let normalized=serde_json::to_value(&d).unwrap();
            lemon_wave::design::apply(d);
            let mut raw=params.base.fl_state();
            for (id,value) in &request.values {
                let k=lemon_wave::controls::SETTINGS.iter().position(|s|s.id==id).ok_or("Unknown parameter")?;
                let s=&lemon_wave::controls::SETTINGS[k];
                lemon_small_fx::params::set_setting(lemon_wave::controls::SPEC,&mut raw,k,(*value).clamp(s.range.0,s.range.1));
            }
            params.base.set_raw(raw);
            for (id,value) in &request.values {logic.vals.edit_plain(id,*value as f32);}
            params.fx_scroll.store(request.fx_scroll,Ordering::Relaxed);
            params.fx_add.store(request.fx_add,Ordering::Relaxed);
            if request.page==2 {let r=lemon_wave::instrument_view::page_button(2);editor.click(r.cx(),r.cy(),None);}
            else {params.page.store(request.page.min(1),Ordering::Relaxed);}
            logic.paint(&mut canvas,&os_plugkit::palette());
            canvas.pixmap.save_png(&request.output).map_err(|e|e.to_string())?;
            let hits:Vec<_>=logic.hits.iter().map(|h|serde_json::json!({"id":h.t.id(),"rect":[h.r.x,h.r.y,h.r.w,h.r.h],"kind":match h.t {lemon_small_fx::view::Target::Knob(_)=>"knob",_=>"selector"}})).collect();
            let parameters:Vec<_>=lemon_wave::controls::SETTINGS.iter().map(|s|serde_json::json!({"id":s.id,"min":s.range.0,"max":s.range.1,"default":s.start})).collect();
            Ok(serde_json::json!({"ok":true,"hits":hits,"parameters":parameters,"design":normalized}))
        })();
        println!("{}",result.unwrap_or_else(|e|serde_json::json!({"ok":false,"error":e})));
        io::stdout().flush().unwrap();
    }
}
