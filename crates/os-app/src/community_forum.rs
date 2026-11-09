// SPDX-License-Identifier: GPL-3.0-or-later
//! Paged community discussions. Uses the voting worker; never runs networking on the audio thread.
use serde::{Serialize,Deserialize};
use os_ui::{Rect,Font};
use crate::app::{App,State,Ui,Action};
use crate::feature_votes::{button,comment_lines,text};
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Topic {pub id:u64,pub title:String,pub author:String,#[serde(default)]pub avatar:String,#[serde(default)]pub member:String,pub created:i64,pub updated:i64,pub locked:u8,pub posts:u64}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Post {pub id:u64,pub thread:u64,pub parent:Option<u64>,#[serde(default)]pub depth:u32,#[serde(default)]pub completed:u8,#[serde(default)]pub parent_author:String,#[serde(default)]pub parent_body:String,pub author:String,#[serde(default)]pub avatar:String,#[serde(default)]pub member:String,pub body:String,pub created:i64,pub up:u64,pub down:u64,pub mine:i8}
#[derive(Clone,Debug,Default,Serialize,Deserialize)]
pub struct Profile {pub name:String,pub avatar:String}
#[derive(Clone,Debug,Default,Serialize,Deserialize)]
pub struct Category {pub id:u64,pub name:String,pub posts:u64}
#[derive(Clone,Debug,Default,Serialize,Deserialize)]
pub struct Page {#[serde(default)]pub category:u64,#[serde(default)]pub categories:Vec<Category>,#[serde(default)]pub profile:Profile,pub threads:Vec<Topic>,pub posts:Vec<Post>,pub topic:Option<Topic>,pub total:u64,pub offset:u64}
#[derive(Clone,Debug,Default)]
pub struct View {wrapped:std::cell::RefCell<Option<(u32,u32,Vec<Vec<String>>)>>,pub collapsed:std::collections::HashSet<u64>,pub editing_profile:bool,pub name:crate::dialog::Field,pub avatar:String,pub avatar_job:Option<std::sync::Arc<std::sync::Mutex<Option<Result<String,String>>>>>,pub page:Page,pub new_thread:bool,pub title:crate::dialog::Field,pub body:crate::dialog::Field,pub focus:u8,pub reply:Option<u64>,pub height:std::cell::Cell<f32>}
fn valid_avatar(s:&str)->bool {s.is_empty()||(s.len()==6144&&s.bytes().all(|b|b.is_ascii_hexdigit()))}
pub fn valid(p:&Page)->bool {
 let safe=|s:&str,max:usize|s.chars().count()<=max&&!s.chars().any(|c|c.is_control()&&c!='\n'&&c!='\t');
 let topic=|t:&Topic|t.id>0&&t.locked<=1&&safe(&t.title,100)&&safe(&t.author,32)&&safe(&t.member,32)&&valid_avatar(&t.avatar);
 p.category<=9&&p.categories.len()<=9&&p.categories.iter().all(|c|c.id>0&&c.id<=9&&safe(&c.name,40))&&safe(&p.profile.name,32)&&valid_avatar(&p.profile.avatar)&&p.threads.len()<=20&&p.posts.len()<=20&&p.offset<=1000000&&p.threads.iter().all(topic)&&p.topic.as_ref().is_none_or(topic)&&p.posts.iter().all(|p|p.id>0&&p.completed<=1&&p.depth<=1000&&safe(&p.parent_author,32)&&safe(&p.parent_body,800)&&safe(&p.body,800)&&safe(&p.author,32)&&safe(&p.member,32)&&valid_avatar(&p.avatar)&&(-1..=1).contains(&p.mine))
}
pub fn request(s:&State)->serde_json::Value {let p=&s.feature_votes.forum.page;serde_json::json!({"action":"forum_list","thread":p.topic.as_ref().map_or(0,|t|t.id),"offset":p.offset,"category":p.category})}
pub fn accept(v:&mut crate::feature_votes::View,page:Page,submitted:bool) {
 v.forum.wrapped.get_mut().take();
 v.forum.page=page;
 if submitted {v.forum.new_thread=false;v.forum.body.set("");v.forum.title.set("");v.forum.reply=None;v.forum.focus=0;}
}
pub fn act(app:&mut App,id:u64,cmd:i8) {
 if cmd==13||cmd==14||cmd==26 {app.state.browser_search_focus=false;app.state.feature_votes.forum.focus=if cmd==26{3}else if cmd==13{1}else{2};app.invalidate();return;}
 if app.state.feature_votes.busy {return;}
 let f=&mut app.state.feature_votes.forum;
 match cmd {
  10=>{f.editing_profile=false;f.new_thread=true;f.focus=1;f.reply=None;app.state.browser_scroll=0.;app.invalidate();},
  11=>{f.editing_profile=false;f.new_thread=false;f.reply=None;f.focus=0;app.state.browser_scroll=0.;crate::feature_votes::start(app,serde_json::json!({"action":"forum_list"}));},
  12=>{f.editing_profile=false;f.new_thread=false;f.reply=None;f.focus=0;app.state.browser_scroll=0.;crate::feature_votes::start(app,serde_json::json!({"action":"forum_list","thread":id}));},
  15=>{
   if !app.state.feature_votes.online{return;}
   let body=f.body.text.trim().to_string();
   if body.is_empty()||body.chars().count()>800 {app.state.feature_votes.status="Write a message of 1–800 characters.".into();app.invalidate();return;}
   let payload=if f.new_thread {
    let title=f.title.text.trim();if !(4..=100).contains(&title.chars().count()){app.state.feature_votes.status="Use 4–100 characters for your thread title.".into();app.invalidate();return;}
    serde_json::json!({"action":"forum_create","title":title,"body":body})
   }else{let Some(t)=&f.page.topic else{return;};if t.locked!=0{return;}
    serde_json::json!({"action":"forum_reply","thread":t.id,"parent":f.reply,"body":body,"category":f.page.category})};
   crate::feature_votes::start(app,payload);
  },
  16=>{if f.page.posts.iter().any(|p|p.id==id){f.reply=Some(id);f.focus=2;app.state.browser_scroll=0.;app.invalidate();}},
  17|18=>{if !app.state.feature_votes.online{return;}let Some(t)=&f.page.topic else{return;};if t.locked!=0{return;}
   let Some(post)=f.page.posts.iter().find(|p|p.id==id) else{return;};let value=if cmd==17{1}else{-1};
   let payload=serde_json::json!({"action":"forum_react","thread":t.id,"offset":f.page.offset,"category":f.page.category,"post":id,"value":if post.mine==value{0}else{value}});crate::feature_votes::start(app,payload);
  },
  19|20=>{let offset=if cmd==19{f.page.offset.saturating_sub(20)}else{f.page.offset+20};if offset>=f.page.total&&offset!=0{return;}
   let payload=serde_json::json!({"action":"forum_list","thread":f.page.topic.as_ref().map_or(0,|t|t.id),"offset":offset,"category":f.page.category});app.state.browser_scroll=0.;crate::feature_votes::start(app,payload);
  },
  21=>{f.reply=None;app.invalidate();},
  22=>{f.editing_profile=true;f.name.set(&f.page.profile.name);f.avatar=f.page.profile.avatar.clone();f.focus=3;app.state.browser_scroll=0.;app.invalidate();},
  23=>{let dlg=crate::filedlg::FileDlg::new(crate::filedlg::Mode::Open,"Choose profile picture","community-avatar",crate::dialog::Then::CommunityAvatar).filter("Picture",&["png","jpg","jpeg"]);app.file_dialog(dlg);},
  24=>{f.avatar.clear();f.avatar_job=None;app.invalidate();},
  25=>{if !app.state.feature_votes.online||f.avatar_job.is_some(){return;}
   let name=f.name.text.trim();if !(2..=32).contains(&name.chars().count()){app.state.feature_votes.status="Use 2–32 characters for your display name.".into();app.invalidate();return;}
   let body=serde_json::json!({"action":"forum_profile","name":name,"avatar":f.avatar,"thread":f.page.topic.as_ref().map_or(0,|t|t.id),"offset":f.page.offset});crate::feature_votes::start(app,body);
  },
  27=>{if !f.collapsed.remove(&id){f.collapsed.insert(id);}app.invalidate();},
  28=>{let thread=f.page.topic.as_ref().map_or(0,|t|t.id);f.reply=None;f.focus=0;app.state.browser_scroll=0.;crate::feature_votes::start(app,serde_json::json!({"action":"forum_list","thread":thread,"category":id}));},
  _=>{}
 }
}
pub fn key(app:&mut App,key:&winit::keyboard::Key,ctrl:bool,alt:bool)->bool {
 if !app.state.feature_votes.community_forum||app.state.feature_votes.forum.focus==0{return false;}
 use winit::keyboard::{Key,NamedKey};
 if app.state.feature_votes.busy{return true;}
 let f=&mut app.state.feature_votes.forum;
 if matches!(key,Key::Named(NamedKey::Escape)){f.focus=0;}
 else if matches!(key,Key::Named(NamedKey::Tab)){f.focus=if f.editing_profile{3}else if f.new_thread&&f.focus==2{1}else{2};}
 else if matches!(key,Key::Named(NamedKey::Enter)) {if ctrl {let cmd=if f.editing_profile{25}else{15};act(app,0,cmd);}else if f.focus==1{f.focus=2;}else if f.focus==2&&f.body.len()<800{f.body.insert("\n");}}
 else {let max=if f.focus==3{32}else if f.focus==1{100}else{800};let field=if f.focus==3{&mut f.name}else if f.focus==1{&mut f.title}else{&mut f.body};let before=field.clone();field.key(key,ctrl,alt);if field.len()>max{*field=before;}}
 app.invalidate();true
}
const TOP:f32=304.;
pub fn max_scroll(s:&State)->f32 {(s.feature_votes.forum.height.get()-(s.screen.1-crate::views::browser::list_top(s)-28.-32.-TOP).max(0.)).max(0.)}
fn input(ui:&mut Ui,r:Rect,field:&crate::dialog::Field,focus:bool,placeholder:&str,cmd:i8) {
 ui.c.fill_rrect(r,5.,ui.t.s(0x172125));ui.c.stroke_rrect(r,5.,1.,if focus{ui.t.h(0xb5d98b)}else{ui.t.s(0x364447)});
 ui.hit(r,Action::FeatureVote(0,cmd),"Type here · Ctrl+Enter posts · Tab changes field · Escape leaves field");
 if cmd==14 {
  let value=if field.text.is_empty(){placeholder}else{&field.text};
  let lines=comment_lines(ui,value,(r.w-14.).max(20.));let before=comment_lines(ui,field.before_caret(),(r.w-14.).max(20.));
  let caret_row=before.len().saturating_sub(1);let first=if focus{caret_row.saturating_sub(2)}else{0};
  ui.c.push_clip(r);if focus&&field.all {ui.c.fill_rect(r,ui.t.accent.shade(-0.3));}
  for (row,line) in lines.iter().enumerate().skip(first).take(3){ui.c.text_clip(Font::new(12),r.x+7.,r.y+3.+(row-first) as f32*16.,r.w-14.,if field.text.is_empty(){ui.t.text_dim}else{ui.t.text},line);}
  if focus {let last=before.last().map(String::as_str).unwrap_or("");let cx=r.x+7.+ui.c.text_width(Font::new(12),last);let cy=r.y+3.+(caret_row-first) as f32*16.;ui.c.line(cx,cy,cx,cy+14.,1.,ui.t.text);}
  ui.c.pop_clip();return;
 }
 let value=if field.text.is_empty(){placeholder}else{&field.text};let caret=ui.c.text_width(Font::new(12),field.before_caret());let offset=if focus{(caret-r.w+16.).max(0.)}else{0.};
 ui.c.push_clip(r);if focus&&field.all{ui.c.fill_rect(r,ui.t.accent.shade(-0.3));}
 ui.c.text(Font::new(12),r.x+7.-offset,r.y+9.,if field.text.is_empty(){ui.t.text_dim}else{ui.t.text},value);
 if focus {let x=r.x+7.+caret-offset;ui.c.line(x,r.y+6.,x,r.bottom()-6.,1.,ui.t.text);}
 ui.c.pop_clip();
}
pub fn paint(ui:&mut Ui,s:&State,r:Rect) {
 let v=&s.feature_votes;let f=&v.forum;let p=&f.page;let x=r.x+9.;let w=r.w-18.;let enabled=v.online&&!v.busy;
 ui.c.fill_rect(r,ui.t.s(0x171f23));ui.hit(r,Action::BrowserList,"Community forum");ui.c.push_clip(r);
 ui.c.text(Font::new(21).bold(),x,r.y+8.,ui.t.text,"Community forum");
 ui.c.text_clip(Font::new(11),x,r.y+37.,w,ui.t.text_dim,"Share ideas. Ask questions. Make music together.");
 ui.c.text_clip(Font::new(10),x,r.y+55.,w,ui.t.h(0xb5d98b),&v.status);
 let bw=(w-18.)/4.;
 button(ui,Rect::new(x,r.y+76.,bw,25.),"Threads",Action::FeatureVote(0,11),!v.busy,false,"Back to all discussions");
 button(ui,Rect::new(x+bw+6.,r.y+76.,bw,25.),"New",Action::FeatureVote(0,10),!v.busy,true,"Start a new public discussion");
 button(ui,Rect::new(x+3.*(bw+6.),r.y+76.,bw,25.),"Refresh",Action::FeatureVote(0,0),!v.busy,false,"Refresh discussions");
 button(ui,Rect::new(x+2.*(bw+6.),r.y+76.,bw,25.),"Profile",Action::FeatureVote(0,22),!v.busy,false,"Choose your name and optional picture");
 if f.editing_profile {
  input(ui,Rect::new(x,r.y+116.,w,32.),&f.name,f.focus==3,"Your display name (2–32 characters)",26);
  avatar(ui,Rect::new(x,r.y+160.,32.,32.),&f.avatar,&f.name.text);
  button(ui,Rect::new(x+40.,r.y+162.,(w-46.)/2.,28.),"Picture…",Action::FeatureVote(0,23),!v.busy,false,"Choose PNG or JPEG; cropped to a small square");
  button(ui,Rect::new(x+46.+(w-46.)/2.,r.y+162.,(w-46.)/2.,28.),"Remove",Action::FeatureVote(0,24),!v.busy,false,"Use an initials avatar");
  text(ui,Rect::new(x,r.y+204.,w,45.),if f.avatar_job.is_some(){"Preparing picture…"}else{"Your name and picture are public. The same member ID stays with your posts."},12);
  button(ui,Rect::new(x,r.y+256.,w,28.),"Save profile",Action::FeatureVote(0,25),enabled&&f.avatar_job.is_none(),true,"Save your public profile");
  f.height.set(0.);ui.c.pop_clip();return;
 }
 let categories=p.topic.as_ref().is_some_and(|t|t.title=="Bugs, Fixes, Issues")&&!p.categories.is_empty();
 if f.new_thread {
  input(ui,Rect::new(x,r.y+110.,w,32.),&f.title,f.focus==1,"Thread title (4–100 characters)",13);
 }else if let Some(t)=&p.topic {text(ui,Rect::new(x,r.y+108.,w,39.),&t.title,14);}
 else {text(ui,Rect::new(x,r.y+111.,w,45.),"A space for the Lemon community. Posts are public; keep personal information private.",12);}
 if f.new_thread||(p.topic.as_ref().is_some_and(|t|t.locked==0)&&(!categories||p.category!=0)) {
  input(ui,Rect::new(x,r.y+152.,w,52.),&f.body,f.focus==2,"Write a message (up to 800 characters)",14);
  let context=if let Some(id)=f.reply {f.page.posts.iter().find(|p|p.id==id).map(|p|format!("Replying to {} · #{}",p.author,id)).unwrap_or_else(||format!("Replying to message #{id}"))}else if f.new_thread{"New discussion".into()}else{"Replying to this discussion".into()};
  ui.c.text_clip(Font::new(11).bold(),x,r.y+238.,w,ui.t.h(0xb5d98b),&context);
  let label=if f.new_thread{"Post thread"}else{"Post reply"};
  button(ui,Rect::new(x,r.y+210.,w*0.52,26.),label,Action::FeatureVote(0,15),enabled,true,"Publish your message · Ctrl+Enter");
  if let Some(id)=f.reply{button(ui,Rect::new(x+w*0.55,r.y+210.,w*0.45,26.),&format!("To #{id} ×"),Action::FeatureVote(0,21),true,false,"Cancel reply target");}
 } else if p.topic.is_some()&&!categories{ui.c.text(Font::new(12),x,r.y+161.,ui.t.text_dim,"Thread closed · messages remain readable");}
 if categories&&!f.new_thread {
  button(ui,Rect::new(x,r.y+112.,w,28.),"Bugs, Fixes, Issues · Categories",Action::FeatureVote(0,28),!v.busy,false,"Browse issue categories");
  if p.category==0{text(ui,Rect::new(x,r.y+154.,w,65.),"Choose a category to report an issue or reply to an existing report. General discussions stay in Threads.",12);}
  else if let Some(c)=p.categories.iter().find(|c|c.id==p.category){ui.c.text_clip(Font::new(11),x,r.y+250.,w,ui.t.text_dim,&c.name);}
 }
 if !f.new_thread {
  button(ui,Rect::new(x,r.y+271.,64.,23.),"Previous",Action::FeatureVote(0,19),!v.busy&&p.offset>0,false,"Previous page");
  button(ui,Rect::new(x+w-64.,r.y+271.,64.,23.),"Next",Action::FeatureVote(0,20),!v.busy&&p.offset+20<p.total,false,"Next page");
  let label=if p.total==0{"No posts yet".into()}else{format!("{}–{} / {}",p.offset+1,(p.offset+20).min(p.total),p.total)};
  ui.c.text_center(Font::new(10),Rect::new(x+66.,r.y+271.,(w-132.).max(0.),23.),ui.t.text_dim,&label);
 }
 let list=Rect::new(x,r.y+TOP,w,(r.h-TOP).max(0.));ui.c.push_clip(list);
 let mut hidden_depth=None;
 let mut cache=f.wrapped.borrow_mut();
 let layout=(w.to_bits(),ui.c.scale.to_bits());
 if cache.as_ref().is_none_or(|(width,scale,lines)|(*width,*scale)!=layout || lines.len()!=p.posts.len()){
  *cache=Some((layout.0,layout.1,p.posts.iter().map(|post|comment_lines(ui,&post.body,w-post.depth.min(2) as f32*10.-24.)).collect()));
 }
 let wrapped=&cache.as_ref().unwrap().2;
 let posts:Vec<_>=p.posts.iter().zip(wrapped).filter(|(post,_)|{if hidden_depth.is_some_and(|d|post.depth>d){return false;}hidden_depth=None;if f.collapsed.contains(&post.id){hidden_depth=Some(post.depth);}true}).collect();
 let height=if categories&&p.category==0&&!f.new_thread{p.categories.len() as f32*66.}else if f.new_thread{0.}else if p.topic.is_some(){posts.iter().map(|(post,lines)|if f.collapsed.contains(&post.id){48.}else{76.+lines.len() as f32*16.+if post.parent.is_some(){48.}else{0.}}).sum()}else{p.threads.len() as f32*104.};f.height.set(height);
 let scroll=s.browser_scroll.min((height-list.h).max(0.));let mut offset=0.;
 if categories&&p.category==0&&!f.new_thread {
  for c in &p.categories {let y=list.y+offset-scroll;offset+=66.;if y+66.<=list.y||y>=list.bottom(){continue;}
   button(ui,Rect::new(x,y,w,56.),&format!("{} · {}",c.name,c.posts),Action::FeatureVote(c.id,28),!v.busy&&y>=list.y&&y+56.<=list.bottom(),false,"Open category and report or discuss an issue");
  }
 }else if !f.new_thread&&p.topic.is_none() {
  if p.threads.is_empty(){text(ui,Rect::new(x+8.,list.y+12.,w-16.,52.),"No discussions yet. Start the first thread.",13);}
  for t in &p.threads {
   let y=list.y+offset-scroll;offset+=104.;if y+104.<=list.y||y>=list.bottom(){continue;}
   ui.c.fill_rrect(Rect::new(x,y,w,96.),7.,ui.t.s(0x273236));
   text(ui,Rect::new(x+10.,y+10.,w-20.,38.),&t.title,14);
   ui.c.text_clip(Font::new(10),x+10.,y+51.,w-20.,ui.t.text_dim,&format!("{} · {} messages{}",t.author,t.posts,if t.locked!=0{" · Closed"}else{""}));
   let b=Rect::new(x+9.,y+68.,w-18.,23.);button(ui,b,"Open discussion",Action::FeatureVote(t.id,12),!v.busy&&b.y>=list.y&&b.bottom()<=list.bottom(),false,"Read messages and reply");
  }
 }else if !f.new_thread {
  for (post,lines) in posts {
   let indent=post.depth.min(2) as f32*10.;let x=x+indent;let w=w-indent;
   let collapsed=f.collapsed.contains(&post.id);let height=if collapsed{48.}else{76.+lines.len() as f32*16.+if post.parent.is_some(){48.}else{0.}};let y=list.y+offset-scroll;offset+=height;if y+height<=list.y||y>=list.bottom(){continue;}
   ui.c.fill_rrect(Rect::new(x,y,w,height-8.),7.,ui.t.s(0x273236));
   ui.hit(Rect::new(x,y.max(list.y),w,((y+height-8.).min(list.bottom())-y.max(list.y)).max(0.)),Action::BrowserList,format!("{} · {} · message #{}{}",post.author,post.member,post.id,post.parent.map(|n|format!(" · reply to #{n}")).unwrap_or_default()));
   let date=chrono::DateTime::from_timestamp(post.created,0).map(|d|d.format("%d %b %H:%M UTC").to_string()).unwrap_or_default();
   avatar(ui,Rect::new(x+8.,y+7.,28.,28.),&post.avatar,&post.author);
   ui.c.text_clip(Font::new(10).bold(),x+43.,y+9.,w-85.,ui.t.h(0xb5d98b),&format!("{} · #{}{}",post.author,post.id,if post.completed!=0{" · Completed"}else{""}));
   ui.c.text_clip(Font::new(9),x+43.,y+24.,w-85.,ui.t.text_dim,&format!("{} · {date}{}",post.member,post.parent.map(|n|format!(" · reply to #{n}")).unwrap_or_default()));
   button(ui,Rect::new(x+w-34.,y+8.,26.,24.),if collapsed{"+"}else{"−"},Action::FeatureVote(post.id,27),y+8.>=list.y&&y+32.<=list.bottom(),false,"Collapse or expand this message and its replies");
   if collapsed{continue;}
   let mut cy=y+41.;
   if let Some(parent)=post.parent {
    ui.c.fill_rect(Rect::new(x+8.,cy,2.,40.),ui.t.h(0x8caa78));
    let who=if post.parent_author.is_empty(){format!("Reply to #{parent} (earlier / unavailable)")}else{format!("Reply to {} · #{parent}",post.parent_author)};
    ui.c.text_clip(Font::new(10).bold(),x+16.,cy,w-24.,ui.t.h(0xb5d98b),&who);
    let quote=post.parent_body.replace('\n'," ");
    for (i,line) in comment_lines(ui,&quote,w-26.).iter().take(2).enumerate(){ui.c.text_clip(Font::new(10),x+16.,cy+13.+i as f32*12.,w-24.,ui.t.text_dim,line);}
    cy+=48.;
   }
   for line in lines {ui.c.text_clip(Font::new(12),x+10.,cy,w-20.,ui.t.text,&line);cy+=16.;}
   let bw=(w-28.)/3.;let usable=enabled&&p.topic.as_ref().is_some_and(|t|t.locked==0)&&cy>=list.y&&cy+24.<=list.bottom();
   for (col,cmd,label,selected) in [(0,17,format!("Like {}",post.up),post.mine==1),(1,18,format!("Dislike {}",post.down),post.mine == -1),(2,16,"Reply".into(),false)] {
    button(ui,Rect::new(x+8.+col as f32*(bw+6.),cy,bw,24.),&label,Action::FeatureVote(post.id,cmd),usable,selected,"React once per message, or reply to this message");
   }
  }
 }
 ui.c.pop_clip();ui.c.pop_clip();
}

fn avatar(ui:&mut Ui,r:Rect,hex:&str,name:&str) {
 if !hex.is_empty()&&valid_avatar(hex) {let px:Vec<u32>=hex.as_bytes().chunks_exact(6).filter_map(|p|u32::from_str_radix(std::str::from_utf8(p).ok()?,16).ok()).collect();ui.c.image(r,32,32,&px);}
 else {ui.c.fill_rrect(r,6.,ui.t.s(0x41533e));let initials:String=name.split_whitespace().filter_map(|s|s.chars().next()).take(2).collect();ui.c.text_center(Font::new(12).bold(),r,ui.t.h(0xd5edb8),if initials.is_empty(){"?"}else{&initials});}
}
pub fn choose_avatar(app:&mut App,path:std::path::PathBuf) {
 let job=std::sync::Arc::new(std::sync::Mutex::new(None));app.state.feature_votes.forum.avatar_job=Some(job.clone());
 std::thread::spawn(move||{let result=(||{
  use std::io::Read;
  let file=std::fs::File::open(path).map_err(|_|"Cannot open picture")?;let mut bytes=Vec::new();file.take(8*1024*1024+1).read_to_end(&mut bytes).map_err(|_|"Cannot read picture")?;
  if bytes.len()>8*1024*1024{return Err("Choose a PNG or JPEG smaller than 8 MB".to_string());}
  let (w,h,px)=crate::wallpaper::decode(&bytes).ok_or("Cannot decode picture; choose PNG or JPEG")?;
  let small=crate::wallpaper::cover(w,h,&px,32,32);Ok(small.iter().map(|p|format!("{:06x}",p&0xffffff)).collect())
 })();if let Ok(mut out)=job.lock(){*out=Some(result);}});app.invalidate();
}
pub fn tick(app:&mut App) {
 let result=app.state.feature_votes.forum.avatar_job.as_ref().and_then(|j|j.lock().ok()?.take());
 if let Some(result)=result {app.state.feature_votes.forum.avatar_job=None;match result{Ok(p)=>app.state.feature_votes.forum.avatar=p,Err(e)=>app.state.feature_votes.status=e}app.invalidate();}
}

#[cfg(test)] mod tests {
 use super::*;
 #[test] fn community_update_forum_profile_reply_and_bounds(){
  let page:Page=serde_json::from_value(serde_json::json!({"threads":[],"posts":[{"id":7,"thread":2,"parent":null,"author":"Test Musician","member":"Member ABC123","avatar":"","body":"A community message. ".repeat(10),"created":1793577600,"up":2,"down":1,"mine":1}],"topic":{"id":2,"title":"Making music together","author":"Test Musician","created":1793577600,"updated":1793577600,"locked":0,"posts":1},"offset":0,"total":1})).unwrap();assert!(valid(&page));
  let mut page=page;let mut reply=page.posts[0].clone();reply.id=8;reply.parent=Some(7);reply.depth=1;reply.parent_author="Test Musician".into();reply.parent_body="Original report".into();reply.body="A reply under its original report".into();page.posts.push(reply);page.total=2;
  let mut app=App::new(None);app.state.show_browser=true;app.state.browser_tab=6;app.state.feature_votes.community_forum=true;app.state.feature_votes.online=true;app.state.feature_votes.forum.page=page;
  act(&mut app,7,16);assert_eq!(app.state.feature_votes.forum.reply,Some(7));assert_eq!(app.state.feature_votes.forum.focus,2);
  let before=app.state.project.clone();assert!(crate::feature_votes::key(&mut app,&winit::keyboard::Key::Character("Hello everyone".into()),false,false));assert_eq!(app.state.project,before);assert_eq!(app.state.feature_votes.forum.body.text,"Hello everyone");assert!(!app.typing_key(winit::keyboard::KeyCode::KeyA,true,false,false));
  for (w,h) in [(1000,600),(1280,900)]{let mut canvas=os_ui::Canvas::new(w,h,1.).unwrap();app.state.browser_scroll=0.;app.paint(&mut canvas,None);assert!(app.hits.iter().any(|h|h.action==Action::FeatureVote(0,6)));assert!(app.hits.iter().any(|h|h.action==Action::FeatureVote(1,6)));app.state.browser_scroll=max_scroll(&app.state);app.paint(&mut canvas,None);assert!(app.hits.iter().filter(|h|matches!(h.action,Action::FeatureVote(_,16..=18))).all(|hit|hit.rect.bottom()<=h as f32-28.));if let Some(dir)=std::env::var_os("OS_SHOT_DIR"){canvas.pixmap.save_png(std::path::PathBuf::from(dir).join(format!("forum-{h}.png"))).unwrap();}}
  act(&mut app,7,27);assert!(app.state.feature_votes.forum.collapsed.contains(&7));
  let mut c=os_ui::Canvas::new(1280,900,1.).unwrap();app.state.browser_scroll=0.;app.paint(&mut c,None);assert_eq!(app.state.feature_votes.forum.height.get(),48.);assert!(!app.hits.iter().any(|h|matches!(h.action,Action::FeatureVote(8,16..=18))));
  act(&mut app,7,27);app.paint(&mut c,None);assert!(app.state.feature_votes.forum.height.get()>48.);
  act(&mut app,0,22);assert!(app.state.feature_votes.forum.editing_profile);assert_eq!(app.state.feature_votes.forum.focus,3);
  if let Some(dir)=std::env::var_os("OS_SHOT_DIR"){let mut canvas=os_ui::Canvas::new(1000,600,1.).unwrap();app.paint(&mut canvas,None);canvas.pixmap.save_png(std::path::PathBuf::from(dir).join("forum-profile.png")).unwrap();}
  let picture=std::env::temp_dir().join(format!("lemon-avatar-{}.png",std::process::id()));let mut pix=tiny_skia::Pixmap::new(64,32).unwrap();pix.fill(tiny_skia::Color::from_rgba8(120,50,30,255));pix.save_png(&picture).unwrap();choose_avatar(&mut app,picture.clone());let start=std::time::Instant::now();while app.state.feature_votes.forum.avatar_job.is_some(){tick(&mut app);assert!(start.elapsed().as_secs()<5);std::thread::sleep(std::time::Duration::from_millis(5));}assert_eq!(app.state.feature_votes.forum.avatar.len(),6144);assert!(valid_avatar(&app.state.feature_votes.forum.avatar));std::fs::remove_file(picture).unwrap();
  assert!(valid_avatar(&"abcdef".repeat(1024)));assert!(!valid_avatar("not-an-image"));
  let mut bad=app.state.feature_votes.forum.page.clone();bad.posts[0].body="x".repeat(801);assert!(!valid(&bad));
  app.state.feature_votes.online=false;act(&mut app,0,15);assert!(!app.state.feature_votes.busy);
 }
}
