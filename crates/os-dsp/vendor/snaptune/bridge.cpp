#include "AutoTuneEngine.h"
#include <new>
struct LemonTune {
 snaptune::AutoTuneEngine engine;
 snaptune::EngineSettings settings;
 float left[256]{},right[256]{};
 double histogram[12]{}; double voiced=0; double sampleRate;
 explicit LemonTune(double rate):sampleRate(rate){
  settings.mode=snaptune::LatencyMode::live;
  settings.voice=snaptune::VoiceType::instrument;
  engine.setSettings(settings);engine.prepare(rate,2);
 }
};
extern "C" {
void* lemon_tune_new(double rate) noexcept {try{return new LemonTune(rate);}catch(...){return nullptr;}}
void lemon_tune_delete(void* p) noexcept {delete static_cast<LemonTune*>(p);}
void lemon_tune_reset(void* p) noexcept {static_cast<LemonTune*>(p)->engine.reset();}
void lemon_tune_clear_learning(void* p) noexcept {auto& t=*static_cast<LemonTune*>(p);std::fill_n(t.histogram,12,0.0);t.voiced=0;}
void lemon_tune_stats(void* p,double* out) noexcept {auto& t=*static_cast<LemonTune*>(p);std::copy_n(t.histogram,12,out);out[12]=t.voiced/t.sampleRate;}
int lemon_tune_latency(void* p) noexcept {return static_cast<LemonTune*>(p)->engine.getLatencySamples();}
void lemon_tune_process(void* p,float* interleaved,int frames,float strength,int key,int scale,int voice,bool learn) noexcept {
 auto& t=*static_cast<LemonTune*>(p);
 t.settings.voice=voice==1?snaptune::VoiceType::soprano:snaptune::VoiceType::lowMale;
 t.settings.key=std::clamp(key,0,11);t.settings.scale=static_cast<snaptune::Scale>(std::clamp(scale,0,12));
 t.settings.amount=std::clamp(strength,0.0f,1.0f);
 t.settings.retuneMs=200.0f*(1.0f-strength)*(1.0f-strength);
 t.settings.humanize=0.5f*(1.0f-strength);
 t.engine.setSettings(t.settings);
 for(int offset=0;offset<frames;offset+=256){int n=std::min(256,frames-offset);
  for(int i=0;i<n;++i){t.left[i]=interleaved[2*(offset+i)];t.right[i]=interleaved[2*(offset+i)+1];}
  float* channels[]={t.left,t.right};t.engine.process(channels,2,0,n);
  if(learn){snaptune::PitchFrame f;if(t.engine.popFrames(&f,1)>0 && f.voiced && std::isfinite(f.inputNote)){int pc=snaptune::pitchClass(int(std::lround(f.inputNote)));t.histogram[pc]+=n;t.voiced+=n;}}
  if(strength>0.0f) for(int i=0;i<n;++i){interleaved[2*(offset+i)]=t.left[i];interleaved[2*(offset+i)+1]=t.right[i];}
 }
}
}
