// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared, bundled release history. Reading/searching never needs a network request.
use std::sync::OnceLock;
use serde::Deserialize;
use os_ui::{Font,Rect};
use crate::app::{Action,State,Ui};
#[derive(Deserialize)]
struct History { intro:String, sections:Vec<Section> }
#[derive(Deserialize)]
struct Section { date:String,title:String,changes:Vec<String> }
fn history()->&'static History {
 static DATA:OnceLock<History>=OnceLock::new();
 DATA.get_or_init(||serde_json::from_str(include_str!("../../../packaging/changelog.json")).expect("validated bundled changelog"))
}
fn lines(ui:&mut Ui,s:&State,width:f32)->Vec<(String,bool)> {
 let mut out=Vec::new();
 let mut add=|text:&str,heading:bool| {
  // Measure every line with the exact same font used for painting.
  out.extend(crate::feature_votes::comment_lines(ui,text,width).into_iter().map(|l|(l,heading)));
  out.push((String::new(),false));
 };
 add(&history().intro,false);
 let query=s.browser_search.trim().to_lowercase();let mut found=false;
 let mut days=std::collections::BTreeMap::<&str,Vec<&str>>::new();
 for section in &history().sections {
  let changes=days.entry(section.date.get(..10).unwrap_or(&section.date)).or_default();
  for change in &section.changes {if !changes.contains(&change.as_str()){changes.push(change);}}
 }
 for (date,changes) in days.iter().rev() {
  let changes:Vec<_>=changes.iter().filter(|c|query.is_empty()||date.contains(&query)||c.to_lowercase().contains(&query)).collect();
  if changes.is_empty(){continue;}found=true;add(date,true);
  for change in changes {add(&format!("• {change}"),false);}
 }
 if !found {add("No changes match your search.",false);}
 out
}
pub fn max_scroll(s:&State)->f32 {s.feature_votes.changelog_scroll.get()}
pub fn paint(ui:&mut Ui,s:&State,r:Rect) {
 ui.c.fill_rect(r,ui.t.s(0x171f23));ui.hit(r,Action::BrowserList,"Change Log · search below to filter the history");
 ui.c.push_clip(r);
 ui.c.text(Font::new(21).bold(),r.x+10.,r.y+10.,ui.t.text,"Change Log");
 ui.c.text_clip(Font::new(11),r.x+10.,r.y+38.,r.w-20.,ui.t.text_dim,"Michael Lane edition · available offline");
 let body=Rect::new(r.x+10.,r.y+64.,(r.w-20.).max(1.),(r.h-64.).max(0.));
 let rows=lines(ui,s,body.w);
 let max=(rows.len() as f32*18.+12.-body.h).max(0.);
 s.feature_votes.changelog_scroll.set(max);
 let offset=s.browser_scroll.clamp(0.,max);
 ui.c.push_clip(body);
 for (i,(line,heading)) in rows.iter().enumerate() {
  let y=body.y+i as f32*18.-offset;
  if y+18.>=body.y && y<body.bottom(){ui.c.text_clip(Font::new(12),body.x,y,body.w,if *heading{ui.t.h(0xb5d98b)}else{ui.t.text},line);}
 }
 ui.c.pop_clip();ui.c.pop_clip();
}
#[cfg(test)]
mod tests {
 use super::*;
 #[test]fn changelog_navigation_search_and_scroll() {
  let mut app=crate::app::App::new(None);app.state.show_browser=true;app.state.browser_tab=6;
  // Reading the changelog remains possible during a community request and offline.
  app.state.feature_votes.busy=true;
  crate::feature_votes::act(&mut app,2,6);
  assert!(app.state.feature_votes.change_log);assert!(!app.state.feature_votes.community_forum);
  for (w,h) in [(1000,600),(1600,900)] {
   app.state.screen=(w as f32,h as f32);app.state.browser_search.clear();
   let mut canvas=os_ui::Canvas::new(w,h,1.).unwrap();app.paint(&mut canvas,None);
   assert!(app.hits.iter().any(|hit|hit.action==Action::FeatureVote(2,6)));
   assert!(max_scroll(&app.state)>1000.);
   app.state.browser_scroll=max_scroll(&app.state);app.paint(&mut canvas,None);
   app.state.browser_search="no-such-change-12345".into();app.paint(&mut canvas,None);
   assert_eq!(max_scroll(&app.state),0.);
  }
  assert_eq!(history().sections.last().unwrap().date,"2026-10-28");
  assert!(history().sections.iter().map(|s|s.changes.len()).sum::<usize>()>100);
  crate::feature_votes::act(&mut app,1,6);assert!(!app.state.feature_votes.change_log);assert!(app.state.feature_votes.community_forum);
  crate::feature_votes::act(&mut app,0,6);assert!(!app.state.feature_votes.change_log);assert!(!app.state.feature_votes.community_forum);
 }
}
