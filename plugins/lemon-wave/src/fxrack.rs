// SPDX-License-Identifier: GPL-3.0-or-later
//! Addable FX slots with stable automation identities, original DSP and no callback allocations.
use crate::{controls::*, dsp::Patch};
use std::f32::consts::{PI,TAU};
pub const NAMES:[&str;14]=["Empty","Distortion","Compressor","Chorus","Delay","Reverb","Equalizer","Filter","Flanger","Phaser","Hyper / Dimension","Frequency Shifter","Convolve","Utility"];
pub const DIVISIONS:[&str;8]=["Free ms","1/4","1/8","1/16","1/32","1/8 dotted","1/8 triplet","1/16 triplet"];
pub const BEATS:[f32;8]=[0.,1.,0.5,0.25,0.125,0.75,1./3.,1./6.];
pub fn base(slot:usize)->usize {RACK_START+slot*RACK_STRIDE}
pub fn modes(kind:usize)->&'static [&'static str] {match kind {
 1=>&["Soft clip","Tube","Hard clip","Wave fold","Rectify","Bit crush","Downsample","Sine fold"],
 2=>&["Multiband OTT","Downward","Upward"],3=>&["Classic","Ensemble"],4=>&["Stereo","Ping pong","Mono"],
 5=>&["Hall","Room","Plate","Spring"],7=>&["Low pass","High pass","Band pass","Notch"],8=>&["Positive","Negative"],9=>&["6 stage","12 stage"],
 10=>&["Hyper","Dimension","Hyper + Dimension"],11=>&["Shift up","Shift down","Ring mod"],12=>&["Studio room IR","Chamber IR","Plate IR","Metal IR"],13=>&["Normal polarity","Invert L","Invert R","Invert both"],_=>&["Three bands"]}}
pub fn captions(kind:usize)->[&'static str;6] {match kind {
 1=>["Drive","Tone","Bias","Output","Crush","Mix"],2=>["Threshold","Ratio","Attack","Release","Makeup","Mix"],
 3=>["Rate","Depth","Width","Feedback","Delay","Mix"],4=>["Free time","Feedback","Tone","Width","Division","Mix"],
 5=>["Size","Decay","Damp","Pre-delay","Width","Mix"],6=>["Low gain","Mid gain","High gain","Low freq","High freq","Mix"],
 7=>["Cutoff","Resonance","Drive","Spread","Output","Mix"],8=>["Rate","Depth","Feedback","Delay","Width","Mix"],
 9=>["Rate","Depth","Center","Feedback","Width","Mix"],10=>["Rate","Detune","Width","Feedback","Dimension","Mix"],
 11=>["Shift","Fine","Feedback","Delay","Width","Mix"],12=>["Length","Damp","Pre-delay","Width","Level","Mix"],
 _=>["Gain","Pan","Width","Bass freq","Mono bass","Mix"]}}
pub fn initial(kind:usize)->[f32;6] {match kind {
 1=>[350.,650.,500.,500.,100.,350.],2=>[500.,350.,50.,200.,0.,500.],3=>[250.,400.,700.,0.,400.,350.],
 4=>[300.,350.,700.,1000.,2.,350.],5=>[500.,500.,500.,80.,1000.,300.],6=>[500.,500.,500.,300.,600.,1000.],
 7=>[800.,150.,0.,0.,500.,1000.],8=>[150.,600.,350.,250.,750.,500.],9=>[200.,650.,450.,200.,750.,500.],
 10=>[200.,350.,800.,0.,350.,400.],11=>[500.,500.,0.,150.,500.,500.],12=>[700.,500.,0.,1000.,500.,350.],
 _=>[500.,500.,500.,300.,0.,1000.]}}
