//! Per-band stereo placement. L/R stage precedes optional M/S stage; all branches share delay.
use super::{Equalizer,Options,Mode,PARAMS,MAIN,BANDS,STATE_LEN};
pub const NAMES:[&str;5]=["Stereo","Mid","Side","Left","Right"];
pub const SHORT:[&str;5]=["ST","M","S","L","R"];
pub fn routes(b:&[u8],spare:bool)->[u8;7]{
 if b.get(STATE_LEN..STATE_LEN+4)!=Some(b"LSER"){return [0;7];}
 std::array::from_fn(|k|b.get(STATE_LEN+4+usize::from(spare)*7+k).copied().unwrap_or(0).min(4))
}
pub fn save_routes(b:&mut Vec<u8>,r:[u8;7],spare:bool){
 if b.len()<STATE_LEN+18 {b.resize(STATE_LEN+18,0);}
 b[STATE_LEN..STATE_LEN+4].copy_from_slice(b"LSER");
 for (k,v) in r.into_iter().enumerate(){b[STATE_LEN+4+usize::from(spare)*7+k]=v.min(4);}
}
#[derive(Default)]
struct DryDelay {line:Vec<f32>,at:usize}
impl DryDelay {
 fn resize(&mut self,n:usize){if self.line.len()!=n{self.line=vec![0.;n];self.at=0;}}
 fn run(&mut self,x:&mut[f32],gain:f32){for v in x{let input=*v*gain;if self.line.is_empty(){*v=input;}else{*v=self.line[self.at];self.line[self.at]=input;self.at=(self.at+1)%self.line.len();}}}
}
fn component(eq:&mut Equalizer,mask:u8,pad:&mut DryDelay,gain:f32,l:&mut[f32],r:&mut[f32],right:bool){
 if mask==0 {pad.run(if right{r}else{l},gain);}else{eq.process(l,r);}
}
pub struct RoutedEq {
 eq:[Equalizer;4], params:[i32;PARAMS],opts:Options,routes:[u8;7],ms:bool,split:bool,
 scratch:[Vec<f32>;2],masks:[u8;4],dry:[DryDelay;4],gain:f32,
}
impl RoutedEq {
 pub fn new(rate:f64,p:&[i32;PARAMS],opts:Options)->Self{
  let mut s=Self{eq:std::array::from_fn(|_|Equalizer::new(rate,p,opts)),params:*p,opts,routes:[0;7],ms:false,split:false,masks:[0;4],dry:std::array::from_fn(|_|DryDelay::default()),gain:1.,scratch:std::array::from_fn(|_|Vec::with_capacity(8192))};s.refresh();s
 }
 pub fn from_state(b:&[u8],rate:f64)->Option<Self>{
  if b.len()<148{return None;}let p=std::array::from_fn(|i|i32::from_le_bytes(b[4+4*i..8+4*i].try_into().unwrap()));
  let mut s=Self::new(rate,&p,Options::from_state(b));s.set_routes(routes(b,false));Some(s)
 }
 pub fn mode(&self)->Mode{self.eq[0].mode()}
 pub fn set_routes(&mut self,r:[u8;7]){let r=r.map(|v|v.min(4));if self.routes!=r{self.routes=r;self.refresh();}}
 pub fn set_param(&mut self,i:usize,v:i32){if i<PARAMS && self.params[i]!=v{self.params[i]=v;if i==MAIN{self.gain=(f64::from(v as f32*0.01*super::design::db_unit())).exp() as f32;}for(j,e)in self.eq.iter_mut().enumerate(){e.set_param(i,if j>=2 && i==MAIN{0}else{v});}}}
 pub fn set_options(&mut self,o:Options){if o!=self.opts{self.opts=o;self.refresh();}}
 fn refresh(&mut self){
  let masks:[u8;4]=std::array::from_fn(|c|{let mut mask=0;for k in 0..BANDS{let r=self.routes[k];if self.opts.mask&(1<<k)!=0 && match c {0=>r==0||r==3,1=>r==0||r==4,2=>r==1,_=>r==2}{mask|=1<<k;}}mask});
  self.masks=masks;self.gain=(f64::from(self.params[MAIN] as f32*0.01*super::design::db_unit())).exp() as f32;
  self.split=masks[0]!=masks[1];self.ms=masks[2]!=0||masks[3]!=0;
  for(c,e)in self.eq.iter_mut().enumerate(){e.set_options(Options{mask:masks[c],..self.opts});e.set_param(MAIN,if c<2{self.params[MAIN]}else{0});self.dry[c].resize(e.latency());}
 }
 pub fn latency(&self)->usize{self.eq[0].latency()*(if self.ms{2}else{1})}
 pub fn reset(&mut self){for e in &mut self.eq{e.reset();}for d in &mut self.dry{d.line.fill(0.);d.at=0;}}
 pub fn preroll(&mut self){for e in &mut self.eq{e.preroll();}}
 pub fn process(&mut self,l:&mut[f32],r:&mut[f32]){
  let n=l.len().min(r.len());let(l,r)=(&mut l[..n],&mut r[..n]);
  if self.routes==[0;7]{self.eq[0].process(l,r);return;}
  if self.split {self.scratch[0].clear();self.scratch[0].extend_from_slice(l);self.scratch[1].clear();self.scratch[1].extend_from_slice(r);
   component(&mut self.eq[0],self.masks[0],&mut self.dry[0],self.gain,l,r,false);let(a,b)=self.scratch.split_at_mut(1);component(&mut self.eq[1],self.masks[1],&mut self.dry[1],self.gain,&mut a[0],&mut b[0],true);r.copy_from_slice(&b[0]);
  }else if self.masks[0]==0 {self.dry[0].run(l,self.gain);self.dry[1].run(r,self.gain);}else{self.eq[0].process(l,r);}
  if self.ms {
   for (l,r) in l.iter_mut().zip(r.iter_mut()){let m=(*l+*r)*0.5;let s=(*l-*r)*0.5;*l=m;*r=s;}
   self.scratch[0].clear();self.scratch[0].extend_from_slice(l);self.scratch[1].clear();self.scratch[1].extend_from_slice(r);
   component(&mut self.eq[2],self.masks[2],&mut self.dry[2],1.,l,r,false);let(a,b)=self.scratch.split_at_mut(1);component(&mut self.eq[3],self.masks[3],&mut self.dry[3],1.,&mut a[0],&mut b[0],true);r.copy_from_slice(&b[0]);
   for (m,s) in l.iter_mut().zip(r.iter_mut()){let left=*m+*s;let right=*m-*s;*m=left;*s=right;}
  }
 }
}

