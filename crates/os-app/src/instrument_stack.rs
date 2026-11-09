//! Additional independent instruments driven by an existing channel's MIDI.
use crate::{app::{App,State},playlist::{Cmd,PItem},plugin_menu::PlugCmd,history::{self,Snap,RackSide}};
use os_model::ChannelKind;
fn eligible(s:&State,row:usize)->bool {
    s.project.channels.get(row).is_some_and(|c|matches!(c.kind,ChannelKind::Plugin|ChannelKind::Sampler))
        && !s.project.channels.iter().enumerate().any(|(i,_)|i!=row && os_model::instrument_stack(&s.project,i).get(1..).unwrap_or(&[]).contains(&row))
}
pub fn menu(s:&State,row:usize)->Vec<PItem>{
    if !eligible(s,row){return vec![];}
    let mut children=Vec::new();
    for child in os_model::instrument_stack(&s.project,row).into_iter().skip(1){
        let name=s.project.channels[child].name.clone().unwrap_or_else(||format!("Instrument {}",child+1));
        let action=|name,a|PItem::new(name,Cmd::Plugin(PlugCmd::StackEdit{row,child,action:a}));
        children.push(PItem::sub(name,vec![action("Open instrument",0),action("Move earlier",1),action("Move later",2),action("Detach to separate channel",3),PItem::new("Remove instrument...",Cmd::Rack(crate::rack::RCmd::Delete(child)))]));
    }
    if children.is_empty() { Vec::new() } else { vec![PItem::sub("Channel instruments", children)] }
}
pub fn add(app:&mut App,row:usize,index:usize)->Option<String>{
    if !eligible(&app.state,row){return Some("Choose a main instrument channel; nested instrument groups are not supported".into());}
    let desc=app.plugins.available.get(index)?.desc.clone();
    if app.state.project.channels.len()>=u16::MAX as usize{return Some("Channel limit reached".into());}
    let child=app.state.project.channels.len();
    if let Err(e)=app.plugins.create(child,index){return Some(format!("Could not add instrument: {e}"));}
    let Some(blob)=app.plugins.state(child) else{app.plugins.drop_key(child);return Some("This instrument could not save its settings; it was not added".into());};
    let s=&mut app.state;
    let before=Snap::Several(vec![Snap::StackLinks(row,s.project.channels[row].layer_children.clone()),Snap::StackInstrument(Box::new(RackSide::now(s,child)),index,blob)]);
    history::record(s,"Add instrument to channel",before);
    let track=s.project.channels[row].mixer_track;
    let actual=crate::app::add_plugin_channel(&mut s.project,&desc.name);debug_assert_eq!(actual,child);
    s.project.channels[child].mixer_track=track;
    let id=s.project.channels[child].index;
    s.project.channels[row].layer_children.push(id);
    s.hosted_rows.push(child);s.piano.dirty=true;s.selected_channel=row;s.rack.select_only(row);
    app.transport.set_instruments(app.plugins.instruments());
    app.params_tick();
    crate::rack::autosize(&mut app.state);
    Some(format!("Added {} — both instruments play this channel's notes",desc.name))
}
pub fn edit(app:&mut App,row:usize,child:usize,action:u8)->Option<String>{
    let s=&mut app.state;
    if !os_model::instrument_stack(&s.project,row).into_iter().skip(1).any(|r|r==child){return None;}
    if action==0 {app.open_channel_window(child);return None;}
    let id=s.project.channels[child].index;
    let at=s.project.channels[row].layer_children.iter().position(|&c|c==id)?;
    let len=s.project.channels[row].layer_children.len();
    if (action==1&&at==0)||(action==2&&at+1==len)||action>3{return None;}
    history::record(s,"Edit channel instruments",Snap::StackLinks(row,s.project.channels[row].layer_children.clone()));
    let children=&mut s.project.channels[row].layer_children;
    match action{1=>children.swap(at,at-1),2=>children.swap(at,at+1),3=>{children.remove(at);},_=>{}}
    s.piano.dirty=true;
    (action==3).then(||"Instrument detached; its settings remain in its own channel".into())
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn batch2_stack_preserves_original_state_and_restores_added_instrument(){
        let mut a=App::new(None);a.state.project=crate::app::test_project();a.plugins.available=crate::plugins::builtin();
        let index=a.plugins.index_of(crate::plugins::LEMON_SAMPLER).unwrap();
        let row=crate::app::add_plugin_channel(&mut a.state.project,crate::plugins::LEMON_SAMPLER);
        a.plugins.create(row,index).unwrap();a.state.hosted_rows.push(row);a.plugins.load_sample(row,"C:/original.wav");
        let original=a.plugins.state(row).unwrap();let handle=a.plugins.hosted[&row].plugin.clone();let pattern=a.state.project.patterns.clone();
        let child=a.state.project.channels.len();assert!(add(&mut a,row,index).unwrap().starts_with("Added"));
        assert!(std::sync::Arc::ptr_eq(&handle,&a.plugins.hosted[&row].plugin));assert_eq!(a.plugins.state(row).unwrap(),original);assert_eq!(a.state.project.patterns,pattern);
        a.plugins.load_sample(child,"C:/added-edited.wav");let saved=a.plugins.state(child).unwrap();a.params_tick();
        history::undo(&mut a.state);a.plugin_tick();assert_eq!(a.state.project.channels.len(),child);assert!(a.state.project.channels[row].layer_children.is_empty());
        history::redo(&mut a.state);a.plugin_tick();assert_eq!(a.plugins.state(child).unwrap(),saved);assert_eq!(a.plugins.state(row).unwrap(),original);
        crate::rack::solo(&mut a.state,row);assert!(a.state.project.channels[row].enabled&&a.state.project.channels[child].enabled);assert_eq!(a.state.rack.soloed,Some(row));crate::rack::solo(&mut a.state,row);assert!(a.state.rack.soloed.is_none());
        let mut p=a.state.project.clone();crate::project_plugins::WRAP_BUILTINS.with(|w|w.set(true));crate::project_plugins::store_hosted(&mut p,&a.plugins).unwrap();crate::project_plugins::WRAP_BUILTINS.with(|w|w.set(false));
        let file=os_flp::project::create(&p).unwrap();let loaded=os_flp::project::load(&file).unwrap();assert_eq!(os_model::instrument_stack(&loaded,row),vec![row,child]);
        let mut b=App::new(None);b.plugins.available=crate::plugins::builtin();b.state.project=loaded;b.sync_wrapped();assert_eq!(b.plugins.sampler_path(row).as_deref(),Some("C:/original.wav"));assert_eq!(b.plugins.sampler_path(child).as_deref(),Some("C:/added-edited.wav"));
        edit(&mut a,row,child,3);assert!(a.state.project.channels[row].layer_children.is_empty());assert_eq!(a.plugins.state(child).unwrap(),saved);history::undo(&mut a.state);assert_eq!(os_model::instrument_stack(&a.state.project,row),vec![row,child]);
        let before=a.state.project.clone();assert!(add(&mut a,child,index).unwrap().contains("nested"));assert_eq!(a.state.project,before);
        crate::rack::clone_rows(&mut a,&[row]);let clone=a.state.selected_channel;let children=os_model::instrument_stack(&a.state.project,clone);assert_eq!(children.len(),2);let copy=children[1];assert_ne!(copy,child);assert_eq!(a.plugins.state(copy).unwrap(),saved);assert!(!std::sync::Arc::ptr_eq(&handle,&a.plugins.hosted[&clone].plugin));
        let count=a.state.project.channels.len();crate::rack::delete_rows(&mut a,&[copy]);assert_eq!(a.state.project.channels.len(),count-1);assert_eq!(os_model::instrument_stack(&a.state.project,clone),vec![clone]);history::undo(&mut a.state);a.plugin_tick();assert_eq!(os_model::instrument_stack(&a.state.project,clone),children);assert_eq!(a.plugins.state(copy).unwrap(),saved);
    }
}