pub fn order(p:&Patch)->Vec<usize> { let mut v:Vec<_>=(0..RACK_SLOTS).filter(|s|p[base(*s)]>0.5).collect();v.sort_by_key(|s|(p[base(*s)+1] as i32,*s));v }
/// Old effects remain on the legacy engine until the rack is first edited.
pub fn effective(mut p:Patch)->Patch {
 if p[RACK_MODE]<0.5 {for slot in 0..5 {
  let mixes=[DIST_MIX,OTT_MIX,CHORUS_MIX,DELAY_MIX,REVERB_MIX];let b=base(slot);let kind=slot+1;
  p[b]=if p[mixes[slot]]>0. {kind as f32} else {0.};p[b+1]=slot as f32;p[b+2]=1.;
  p[b+3]=p[FX_ROUTES[slot][0]];p[b+4]=p[FX_ROUTES[slot][1]];p[b+5]=if kind==4 {1.} else {0.};
  p[b+6..b+12].copy_from_slice(&initial(kind));p[b+11]=p[mixes[slot]];
  match kind {1=>{p[b+6]=p[DIST_DRIVE];p[b+7]=p[DIST_TONE];},2=>{p[b+9]=p[OTT_TIME]*0.5;},
   3=>{p[b+6]=p[CHORUS_RATE];p[b+7]=p[CHORUS_DEPTH];},4=>{p[b+6]=p[DELAY_TIME]/3.;p[b+7]=p[DELAY_FEEDBACK];p[b+10]=0.;},
   5=>{p[b+6]=p[REVERB_SIZE];p[b+8]=p[REVERB_DAMP];},_=>{}}
 }}p
}
pub fn value(kind:usize,col:usize,v:f32)->String {
 let n=v/1000.;if col==5{return format!("{:.0}%",n*100.);}
 match (kind,col) {
  (4,4)=>DIVISIONS[(v.round() as usize).min(7)].into(),(4,0)=>format!("{:.0}ms",50.+3000.*n),
  (3|8|9|10,0)=>format!("{:.2}Hz",0.05+5.*n),(11,0)=>format!("{:+.0}Hz",(n-0.5)*2000.),(11,1)=>format!("{:+.1}Hz",(n-0.5)*20.),
  (2,0)=>format!("{:.0}dB",-48.+48.*n),(2,1)=>format!("{:.1}:1",1.+19.*n),(2,2)=>format!("{:.1}ms",0.1+50.*n),(2,3)=>format!("{:.0}ms",20.+1000.*n),(2,4)=>format!("{:.0}dB",24.*n),
  (5,3)|(12,2)=>format!("{:.0}ms",200.*n),(6,0..=2)=>format!("{:+.0}dB",(n-0.5)*36.),(6,3)=>format!("{:.0}Hz",40.+760.*n),(6,4)=>format!("{:.1}k",1.+15.*n),
  (7,0)=>format!("{:.0}Hz",20.*1000_f32.powf(n)),(13,0)=>format!("{:+.0}dB",(n-0.5)*48.),(13,2)=>format!("{:.0}%",n*200.),(13,3)=>format!("{:.0}Hz",20.+480.*n),
  _=>format!("{:.0}%",n*100.)
 }
}
pub fn delay_seconds(time:f32,division:usize,tempo:f32)->f32 {
 if division==0 {(50.+3.*time)/1000.} else {60./tempo.clamp(20.,400.)*BEATS[division.min(7)]}
}
struct Line {data:Vec<f32>,head:usize,filled:usize}
impl Line {
 fn new(n:usize)->Self{Self{data:vec![0.;n.max(4)],head:0,filled:0}}
 fn clear(&mut self){self.head=0;self.filled=0;}
 fn read(&self,d:f32)->f32 {
  let d=d.clamp(1.,self.data.len() as f32-2.);if d>self.filled as f32{return 0.;}
  let at=(self.head as f32-d).rem_euclid(self.data.len() as f32);let i=at as usize%self.data.len();let f=at-at.floor();
  self.data[i]*(1.-f)+self.data[(i+1)%self.data.len()]*f
 }
 fn push(&mut self,x:f32){self.data[self.head]=x;self.head=(self.head+1)%self.data.len();self.filled=(self.filled+1).min(self.data.len());}
}
struct Module {
 kind:usize, delay:[Line;2],short:[Line;2],verb:[Line;8],history:[Line;2],
 phase:f32,low:[f32;2],high:[f32;2],tone:[f32;2],env:[f32;3],damp:[f32;8],ap:[[f32;12];2],
 feedback:[f32;2],held:[f32;2],counter:usize,
 ir_time:[[f32;64];2],ir_gain:[[f32;64];2],hilbert_coeff:[f32;32],
}
impl Module {
 fn new(rate:f32)->Self {Self {kind:0,delay:std::array::from_fn(|_|Line::new((rate*3.1) as usize+4)),
 short:std::array::from_fn(|_|Line::new((rate*0.12) as usize+4)),verb:std::array::from_fn(|_|Line::new((rate*0.16) as usize+4)),
 history:std::array::from_fn(|_|Line::new((rate*0.8) as usize+128)),phase:0.,low:[0.;2],high:[0.;2],tone:[0.;2],env:[0.;3],damp:[0.;8],ap:[[0.;12];2],feedback:[0.;2],held:[0.;2],counter:0,ir_time:[[0.;64];2],ir_gain:[[0.;64];2],hilbert_coeff:std::array::from_fn(|i|{let tap=(i*2+1) as f32;2./(PI*(tap-32.))*(0.54-0.46*(TAU*tap/64.).cos())})}}
 fn reset(&mut self){for l in self.delay.iter_mut().chain(self.short.iter_mut()).chain(self.verb.iter_mut()).chain(self.history.iter_mut()){l.clear();}
 self.phase=0.;self.low.fill(0.);self.high.fill(0.);self.tone.fill(0.);self.env.fill(0.);self.damp.fill(0.);self.ap=[[0.;12];2];self.feedback.fill(0.);self.held.fill(0.);self.counter=0;}
 fn tick(&mut self,x:[f32;2],kind:usize,mode:usize,v:[f32;6],rate:f32,tempo:f32)->[f32;2] {
  if kind!=self.kind {self.reset();self.kind=kind;}
  let n=v.map(|v|(v/1000.).clamp(0.,1.));let mut y=x;
  let lp=|hz:f32|1.-(-TAU*hz.min(rate*0.45)/rate).exp();
  let phase=self.phase;self.phase=(phase+(0.05+5.*n[0])/rate).fract();
  match kind {
   1=>{let drive=1.+n[0]*30.;let bias=(n[2]-0.5)*0.8;let bits=2_f32.powf(16.-n[4]*14.);let every=1+(n[4]*63.) as usize;
    let shape=|s:f32|match mode {1=>(s+s*s*0.15).tanh(),2=>s.clamp(-1.,1.),3=>1.-((s+1.).rem_euclid(4.)-2.).abs(),4=>s.abs(),5=>(s*bits).round()/bits,7=>s.sin(),_=>s.tanh()};
    for ch in 0..2 {let z=(shape(x[ch]*drive+bias)-shape(bias))/drive.sqrt();if self.counter%every==0{self.held[ch]=z;}
      let z=if mode==6{self.held[ch]}else{z};self.tone[ch]+=lp(400.+n[1]*17000.)*(z-self.tone[ch]);y[ch]=self.tone[ch]*2_f32.powf((n[3]-0.5)*4.);}
    self.counter=self.counter.wrapping_add(1);
   },
   2=>{let mut bands=[[0.;2];3];for ch in 0..2 {self.low[ch]+=lp(200.)*(x[ch]-self.low[ch]);self.high[ch]+=lp(2500.)*(x[ch]-self.high[ch]);bands[0][ch]=self.low[ch];bands[1][ch]=self.high[ch]-self.low[ch];bands[2][ch]=x[ch]-self.high[ch];}
    if mode>0 {bands=[x,[0.;2],[0.;2]];}y=[0.;2];let threshold=10_f32.powf((-48.+48.*n[0])/20.);let ratio=1.+19.*n[1];
    for band in 0..if mode==0{3}else{1}{let peak=bands[band][0].abs().max(bands[band][1].abs());let seconds=if peak>self.env[band]{0.0001+0.05*n[2]}else{0.02+n[3]};self.env[band]+=(1.-(-1./(rate*seconds)).exp())*(peak-self.env[band]);
     let e=self.env[band].max(1e-8);let gain=if e>threshold && mode!=2 {(threshold/e).powf(1.-1./ratio)} else if e<threshold && mode!=1 {(threshold/e).powf((1.-1./ratio)*0.5).min(4.)}else{1.};
     for ch in 0..2{y[ch]+=bands[band][ch]*gain*10_f32.powf(n[4]*24./20.);}}
   },
   3|8|10=>{for ch in 0..2 {let phase=phase+ch as f32*n[2]*0.25;let mut wet=0.;let voices=if kind==10||kind==3&&mode==1{3}else{1};
     for voice in 0..voices {let lfo=(TAU*(phase+voice as f32/voices as f32)).sin();let ms=if kind==8 {0.4+n[3]*4.+(lfo+1.)*n[1]*2.} else {8.+n[4]*20.+lfo*n[1]*6.};wet+=self.short[ch].read(ms*rate*0.001)/voices as f32;}
     if kind==10&&mode>0 {let dimension=self.short[1-ch].read((8.+n[4]*24.)*rate*0.001);wet=if mode==1{dimension}else{(wet+dimension)*0.5};}
     let fb=if kind==8{n[2]*0.9*(if mode==1{-1.}else{1.})}else{n[3]*0.75};self.feedback[ch]=wet;
     y[ch]=wet;self.held[ch]=(x[ch]+wet*fb).tanh();}
    for ch in 0..2{self.short[ch].push(self.held[ch]);}
   },
   4=>{let seconds=delay_seconds(v[0],v[4].round() as usize,tempo);let times=[seconds,seconds*(if mode==0{0.75+0.25*n[3]}else{1.})];
    let wet:[f32;2]=std::array::from_fn(|ch|self.delay[ch].read(times[ch]*rate));
    for ch in 0..2{self.tone[ch]+=lp(300.+17000.*n[2])*(wet[ch]-self.tone[ch]);}
    for ch in 0..2{let input=if mode==1 {if ch==0{(x[0]+x[1])*0.5}else{0.}}else if mode==2{(x[0]+x[1])*0.5}else{x[ch]};let fb=self.tone[if mode==1{1-ch}else{ch}];self.delay[ch].push((input+fb*n[1]*0.94).tanh());y[ch]=self.tone[ch];}
   },
   5=>{let input:[f32;2]=std::array::from_fn(|ch|if n[3]<0.001{x[ch]}else{self.delay[ch].read(n[3]*rate*0.2)});for ch in 0..2{self.delay[ch].push(x[ch]);}
    let scale=(0.4+n[0]*1.5)*[1.,0.48,0.77,0.31][mode.min(3)];
    let taps:[f32;8]=std::array::from_fn(|i|self.verb[i].read(rate*(0.031+i as f32*0.0047)*scale));
    for i in 0..8{self.damp[i]+=lp(600.+(1.-n[2])*15000.)*(taps[i]-self.damp[i]);}
    let sum=self.damp.iter().sum::<f32>()*0.25;for i in 0..8{let feedback=(sum-self.damp[i])*(0.45+n[1]*0.53);self.verb[i].push(input[i%2]*0.25+feedback);}
    y=[taps[0]+taps[2]-taps[4]+taps[6],taps[1]+taps[3]-taps[5]+taps[7]];if mode==3{for ch in 0..2{let a=0.65;let z=-a*y[ch]+self.ap[ch][0];self.ap[ch][0]=y[ch]+a*z;y[ch]=z;}}
    let mid=(y[0]+y[1])*0.5;for ch in 0..2{y[ch]=mid+(y[ch]-mid)*n[4];}
   },
   6=>{for ch in 0..2{self.low[ch]+=lp(40.+n[3]*760.)*(x[ch]-self.low[ch]);self.high[ch]+=lp(1000.+n[4]*15000.)*(x[ch]-self.high[ch]);y[ch]=self.low[ch]*10_f32.powf((n[0]-0.5)*36./20.)+(self.high[ch]-self.low[ch])*10_f32.powf((n[1]-0.5)*36./20.)+(x[ch]-self.high[ch])*10_f32.powf((n[2]-0.5)*36./20.);}},
   7=>{for ch in 0..2{let hz=(20.*1000_f32.powf(n[0])*2_f32.powf((ch as f32-0.5)*n[3])).min(rate*0.4);let g=(PI*hz/rate).tan();let k=2.-1.9*n[1];let input=(x[ch]*(1.+n[2]*15.)).tanh();let a=1./(1.+g*(g+k));let band=a*(self.low[ch]+g*(input-self.high[ch]));let low=self.high[ch]+g*band;self.low[ch]=2.*band-self.low[ch];self.high[ch]=2.*low-self.high[ch];y[ch]=match mode{1=>input-k*band-low,2=>band,3=>input-k*band,_=>low}*2_f32.powf((n[4]-0.5)*4.);}},
   9=>{for ch in 0..2{let sweep=(TAU*(phase+ch as f32*n[4]*0.25)).sin();let hz=(100.+n[2]*4000.)*2_f32.powf(sweep*n[1]*2.);let t=(PI*hz.min(rate*0.4)/rate).tan();let a=(1.-t)/(1.+t);let mut z=(x[ch]+self.feedback[ch]*n[3]*0.85).tanh();for stage in 0..if mode==0{6}else{12}{let next=-a*z+self.ap[ch][stage];self.ap[ch][stage]=z+a*next;z=next;}self.feedback[ch]=z;y[ch]=z;}},
   11=>{let shift=((n[0]-0.5)*2000.+(n[1]-0.5)*20.)*(if mode==1{-1.}else{1.});self.phase=(phase+shift/rate).rem_euclid(1.);
    for ch in 0..2{let feedback=self.delay[ch].read((1.+n[3]*200.)*rate*0.001)*n[2]*0.85;self.history[ch].push((x[ch]+feedback).tanh());let real=self.history[ch].read(32.);let mut hilbert=0.;
     for i in 0..32{hilbert+=self.history[ch].read((i*2+1) as f32)*self.hilbert_coeff[i];}
     let angle=TAU*(phase+ch as f32*(n[4]-0.5)*0.5);y[ch]=if mode==2{real*angle.cos()}else{real*angle.cos()-hilbert*angle.sin()};self.delay[ch].push(y[ch]);}
   },
   12=>{
    // Refresh FIR tap positions/gains at control rate, avoiding 128 exponentials per sample.
    if self.counter%64==0 {for ch in 0..2{for tap in 0..64 {let t=(tap+1) as f32/64.;let jitter=((tap*37+mode*19+ch*11)%31) as f32/31.;
        self.ir_time[ch][tap]=((n[2]*0.2+(0.02+n[0]*0.5)*(t*t*0.9+jitter*0.06))*rate).max(1.);
        self.ir_gain[ch][tap]=(if (tap+mode*3+ch)%3==0{-1.}else{1.})*(-t*(3.+n[1]*6.)).exp()*0.17;
    }}}self.counter=self.counter.wrapping_add(1);
    for ch in 0..2{self.history[ch].push(x[ch]);let mut wet=0.;
     // Original generated sparse impulse responses, evaluated as a true FIR convolution.
     for tap in 0..64 {wet+=self.history[ch].read(self.ir_time[ch][tap])*self.ir_gain[ch][tap];}
     self.tone[ch]+=lp(1000.+(1.-n[1])*15000.)*(wet-self.tone[ch]);y[ch]=self.tone[ch]*2_f32.powf((n[4]-0.5)*4.);}
     let mid=(y[0]+y[1])*0.5;for ch in 0..2{y[ch]=mid+(y[ch]-mid)*n[3];}
   },
   13=>{let gain=10_f32.powf((n[0]-0.5)*48./20.);let mid=(x[0]+x[1])*0.5;let bass_coeff=lp(20.+n[3]*480.);for ch in 0..2{self.low[ch]+=bass_coeff*(x[ch]-self.low[ch]);}let bass=(self.low[0]+self.low[1])*0.5;
    for ch in 0..2{let full=mid+(x[ch]-mid)*n[2]*2.;let mono=(bass-self.low[ch])*n[4];let pan=if ch==0{(1.-n[1])*2.}else{n[1]*2.};y[ch]=(full+mono)*gain*pan.min(1.)*(if mode==3||mode==ch+1{-1.}else{1.});}
   },_=>{}
  }
  std::array::from_fn(|ch|{let wet=if y[ch].is_finite(){y[ch].clamp(-32.,32.)}else{0.};x[ch]+n[5]*(wet-x[ch])})
 }
}
pub struct Rack {modules:Vec<Module>,order:[usize;RACK_SLOTS],rate:f32}
impl Rack {
 pub fn new(rate:f32)->Self{Self{modules:(0..RACK_SLOTS).map(|_|Module::new(rate)).collect(),order:std::array::from_fn(|i|i),rate}}
 pub fn reset(&mut self){for m in &mut self.modules{m.reset();m.kind=0;}}
 pub fn prepare(&mut self,p:&Patch){self.order.sort_unstable_by_key(|s|(p[base(*s)+1] as i32,*s));}
 pub fn tick(&mut self,mut x:[f32;2],p:&Patch,target:&Patch,o:usize,tempo:f32)->[f32;2]{for slot in self.order {let b=base(slot);let kind=(target[b] as usize).min(13);if kind==0{continue;}
  let mut v=std::array::from_fn(|i|p[b+6+i]);v[5]*=p[b+2].clamp(0.,1.)*p[b+3+o].clamp(0.,1.);if kind==4{v[4]=target[b+10].min(7.);}
  if v[5]<0.00001 {if self.modules[slot].kind!=0{self.modules[slot].reset();self.modules[slot].kind=0;}continue;}
  x=self.modules[slot].tick(x,kind,target[b+5] as usize,v,self.rate,tempo);
 }x}
}

