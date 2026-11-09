// SPDX-License-Identifier: GPL-3.0-or-later
//! Reference-proportioned native instrument surface. Extra controls expand in-place.
use super::*;
use std::sync::atomic::Ordering;
struct SurfaceCache {
 key:(usize,usize,u32,u32,u32,u32,u32),
 _owner:std::sync::Arc<crate::assets::Tables>,
 frames:Vec<Vec<f32>>,amplitude:f32,pixels:Vec<u32>,width:usize,height:usize,
}
thread_local!{static SURFACES:std::cell::RefCell<[Option<SurfaceCache>;2]>=const {std::cell::RefCell::new([None,None])};}
fn more(u:&mut Ui,p:&WaveParams,i:usize){let r=detail_button(i);u.c.text_center(font(16),r,if detail_open(p,i){theme_a()}else{theme_dim()},if detail_open(p,i){"×"}else{"···"});}
fn title(u:&mut Ui,x:f32,y:f32,text:&str){u.c.text(font(22).bold(),x,y,theme_ink(),text);}
fn dial(u:&mut Ui,k:usize,x:f32,y:f32,name:&str,color:Color){u.dial(k,x,y,name,color,28.);}
fn button(u:&mut Ui,r:Rect,text:&str,color:Color){raised(u.c,r,6.,Color::rgb(0x080e13));u.c.text_center(font(12),r,color,text);}
fn source_name(m:&dyn Model,t:&crate::assets::Tables,o:usize)->String {
 let base=o*8;let source=if o==0{A_SOURCE}else{B_SOURCE};
 if m.plain(SETTINGS[source].id)>0.5{return "Reference / Saw".into()}
 if let Some(s)=&t.samples[o]{return s.source.name.rsplit(" / ").next().unwrap_or(&s.source.name).into()}
 if let Some(t)=&t.oscillators[o]{return t.source.name.clone()}
 ["Basic Morph","Hollow","Glass","Vowel"][(m.plain(SETTINGS[base].id) as usize).min(3)].into()
}
fn surface(u:&mut Ui,t:&std::sync::Arc<crate::assets::Tables>,o:usize,r:Rect,color:Color){
 let base=o*8;let source=if o==0{A_SOURCE}else{B_SOURCE};let reference=u.m.plain(SETTINGS[source].id)>0.5;
 let position=u.m.plain(SETTINGS[base+1].id)/1000.;let bank=u.m.plain(SETTINGS[base].id) as usize;
 u.c.push_clip(r);
 if reference || t.samples[o].is_some(){
  let mode=u.m.plain(SETTINGS[source+1].id) as usize;let warp=Warp::from_reference_id(WARP_IDS[mode.min(10)]).unwrap();
  let amount=(u.m.plain(SETTINGS[source+2].id)/1000.+u.m.plain(SETTINGS[base+7].id)/200.).clamp(0.,1.);
  let pts:Vec<_>=(0..320).map(|i|{let phase=i as f32/319.;let v=if reference{let(q,g)=warp.apply(phase.min(0.99999) as f64,amount);(((q+0.5).fract()*2.-1.) as f32)*g}else{let s=&t.samples[o].as_ref().unwrap().source.samples;s[(phase*(s.len()-1) as f32) as usize]};(r.x+phase*r.w,r.cy()-v*65.)}).collect();luminous(u.c,&pts,1.6,color);
 }else{
  let key=(std::sync::Arc::as_ptr(t) as usize,u.m.plain(SETTINGS[base].id) as usize,u.c.scale.to_bits(),u.c.pixmap.width(),u.c.pixmap.height(),r.w.to_bits(),((color.r as u32)<<16)|((color.g as u32)<<8)|color.b as u32);
  SURFACES.with(|store|{let mut store=store.borrow_mut();
  if store[o].as_ref().is_none_or(|cache|cache.key!=key||cache.pixels.is_empty()){
  let table=t.oscillators[o].as_ref();let cols=128;let rows=28;
  let project=|phase:f32,depth:f32,v:f32|(r.x+6.+phase*(r.w-38.)+depth*26.,r.cy()+24.-depth*55.-v.clamp(-1.,1.)*66.);
  let sample=|depth:f32,phase:f32|table.map(|t|t.sample(depth,phase,220.,48000.)).unwrap_or_else(||crate::dsp::preview(bank,depth,phase));
  // Periodic low-pass overview removes high-frequency wire aliasing. Detailed
  // unfiltered samples remain in Table Edit. No change to the audio table.
  let frames:Vec<Vec<f32>>=(0..rows).map(|j|{let d=j as f32/(rows-1) as f32;let raw:Vec<_>=(0..128).map(|i|sample(d,i as f32/128.)).collect();(0..128).map(|i|{let mut sum=0.;let mut norm=0.;for tap in -12i32..=12{let weight=(-0.5*(tap as f32/5.5).powi(2)).exp();sum+=raw[((i as i32+tap).rem_euclid(128)) as usize]*weight;norm+=weight;}sum/norm}).collect()}).collect();
  let amplitude=frames.iter().flatten().copied().map(f32::abs).fold(0.2,f32::max);
  let smooth=|row:usize,phase:f32|{let at=phase.fract()*128.;let i=at.floor() as usize;let f=at-i as f32;(frames[row][i%128]*(1.-f)+frames[row][(i+1)%128]*f)/amplitude};
  let mesh:Vec<Vec<_>>=(0..rows).map(|j|{let d=j as f32/(rows-1) as f32;(0..cols).map(|i|{let phase=i as f32/(cols-1) as f32;project(phase,d,smooth(j,phase*2.))}).collect()}).collect();
  // Translucent light-scattering sheets with fine cross-wires, rendered back to front.
  for j in (0..rows-1).rev(){for i in 0..cols-1{let q=[mesh[j][i],mesh[j][i+1],mesh[j+1][i+1],mesh[j+1][i]];let slope=(q[1].1-q[0].1)/(q[1].0-q[0].0).max(0.01);let light=(0.52+0.48*(slope*0.35+0.45).cos()).clamp(0.15,1.);let shine=light.powi(18)*0.28;
   let channel=|v:u8|((v as f32*light+255.*shine).min(255.)) as u32;let lit=Color::rgb((channel(color.r)<<16)|(channel(color.g)<<8)|channel(color.b));u.c.fill_polygon(&q,lit.with_alpha(175));}}
  for row in &mesh {u.c.polyline(row,7.,color.with_alpha(15));u.c.polyline(row,0.65,color.with_alpha(130));}
  for i in (0..cols).step_by(4){let line:Vec<_>=mesh.iter().map(|row|row[i]).collect();u.c.polyline(&line,0.55,color.with_alpha(65));}

   let scale=u.c.scale;let sx=(r.x*scale).round() as usize;let sy=(r.y*scale).round() as usize;let width=(r.w*scale).round() as usize;let height=(r.h*scale).round() as usize;let cw=u.c.pixmap.width() as usize;let ch=u.c.pixmap.height() as usize;
   let mut pixels=Vec::new();if sx+width<=cw&&sy+height<=ch {let raw=u.c.pixmap.pixels();pixels.reserve(width*height);for y in sy..sy+height{for x in sx..sx+width{let p=raw[y*cw+x];pixels.push(((p.red() as u32)<<16)|((p.green() as u32)<<8)|p.blue() as u32);}}}
   store[o]=Some(SurfaceCache{key,_owner:t.clone(),frames,amplitude,pixels,width,height});
  }else{let cached=store[o].as_ref().unwrap();if !cached.pixels.is_empty(){u.c.image(r,cached.width,cached.height,&cached.pixels);}}
  let cached=store[o].as_ref().unwrap();
  let smooth=|row:usize,phase:f32|{let at=phase.fract()*128.;let i=at.floor() as usize;let f=at-i as f32;(cached.frames[row][i%128]*(1.-f)+cached.frames[row][(i+1)%128]*f)/cached.amplitude};
  let project=|phase:f32,depth:f32,v:f32|(r.x+6.+phase*(r.w-38.)+depth*26.,r.cy()+24.-depth*55.-v.clamp(-1.,1.)*66.);
  let pivot=0.5+u.m.plain(SETTINGS[base+7].id)/200.;
  let selected:Vec<_>=(0..320).map(|i|{let p=i as f32/319.;let phase=(p*2.).fract();let q=if phase<pivot{phase/pivot*0.5}else{0.5+(phase-pivot)/(1.-pivot)*0.5};{let frame=position*(cached.frames.len()-1) as f32;let lo=frame.floor() as usize;let hi=(lo+1).min(cached.frames.len()-1);let mix=frame-lo as f32;project(p,position,smooth(lo,q)*(1.-mix)+smooth(hi,q)*mix)}}).collect();luminous(u.c,&selected,1.5,color.shade(0.16));
  });
 }
 u.c.pop_clip();
}
pub(super) fn paint_synth(u:&mut Ui,p:&WaveParams,cycles:[usize;2],hint:&str){
 let tables=p.tables.read().unwrap();
 for o in 0..2 {
  let x=osc_x(o);let w=osc_w(o);let color=if o==0{theme_a()}else{theme_b()};let base=o*8;let phase=if o==0{A_PHASE}else{B_PHASE};let source=if o==0{A_SOURCE}else{B_SOURCE};
  raised(u.c,Rect::new(x,81.,w,865.),12.,theme_panel());title(u,x+14.,94.,if o==0{"OSCILLATOR A"}else{"OSCILLATOR B"});
  u.toggle(if o==0{A_ON}else{B_ON},osc_power(o),"ON","OFF",color);
  u.c.text_center(font(10),edit_table_button(o),theme_dim(),"EDIT");more(u,p,o);
  let br=crate::sound_browser::button(o);dropdown(u.c,br,&source_name(u.m,&tables,o),theme_ink(),u.hover(br));
  if detail_open(p,o) {
   u.select(source,Rect::new(x+24.,196.,180.,30.),color);u.select(source+1,Rect::new(x+218.,196.,w-242.,30.),color);
   if crate::visibility::shown(1) {
    let controls = [(base+7,"Bend"),(phase+1,"Random"),(A_PHASE_FM+o*2,"Phase FM"),(A_FM_RATIO+o*2,"Ratio"),(B_RING,"Ring A×B")];
    let count = if o == 0 { 4 } else { 5 };
    for (i, &(k, name)) in controls[..count].iter().enumerate() {
     u.dial(k, x+51.+(w-102.)*i as f32/(count-1) as f32, 284., name, color, 22.);
    }
   } else {
    for (i,(k,name)) in [(base+7,"Bend"),(phase+1,"Random")].into_iter().enumerate(){dial(u,k,x+110.+190.*i as f32,285.,name,color);}
   }
  }else{surface(u,&tables,o,Rect::new(x+12.,180.,w-24.,166.),color);}
  for (i,(k,name)) in [(base+1,"Position"),(base+3,"Tune"),(base+4,"Unison"),(base+5,"Detune"),(phase+2,"Blend")].into_iter().enumerate(){dial(u,k,x+56.+(w-108.)*i as f32/4.,415.,name,color);}
  for (i,(k,name)) in [(source+2,"Warp"),(phase,"Phase"),(base+6,"Pan"),(base+2,"Level")].into_iter().enumerate(){dial(u,k,x+56.+(w-128.)*i as f32/3.,550.,name,color);}
  if tables.samples[o].is_some() && u.m.plain(SETTINGS[source].id)<0.5 {
   for (i,name) in ["Root -12","Root -1","Root +1","Root +12"].iter().enumerate(){button(u,crate::sound_browser::root_button(o,i),name,theme_dim());}
  }else{for (r,name) in table_buttons(o).into_iter().zip(["Import table".to_string(),format!("Cycle {}",crate::wavetable::CYCLES[cycles[o]]),"Reset table".into()]){u.c.text_center(font(10),r,theme_dim(),&name);}}
  u.c.fill_rect(Rect::new(x+14.,632.,w-28.,1.),Color::rgb(0x30434e).with_alpha(180));
 }
 drop(tables);
 envelopes(u,p);sources(u,p);filter(u,p);rack(u,p);
 let quick_patch=crate::fxrack::effective(std::array::from_fn(|k|u.m.plain(SETTINGS[k].id)));
 for(k,r,value,label)in quick_choices_for(p,&quick_patch){let selected=u.m.plain(SETTINGS[k].id).round() as i32==value;
  raised(u.c,r.inset(1.),6.,if selected{theme_a()}else{Color::rgb(0x080e13)});if selected{soft_glow(u.c,r,theme_a(),0.05);}u.c.text_center(font(12),r,if selected{Color::rgb(0x101506)}else{theme_ink()},label);u.hit(r,Target::Choice(SETTINGS[k].id.into(),value));
 }
 if !hint.is_empty(){
  let mut line=String::new();let mut y=6.;
  for word in hint.split_whitespace(){
   let next=if line.is_empty(){word.to_string()}else{format!("{line} {word}")};
   if !line.is_empty()&&u.c.text_width(font(10),&next)>226.{u.c.text_clip(font(10),1342.,y,226.,theme_dim(),&line);y+=14.;line=word.into();}else{line=next;}
   if y>44.{break;}
  }
  if y<=44.{u.c.text_clip(font(10),1342.,y,226.,theme_dim(),&line);}
 }
 for (_,r,label) in source_buttons(0,p){paint_source_handle(u.c,r,label,u.hover(r));}

}
fn envelopes(u:&mut Ui,p:&WaveParams){
 let env=p.env_tab.load(Ordering::Relaxed).min(1);let lfo=p.lfo_tab.load(Ordering::Relaxed).min(3);let base=if env==0{ATTACK}else{ENV_ATTACK};let hold=if env==0{AMP_HOLD}else{ENV_HOLD};let ec=if env==0{theme_a()}else{theme_b()};
 title(u,24.,647.,"ENVELOPE");title(u,486.,647.,"LFO");
 for i in 0..2{let r=envelope_tab(i);button(u,r,&(i+1).to_string(),if i==env{ec}else{theme_dim()});}
 for i in 0..4{let r=lfo_tab(i);button(u,r,&(i+1).to_string(),if i==lfo{theme_mod()}else{theme_dim()});}
 more(u,p,5);more(u,p,6);
 if detail_open(p,5){
  u.select(ENV_ENGINE,Rect::new(176.,648.,136.,27.),ec);
  for (i,name) in ["Attack curve","Decay curve","Release curve"].iter().enumerate(){u.dial(hold+1+i,if env==0{90.+145.*i as f32}else{70.+107.*i as f32},759.,name,ec,22.);}
  if env==1{u.dial(ENV_AMOUNT,391.,759.,"To filter",ec,22.);}
 }else{u.env(base,synth_env_graph(),ec);for (_,_,x,y) in envelope_nodes(env,|k|u.m.plain(SETTINGS[k].id)){u.c.fill_circle(x,y,3.6,ec);}}
 for (i,(k,name)) in [(base,"Attack"),(hold,"Hold"),(base+1,"Decay"),(base+2,"Sustain"),(base+3,"Release")].into_iter().enumerate(){dial(u,k,64.+86.*i as f32,868.,name,ec);}
 let keys=LFO_CONTROLS[lfo];let r=synth_lfo_graph();
 if detail_open(p,6){
  for (i,k) in [keys[2],keys[3],LFO_DRAW,LFO_TARGET].into_iter().enumerate(){if lfo>0&&i>1{continue;}u.select(k,Rect::new(492.+(i%2) as f32*198.,700.+(i/2) as f32*41.,188.,30.),theme_mod());}
 }else{
  let curves=*p.curves.read().unwrap();let pts:Vec<_>=(0..256).map(|i|{let t=i as f32/255.;let v=if lfo==0&&u.m.plain("lfo_draw")<0.5{match u.m.plain("lfo_shape") as usize{1=>1.-4.*(t-0.5).abs(),2=>t*2.-1.,3=>if t<0.5{1.}else{-1.},_=>(t*std::f32::consts::TAU).sin()}}else{curves.sample(lfo,t)};(r.x+t*r.w,r.cy()-v*(r.h*0.46))}).collect();glowing_area(u.c,&pts,r.bottom(),theme_mod());
 }
 if lfo==0{u.select(LFO_SHAPE,Rect::new(686.,648.,136.,32.),theme_ink());}
 dial(u,keys[0],528.,868.,"Rate",theme_mod());u.select(keys[1],Rect::new(785.,852.,24.,33.),theme_ink());u.c.text_center(font(14),Rect::new(626.,814.,155.,20.),theme_ink(),"Sync");
 if lfo==0{dial(u,LFO_AMOUNT,850.,868.,"Depth",theme_mod());}else{u.c.text_center(font(12),Rect::new(790.,840.,105.,55.),theme_dim(),"Route on MOD");}
 for (i,name) in ["Sine","Triangle","Ramp","Square","Undo","Copy","Paste"].iter().enumerate(){u.c.text_center(font(9),synth_shape_button(i),theme_dim(),name);}
}
fn sources(u:&mut Ui,p:&WaveParams){
 raised(u.c,Rect::new(914.,81.,244.,349.),12.,theme_panel());title(u,927.,97.,"SUB OSC");u.toggle(SUB_ON,Rect::new(1110.,96.,37.,26.),"ON","OFF",theme_a());more(u,p,2);
 u.select(SUB_SHAPE,Rect::new(932.,143.,208.,34.),theme_ink());
 if detail_open(p,2){u.toggle(SUB_TRACK,Rect::new(930.,210.,104.,28.),"KEY ON","FIXED C4",theme_b());u.toggle(SUB_DIRECT,Rect::new(1040.,210.,104.,28.),"DIRECT ON","DIRECT OFF",theme_b());}
 else{let r=Rect::new(934.,198.,204.,70.);u.grid(r);let shape=u.m.plain(SETTINGS[SUB_SHAPE].id) as usize;let pts:Vec<_>=(0..240).map(|i|{let t=i as f32/239.;(r.x+t*r.w,r.cy()-crate::sources::sub((t*2.).fract(),0.001,shape)*30.)}).collect();luminous(u.c,&pts,1.5,theme_b());}
 for(i,(k,name))in [(SUB_OCT,"Tune"),(SUB_PAN,"Pan"),(SUB,"Level")].into_iter().enumerate(){u.dial(k,962.+78.*i as f32,350.,name,theme_b(),26.);}
 raised(u.c,Rect::new(914.,446.,244.,500.),12.,theme_panel());title(u,927.,464.,"NOISE OSC");u.toggle(NOISE_ON,Rect::new(1110.,464.,37.,26.),"ON","OFF",theme_a());more(u,p,3);u.select(NOISE_TYPE,Rect::new(932.,509.,208.,34.),theme_ink());
 if detail_open(p,3){
  for(i,(k,on,off))in[(NOISE_TRACK,"KEY ON","KEY OFF"),(NOISE_ONESHOT,"ONE SHOT","LOOP"),(NOISE_RANDOM,"RANDOM","RETRIGGER"),(NOISE_DIRECT,"DIRECT ON","DIRECT OFF")].into_iter().enumerate(){u.toggle(k,Rect::new(930.+(i%2) as f32*110.,562.+(i/2) as f32*39.,104.,30.),on,off,theme_b());}
 }else{
  let tables=p.tables.read().unwrap();let kind=(u.m.plain(SETTINGS[NOISE_TYPE].id) as usize).min(8);let sample=if kind==8{tables.noise.as_ref()}else{Some(&crate::sources::library()[kind])};
  if let Some(s)=sample{let r=Rect::new(934.,562.,204.,76.);for i in 0..204{let a=i*s.source.samples.len()/204;let peak=s.source.samples[a].abs();let h=peak.min(1.)*32.;let line=[(r.x+i as f32,r.cy()-h),(r.x+i as f32,r.cy()+h)];u.c.polyline(&line,3.,theme_b().with_alpha(10));u.c.polyline(&line,0.7,theme_b().with_alpha(160));}luminous(u.c,&[(r.x,r.cy()),(r.right(),r.cy())],1.,theme_b());}
 }
 for(i,(k,name))in[(NOISE_PITCH,"Pitch"),(NOISE_PAN,"Pan"),(NOISE,"Level")].into_iter().enumerate(){u.dial(k,962.+78.*i as f32,714.,name,theme_b(),26.);}
 for(r,name)in[(NOISE_IMPORT,"Import WAV"),(NOISE_CLEAR,"Clear WAV")]{u.c.text_center(font(11),r,theme_dim(),name);}
}
fn filter(u:&mut Ui,p:&WaveParams){
 let f=p.filter_tab.load(Ordering::Relaxed).min(1);let k=if f==0{[FILTER,FILTER_MODEL,CUTOFF,RESONANCE,DRIVE,F1_MIX,F1_KEY,F1_PAN,F1_ON]}else{[F2_FILTER,F2_MODEL,F2_CUTOFF,F2_RESONANCE,F2_DRIVE,F2_MIX,F2_KEY,F2_PAN,F2_ON]};
 raised(u.c,Rect::new(1170.,81.,410.,392.),12.,theme_panel());
 for i in 0..2{let r=filter_tab(i);if f==i{soft_glow(u.c,r,theme_a(),0.15);}raised(u.c,r,7.,if f==i{theme_a()}else{Color::rgb(0x0a1117)});u.c.text_center(font(16),r,if f==i{Color::rgb(0x121600)}else{theme_ink()},if i==0{"FILTER 1"}else{"FILTER 2"});}
 u.select(k[0],Rect::new(1198.,143.,328.,33.),theme_ink());more(u,p,4);
 if detail_open(p,4){
  u.select(k[1],Rect::new(1190.,195.,178.,30.),theme_a());u.select(FILTER_TOPOLOGY,Rect::new(1380.,195.,178.,30.),theme_a());u.toggle(k[8],Rect::new(1514.,255.,40.,28.),"ON","OFF",theme_a());
  for(i,label)in["A","B","SUB","N"].iter().enumerate(){u.toggle(FILTER_INPUTS[f][i],filter_input(i),label,label,theme_a());}
 }else{
  let r=Rect::new(1212.,194.,340.,116.);u.grid(r);let fc=cutoff(u.m.plain(SETTINGS[k[2]].id));let mode=u.m.plain(SETTINGS[k[0]].id) as usize;let q=1./(2.-1.9*u.m.norm(SETTINGS[k[3]].id));
  let pts:Vec<_>=(0..320).map(|i|{let t=i as f32/319.;let w=20.*1000_f32.powf(t)/fc;let den=((1.-w*w).powi(2)+(w/q).powi(2)).sqrt().max(1e-6);let mut mag=match mode{1=>1./den,2=>w/den,3=>w*w/den,_=>1.};if u.m.plain(SETTINGS[k[1]].id)>1.5{mag*=mag;}(r.x+t*r.w,r.y+18.+(-20.*mag.max(1e-4).log10()/48.).clamp(-0.1,1.)*92.)}).collect();glowing_area(u.c,&pts,r.bottom(),theme_b());
  for(i,label)in["20 Hz","100 Hz","1 kHz","10 kHz","20 kHz"].iter().enumerate(){u.c.text_center(font(10),Rect::new(1188.+i as f32*78.,318.,70.,16.),theme_dim(),label);}
 }
 let controls=if detail_open(p,4){[(k[6],"Key track"),(k[7],"Pan"),(SPREAD,"Width"),(GAIN,"Output")]}else{[(k[2],"Cutoff"),(k[3],"Resonance"),(k[4],"Drive"),(k[5],"Mix")]};
 for(i,(key,name))in controls.into_iter().enumerate(){u.dial(key,1232.+94.*i as f32,404.,name,theme_a(),26.);}
}
fn rack(u:&mut Ui,p:&WaveParams){
 raised(u.c,Rect::new(1170.,478.,410.,468.),12.,theme_panel());title(u,1188.,488.,"EFFECTS");button(u,FX_ADD,if p.fx_add.load(Ordering::Relaxed){"Close"}else{"+ Add"},theme_a());
 if p.fx_add.load(Ordering::Relaxed){for kind in 1..crate::fxrack::NAMES.len(){button(u,fx_menu_button(kind),crate::fxrack::NAMES[kind],theme_ink());}return;}
 let patch=crate::fxrack::effective(std::array::from_fn(|k|u.m.plain(SETTINGS[k].id)));let slots=crate::fxrack::order(&patch);let scroll=p.fx_scroll.load(Ordering::Relaxed).min(fx_max_scroll(slots.len()));p.fx_scroll.store(scroll,Ordering::Relaxed);
 if slots.is_empty(){u.c.text_center(font(16),Rect::new(1190.,620.,370.,40.),theme_dim(),"Add an effect to begin");}
 let model=FxModel(patch);let mut v=Ui{c:u.c,m:&model,st:u.st,hits:Vec::new(),live:u.live};v.c.push_clip(FX_AREA);
 for(position,slot)in slots.iter().enumerate(){let r=fx_card(position,scroll);if r.bottom()<FX_AREA.y||r.y>FX_AREA.bottom(){continue;}let b=crate::fxrack::base(*slot);let kind=model.0[b] as usize;let extra=detail_open(p,8+*slot);
  raised(v.c,r,9.,Color::rgb(0x090f15));v.c.text_clip(font(15).bold(),r.x+12.,r.y+12.,145.,theme_ink(),crate::fxrack::NAMES[kind]);
  for(action,label)in["ON","A","B","↑","↓","×"].iter().enumerate(){let br=fx_button(r,action);let active=action<3&&model.0[b+2+action]>0.5;if action==0{power(v.c,br,active);}else{raised(v.c,br,4.,if active{theme_a()}else{Color::rgb(0x0a1117)});v.c.text_center(font(11),br,if active{Color::rgb(0x0d1406)}else{theme_dim()},label);}}
  v.c.text_center(font(15),fx_more(r),theme_dim(),if extra{"×"}else{"⌄"});
  if kind!=4||extra{v.select(b+5,Rect::new(r.x+15.,r.y+48.,if kind==4{165.}else{344.},29.),theme_ink());if kind==4{v.select(b+10,Rect::new(r.x+195.,r.y+48.,164.,29.),theme_ink());}}
  let indices:Vec<usize>=if extra{if kind==4{vec![2]}else{vec![3,4]}}else if kind==4{vec![0,1,3,5]}else{vec![0,1,2,5]};let names=crate::fxrack::captions(kind);
  for(i,index)in indices.into_iter().enumerate(){v.dial(b+6+index,r.x+46.+90.*i as f32,r.y+135.,names[index],theme_a(),22.);}
 }
 v.c.pop_clip();for mut h in v.hits{h.r=h.r.intersect(&FX_AREA);if h.r.w>0.&&h.r.h>0.{u.hits.push(h);}}
 if fx_max_scroll(slots.len())>0{let h=(FX_AREA.h*FX_AREA.h/(slots.len() as f32*FX_CARD_HEIGHT)).max(24.);let y=FX_AREA.y+(FX_AREA.h-h)*scroll as f32/fx_max_scroll(slots.len()) as f32;u.c.fill_rrect(Rect::new(1570.,FX_AREA.y,3.,FX_AREA.h),1.5,theme_line());u.c.fill_rrect(Rect::new(1570.,y,3.,h),1.5,theme_a());}
}
