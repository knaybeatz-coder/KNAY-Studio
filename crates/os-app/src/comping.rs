// SPDX-License-Identifier: GPL-3.0-or-later
//! Non-destructive audio take groups. Muted child lanes retain complete source clips;
//! the leader contains ordinary editable audio clips, so playback/export need no extra DSP.
use crate::app::State;
use crate::playlist::{self as pl, PickKind};
use os_model::{Clip, ClipSource, PlaylistTrack};

pub fn parent(s: &State, row: u16) -> Option<u16> {
    pl::group_track(s,row).filter(|&p| pl::track_info(s,p).is_some_and(|t| t.comp.is_some()))
}
pub fn for_mixer(s: &State, mixer: u16) -> Option<u16> {
    s.project.arrangement()?.tracks.iter().position(|t| t.comp==Some(mixer) && !t.grouped).map(|i|i as u16)
}
/// Reuse a compatible audio row: overlap first, then the nearest earlier recording.
/// Whether to create take lanes is decided separately by `recorded`.
pub fn recording_target(s:&State,mixer:u16,a:u32,b:u32)->Option<u16> {
    pl::clips(s).iter().filter(|c| route(s,c)==mixer && pl::source_kind(s,c.source)==PickKind::Audio)
        .filter(|c| {
            let row=c.track;
            !pl::track_info(s,row).is_some_and(|t|t.locked||t.grouped) &&
            (!pl::is_group_track(s,row)||pl::track_info(s,row).is_some_and(|t|t.comp==Some(mixer))) &&
            pl::clips(s).iter().filter(|c|c.track==row).all(|c|pl::source_kind(s,c.source)==PickKind::Audio&&route(s,c)==mixer)
        }).min_by_key(|c| {
            if c.position<b && c.end()>a {(0,0,c.track)}
            else if c.end()<=a {(1,a-c.end(),c.track)}
            else {(2,c.position.saturating_sub(b),c.track)}
        }).map(|c|c.track)
}
fn ensure(s: &mut State) {
    let a=pl::arr_mut(s);
    while a.tracks.len()<pl::TRACKS as usize {a.tracks.push(PlaylistTrack::new(a.tracks.len() as u32+1));}
}
fn capacity(s: &State, count: usize) -> bool {
    count < pl::TRACKS as usize && (pl::TRACKS as usize-count..pl::TRACKS as usize).all(|r| {
        !pl::clips(s).iter().any(|c| c.track as usize==r) && pl::track_info(s,r as u16).is_none_or(|t| t.name.is_none() && t.comp.is_none() && !t.grouped && !t.locked)
    })
}
fn child(s: &mut State, p: u16) -> u16 {
    ensure(s);
    let mut row=p+1;
    while row<pl::TRACKS && pl::track_info(s,row).is_some_and(|t|t.grouped) {row+=1;}
    pl::insert_track(s,row);
    let color=pl::track_info(s,p).and_then(|t|t.color);
    let t=&mut pl::arr_mut(s).tracks[row as usize];
    t.name=Some(format!("Take {}",row-p));t.color=color;t.grouped=true;t.enabled=false;
    pl::arr_mut(s).tracks[p as usize].collapsed=false;
    row
}
fn route(s: &State,c: &Clip)->u16 {
    if let ClipSource::Channel(ch)=c.source {s.project.channels.iter().find(|c|c.index==ch).map_or(0,|c|c.mixer_track)}else{0}
}
pub fn enable(s: &mut State,row:u16)->String {
    if pl::track_info(s,row).is_some_and(|t|t.comp.is_some()) {return "This track already has take lanes".into();}
    if pl::track_info(s,row).is_some_and(|t|t.locked||t.grouped) || pl::is_group_track(s,row) {return "Choose an unlocked, ungrouped audio track".into();}
    let clips:Vec<_>=pl::clips(s).iter().filter(|c|c.track==row).copied().collect();
    if clips.is_empty() || clips.iter().any(|c|pl::source_kind(s,c.source)!=PickKind::Audio) {return "Choose a track containing audio clips".into();}
    let mixer=route(s,&clips[0]);
    if clips.iter().any(|c|route(s,c)!=mixer) {return "The takes must use the same mixer insert".into();}
    if !capacity(s,clips.len()) || row as usize+clips.len()>=pl::TRACKS as usize {return "Not enough empty tracks at the bottom for take lanes".into();}
    pl::snapshot_as(s,"Create take lanes");ensure(s);
    pl::arr_mut(s).tracks[row as usize].comp=Some(mixer);
    pl::arr_mut(s).clips.retain(|c|c.track!=row);
    for mut c in clips {
        c.track=child(s,row);c.muted=true;c.set_id(0);
        pl::arr_mut(s).clips.push(c);
        replace(s,row,c,c.position,c.end());
    }
    s.piano.dirty=true;
    "Take lanes created. Select a time range, then right-click a take to use it in the comp".into()
}
fn crop(s:&State,c:Clip,a:u32,b:u32)->Clip {
    let mut out=c;
    let w=pl::window(s,&c).0;
    out.window=Some((w+pl::to_src(s,c.source,(a-c.position) as f64),w+pl::to_src(s,c.source,(b-c.position) as f64)));
    out.position=a;out.length=b-a;out.set_id(0);out
}
fn replace(s:&mut State,p:u16,c:Clip,a:u32,b:u32) {
    // Remove previously generated overlaps before making another edit, then rebuild
    // joins once. This prevents successive choices accumulating tiny old fragments.
    let mut leader:Vec<_>=pl::clips(s).iter().filter(|c|c.track==p).copied().collect();
    leader.sort_by_key(|c|c.position);
    let mut prior_end=0;
    for old in &mut leader {
        if old.position<prior_end && prior_end<old.end() {*old=crop(s,*old,prior_end,old.end());}
        old.set_fade_in(None);old.set_fade_out(None);prior_end=old.end();
    }
    let mut result:Vec<_>=pl::clips(s).iter().filter(|c|c.track!=p).copied().collect();
    for old in leader {
        if old.track!=p || old.end()<=a || old.position>=b {result.push(old);continue;}
        if old.position<a {result.push(crop(s,old,old.position,a));}
        if old.end()>b {result.push(crop(s,old,b,old.end()));}
    }
    let mut new=crop(s,c,a,b);new.track=p;new.muted=false;new.set_fade_in(None);new.set_fade_out(None);result.push(new);
    // A short overlap at adjacent source changes, using audio before the incoming edit.
    // At a source's beginning (no preceding audio), use a short edge fade instead.
    let mut order:Vec<_>=result.iter().enumerate().filter(|(_,c)|c.track==p).map(|(i,_)|i).collect();
    order.sort_by_key(|&i|result[i].position);
    for pair in order.windows(2) {
        let (l,r)=(pair[0],pair[1]);
        if result[l].end()!=result[r].position {continue;}
        let ticks=((0.003*s.tempo/60.0*s.project.ppq as f64).round() as u32).max(1).min(result[l].length/2).min(result[r].length/2);
        if ticks==0 {continue;}
        let delta=pl::to_src(s,result[r].source,ticks as f64);
        let ms=(ticks as f64*60000.0/(s.tempo.max(1.0)*s.project.ppq.max(1) as f64)) as f32;
        let (w0,w1)=pl::window(s,&result[r]);
        if w0>=delta && result[r].position>=ticks {
            result[r].position-=ticks;result[r].length+=ticks;result[r].window=Some((w0-delta,w1));
        }
        result[l].set_fade_out(Some((ms,0.0)));result[r].set_fade_in(Some((ms,0.0)));
    }
    pl::arr_mut(s).clips=result;
    s.piano.dirty=true;
}
/// Nominal comp sections, excluding the incoming overlap used for fades.
pub fn sections(s:&State,p:u16)->Vec<Clip> {
    let mut v:Vec<_>=pl::clips(s).iter().filter(|c|c.track==p).copied().collect();
    v.sort_by_key(|c|c.position);
    let mut end=0;
    for c in &mut v {if c.position<end && end<c.end(){*c=crop(s,*c,end,c.end());}end=c.end();}
    v
}
pub fn section_at(s:&State,i:usize,tick:u32)->Option<(u32,u32)> {
    let c=pl::clips(s).get(i)?;let p=parent(s,c.track)?;
    let piece=sections(s,p).into_iter().find(|c|tick>=c.position&&tick<c.end());
    let (a,b)=piece.map_or((c.position,c.end()),|c|(c.position,c.end()));
    Some((a.max(c.position),b.min(c.end())))
}
/// Regions in this source lane currently used by the comp. Source offsets disambiguate loop passes.
pub fn used(s:&State,c:&Clip)->Vec<(u32,u32)> {
    let Some(p)=parent(s,c.track) else{return vec![];};
    sections(s,p).into_iter().filter_map(|piece| {
        let (a,b)=(c.position.max(piece.position),c.end().min(piece.end()));
        if a>=b || piece.source!=c.source {return None;}
        let source=pl::window(s,c).0+pl::to_src(s,c.source,(a-c.position) as f64);
        let chosen=pl::window(s,&piece).0+pl::to_src(s,piece.source,(a-piece.position) as f64);
        ((source-chosen).abs()<0.05).then_some((a,b))
    }).collect()
}
pub fn choose(s:&mut State,i:usize,selection:bool)->String {
    let Some(c)=pl::clips(s).get(i).copied() else{return "Take no longer exists".into();};
    let range=if selection {let Some(r)=s.playlist.range else{return "Select a time range in the Playlist ruler first".into();};r}else{(c.position,c.end())};
    choose_range(s,i,range)
}
pub fn choose_range(s:&mut State,i:usize,(a,b):(u32,u32))->String {
    let Some(c)=pl::clips(s).get(i).copied() else{return "Take no longer exists".into();};
    let Some(p)=parent(s,c.track) else{return "Choose a clip in a take lane".into();};
    if pl::track_info(s,p).is_some_and(|t|t.locked) {return "The comp track is locked".into();}
    let (a,b)=(a.min(b).max(c.position),a.max(b).min(c.end()));
    if b<=a {return "The selected time range does not overlap this take".into();}
    pl::snapshot_as(s,"Choose take for comp");replace(s,p,c,a,b);
    s.playlist.sel.clear();
    "Comp updated; original take preserved".into()
}
/// Turn the newly recorded clip into source lanes, optionally unwrapping a fixed-tempo loop.
/// The recorder retains one complete WAV; each lane addresses its own source window.
pub fn recorded(s:&mut State,i:usize,mixer:u16,loop_span:Option<(u32,u32)>)->Result<(),String> {
    let c=pl::clips(s)[i];
    let mut takes=Vec::new();
    if let Some((a,b))=loop_span.filter(|&(a,b)|b>a&&c.position>=a&&c.position<b) {
        let mut used=0;
        while used<c.length {
            let start=if used==0 {c.position}else{a};
            let n=(b-start).min(c.length-used);
            let mut take=crop(s,c,c.position+used,c.position+used+n);take.position=start;takes.push(take);used+=n;
            if takes.len()>=pl::TRACKS as usize {return Err("Recording retained as one clip: too many loop takes for the Playlist".into());}
        }
    }else{takes.push(c);}
    let p=c.track;
    let overlaps=pl::clips(s).iter().enumerate().any(|(j,old)|j!=i && old.track==p && takes.iter().any(|t|old.position<t.end()&&old.end()>t.position));
    // A first pass is the ordinary audible clip, not a comp plus a duplicate source lane.
    if !overlaps && takes.len()==1 {return Ok(());}
    let extra=takes.len()-usize::from(!overlaps);
    let count=(p+1..pl::TRACKS).take_while(|&r|pl::track_info(s,r).is_some_and(|t|t.grouped)).count();
    if !capacity(s,extra) || p as usize+count+extra>=pl::TRACKS as usize {return Err("Recording retained as one clip: no room for more take lanes".into());}
    ensure(s);pl::arr_mut(s).tracks[p as usize].comp=Some(mixer);
    pl::arr_mut(s).clips.remove(i);
    if !overlaps {
        let mut first=takes.remove(0);first.track=p;first.muted=false;
        pl::arr_mut(s).clips.push(first);
    }
    let mut number=(p+1..p+1+count as u16).filter_map(|r|pl::track_info(s,r)?.name.as_deref()?.strip_prefix("Take ")?.parse::<u32>().ok()).max().unwrap_or(1)+1;
    for mut take in takes {
        take.track=child(s,p);take.muted=true;take.set_id(0);
        pl::arr_mut(s).tracks[take.track as usize].name=Some(format!("Take {number}"));number+=1;
        pl::arr_mut(s).clips.push(take);
    }
    s.piano.dirty=true;
    s.playlist.sel.clear();Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app()->crate::app::App {
        let mut app=crate::app::App::new(None);app.state.project=crate::app::test_project();app.state.tempo=120.;
        app.state.project.channels[0].kind=os_model::ChannelKind::AudioClip;
        app.state.project.channels[1].kind=os_model::ChannelKind::AudioClip;
        app.state.project.channels[1].mixer_track=1;
        ensure(&mut app.state);app
    }
    fn clip(ch:u16,length:u32)->Clip { let mut c=Clip::new(0,length,0,ClipSource::Channel(ch));c.window=Some((0.,length as f64*60000./(120.*96.)));c }
    #[test]
    fn takes_comp_selection_undo_and_save_preserve_sources() {
        let mut app=app();let s=&mut app.state;
        pl::arr_mut(s).clips=vec![clip(0,384),clip(1,384)];
        assert!(enable(s,0).starts_with("Take lanes created"));
        assert_eq!(parent(s,1),Some(0));assert_eq!(parent(s,2),Some(0));
        assert!(!pl::track_on(s,1)&&!pl::track_on(s,2));
        let originals:Vec<_>=pl::clips(s).iter().filter(|c|c.track>0).copied().collect();
        assert!(originals.iter().all(|c|c.muted&&c.length==384));
        let i=pl::clips(s).iter().position(|c|c.track==1).unwrap();
        s.playlist.range=Some((96,192));
        assert!(pl::clip_menu(s,i,0.,0.).items.iter().any(|p|p.label=="Use selected time range in comp"&&p.enabled));
        assert!(choose(s,i,true).starts_with("Comp updated"));
        for _ in 0..3 {
            let i=pl::clips(s).iter().position(|c|c.track==1).unwrap();
            choose(s,i,true);
            assert_eq!(pl::clips(s).iter().filter(|c|c.track==0).count(),3);
        }
        let edited=pl::clips(s).to_vec();
        let comp:Vec<_>=edited.iter().filter(|c|c.track==0).collect();assert_eq!(comp.len(),3);
        assert_eq!(comp.iter().filter(|c|c.source==ClipSource::Channel(0)).count(),1);
        let picked=comp.iter().find(|c|c.source==ClipSource::Channel(0)).unwrap();
        assert!(picked.position<=96&&picked.end()==192);
        assert!(picked.fade_in().is_some()&&picked.fade_out().is_some());
        assert_eq!(pl::clips(s).iter().filter(|c|c.track>0).copied().collect::<Vec<_>>(),originals);
        for _ in 0..4 {assert!(crate::history::undo(s).is_some());}assert_eq!(pl::clips(s).iter().filter(|c|c.track==0).count(),1);
        for _ in 0..4 {crate::history::redo(s);}assert_eq!(pl::clips(s),edited);
        let mut file=os_flp::project::create(&s.project).unwrap();
        let back=os_flp::project::load(&os_flp::FlpFile::parse(&file.to_bytes().unwrap()).unwrap()).unwrap();
        let a=back.arrangement().unwrap();assert_eq!(a.tracks[0].comp,Some(1));assert!(a.tracks[1].grouped&&!a.tracks[1].enabled);
        assert_eq!(a.clips.len(),edited.len());
        let mut got=a.clips.clone();let mut expected=edited.clone();
        got.sort_by_key(|c|(c.track,c.position));expected.sort_by_key(|c|(c.track,c.position));
        for (a,b) in got.iter().zip(&expected) {
            assert_eq!((a.position,a.length,a.track,a.muted),(b.position,b.length,b.track,b.muted));
            let (aw,bw)=(a.window.unwrap(),b.window.unwrap());assert!((aw.0-bw.0).abs()<0.01 && (aw.1-bw.1).abs()<0.01);
            assert_eq!(a.fade_in(),b.fade_in());assert_eq!(a.fade_out(),b.fade_out());
        }
        let mut changed=back;changed.arrangements[0].tracks[0].comp=None;os_flp::project::apply(&changed,&mut file).unwrap();
        assert_eq!(os_flp::project::load(&file).unwrap().arrangement().unwrap().tracks[0].comp,None);
    }
    #[test]
    fn loop_recording_unwraps_full_and_partial_takes_without_losing_audio() {
        let mut app=app();let s=&mut app.state;
        pl::arr_mut(s).clips=vec![clip(0,960)];
        recorded(s,0,1,Some((0,384))).unwrap();
        let takes:Vec<_>=pl::clips(s).iter().filter(|c|c.track>0).collect();assert_eq!(takes.len(),2);
        assert_eq!(pl::clips(s).iter().map(|c|c.length).sum::<u32>(),960);
        assert_eq!(takes.iter().map(|c|c.position).collect::<Vec<_>>(),vec![0,0]);
        assert_eq!(takes.iter().map(|c|c.window.unwrap().0).collect::<Vec<_>>(),vec![2000.,4000.]);
        assert_eq!(takes.last().unwrap().length,192);
        assert!(takes.iter().all(|c|c.muted));
        assert!(pl::clips(s).iter().filter(|c|c.track==0).all(|c|!c.muted));
        // A further pass appends under the same group, preserving the old sources.
        let i=pl::clips(s).len();pl::arr_mut(s).clips.push(clip(1,384));recorded(s,i,1,None).unwrap();
        assert_eq!(pl::clips(s).iter().filter(|c|c.track>0).count(),3);
        assert_eq!(s.project.arrangement().unwrap().tracks.iter().filter(|t|t.comp.is_some()).count(),1);
    }
    #[test]
    fn solo_take_auditions_muted_sources_and_restores_parent_mutes() {
        let path=std::env::temp_dir().join(format!("lemon-solo-take-{}.wav",std::process::id()));
        os_engine::wav::write_f32_stereo(&mut std::fs::File::create(&path).unwrap(),48000,&vec![0.25;192000]).unwrap();
        for main_on in [false,true] {
            let mut app=app();let s=&mut app.state;
            for ch in &mut s.project.channels[..2] {ch.sample_path=Some(path.to_string_lossy().into_owned());}
            pl::arr_mut(s).clips=vec![clip(0,384),clip(1,384)];recorded(s,1,1,None).unwrap();
            pl::arr_mut(s).tracks[0].enabled=main_on;
            let original=pl::clips(s).to_vec();
            let states:Vec<_>=pl::arr_mut(s).tracks.iter().map(|t|t.enabled).collect();
            pl::toggle_solo(s,1);
            let p=crate::rack::play_project(s);
            assert!(!p.arrangement().unwrap().tracks[0].enabled);
            assert!(!p.arrangement().unwrap().clips.iter().find(|c|c.track==1).unwrap().muted);
            let mut engine=os_engine::Engine::new(&p,&mut os_engine::sample::SampleLibrary::default(),48000,os_engine::PlayMode::Song);
            assert!(engine.render_all(256).iter().any(|v|v.abs()>0.01),"solo must be audible with parent muted");
            assert_eq!(pl::clips(s),original,"audition must not change stored source mutes");
            pl::toggle_solo(s,1);
            assert_eq!(pl::arr_mut(s).tracks.iter().map(|t|t.enabled).collect::<Vec<_>>(),states);
            assert!(crate::rack::play_project(s).arrangement().unwrap().clips.iter().find(|c|c.track==1).unwrap().muted);
        }
        let _=std::fs::remove_file(path);
    }
    #[test]
    fn first_take_stays_normal_and_only_overlaps_create_extra_lanes() {
        let mut app=app();let s=&mut app.state;
        s.project.channels[0].mixer_track=1;
        let first=clip(0,384);pl::arr_mut(s).clips=vec![first];
        recorded(s,0,1,None).unwrap();
        assert_eq!(pl::clips(s),&[first]);assert_eq!(for_mixer(s,1),None);
        assert_eq!(recording_target(s,1,384,500),Some(0),"continue on the same row");
        assert_eq!(recording_target(s,2,0,192),None,"other mixer stays separate");
        assert_eq!(recording_target(s,1,96,192),Some(0));
        let mut second=clip(1,96);second.position=96;
        pl::arr_mut(s).clips.push(second);recorded(s,1,1,None).unwrap();
        assert_eq!(pl::clips(s).len(),2);assert_eq!(pl::clips(s)[0],first);
        assert_eq!(pl::clips(s)[1].track,1);assert!(pl::clips(s)[1].muted);
        assert_eq!(pl::track_info(s,1).unwrap().name.as_deref(),Some("Take 2"));
        assert_eq!(recording_target(s,1,500,600),Some(0),"continue on the comp leader, not a source lane");
        choose_range(s,1,(96,192));
        assert_eq!(sections(s,0).len(),3);
        assert_eq!(sections(s,0)[1].source,second.source);
        crate::history::undo(s);assert_eq!(pl::clips(s)[0],first);
        let mut third=second;third.length=48;let i=pl::clips(s).len();pl::arr_mut(s).clips.push(third);
        recorded(s,i,1,None).unwrap();assert_eq!(pl::clips(s).len(),3);
        assert_eq!(pl::clips(s)[0],first);assert_eq!(pl::track_info(s,2).unwrap().name.as_deref(),Some("Take 3"));
    }
    #[test]
    fn full_playlist_keeps_recording_and_comp_selection_outside_take_is_noop() {
        let mut app=app();let s=&mut app.state;pl::arr_mut(s).clips=vec![clip(0,384)];
        pl::arr_mut(s).tracks[499].name=Some("Keep me".into());pl::arr_mut(s).clips.push(clip(1,384));let old=pl::clips(s).to_vec();
        assert!(recorded(s,1,1,None).is_err());assert_eq!(pl::clips(s),old);
        pl::arr_mut(s).tracks[499].name=None;enable(s,0);let old=pl::clips(s).to_vec();
        let i=pl::clips(s).iter().position(|c|c.track==1).unwrap();s.playlist.range=Some((400,500));
        assert!(choose(s,i,true).contains("does not overlap"));assert_eq!(pl::clips(s),old);
    }
}
