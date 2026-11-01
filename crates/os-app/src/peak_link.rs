//! Direct, project-local audio-envelope linking, without a controller plugin slot.
use crate::{app::State,playlist::{Cmd,PItem},history::{self,Snap}};
#[derive(Debug,Clone,PartialEq)]
pub enum Command {Open(u32,f64),Set(u32,u16,f32,bool),Remove(u32),Depth(u32,f32),Release(u32,u16)}
pub fn menu(_s:&State,target:u32,value:f64)->PItem {
 PItem::new("Follow audio level...",Cmd::PeakLink(Command::Open(target,value)))
}
pub fn popup(s:&State,target:u32,value:f64,x:f32,y:f32)->crate::playlist::Popup {
 let sources=|inverted|s.project.mixer.iter().filter(|m|m.index!=0).map(|m|{
  let label=format!("{}: {}",m.index,m.name.as_deref().unwrap_or("Insert"));
  PItem::new(label,Cmd::PeakLink(Command::Set(target,m.index,value as f32,inverted)))
 }).collect();
 let mut items=vec![PItem::sub("Follow source",sources(false)),PItem::sub("Duck from source",sources(true))];
 if let Some(l)=s.project.peak_links.iter().find(|l|l.target==target){
  items.insert(0,PItem::head(format!("Source {} / {}",l.source,if l.inverted{"Duck"}else{"Follow"})));
  items.push(PItem::sub("Depth",[0.25,0.5,0.75,1.].into_iter().map(|d|PItem::new(format!("{:.0}%",d*100.),Cmd::PeakLink(Command::Depth(target,d))).mark(crate::playlist::Mark::Radio((l.depth-d).abs()<0.001))).collect()));
  items.push(PItem::sub("Release",[50,150,300,600].into_iter().map(|ms|PItem::new(format!("{ms} ms"),Cmd::PeakLink(Command::Release(target,ms))).mark(crate::playlist::Mark::Radio(l.release_ms==ms))).collect()));
  items.push(PItem::new("Remove audio-level link",Cmd::PeakLink(Command::Remove(target))));
 }
 crate::playlist::Popup{x,y,items,sub:None,scroll:[0;2], cascade: Vec::new()}
}
pub fn run(s:&mut State,cmd:Command){
 history::record(s,"Audio-level link",Snap::PeakLinks(s.project.peak_links.clone()));
 match cmd {
  Command::Open(..)=>return,
  Command::Set(target,source,base,inverted)=>{if !base.is_finite()||!s.project.mixer.iter().any(|m|m.index==source){return;}
   let base=s.project.peak_links.iter().find(|l|l.target==target).map_or(base,|l|l.base);
   s.project.peak_links.retain(|l|l.target!=target);
   s.project.peak_links.push(os_model::PeakLink{source,target,base:base.clamp(0.,1.),depth:1.,inverted,release_ms:150});
  },
  Command::Remove(target)=>{
   if let Some(l)=s.project.peak_links.iter().find(|l|l.target==target){if let Some((key,index))=crate::params::key_of(s,os_model::target::decode(target)){crate::params::set_param_value(s,key,index,l.base as f64);}}
   s.project.peak_links.retain(|l|l.target!=target);
  },
  Command::Depth(target,d)=>{if let Some(l)=s.project.peak_links.iter_mut().find(|l|l.target==target){l.depth=d.clamp(0.,1.);}},
  Command::Release(target,ms)=>{if let Some(l)=s.project.peak_links.iter_mut().find(|l|l.target==target){l.release_ms=ms.clamp(10,2000);}},
 }
 s.piano.dirty=true;
}
pub fn mixer_target(t:u32)->Option<u16>{
 use os_model::target::Target::*;
 match os_model::target::decode(t){MixerVolume(n)|MixerPan(n)|MixerSeparation(n)|MixerEq(n,..)|MixerStrip(n,..)|MixerSend(n,..)|SlotOn(n,..)|SlotMix(n,..)|SlotParam(n,..)=>Some(n),_=>None}
}
pub fn remap_target(t:u32,m:&dyn Fn(u16)->u16)->u32{
 use os_model::target::{self as t,Target::*};match t::decode(t){
 MixerVolume(n)=>t::mixer(m(n),t::MIXER_VOLUME),MixerPan(n)=>t::mixer(m(n),t::MIXER_PAN),MixerSeparation(n)=>t::mixer(m(n),t::MIXER_SEPARATION),
 MixerEq(n,b,k)=>t::mixer(m(n),[t::EQ_GAIN,t::EQ_FREQ,t::EQ_WIDTH][k as usize]+b),MixerStrip(n,k)=>t::strip(m(n),k),MixerSend(n,d)=>t::send(m(n),m(d)),SlotOn(n,k)=>t::slot(m(n),k,t::SLOT_ON),SlotMix(n,k)=>t::slot(m(n),k,t::SLOT_MIX),SlotParam(n,k,p)=>t::slot_param(m(n),k,p),_=>t}
}

pub fn restore_removed(s:&State,next:&[os_model::PeakLink]){
 for l in &s.project.peak_links {if !next.iter().any(|n|n.target==l.target){if let Some((key,index))=crate::params::key_of(s,os_model::target::decode(l.target)){crate::params::set_param_value(s,key,index,l.base as f64);}}}
}
pub fn remove_slot(s:&mut State,track:u16,slot:u8){s.project.peak_links.retain(|l|!matches!(os_model::target::decode(l.target),os_model::target::Target::SlotParam(t,k,_) if t==track&&k==slot));}

#[cfg(test)]mod tests{
 use super::*;
 #[test]fn peak_eq_link_menu_undo_and_reorder(){
  let mut a=crate::app::App::new(None);a.state.project=crate::app::test_project();let t=os_model::target::slot_param(2,0,3);
  crate::playlist::run(&mut a,Cmd::PeakLink(Command::Open(t,0.8)));
  let popup=a.state.playlist.popup.as_ref().unwrap();let source=popup.items.iter().find(|i|i.label=="Duck from source").unwrap();
  assert!(!source.children.is_empty());assert!(source.children.iter().all(|i|i.children.is_empty()),"only one submenu level");
  run(&mut a.state,Command::Set(t,1,0.8,true));assert_eq!(a.state.project.peak_links.len(),1);
  a.undo_key(false);assert!(a.state.project.peak_links.is_empty());a.undo_key(true);assert_eq!(a.state.project.peak_links.len(),1);
  run(&mut a.state,Command::Depth(t,0.5));assert_eq!(a.state.project.peak_links[0].depth,0.5);
  crate::mixer::run(&mut a,crate::mixer::MCmd::MoveRight(1));
  let l=a.state.project.peak_links[0];assert_eq!(l.source,2);assert_eq!(l.target,os_model::target::slot_param(1,0,3));
  crate::mixer::run(&mut a,crate::mixer::MCmd::Delete(2));assert!(a.state.project.peak_links.is_empty());
 }
}