#[cfg(test)]mod tests{
 use super::*;
 #[test]fn peak_eq_component_isolation_and_state(){
  let rate=48000.;let mut settings=super::super::Settings::default();settings.bands[3].db=9.;settings.bands[3].freq=super::super::freq_pos(1000.);let p=settings.params();
  let input:Vec<_>=(0..8192).map(|i|0.1*(std::f64::consts::TAU*1000.*i as f64/rate).sin() as f32).collect();
  for route in 1..=4 {
   let mut eq=RoutedEq::new(rate,&p,Options::default());let mut routes=[0;7];routes[3]=route;eq.set_routes(routes);
   let mut l=input.clone();let mut r:Vec<_>=input.iter().map(|v|v*0.3).collect();eq.process(&mut l,&mut r);
   let diff:f32=match route {
    1=>l.iter().zip(&r).zip(&input).map(|((l,r),x)|((l-r)-x*0.7).abs()).sum(),
    2=>l.iter().zip(&r).zip(&input).map(|((l,r),x)|((l+r)-x*1.3).abs()).sum(),
    3=>r.iter().zip(&input).map(|(r,x)|(r-x*0.3).abs()).sum(),
    _=>l.iter().zip(&input).map(|(l,x)|(l-x).abs()).sum(),
   };
   assert!(diff<0.001,"unselected component changed: route{route} error{diff}");
   assert!(l.iter().zip(&r).zip(&input).skip(4000).any(|((l,r),x)|(l-x).abs()+(r-x*0.3).abs()>0.03));
  }
  for opts in [Options::default(),Options{hq:1,flags:2,..Options::default()},Options{flags:1,lin_size:0,..Options::default()}]{
   let mut routed=RoutedEq::new(rate,&p,opts);let mut original=Equalizer::new(rate,&p,opts);routed.preroll();original.preroll();
   let mut l=input.clone();let mut r=input.clone();let mut a=l.clone();let mut b=r.clone();routed.process(&mut l,&mut r);original.process(&mut a,&mut b);assert_eq!(l,a);assert_eq!(r,b);
   let latency=routed.latency();routed.set_routes([1;7]);assert_eq!(routed.latency(),2*latency);
  }
  let mut state=vec![0;STATE_LEN];save_routes(&mut state,[0,1,2,3,4,0,1],false);save_routes(&mut state,[4;7],true);
  assert_eq!(routes(&state,false),[0,1,2,3,4,0,1]);assert_eq!(routes(&state,true),[4;7]);assert_eq!(routes(&state[..STATE_LEN],false),[0;7]);
 }
}
