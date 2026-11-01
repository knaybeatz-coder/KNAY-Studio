// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared feature board. Networking runs off the UI/audio threads; offline is an ordinary state.
use std::{io::{Read,Write},sync::{Mutex,OnceLock},time::{Duration,Instant}};
use serde::{Serialize,Deserialize};
use sha2::{Sha256,Digest};
use os_ui::{Rect,Font};
use crate::app::{App,State,Ui,Action};
pub const TAB:usize=6;
pub const ROW_H:f32=204.;
pub const HEADER:f32=208.;
const URL:&str="https://michaellanemusic.de/lemon-studio/features/api.php";
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Comment {pub id:u64,pub body:String,pub created:i64,#[serde(default)]pub edited:i64}
fn open_status()->String {"Open".into()}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Item {pub id:u64,pub title:String,pub up:u64,pub down:u64,pub mine:i8,#[serde(default)] pub locked:u8,#[serde(default="open_status")]pub status:String,#[serde(default)]pub comments:Vec<Comment>}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Announcement {pub id:u64,pub title:String,pub body:String,pub created:i64,#[serde(default)]pub edited:i64}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Board {pub items:Vec<Item>,#[serde(default)]pub announcements:Vec<Announcement>}
#[derive(Clone,Debug)]
pub struct View {pub board:Board,pub draft:crate::dialog::Field,pub focus:bool,pub online:bool,pub busy:bool,pub status:String,pub initialized:bool,pub last:Option<Instant>,pub expanded:Option<u64>,pub expanded_height:std::cell::Cell<f32>,pub announcements_height:std::cell::Cell<f32>}
impl Default for View {fn default()->Self {Self{board:Board{announcements:Vec::new(),items:["Track freeze and unfreeze to save CPU","ARA integration for compatible audio editors","Live collaboration on projects"].iter().enumerate().map(|(i,t)|Item{id:i as u64+1,title:(*t).into(),up:0,down:0,mine:0,locked:0,status:open_status(),comments:Vec::new()}).collect()},draft:Default::default(),focus:false,online:false,busy:false,status:"Starter ideas — connect for live votes.".into(),initialized:false,last:None,expanded:None,expanded_height:std::cell::Cell::new(0.),announcements_height:std::cell::Cell::new(0.)}}}
type ResultBoard=Result<Board,(String,bool)>;
#[derive(Default)]
struct Job {result:Option<(ResultBoard,bool)>,busy:bool}
fn job()->&'static Mutex<Job>{static J:OnceLock<Mutex<Job>>=OnceLock::new();J.get_or_init(Default::default)}
fn request(url:&str,token:&str,body:serde_json::Value)->Result<serde_json::Value,(String,bool)>{
 let agent=ureq::AgentBuilder::new().redirects(0).timeout(Duration::from_secs(8)).build();
 let r=agent.post(url).set("Content-Type","application/json").set("X-Lemon-Voter",token).send_string(&body.to_string());
 let (response,ok)=match r {Ok(r)=>(r,true),Err(ureq::Error::Status(code,r)) if code<500=>(r,false),Err(ureq::Error::Status(_, _))=>return Err(("Offline / unavailable — showing saved ideas.".into(),false)),Err(_)=>return Err(("Offline / unavailable — showing saved ideas.".into(),false))};
 let mut bytes=Vec::new();response.into_reader().take(16777217).read_to_end(&mut bytes).map_err(|_|("Connection interrupted — retry when online.".into(),false))?;
 if bytes.len()>16777216{return Err(("Board unavailable — please try later.".into(),false));}
 let value:serde_json::Value=serde_json::from_slice(&bytes).map_err(|_|("Board unavailable — please try later.".into(),false))?;
 if !ok {let message=value["error"].as_str().unwrap_or("Board unavailable — please try later.").chars().take(160).collect();return Err((message,true));}Ok(value)
}
fn valid(board:&Board)->bool {board.announcements.len()<=50 && board.announcements.iter().all(|a|a.id>0&&(1..=120).contains(&a.title.chars().count())&&!a.title.chars().any(char::is_control)&&(1..=2000).contains(&a.body.chars().count())&&!a.body.chars().any(|c|c.is_control()&&c!='\n'&&c!='\t')) && board.items.len()<=500 && board.items.iter().all(|i|i.id>0&&i.title.chars().count()<=120&&!i.title.chars().any(char::is_control)&&(-1..=1).contains(&i.mine)&&i.locked<=1&&["Open","Planned","In progress","Completed","Declined"].contains(&i.status.as_str())&&i.comments.len()<=10&&i.comments.iter().all(|c|c.id>0&&c.body.chars().count()<=600&&!c.body.chars().any(|ch|ch.is_control()&&ch!='\n'&&ch!='\t')))}
fn solve(challenge:&str)->Option<String>{
 let deadline=Instant::now()+Duration::from_secs(10);
 for n in 0..10_000_000u64 {if n%4096==0 && Instant::now()>deadline{return None;}
  let digest=Sha256::digest(format!("{challenge}:{n}").as_bytes());if digest[0]==0&&digest[1]==0{return Some(n.to_string());}
 }None
}
fn token_path()->Result<std::path::PathBuf,(String,bool)>{os_paths::user_dir().map(|p|p.join("feature-voter.key")).ok_or(("Cannot save voter identity. Check your user folder.".into(),true))}
fn read_token()->Result<String,(String,bool)>{
 let path=token_path()?;match std::fs::read_to_string(path){
  Ok(t) if t.len()==64&&t.bytes().all(|b|b.is_ascii_hexdigit())=>Ok(t),
  Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(String::new()),
  _=>Err(("Cannot read voter identity. Contact support.".into(),true))
 }
}
fn save_token(token:&str)->Result<String,(String,bool)>{
 let path=token_path()?;
 std::fs::create_dir_all(path.parent().unwrap()).map_err(|_|("Cannot save voter identity.".into(),true))?;
 let mut options=std::fs::OpenOptions::new();options.write(true).create_new(true);
 #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
 match options.open(path){Ok(mut file)=>{file.write_all(token.as_bytes()).and_then(|_|file.sync_all()).map_err(|_|("Cannot save voter identity.".into(),true))?;Ok(token.into())},Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>read_token(),Err(_)=>Err(("Cannot save voter identity.".into(),true))}
}
fn start(app:&mut App,body:serde_json::Value){
 let mut j=job().lock().unwrap();if j.busy{return;}j.busy=true;drop(j);
 app.state.feature_votes.busy=true;app.state.feature_votes.last=Some(Instant::now());
 app.state.feature_votes.status="Connecting…".into();app.invalidate();
 std::thread::spawn(move||{
  let submit=body["action"]=="suggest";
  let result=(||{
   let mut token=read_token()?;
   if body["action"]!="list" && token.is_empty(){
    let challenge=request(URL,"",serde_json::json!({"action":"challenge"}))?;
    let challenge=challenge["challenge"].as_str().filter(|s|s.len()<140).ok_or(("Connection unavailable.".into(),false))?;
    let nonce=solve(challenge).ok_or(("Please retry the connection.".into(),false))?;
    let registered=request(URL,"",serde_json::json!({"action":"register","challenge":challenge,"nonce":nonce}))?;
    token=registered["token"].as_str().filter(|s|s.len()==64&&s.bytes().all(|b|b.is_ascii_hexdigit())).ok_or(("Connection unavailable.".into(),false))?.into();
    token=save_token(&token)?;
   }
   let data=request(URL,&token,body)?;
   let board:Board=serde_json::from_value(data).map_err(|_|("Board unavailable.".into(),false))?;
   if !valid(&board){return Err(("Board unavailable.".into(),false));}Ok(board)
  })();
  job().lock().unwrap().result=Some((result,submit));
 });
}
pub fn tick(app:&mut App){
 let result={let mut j=job().lock().unwrap();let r=j.result.take();if r.is_some(){j.busy=false;}r};
 if let Some((result,submit))=result {
  let v=&mut app.state.feature_votes;v.busy=false;
  match result {Ok(board)=>{crate::prefs::set_text("feature_board_cache",&serde_json::to_string(&board).unwrap());v.board=board;v.online=true;v.status=if submit {v.draft.set("");"Suggestion added."}else{"Live votes • one vote per suggestion."}.into();},Err((message,online))=>{v.online=online;v.status=message;}}
  app.invalidate();
 }
 if app.state.show_browser && app.state.browser_tab==TAB && !app.state.browser_auto_hidden {
  if !app.state.feature_votes.initialized {
   let v=&mut app.state.feature_votes;v.initialized=true;
   if let Some(b)=crate::prefs::text("feature_board_cache").and_then(|s|serde_json::from_str::<Board>(&s).ok()).filter(valid){v.board=b;}
  }
  if !app.state.feature_votes.busy && app.state.feature_votes.last.is_none_or(|t|t.elapsed()>Duration::from_secs(60)){start(app,serde_json::json!({"action":"list"}));}
 }
}
pub fn act(app:&mut App,id:u64,command:i8){
 if command==4 {
  let v=&mut app.state.feature_votes;
  if v.board.items.iter().any(|i|i.id==id&&!i.comments.is_empty()){v.expanded=if v.expanded==Some(id){None}else{Some(id)};v.expanded_height.set(0.);app.invalidate();}
  return;
 }
 if command==3 {app.state.feature_votes.focus=true;app.state.browser_search_focus=false;app.invalidate();return;}
 if app.state.feature_votes.busy{return;}
 if command==0 {if app.state.feature_votes.last.is_none_or(|t|t.elapsed()>Duration::from_secs(2)){start(app,serde_json::json!({"action":"list"}));}return;}
 if !app.state.feature_votes.online{return;}
 if command==2 {let title=app.state.feature_votes.draft.text.trim().to_string();if !(8..=120).contains(&title.chars().count()){app.state.feature_votes.status="Describe your idea in 8–120 characters.".into();app.invalidate();return;}start(app,serde_json::json!({"action":"suggest","title":title}));}
 else if [-1,1].contains(&command) && app.state.feature_votes.board.items.iter().any(|i|i.id==id&&i.mine!=command&&i.locked==0){start(app,serde_json::json!({"action":"vote","id":id,"value":command}));}
}
pub fn key(app:&mut App,key:&winit::keyboard::Key,ctrl:bool,alt:bool)->bool{
 if !app.state.feature_votes.focus||app.state.browser_tab!=TAB||!app.state.show_browser{return false;}
 use winit::keyboard::{Key,NamedKey};
 if matches!(key,Key::Named(NamedKey::Escape|NamedKey::Tab)){app.state.feature_votes.focus=false;}
 else if matches!(key,Key::Named(NamedKey::Enter)){act(app,0,2);}
 else {let field=&mut app.state.feature_votes.draft;let old=field.clone();field.key(key,ctrl,alt);if field.len()>120 {*field=old;}}
 app.invalidate();true
}
pub fn items(s:&State)->Vec<&Item>{let q=s.browser_search.to_lowercase();s.feature_votes.board.items.iter().filter(|i|i.title.to_lowercase().contains(&q)).collect()}
pub fn max_scroll(s:&State)->f32 {(s.feature_votes.announcements_height.get()+items(s).len() as f32*ROW_H+if items(s).iter().any(|i|Some(i.id)==s.feature_votes.expanded){s.feature_votes.expanded_height.get()}else{0.}-(s.screen.1-crate::views::browser::list_top(s)-28.-HEADER).max(0.)).max(0.)}
fn comment_lines(ui:&mut Ui,value:&str,width:f32)->Vec<String>{
 let mut out=Vec::new();
 for paragraph in value.lines(){
  let mut line=String::new();
  for word in paragraph.split_whitespace(){
   let candidate=if line.is_empty(){word.to_string()}else{format!("{line} {word}")};
   if ui.c.text_width(Font::new(12),&candidate)<=width {line=candidate;continue;}
   if !line.is_empty(){out.push(std::mem::take(&mut line));}
   for ch in word.chars(){let candidate=format!("{line}{ch}");if !line.is_empty()&&ui.c.text_width(Font::new(12),&candidate)>width {out.push(std::mem::take(&mut line));}line.push(ch);}
  }
  out.push(line);
 }
 out
}
fn text(ui:&mut Ui,r:Rect,value:&str,size:u16){
 let font=Font::new(size);let mut line=String::new();let mut y=r.y;
 for word in value.split_whitespace(){let next=if line.is_empty(){word.into()}else{format!("{line} {word}")};if !line.is_empty() && ui.c.text_width(font,&next)>r.w {ui.c.text_clip(font,r.x,y,r.w,ui.t.text,&line);y+=16.;line=word.into();}else{line=next;}if y+14.>r.bottom(){return;}}
 ui.c.text_clip(font,r.x,y,r.w,ui.t.text,&line);
}
// Flat vector surfaces only: no textures, blur, animation timers or audio-thread work.
fn button(ui:&mut Ui,r:Rect,label:&str,action:Action,enabled:bool,primary:bool,tip:&str){
 let hover=enabled && ui.hit(r,action,tip.to_string());
 let bg=if primary&&enabled {ui.t.h(0xb5d98b)}else if hover {ui.t.s(0x3e4c4a)}else{ui.t.s(0x293635)};
 ui.c.fill_rrect(r,5.,bg);
 ui.c.text_center(Font::new(12).bold(),r,if primary&&enabled{ui.t.s(0x19251b)}else if enabled{ui.t.text}else{ui.t.text_dim},label);
}
fn vote_button(ui:&mut Ui,r:Rect,id:u64,dir:i8,count:u64,enabled:bool,selected:bool){
 let accent=if dir==1 {ui.t.h(0xa7d994)}else{ui.t.h(0xe4b583)};
 let tip=if dir==1 {"Upvote: support this idea. Changing direction replaces your vote."}else{"Downvote: lower this idea's priority. Changing direction replaces your vote."};
 let hover=enabled&&!selected&&ui.hit(r,Action::FeatureVote(id,dir),tip);
 ui.c.fill_rrect(r,5.,if selected {if dir==1 {ui.t.s(0x314b36)}else{ui.t.s(0x493b2c)}}else if hover{ui.t.s(0x34423f)}else{ui.t.s(0x202b2d)});
 ui.c.stroke_rrect(r,5.,1.,if selected {accent}else{ui.t.s(0x3a494a)});
 let ink=if selected||enabled{accent}else{ui.t.text_dim};
 let cx=r.x+11.;let cy=r.cy();let d=if dir==1 {-1.}else{1.};
 ui.c.line(cx-3.,cy-d*2.,cx,cy+d*2.,1.5,ink);ui.c.line(cx,cy+d*2.,cx+3.,cy-d*2.,1.5,ink);
 ui.c.text(Font::new(11).bold(),r.x+20.,r.y+8.,ink,if dir==1 {"UP"}else{"DOWN"});
 let count=if count>=10000{format!("{:.0}k",count as f64/1000.)}else{count.to_string()};
 let cw=ui.c.text_width(Font::new(14).bold(),&count);
 ui.c.text(Font::new(14).bold(),r.right()-cw-8.,r.y+6.,if enabled||selected{ui.t.text}else{ui.t.text_dim},&count);
}
pub fn paint(ui:&mut Ui,s:&State,r:Rect){
 let v=&s.feature_votes;let x=r.x+9.;let w=r.w-18.;let t=ui.t;
 ui.c.fill_rect(r,t.s(0x171f23));ui.hit(r,Action::BrowserList,"Feature voting");ui.c.push_clip(r);
 // Compact editorial header, with connection details available on hover.
 ui.c.text(Font::new(10).bold(),x,r.y+9.,t.h(0xb5d98b),"LEMON COMMUNITY");
 ui.c.text(Font::new(24).bold(),x,r.y+25.,t.text,"What's next?");
 let routine=v.status.starts_with("Live votes")||v.status.starts_with("Starter ideas")||v.status.starts_with("Connecting")||v.status.starts_with("Offline / unavailable");
 let subtitle=if v.status=="Suggestion added." {"Thanks! Your idea is on the board."}else if !routine {&v.status}else if v.online {"Your ideas. A better Lemon Studio."}else{"Saved ideas. Reconnect to vote."};
 ui.c.text_clip(Font::new(12),x,r.y+55.,w,if routine{t.text_dim}else{t.h(0xe4b583)},subtitle);
 let status=Rect::new(x,r.y+77.,w-77.,21.);
 ui.c.fill_rrect(status,10.,t.s(0x263330));
 let color=if v.online {t.h(0xa7d994)}else{t.h(0xe4b583)};
 ui.c.fill_circle(status.x+9.,status.cy(),2.5,color);
 ui.c.text(Font::new(10).bold(),status.x+17.,status.y+4.,color,if v.busy{"CONNECTING"}else if v.online{"LIVE BOARD"}else{"OFFLINE"});
 ui.hit(status,Action::BrowserList,v.status.clone());
 button(ui,Rect::new(x+w-68.,r.y+77.,68.,21.),"Refresh",Action::FeatureVote(0,0),!v.busy,false,"Refresh shared votes");
 let composer=Rect::new(x,r.y+109.,w,69.);ui.c.fill_rrect(composer,7.,t.s(0x243034));
 ui.c.stroke_rrect(composer,7.,1.,t.s(0x364447));
 let field=Rect::new(x+6.,r.y+115.,w-12.,25.);
 ui.c.fill_rrect(field,4.,t.s(0x172125));
 if v.focus {ui.c.stroke_rrect(field,4.,1.,t.h(0xb5d98b));}
 ui.hit(field,Action::FeatureVote(0,3),"Suggest a feature in 8–120 characters. Enter submits; Escape leaves the field.");
 let value=if v.draft.text.is_empty(){"What would you love to add?"}else{&v.draft.text};
 ui.c.push_clip(field);
 let caret=ui.c.text_width(Font::new(13),v.draft.before_caret());let offset=if v.focus {(caret-field.w+12.).max(0.)}else{0.};
 if v.focus&&v.draft.all {ui.c.fill_rect(Rect::new(field.x+3.,field.y+2.,field.w-6.,field.h-4.),t.accent.shade(-0.3));}
 ui.c.text(Font::new(13),field.x+5.-offset,field.y+4.,if v.draft.text.is_empty(){t.text_dim}else{t.text},value);
 if v.focus {let cx=field.x+5.+caret-offset;ui.c.line(cx,field.y+4.,cx,field.bottom()-4.,1.,t.text);}
 ui.c.pop_clip();
 button(ui,Rect::new(x+6.,r.y+146.,w-12.,25.),if v.online{"+  Share your idea"}else{"Connect to share an idea"},Action::FeatureVote(0,2),v.online&&!v.busy,true,"Submit a public suggestion. No personal information or links.");
 ui.c.text(Font::new(10).bold(),x,r.y+189.,t.text_dim,if v.board.announcements.is_empty(){"COMMUNITY IDEAS"}else{"ANNOUNCEMENTS & IDEAS"});
 let filtered=items(s);let count=filtered.len().to_string();let cw=ui.c.text_width(Font::new(11),&count);ui.c.text(Font::new(11),x+w-cw,r.y+188.,t.text_dim,&count);
 let list=Rect::new(x,r.y+HEADER,w,(r.h-HEADER).max(0.));ui.c.push_clip(list);
 let expanded=filtered.iter().find(|i|Some(i.id)==v.expanded);
 let comment_rows:Vec<_>=expanded.into_iter().flat_map(|i|&i.comments).map(|c|(c,comment_lines(ui,&c.body,(w-24.).max(20.)))).collect();
 let extra=comment_rows.iter().map(|(_,lines)|32.+lines.len() as f32*16.).sum::<f32>();
 v.expanded_height.set(extra);
 let news:Vec<_>=v.board.announcements.iter().map(|a|(a,comment_lines(ui,&a.title,(w-24.).max(20.)),comment_lines(ui,&a.body,(w-24.).max(20.)))).collect();
 let news_height=news.iter().map(|(_,title,body)|58.+(title.len()+body.len()) as f32*16.).sum::<f32>();
 v.announcements_height.set(news_height);
 let scroll=s.browser_scroll.min((news_height+filtered.len() as f32*ROW_H+extra-list.h).max(0.));
 if filtered.is_empty(){text(ui,Rect::new(x+8.,list.y+news_height-scroll+16.,w-16.,64.),"No matching ideas. Try another search, or share your own.",13);}
 let mut offset=0.;
 for (announcement,title,body) in news {
  let height=58.+(title.len()+body.len()) as f32*16.;let y=list.y+offset-scroll;offset+=height;
  if y+height<=list.y||y>=list.bottom(){continue;}
  ui.c.fill_rrect(Rect::new(x,y,w,height-10.),7.,t.s(0x2c3b31));
  let date=chrono::DateTime::from_timestamp(announcement.created,0).map(|d|d.format("%d %b %Y").to_string()).unwrap_or_default();
  ui.c.text_clip(Font::new(10).bold(),x+12.,y+10.,w-24.,t.h(0xb5d98b),&format!("MICHAEL · {date}"));
  let mut cy=y+29.;
  for line in title {ui.c.text_clip(Font::new(12),x+12.,cy,w-24.,t.h(0xd5edb8),&line);cy+=16.;}
  cy+=7.;
  for line in body {ui.c.text_clip(Font::new(12),x+12.,cy,w-24.,t.text,&line);cy+=16.;}
 }
 for item in filtered {let height=ROW_H+if Some(item.id)==v.expanded{extra}else{0.};let y=list.y+offset-scroll;offset+=height;if y+height<=list.y||y>=list.bottom(){continue;}
  let card=Rect::new(x,y,w,height-10.);ui.c.fill_rrect(Rect::new(x,y+2.,w,card.h),8.,t.s(0x11191c));ui.c.fill_rrect(card,8.,t.s(0x273236));
  ui.c.stroke_rrect(card,8.,1.,if item.mine==1 {t.s(0x50604c)}else if item.mine == -1 {t.s(0x66533e)}else{t.s(0x39464a)});
  ui.c.text(Font::new(10).bold(),x+10.,y+9.,t.h(0xb5c7b0),&format!("IDEA {:02}",item.id));
  let votes=item.up.saturating_add(item.down);let label=format!("{votes} vote{}",if votes==1{""}else{"s"});let tw=ui.c.text_width(Font::new(10),&label);ui.c.text(Font::new(10),x+w-10.-tw,y+9.,t.text_dim,&label);
  let visible_top=y.max(list.y);let visible_bottom=card.bottom().min(list.bottom());
  ui.hit(Rect::new(x,visible_top,w,(visible_bottom-visible_top).max(0.)),Action::BrowserList,item.title.clone());
  let status_color=match item.status.as_str(){"Completed"=>t.h(0xb5d98b),"In progress"=>t.h(0xf0ce87),"Declined"=>t.h(0xe4a89b),_=>t.h(0xb3cad8)};
  ui.c.text(Font::new(10).bold(),x+10.,y+29.,status_color,&item.status.to_uppercase());
  text(ui,Rect::new(x+10.,y+48.,w-20.,53.),&item.title,14);
  let half=(w-26.)/2.;let enabled=v.online&&!v.busy&&item.locked==0&&y+105.>=list.y&&y+136.<=list.bottom();
  vote_button(ui,Rect::new(x+9.,y+105.,half,31.),item.id,1,item.up,enabled,item.mine==1);
  vote_button(ui,Rect::new(x+17.+half,y+105.,half,31.),item.id,-1,item.down,enabled,item.mine == -1);
  ui.c.text(Font::new(10),x+10.,y+142.,if item.mine!=0{t.h(0xb5d98b)}else{t.text_dim},if item.locked!=0 {"Voting closed"}else{match item.mine{1=>"You upvoted this idea",-1=>"You downvoted this idea",_=>"One voice. One vote per idea."}});
  let comments_button=Rect::new(x+9.,y+164.,w-18.,23.);
  if item.comments.is_empty(){ui.c.text(Font::new(10),x+10.,y+169.,t.text_dim,"No comments from Michael yet");}
  else {button(ui,comments_button,&format!("{} ({})",if Some(item.id)==v.expanded{"Hide comments"}else{"Comments from Michael"},item.comments.len()),Action::FeatureVote(item.id,4),comments_button.y>=list.y&&comments_button.bottom()<=list.bottom(),false,"Read public updates from Michael / Lemon Studio");}
  if Some(item.id)==v.expanded {
   let mut cy=y+ROW_H;
   for (comment,lines) in &comment_rows {
    let date=chrono::DateTime::from_timestamp(comment.created,0).map(|d|d.format("%d %b %Y").to_string()).unwrap_or_default();
    ui.c.text_clip(Font::new(10).bold(),x+12.,cy,w-24.,t.h(0xb5d98b),&format!("Michael · {date}{}",if comment.edited>comment.created{" · edited"}else{""}));
    cy+=19.;
    for line in lines {ui.c.text_clip(Font::new(12),x+12.,cy,w-24.,t.text,line);cy+=16.;}
    cy+=13.;
   }
  }
 }
 ui.c.pop_clip();ui.c.pop_clip();
}
#[cfg(test)] mod tests {use super::*;
 #[test]fn voting_validation_and_offline_input(){
  let mut a=App::new(None);a.state.browser_tab=TAB;a.state.feature_votes.focus=true;
  assert!(key(&mut a,&winit::keyboard::Key::Character("Freeze tracks".into()),false,false));
  assert_eq!(a.state.feature_votes.draft.text,"Freeze tracks");act(&mut a,0,2);assert!(!a.state.feature_votes.busy);assert_eq!(a.state.feature_votes.draft.text,"Freeze tracks");
  assert!(valid(&a.state.feature_votes.board));a.state.feature_votes.board.items[0].mine=2;assert!(!valid(&a.state.feature_votes.board));
 }
 #[test]fn public_status_and_comments_survive_cache_and_expand(){
  let old:Board=serde_json::from_str(r#"{"items":[{"id":1,"title":"Existing idea","up":2,"down":0,"mine":0}]}"#).unwrap();
  assert!(valid(&old));assert_eq!(old.items[0].status,"Open");assert!(old.items[0].comments.is_empty());
  let mut a=App::new(None);a.state.show_browser=true;a.state.browser_tab=TAB;a.state.screen=(1000.,600.);
  a.state.feature_votes.board=old;
  a.state.feature_votes.board.items[0].status="In progress".into();
  a.state.feature_votes.board.items[0].comments=vec![Comment{id:1,created:1793577600,edited:1793577601,body:"A public update.\n".to_string()+&"Long text with Unicode 🎵 and <markup> kept as plain text. ".repeat(8)}];
  assert!(valid(&a.state.feature_votes.board));
  let saved=serde_json::to_string(&a.state.feature_votes.board).unwrap();let back:Board=serde_json::from_str(&saved).unwrap();assert_eq!(back.items[0].comments[0].body,a.state.feature_votes.board.items[0].comments[0].body);
  let mut c=os_ui::Canvas::new(1000,600,1.).unwrap();a.paint(&mut c,None);
  assert!(a.hits.iter().any(|h|h.action==Action::FeatureVote(1,4)),"comments remain readable offline");
  act(&mut a,1,4);a.paint(&mut c,None);assert!(max_scroll(&a.state)>0.);assert!(a.state.feature_votes.expanded_height.get()>100.);
  if let Some(dir)=std::env::var_os("OS_SHOT_DIR"){c.pixmap.save_png(std::path::PathBuf::from(dir).join("feature-comments.png")).unwrap();}
  a.state.browser_scroll=max_scroll(&a.state);a.paint(&mut c,None);
  assert!(a.hits.iter().filter(|h|matches!(h.action,Action::FeatureVote(_,4))).all(|h|h.rect.bottom()<=572.));
  act(&mut a,1,4);a.paint(&mut c,None);assert_eq!(a.state.feature_votes.expanded_height.get(),0.);
  a.state.feature_votes.board.items[0].comments[0].body="x".repeat(601);assert!(!valid(&a.state.feature_votes.board));
 }
 #[test]fn announcements_are_cached_bounded_and_scroll_above_ideas(){
  let mut a=App::new(None);a.state.show_browser=true;a.state.browser_tab=TAB;
  a.state.feature_votes.board.announcements=vec![Announcement{id:1,title:"Audio engine update".into(),body:"New monitoring fixes.\nRead this offline too. 🎵 ".repeat(12),created:1793577600,edited:0}];
  assert!(valid(&a.state.feature_votes.board));
  let b:Board=serde_json::from_str(&serde_json::to_string(&a.state.feature_votes.board).unwrap()).unwrap();assert_eq!(b.announcements[0].title,"Audio engine update");
  for (w,h) in [(1000,600),(1600,900)] {
   let mut c=os_ui::Canvas::new(w,h,1.).unwrap();a.state.browser_scroll=0.;a.paint(&mut c,None);
   assert!(a.state.feature_votes.announcements_height.get()>200.);assert!(max_scroll(&a.state)>0.);
   if let Some(dir)=std::env::var_os("OS_SHOT_DIR"){c.pixmap.save_png(std::path::PathBuf::from(dir).join(format!("announcements-{w}.png"))).unwrap();}
   a.state.feature_votes.online=true;a.state.browser_scroll=max_scroll(&a.state);a.paint(&mut c,None);
   assert!(a.hits.iter().any(|h|matches!(h.action,Action::FeatureVote(_,1|-1))));
  }
  a.state.feature_votes.board.announcements[0].body="x".repeat(2001);assert!(!valid(&a.state.feature_votes.board));
 }
 #[test]fn quiet_offline_request(){let r=request("http://127.0.0.1:1","",serde_json::json!({"action":"list"}));assert!(matches!(r,Err((_,false))));}

 #[test]fn feature_board_browser_controls(){
  crate::prefs::set_slider(&crate::prefs::SMOOTHING,0);
  let mut a=App::new(None);a.state.show_browser=true;a.browser_act(&Action::BrowserTab(TAB));a.state.feature_votes.online=true;
  for (width,height) in [(1280,900),(1000,600)]{
   a.state.screen=(width as f32,height as f32);a.state.browser_scroll=0.;let mut c=os_ui::Canvas::new(width,height,1.).unwrap();a.paint(&mut c,None);
   assert!(a.hits.iter().any(|h|h.action==Action::BrowserTab(TAB)));
   assert!(a.hits.iter().any(|h|h.action==Action::FeatureVote(1,1)));assert!(a.hits.iter().any(|h|h.action==Action::FeatureVote(1,-1)));
   let r=a.hits.iter().find(|h|h.action==Action::FeatureVote(0,3)).unwrap().rect;
   a.mouse_move(r.cx(),r.cy());a.mouse_down();a.mouse_up();assert!(a.state.feature_votes.focus);assert!(a.drag.is_none());
   let before=a.state.project.clone();a.key(&winit::keyboard::Key::Character("New feature".into()),false,false);assert_eq!(a.state.project,before);
   assert!(!a.typing_key(winit::keyboard::KeyCode::KeyA,true,false,false));
   a.mouse_move(14.,crate::views::browser::list_top(&a.state)+HEADER+24.);a.wheel(0.,-100.,false,false);assert_eq!(a.state.browser_scroll,max_scroll(&a.state));a.paint(&mut c,None);assert!(a.hits.iter().filter(|h|matches!(h.action,Action::FeatureVote(_,1|-1))).all(|h|h.rect.bottom()<=height as f32-28.));
   a.state.browser_scroll=0.;a.paint(&mut c,None);
   if let Some(dir)=std::env::var_os("OS_SHOT_DIR"){c.pixmap.save_png(std::path::PathBuf::from(dir).join(format!("feature-votes-{height}.png"))).unwrap();}
  }
  a.state.feature_votes.board.items[0].locked=1;a.state.browser_scroll=0.;
  let mut closed=os_ui::Canvas::new(1000,600,1.).unwrap();a.paint(&mut closed,None);
  assert!(!a.hits.iter().any(|h|matches!(h.action,Action::FeatureVote(1,1|-1))));
  a.state.feature_votes.busy=false;act(&mut a,1,1);assert!(!a.state.feature_votes.busy,"closed votes must not start a request");
  a.state.feature_votes.online=false;let mut c=os_ui::Canvas::new(1000,600,1.).unwrap();a.paint(&mut c,None);
  assert!(!a.hits.iter().any(|h|matches!(h.action,Action::FeatureVote(_,1|-1|2))));assert!(a.state.dialogs.is_empty());
  a.state.browser_search="ARA".into();assert_eq!(items(&a.state).len(),1);
 }
 #[test]
 #[ignore="Explicit live read-only service check; no voter registration or mutation"]
 fn feature_board_live_read(){let data=request(URL,"",serde_json::json!({"action":"list"})).unwrap();let b:Board=serde_json::from_value(data).unwrap();assert!(valid(&b));assert!(b.items.len()>=3);}
}
