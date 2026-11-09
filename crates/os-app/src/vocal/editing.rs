// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn related(app:&App,channel:u16)->Vec<(u16,usize,Note)>{app.state.project.vocal_edits.iter().filter(|d|d.channel!=channel).flat_map(|d|d.notes.iter().enumerate().filter(|(i,_)|app.state.playlist.vocal.selected.contains(&(d.channel,*i))).map(move|(i,n)|(d.channel,i,n.clone()))).collect()}
fn strengths(app:&App)->Vec<(u16,f32)>{app.state.project.vocal_edits.iter().filter(|d|pl::clips(&app.state).iter().any(|c|Some(c.track)==app.state.playlist.vocal.row&&c.source==ClipSource::Channel(d.channel))).map(|d|(d.channel,d.strength)).collect()}
pub fn begin(app:&mut App,clip:usize,note:Option<usize>){
 let tool=app.state.playlist.vocal.tool;
 if matches!(tool,Tool::Hand|Tool::Zoom){navigation::begin(app,if tool==Tool::Hand{navigation::Kind::Pan}else{navigation::Kind::Zoom});return;}
 let Some(i)=doc_index(&app.state,clip)else{return};let d=app.state.project.vocal_edits[i].clone();
 if let Some(n)=note{if n>=d.notes.len(){return;}let entry=(d.channel,n);
  if app.ctrl{if let Some(i)=app.state.playlist.vocal.selected.iter().position(|p|*p==entry){app.state.playlist.vocal.selected.remove(i);}else{app.state.playlist.vocal.selected.push(entry);}app.invalidate();return;}
  if app.shift{if !app.state.playlist.vocal.selected.contains(&entry){app.state.playlist.vocal.selected.push(entry);}}else if !app.state.playlist.vocal.selected.contains(&entry){app.state.playlist.vocal.selected=vec![entry];}
  let now=Instant::now();let double=app.state.playlist.vocal.click.is_some_and(|(ch,ni,t,at)|ch==d.channel&&ni==n&&t==tool&&now.duration_since(at).as_millis()<330);
  app.state.playlist.vocal.click=if double{None}else{Some((d.channel,n,tool,now))};
  if double&&!matches!(tool,Tool::Split|Tool::Draw|Tool::Time|Tool::Attack){snapshot(&mut app.state,"Vocal tool quick action");let(root,scale)=(app.state.playlist.vocal.key,app.state.playlist.vocal.scale);let selected=app.state.playlist.vocal.selected.clone();
   for edit in &mut app.state.project.vocal_edits{for(ni,n)in edit.notes.iter_mut().enumerate().filter(|(ni,_)|selected.contains(&(edit.channel,*ni))){let _=ni;match tool{Tool::Modulation=>n.variation=if n.variation.abs()<0.001{1.}else{0.},Tool::Drift=>n.drift=if n.drift.abs()<0.001{1.}else{0.},Tool::Transition=>n.transition=0.,Tool::Formant=>n.formant=0.,Tool::Amplitude=>n.muted=!n.muted,_=>n.shift=nearest(n.base+n.shift,root,scale)-n.base}}}
   edited(app);return;
  }
 }
 if tool==Tool::Split{if let Some(n)=note{if app.alt{app.state.playlist.vocal.selected=vec![(d.channel,n),(d.channel,(n+1).min(d.notes.len()-1))];command(app,Command::Join);return;}if let Some(frame)=frame_at(app,clip,app.mouse.0){let original=d.notes[n].clone();if frame>original.start+2&&frame+2<original.end{snapshot(&mut app.state,"Split vocal note");let notes=&mut app.state.project.vocal_edits[i].notes;notes[n].end=frame;let mut right=original;right.start=frame;notes.insert(n+1,right);app.state.playlist.vocal.selected.clear();edited(app);}}}app.invalidate();return;}
 if note.is_none()&&tool!=Tool::Draw{navigation::begin(app,navigation::Kind::Box);return;}
 snapshot(&mut app.state,"Edit vocal tuning");
 let selected=app.state.playlist.vocal.selected.iter().filter(|(ch,_)|*ch==d.channel).map(|(_,n)|*n).collect();
 app.drag=Some(Drag::Vocal(Gesture{clip,note,before:Arc::new(d.clone()),selected,x:app.mouse.0,y:app.mouse.1,last:None,tool,edge:0,axis:0,field:None,related:related(app,d.channel),strengths:strengths(app)}));
 if let Some(n)=note{audition::start(app,clip,n);}
 if tool==Tool::Draw{drag(app,app.mouse.0,app.mouse.1);}app.invalidate();
}
pub fn boundary(app:&mut App,clip:usize,n:usize){
 let Some(i)=doc_index(&app.state,clip)else{return};let d=app.state.project.vocal_edits[i].clone();if n>=d.notes.len(){return;}
 snapshot(&mut app.state,"Edit vocal boundary");app.state.playlist.vocal.selected=vec![(d.channel,n)];
 app.drag=Some(Drag::Vocal(Gesture{clip,note:Some(n),before:Arc::new(d.clone()),selected:vec![n],x:app.mouse.0,y:app.mouse.1,last:None,tool:if app.state.playlist.vocal.tool==Tool::Split{Tool::Split}else{Tool::Time},edge:1,axis:0,field:None,related:related(app,d.channel),strengths:strengths(app)}));app.invalidate();
}
pub fn parameter(app:&mut App,field:Field){
 let selected=app.state.playlist.vocal.selected.clone();let ci=pl::clips(&app.state).iter().enumerate().find_map(|(ci,c)|{let d=doc(&app.state,ci)?;(Some(c.track)==app.state.playlist.vocal.row&&(field==Field::Strength||selected.iter().any(|p|p.0==d.channel))).then_some(ci)});
 let Some(ci)=ci else{return};let d=doc(&app.state,ci).unwrap();let n=selected.iter().find(|p|p.0==d.channel).map(|p|p.1).unwrap_or(0);let old=app.state.playlist.vocal.tool;
 app.state.playlist.vocal.tool=match field{Field::Pitch=>Tool::Select,Field::Modulation=>Tool::Modulation,Field::Drift=>Tool::Drift,Field::Transition=>Tool::Transition,Field::Formant=>Tool::Formant,Field::Amplitude=>Tool::Amplitude,Field::Strength=>Tool::Select};app.state.playlist.vocal.click=None;begin(app,ci,Some(n));app.state.playlist.vocal.tool=old;
 if let Some(Drag::Vocal(g))=&mut app.drag{g.field=Some(field);}
 if app.alt{if let Some(Drag::Vocal(g))=app.drag.take(){let i=doc_index(&app.state,ci).unwrap();let d=&mut app.state.project.vocal_edits[i];for n in g.selected{let n=&mut d.notes[n];match field{Field::Pitch=>n.shift=0.,Field::Modulation=>n.variation=1.,Field::Drift=>n.drift=1.,Field::Transition=>n.transition=0.,Field::Formant=>n.formant=0.,Field::Amplitude=>{n.gain=0.;n.muted=false;},Field::Strength=>d.strength=1.}}edited(app);}}
}
fn edited(app:&mut App){app.state.playlist.vocal.last_edit=Instant::now();app.state.piano.dirty=true;app.invalidate();}
fn anchor(points:&mut Vec<(f64,f64)>,source:f64,dest:f64){if let Some(p)=points.iter_mut().find(|p|(p.0-source).abs()<0.00001){p.1=dest;}else{points.push((source,dest));}points.sort_by(|a,b|a.0.total_cmp(&b.0));}
fn limit_timing(before:&Edit,after:&mut Edit){
 if engine::timing_valid(after)||!engine::timing_valid(before){return;}let candidate=after.timing.clone();let mut best=before.timing.clone();let(mut lo,mut hi)=(0.,1.);
 for _ in 0..24{let t=(lo+hi)/2.;after.timing=candidate.iter().map(|&(x,y)|(x,before.destination(x)+(y-before.destination(x))*t)).collect();if engine::timing_valid(after){best=after.timing.clone();lo=t;}else{hi=t;}}
 after.timing=best;
}
pub fn drag(app:&mut App,x:f32,y:f32){
 let Some(Drag::Vocal(mut g))=app.drag.clone()else{return};let Some(i)=doc_index(&app.state,g.clip)else{return};let Some(grid)=app.pl_grid()else{return};let geom=pl::PGeom::new(&app.state,grid);let clip=pl::clips(&app.state)[g.clip];let r=view::pitch_rect(&geom,clip.track);
 if (x-g.x).abs()>3.||(y-g.y).abs()>3.{app.state.playlist.vocal.click=None;}
 let mut tool=g.tool;if tool==Tool::Main{if g.axis==0&&((x-g.x).abs()>3.||(y-g.y).abs()>3.){g.axis=if (x-g.x).abs()>(y-g.y).abs(){1}else{2};}if g.axis==0{return;}tool=if g.axis==1{Tool::Time}else{Tool::Select};}
 let mut d=if tool==Tool::Draw{app.state.project.vocal_edits[i].clone()}else{g.before.as_ref().clone()};
 if g.field==Some(Field::Strength){d.strength=(g.before.strength+(g.y-y)/150.).clamp(0.,1.);}
 else if tool==Tool::Draw{if let Some(frame)=frame_at(app,g.clip,x){let pitch=app.state.playlist.vocal.low+(r.bottom()-y)/r.h*app.state.playlist.vocal.span;let start=g.last.unwrap_or((frame,pitch));let(a,b)=(start.0.min(frame),start.0.max(frame));let mut clean=d.clone();clean.curve.clear();let baseline=clean.pitches();for f in a..=b{if d.f0[f as usize]<=0.{continue;}let t=if start.0==frame{1.}else{(f as f32-start.0 as f32)/(frame as f32-start.0 as f32)};let target=start.1+(pitch-start.1)*t;let delta=((target-baseline[f as usize])/d.strength.max(0.01)).clamp(-24.,24.);match d.curve.binary_search_by_key(&f,|p|p.0){Ok(i)=>d.curve[i].1=delta,Err(i)=>d.curve.insert(i,(f,delta))}}g.last=Some((frame,pitch));}}
 else if tool==Tool::Split{if let(Some(n),Some(frame))=(g.note,frame_at(app,g.clip,x)){let end=if n+1<d.notes.len(){d.notes[n+1].end.saturating_sub(3)}else{d.f0.len()as u32-1};let at=frame.clamp(d.notes[n].start+3,end.max(d.notes[n].start+3));d.notes[n].end=at;if n+1<d.notes.len(){d.notes[n+1].start=at;}}}
 else if matches!(tool,Tool::Time|Tool::Attack){if let Some(n)=g.note{let note=&g.before.notes[n];let(a,b)=(note.start as f64*STEP,(note.end as f64*STEP).min(d.frames as f64/d.rate as f64));let duration=d.frames as f64/d.rate as f64;let cr=geom.clip_rect(&clip);let(start,end)=pl::window(&app.state,&clip);let mut ticks=(x-g.x)/geom.ppt;if !app.alt&&app.state.playlist.vocal.snap{let step=geom.ppq as f32/4.;ticks=(ticks/step).round()*step;}let delta=f64::from(ticks*geom.ppt/cr.w)*(end-start)/1000.;
  let epsilon=1./d.rate as f64;
  if tool==Tool::Attack{let mid=(a+b)/2.;if b-a>0.02&&g.before.destination(b)-g.before.destination(a)>2.*epsilon{let left=g.before.destination(a);let right=g.before.destination(b);let target=(g.before.destination(mid)+delta).clamp(left+epsilon,right-epsilon);anchor(&mut d.timing,a,left);anchor(&mut d.timing,b,right);d.timing.retain(|p|p.0<=a||p.0>=b);anchor(&mut d.timing,mid,target);}}
  else if g.edge!=0{if b>0.005&&b<duration-0.005{let left=g.before.timing.iter().filter(|p|p.0<b-0.00001).last().copied().unwrap_or((0.,0.));let right=g.before.timing.iter().find(|p|p.0>b+0.00001).copied().unwrap_or((duration,duration));let(lo,hi)=(left.1+epsilon,right.1-epsilon);if lo<=hi{anchor(&mut d.timing,b,(g.before.destination(b)+delta).clamp(lo,hi));}}}
  else if a>0.&&b<duration{let mut timing:Vec<_>=d.timing.iter().copied().filter(|p|p.0<a-0.00001||p.0>b+0.00001).collect();let left=timing.iter().filter(|p|p.0<a).last().copied().unwrap_or((0.,0.));let right=timing.iter().find(|p|p.0>b).copied().unwrap_or((duration,duration));let min=left.1+epsilon-g.before.destination(a);let max=right.1-epsilon-g.before.destination(b);if min<=max{let delta=delta.clamp(min,max);timing.extend([(a,g.before.destination(a)+delta),(b,g.before.destination(b)+delta)]);timing.sort_by(|a,b|a.0.total_cmp(&b.0));d.timing=timing;}}
 }}else{let fine=if app.alt{0.2}else{1.};let dy=(g.y-y)*fine;for &n in &g.selected{if let Some(note)=d.notes.get_mut(n){match tool{
 Tool::Select=>{let mut delta=dy/r.h*app.state.playlist.vocal.span;if !app.alt&&app.state.playlist.vocal.snap{delta=delta.round();}note.shift=(note.shift+delta).clamp(-24.,24.);},
 Tool::Modulation=>note.variation=(note.variation+dy/100.).clamp(-1.,2.),Tool::Drift=>note.drift=(note.drift+dy/100.).clamp(-1.,2.),Tool::Transition=>note.transition=(note.transition+dy/100.).clamp(0.,1.),Tool::Formant=>note.formant=(note.formant+dy/12.).clamp(-12.,12.),Tool::Amplitude=>note.gain=(note.gain+dy/4.).clamp(-60.,24.),_=>{}
 }}}}
 if matches!(tool,Tool::Time|Tool::Attack){limit_timing(&g.before,&mut d);}
 if d.valid()&&(!matches!(tool,Tool::Time|Tool::Attack)||engine::timing_valid(&d)){app.state.project.vocal_edits[i]=d;
  let fine=if app.alt{0.2}else{1.};let dy=(g.y-y)*fine;
  if g.field==Some(Field::Strength){for(ch,original)in &g.strengths{if let Some(edit)=app.state.project.vocal_edits.iter_mut().find(|d|d.channel==*ch){edit.strength=(*original+(g.y-y)/150.).clamp(0.,1.);}}}
  for(ch,ni,before)in g.related.iter().filter(|_|g.field!=Some(Field::Strength)){if let Some(edit)=app.state.project.vocal_edits.iter_mut().find(|d|d.channel==*ch){if let Some(note)=edit.notes.get_mut(*ni){let mut n=before.clone();match tool{Tool::Select=>{let mut delta=dy/r.h*app.state.playlist.vocal.span;if !app.alt&&app.state.playlist.vocal.snap{delta=delta.round();}n.shift=(n.shift+delta).clamp(-24.,24.);},Tool::Modulation=>n.variation=(n.variation+dy/100.).clamp(-1.,2.),Tool::Drift=>n.drift=(n.drift+dy/100.).clamp(-1.,2.),Tool::Transition=>n.transition=(n.transition+dy/100.).clamp(0.,1.),Tool::Formant=>n.formant=(n.formant+dy/12.).clamp(-12.,12.),Tool::Amplitude=>n.gain=(n.gain+dy/4.).clamp(-60.,24.),_=>{}}*note=n;}}}
  edited(app);audition::update(app);}app.drag=Some(Drag::Vocal(g));app.invalidate();
}
