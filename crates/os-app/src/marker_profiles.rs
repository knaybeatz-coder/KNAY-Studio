//! Portable marker-only profiles; musical content is never replaced.
use crate::{app::{App,State},playlist as pl};
use serde::{Serialize,Deserialize};
use std::path::Path;
#[derive(Serialize,Deserialize)]
struct Profile {version:u8,ppq:u16,markers:Vec<Marker>}
#[derive(Serialize,Deserialize)]
struct Marker {#[serde(default)] color:Option<u32>,position:u32,kind:u8,name:String,numerator:u8,denominator:u8,scale:u16}
fn encode(s:&State)->Result<Vec<u8>,String> {
    let markers=pl::markers(s).iter().map(|m|Marker{color:m.color,position:m.position,kind:m.kind,name:m.name.clone(),numerator:m.numerator,denominator:m.denominator,scale:m.scale}).collect();
    serde_json::to_vec_pretty(&Profile{version:1,ppq:s.project.ppq,markers}).map_err(|e|e.to_string())
}
fn load(s:&mut State,data:&[u8],merge:bool)->Result<(),String> {
    if data.len()>4*1024*1024 {return Err("Marker profile is too large".into());}
    let profile:Profile=serde_json::from_slice(data).map_err(|e|e.to_string())?;
    if profile.version!=1||profile.ppq==0||profile.markers.len()>4096||profile.markers.iter().any(|m|m.name.len()>1024||m.numerator==0||m.denominator==0){return Err("Invalid marker profile".into());}
    let mut markers=Vec::new();
    for m in profile.markers {
        let pos=(u64::from(m.position)*u64::from(s.project.ppq)+u64::from(profile.ppq)/2)/u64::from(profile.ppq);
        if pos>u32::MAX as u64{return Err("Marker position exceeds project range".into());}
        markers.push(os_model::TimeMarker{color:m.color.map(|c|c&0xffffff),position:pos as u32,kind:m.kind,name:m.name,numerator:m.numerator,denominator:m.denominator,scale:m.scale});
    }
    pl::snapshot_as(s,"Load marker profile");
    let dest=&mut pl::arr_mut(s).markers;
    if !merge{dest.clear();}
    for marker in markers {if !dest.contains(&marker){dest.push(marker);}}
    dest.sort_by_key(|m|m.position);s.piano.dirty=true;Ok(())
}
pub fn open(app:&mut App,mode:u8) {
    let dir=os_paths::user_file("Marker Profiles");
    if let Some(dir)=&dir{let _=std::fs::create_dir_all(dir);}
    if mode==3 {
        if let Some(dir)=dir {
            #[cfg(windows)] let _=std::process::Command::new("explorer").arg(dir).spawn();
            #[cfg(target_os="macos")] let _=std::process::Command::new("open").arg(dir).spawn();
            #[cfg(target_os="linux")] let _=std::process::Command::new("xdg-open").arg(dir).spawn();
        }
        return;
    }
    let (kind,title)=if mode==0{(crate::filedlg::Mode::Save,"Save marker profile")}else{(crate::filedlg::Mode::Open,if mode==1{"Replace markers from profile"}else{"Merge marker profile"})};
    let dlg=crate::filedlg::FileDlg::new(kind,title,"marker-profile",crate::dialog::Then::MarkerProfile(mode)).filter("Marker profile",&["json"]).start(dir.as_deref());
    app.file_dialog(dlg);
}
pub fn chosen(app:&mut App,path:&Path,mode:u8) {
    let result=if mode==0 {encode(&app.state).and_then(|bytes|std::fs::write(path,bytes).map_err(|e|e.to_string()))}else{
        std::fs::metadata(path).map_err(|e|e.to_string()).and_then(|m|if m.len()>4*1024*1024{Err("Marker profile is too large".into())}else{std::fs::read(path).map_err(|e|e.to_string())}).and_then(|data|load(&mut app.state,&data,mode==2))
    };
    app.state.message=Some(match result{Ok(())=>if mode==0{"Marker profile saved"}else{"Marker profile loaded; clips unchanged"}.into(),Err(e)=>format!("Marker profile: {e}")});
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn batch2_pattern_stretch_roundtrip_reset_split_and_undo(){
        use os_model::{Clip,ClipSource};
        let mut a=App::new(None);a.state.project=crate::app::test_project();a.state.playlist.stretch=true;
        let orig=Clip::new(0,192,0,ClipSource::Pattern(1));pl::arr_mut(&mut a.state).clips=vec![orig,Clip::new(768,192,0,ClipSource::Pattern(1))];
        let patterns=a.state.project.patterns.clone();pl::snapshot(&mut a.state);assert!(pl::stretch_clip(&mut a.state,0,&orig,0,384));
        assert_eq!(pl::clips(&a.state)[0].pattern_speed,0.5);assert_eq!(pl::clips(&a.state)[1].pattern_speed,1.);assert_eq!(a.state.project.patterns,patterns);
        let file=os_flp::project::create(&a.state.project).unwrap();let loaded=os_flp::project::load(&os_flp::FlpFile::parse(&file.to_bytes().unwrap()).unwrap()).unwrap();
        assert_eq!(loaded.arrangements[0].clips[0].pattern_speed,0.5);
        crate::history::undo(&mut a.state);assert_eq!(pl::clips(&a.state)[0],orig);crate::history::redo(&mut a.state);
        pl::split(&mut a.state,0,192).unwrap();assert_eq!(pl::clips(&a.state)[2].window.unwrap().0,96.);
        pl::run(&mut a,pl::Cmd::ResetPatternStretch(0));assert_eq!(pl::clips(&a.state)[0].length,96);assert_eq!(pl::clips(&a.state)[0].pattern_speed,1.);
    }
    #[test]fn batch2_profiles_preserve_properties_rescale_merge_and_undo_without_touching_clips(){
        let mut a=App::new(None);a.state.project=crate::app::test_project();
        let mut marker=os_model::TimeMarker::new(96,"Verse");marker.color=Some(0x7298b1);marker.kind=os_model::TimeMarker::SIGNATURE;marker.numerator=3;marker.denominator=4;
        pl::arr_mut(&mut a.state).markers=vec![marker];a.state.project.ppq=96;
        let bytes=encode(&a.state).unwrap();
        a.state.project.ppq=192;pl::arr_mut(&mut a.state).markers=vec![os_model::TimeMarker::new(0,"Intro")];
        let before=a.state.project.clone();load(&mut a.state,&bytes,true).unwrap();
        assert_eq!(pl::markers(&a.state).len(),2);assert_eq!(pl::markers(&a.state)[1].position,192);assert_eq!(pl::markers(&a.state)[1].numerator,3);assert_eq!(pl::markers(&a.state)[1].color,Some(0x7298b1));
        let file=os_flp::project::create(&a.state.project).unwrap();let loaded=os_flp::project::load(&file).unwrap();assert_eq!(loaded.arrangements[0].markers,pl::markers(&a.state));
        assert_eq!(a.state.project.arrangements[0].clips,before.arrangements[0].clips);
        crate::history::undo(&mut a.state);assert_eq!(a.state.project,before);
        load(&mut a.state,&bytes,false).unwrap();assert_eq!(pl::markers(&a.state).len(),1);
        let before=a.state.project.clone();assert!(load(&mut a.state,b"not json",false).is_err());assert_eq!(a.state.project,before);
    }
}
