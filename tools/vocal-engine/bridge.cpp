// SPDX-License-Identifier: GPL-3.0-or-later
// Lemon's bounded C ABI around WORLD (BSD-3-Clause, source/notice in vendor/world).
#include <algorithm>
#include <cmath>
#include <vector>
#include "world/harvest.h"
#include "world/dio.h"
#include "world/stonemask.h"
#include "world/cheaptrick.h"
#include "world/d4c.h"
#include "world/synthesis.h"
#ifdef _WIN32
#define API extern "C" __declspec(dllexport)
#else
#define API extern "C" __attribute__((visibility("default")))
#endif
static constexpr double step=0.005;
API int lemon_vocal_version() {return 1;}
static bool valid(int n,int fs){return n>0&&fs>=16000&&fs<=192000&&n<=fs*600&&n<=33554432;}
static std::vector<double> mono(const float*l,const float*r,int n){std::vector<double>x(n);for(int i=0;i<n;++i)x[i]=std::isfinite(l[i])&&std::isfinite(r[i])?0.5*(l[i]+r[i]):0.;return x;}
API int lemon_vocal_analyze(const float*l,const float*r,int n,int fs,double*out,int capacity) noexcept {
 try{
  if(!l||!r||!out||!valid(n,fs))return -1;
  int frames=GetSamplesForHarvest(fs,n,5.);if(capacity<frames)return -2;
  auto x=mono(l,r,n);std::vector<double>t(frames);
  HarvestOption opt;InitializeHarvestOption(&opt);opt.frame_period=5.;opt.f0_floor=55.;opt.f0_ceil=1100.;
  Harvest(x.data(),n,fs,&opt,t.data(),out);
  // DIO/StoneMask is a useful fallback for clean, nearly sinusoidal recordings.
  int voiced=0;for(int i=0;i<frames;++i)voiced+=out[i]>0.;
  if(voiced<frames/20){DioOption d;InitializeDioOption(&d);d.frame_period=5.;d.f0_floor=55.;d.f0_ceil=1100.;
   std::vector<double>f(frames);Dio(x.data(),n,fs,&d,t.data(),f.data());StoneMask(x.data(),n,fs,t.data(),f.data(),frames,out);}
  return frames;
 }catch(...){return -3;}
}
API int lemon_vocal_render(const float*l,const float*r,int n,int fs,const double*original,const double*corrected,int frames,double formant,float*outl,float*outr) noexcept {
 try{
  if(!l||!r||!original||!corrected||!outl||!outr||!valid(n,fs)||frames!=GetSamplesForHarvest(fs,n,5.)||!std::isfinite(formant)||std::abs(formant)>12.)return -1;
  std::copy(l,l+n,outl);std::copy(r,r+n,outr);
  bool changed=std::abs(formant)>0.001;for(int i=0;i<frames;++i){if(!std::isfinite(original[i])||!std::isfinite(corrected[i])||original[i]<0.||original[i]>2000.||corrected[i]<0.||corrected[i]>4000.)return -2;changed|=std::abs(original[i]-corrected[i])>0.01;}
  if(!changed)return 0;
  auto x=mono(l,r,n);std::vector<double>delta(n),weights(n);
  const int block=fs*10,pad=fs/10;
  for(int begin=0;begin<n;begin+=block){
   int start=std::max(0,begin-pad),end=std::min(n,begin+block+pad),len=end-start;
   int nf=GetSamplesForHarvest(fs,len,5.);std::vector<double>t(nf),f(nf),target(nf),wet(nf);
   for(int i=0;i<nf;++i){t[i]=i*step;int gi=std::min(frames-1,int(start/(fs*step))+i);f[i]=original[gi];target[i]=corrected[gi];
    for(int j=std::max(0,gi-4);j<=std::min(frames-1,gi+4);++j){if(original[j]>0.&&(std::abs(original[j]-corrected[j])>0.01||std::abs(formant)>0.001))wet[i]+=1./9.;}}
   if(*std::max_element(wet.begin(),wet.end())<0.001)continue;
   CheapTrickOption cp;InitializeCheapTrickOption(fs,&cp);cp.f0_floor=55.;int fft=GetFFTSizeForCheapTrick(fs,&cp),bins=fft/2+1;cp.fft_size=fft;
   std::vector<std::vector<double>>sp(nf,std::vector<double>(bins)),ap(nf,std::vector<double>(bins));std::vector<double*>spp(nf),app(nf);
   for(int i=0;i<nf;++i){spp[i]=sp[i].data();app[i]=ap[i].data();}
   CheapTrick(x.data()+start,len,fs,t.data(),f.data(),nf,&cp,spp.data());
   D4COption d;InitializeD4COption(&d);D4C(x.data()+start,len,fs,t.data(),f.data(),nf,fft,&d,app.data());
   if(std::abs(formant)>0.001){double factor=std::pow(2.,formant/12.);for(auto&row:sp){auto old=row;for(int k=0;k<bins;++k){double at=std::min(double(bins-1),k/factor);int lo=int(at),hi=std::min(bins-1,lo+1);row[k]=old[lo]+(old[hi]-old[lo])*(at-lo);}}}
   std::vector<double>y(len);Synthesis(target.data(),nf,spp.data(),app.data(),fft,5.,fs,len,y.data());
   if(std::any_of(y.begin(),y.end(),[](double v){return !std::isfinite(v);}))return -5;
   for(int i=0;i<len;++i){int gi=start+i;double edge=1.;if(start>0)edge=std::min(edge,double(i)/pad);if(end<n)edge=std::min(edge,double(len-1-i)/pad);edge=std::max(0.,edge);int fi=std::min(nf-1,int(i/(fs*step)));double mix=std::clamp(wet[fi],0.,1.);
    if(std::isfinite(y[i])){delta[gi]+=(y[i]-x[gi])*edge*mix;weights[gi]+=edge;}}
  }
  for(int i=0;i<n;++i){double d=weights[i]>0.?delta[i]/weights[i]:0.;outl[i]=float(l[i]+d);outr[i]=float(r[i]+d);if(!std::isfinite(outl[i])||!std::isfinite(outr[i]))return -3;}
  return 0;
 }catch(...){return -4;}
}