#[cfg(test)]
mod tests {
 use super::*;
 #[test]
 fn all_modules_and_modes_are_finite_resettable_and_bypass_exactly() {
  for rate in [8000.,48000.,96000.] {let mut m=Module::new(rate);
   for kind in 1..14 {for mode in 0..modes(kind).len() {
    m.reset();m.kind=0;let mut v=initial(kind);v[5]=1000.;if kind==6{v[0]=700.;}if kind==13{v[0]=650.;}
    let mut changed=0.;for n in 0..(rate*0.15) as usize {let x=[(n as f32*0.13).sin()*0.2,(n as f32*0.17).sin()*0.15];let y=m.tick(x,kind,mode,v,rate,120.);assert!(y.iter().all(|v|v.is_finite()&&v.abs()<=32.),"{kind}/{mode}");changed+=(x[0]-y[0]).abs();}
    assert!(changed>0.01,"{kind}/{mode} did nothing");v[5]=0.;assert_eq!(m.tick([0.13,-0.17],kind,mode,v,rate,120.),[0.13,-0.17]);
    m.reset();v[5]=1000.;for _ in 0..100{assert_eq!(m.tick([0.;2],kind,mode,v,rate,120.),[0.;2],"reset {kind}/{mode}");}
   }}
  }
 }
 #[test]
 fn synced_delay_divisions_and_ping_pong_echo_direction() {
  for (division,seconds) in [(1,0.5),(2,0.25),(3,0.125),(5,0.375),(6,1./6.)] {assert!((delay_seconds(0.,division,120.)-seconds).abs()<1e-6);}
  let rate=8000.;for mode in [0,1] {let mut m=Module::new(rate);let mut v=initial(4);v[4]=3.;v[3]=1000.;v[5]=1000.;v[1]=700.;let mut first=[0.;2];let mut second=[0.;2];
   for n in 0..2100 {let y=m.tick(if n==0{[1.,0.]}else{[0.;2]},4,mode,v,rate,120.);if (1000..1020).contains(&n){for c in 0..2{first[c]+=y[c].abs();}}if (2000..2020).contains(&n){for c in 0..2{second[c]+=y[c].abs();}}}
   assert!(first[0]>0.1);assert_eq!(first[1],0.);if mode==1 {assert!(second[1]>0.01);assert!(second[0]<0.001);}else{assert!(second[0]>0.01);assert_eq!(second[1],0.);}
  }
 }
 #[test]
 fn slot_order_duplicate_effects_and_oscillator_isolation() {
  let mut p=crate::dsp::defaults();p[RACK_MODE]=1.;
  for (slot,kind) in [(0,1),(1,4),(2,1)] {let b=base(slot);p[b]=kind as f32;p[b+6..b+12].copy_from_slice(&initial(kind));p[b+11]=800.;p[b+4]=0.;}
  p[base(1)+6]=0.;p[base(1)+10]=0.;
  let mut rack=Rack::new(8000.);rack.prepare(&p);let mut out=Vec::new();
  for n in 0..1200 {let x=[(n as f32*0.2).sin()*0.2;2];out.push(rack.tick(x,&p,&p,0,120.));}
  rack.reset();p[base(0)+1]=2.;p[base(2)+1]=0.;p[base(2)+6]=900.;rack.prepare(&p);let mut delta=0.;
  for n in 0..1200{let x=[(n as f32*0.2).sin()*0.2;2];let y=rack.tick(x,&p,&p,0,120.);delta+=(y[0]-out[n][0]).abs();}assert!(delta>0.01);
  rack.reset();for n in 0..1200{let x=[(n as f32*0.2).sin()*0.2;2];assert_eq!(rack.tick(x,&p,&p,1,120.),x);}
 }
}
