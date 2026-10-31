// Lemon Studio support primitives; no JUCE implementation is included.
#pragma once
#include <algorithm>
#include <cmath>
#include <complex>
#include <vector>
namespace lemon_support {
template<class T> T jlimit(T lo,T hi,T x) { return std::clamp(x,lo,hi); }
template<class T> struct MathConstants { static constexpr T pi=T(3.1415926535897932384626433832795); static constexpr T twoPi=2*pi; };
template<class T> class SmoothedValue {
 T current=0,target=0,step=0; int length=1,left=0;
 public:
 void reset(double rate,double seconds){length=std::max(1,int(rate*seconds));left=0;current=target;}
 void setCurrentAndTargetValue(T x){current=target=x;left=0;}
 void setTargetValue(T x){if(x!=target){target=x;left=length;step=(target-current)/T(left);}}
 T getNextValue(){if(left>0){current+=step;if(--left==0)current=target;}return current;}
};
namespace dsp {
// Unnormalised forward, 1/N inverse. Scratch and twiddles allocated only at preparation.
class FFT {
 int n; std::vector<std::complex<double>> data, roots; std::vector<int> reverse;
 void transform(bool inverse){
  for(int i=0;i<n;++i)if(i<reverse[i])std::swap(data[i],data[reverse[i]]);
  for(int len=2;len<=n;len*=2)for(int base=0;base<n;base+=len)for(int j=0;j<len/2;++j){
   auto w=roots[j*(n/len)]; if(inverse)w=std::conj(w);
   auto a=data[base+j],b=data[base+j+len/2]*w; data[base+j]=a+b;data[base+j+len/2]=a-b;
  }
  if(inverse)for(auto& x:data)x/=double(n);
 }
 public:
 explicit FFT(int order):n(1<<order),data(n),roots(n/2),reverse(n){
  for(int i=0;i<n;++i){int x=i,r=0;for(int j=0;j<order;++j){r=(r<<1)|(x&1);x>>=1;}reverse[i]=r;}
  for(int i=0;i<n/2;++i)roots[i]=std::polar(1.0,-MathConstants<double>::twoPi*i/n);
 }
 void performRealOnlyForwardTransform(float* p,bool){
  for(int i=0;i<n;++i)data[i]={p[i],0};transform(false);
  for(int i=0;i<=n/2;++i){p[2*i]=float(data[i].real());p[2*i+1]=float(data[i].imag());}
 }
 void performRealOnlyInverseTransform(float* p){
  for(int i=0;i<=n/2;++i)data[i]={p[2*i],p[2*i+1]};
  for(int i=n/2+1;i<n;++i)data[i]=std::conj(data[n-i]);transform(true);
  for(int i=0;i<n;++i)p[i]=float(data[i].real());
 }
};
}
}
