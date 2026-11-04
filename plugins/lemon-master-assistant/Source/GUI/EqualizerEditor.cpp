#include "EqualizerEditor.h"
#include "Theme.h"
#include <cmath>
namespace oxygen {
EqualizerEditor::EqualizerEditor(EqualizerModule& p, juce::AudioProcessorValueTreeState& v)
    : AudioProcessorEditor(&p),audioProcessor(p),apvts(v) {
    for(auto* s:{&frequency,&gain,&bandwidth}) {
        addAndMakeVisible(s); s->setSliderStyle(juce::Slider::RotaryHorizontalVerticalDrag);
        s->setTextBoxStyle(juce::Slider::TextBoxRight,false,88,24);
        s->setScrollWheelEnabled(false);
    }
    frequency.setTextValueSuffix(" Hz");gain.setTextValueSuffix(" dB");bandwidth.setTextValueSuffix(" Q");
    frequency.setTooltip("Centre frequency. Drag a band horizontally to move it.");
    gain.setTooltip("Band gain. Drag vertically; hold Ctrl for fine adjustment.");
    bandwidth.setTooltip("Bandwidth. Scroll over a band to adjust Q.");
    addAndMakeVisible(reset);reset.onClick=[this]{auto* p=apvts.getParameter("Gain"+juce::String(selected));p->beginChangeGesture();set("Gain",selected,0);p->endChangeGesture();};
    select(selected);setSize(820,340);timerCallback();startTimerHz(24);
}
EqualizerEditor::~EqualizerEditor(){stopTimer();if(dragging>=0)gesture(false);}
float EqualizerEditor::value(const char* id,int i)const{return apvts.getRawParameterValue(juce::String(id)+juce::String(i))->load();}
float EqualizerEditor::x(float f)const{return graph.getX()+std::log(f/20.f)/std::log(1000.f)*graph.getWidth();}
float EqualizerEditor::y(float v)const{return graph.getCentreY()-v/30.f*graph.getHeight();}
float EqualizerEditor::hz(float xx)const{return 20.f*std::pow(1000.f,juce::jlimit(0.f,1.f,(xx-graph.getX())/graph.getWidth()));}
float EqualizerEditor::db(float yy)const{return juce::jlimit(-12.f,12.f,(graph.getCentreY()-yy)/graph.getHeight()*30.f);}
juce::Point<float> EqualizerEditor::point(int i)const{return{x(value("Freq",i)),y(value("Gain",i))};}
int EqualizerEditor::nearest(juce::Point<float> pt)const{
    int band=-1;float distance=16.f;
    for(int i=0;i<EqualizerModule::NumBands;++i){float d=pt.getDistanceFrom(point(i));if(d<distance){distance=d;band=i;}}return band;
}
void EqualizerEditor::select(int i){
    selected=i;attachments={};
    attachments[0]=std::make_unique<juce::AudioProcessorValueTreeState::SliderAttachment>(apvts,"Freq"+juce::String(i),frequency);
    attachments[1]=std::make_unique<juce::AudioProcessorValueTreeState::SliderAttachment>(apvts,"Gain"+juce::String(i),gain);
    attachments[2]=std::make_unique<juce::AudioProcessorValueTreeState::SliderAttachment>(apvts,"Q"+juce::String(i),bandwidth);
    timerCallback();
}
void EqualizerEditor::set(const char* id,int i,float v){auto* p=apvts.getParameter(juce::String(id)+juce::String(i));p->setValueNotifyingHost(p->convertTo0to1(v));}
void EqualizerEditor::gesture(bool begin){for(auto id:{"Freq","Gain"}){auto* p=apvts.getParameter(juce::String(id)+juce::String(dragging));if(begin)p->beginChangeGesture();else p->endChangeGesture();}}
void EqualizerEditor::mouseDown(const juce::MouseEvent& e){
    const int band=nearest(e.position);if(band<0)return;
    select(band);dragging=band;origin=e.position;initialFreq=value("Freq",band);initialGain=value("Gain",band);gesture(true);
}
void EqualizerEditor::mouseDrag(const juce::MouseEvent& e){
    if(dragging<0)return;const float fine=e.mods.isCtrlDown()?0.1f:1.f;
    auto delta=(e.position-origin)*fine;
    initialFreq=hz(x(initialFreq)+delta.x);initialGain=db(y(initialGain)+delta.y);origin=e.position;
    set("Freq",dragging,initialFreq);set("Gain",dragging,initialGain);timerCallback();
}
void EqualizerEditor::mouseUp(const juce::MouseEvent&){if(dragging>=0)gesture(false);dragging=-1;}
void EqualizerEditor::mouseDoubleClick(const juce::MouseEvent& e){
    if(!graph.contains(e.position))return;
    if(dragging>=0){gesture(false);dragging=-1;}
    int band=nearest(e.position);
    if(band>=0){select(band);reset.triggerClick();return;}
    float distance=std::numeric_limits<float>::max();
    for(int i=0;i<EqualizerModule::NumBands;++i)if(std::abs(value("Gain",i))<0.05f){float d=std::abs(x(value("Freq",i))-e.x);if(d<distance){distance=d;band=i;}}
    if(band<0)return;
    select(band);dragging=band;gesture(true);set("Freq",band,hz(e.position.x));set("Gain",band,db(e.position.y));gesture(false);dragging=-1;timerCallback();
}
void EqualizerEditor::mouseWheelMove(const juce::MouseEvent& e,const juce::MouseWheelDetails& w){
    const int band=nearest(e.position);if(band<0){AudioProcessorEditor::mouseWheelMove(e,w);return;}
    select(band);auto* p=apvts.getParameter("Q"+juce::String(band));p->beginChangeGesture();
    set("Q",band,juce::jlimit(0.1f,12.f,value("Q",band)*std::pow(2.f,w.deltaY*(e.mods.isCtrlDown()?0.3f:3.f))));p->endChangeGesture();timerCallback();
}
void EqualizerEditor::resized(){
    auto r=getLocalBounds().reduced(14,8);auto controls=r.removeFromBottom(58);r.removeFromBottom(23);graph=r.toFloat().reduced(30,12);
    controls.removeFromLeft(50);frequency.setBounds(controls.removeFromLeft(190));gain.setBounds(controls.removeFromLeft(170));bandwidth.setBounds(controls.removeFromLeft(170));reset.setBounds(controls.removeFromRight(110).reduced(3,14));
}
void EqualizerEditor::timerCallback(){
    response.fill(0);bandResponse.fill(0);const double rate=audioProcessor.getSampleRate()>0?audioProcessor.getSampleRate():48000.;
    for(int b=0;b<EqualizerModule::NumBands;++b){
        auto coeff=juce::dsp::IIR::Coefficients<float>::makePeakFilter(rate,juce::jlimit(20.f,(float)rate*.45f,value("Freq",b)),value("Q",b),juce::Decibels::decibelsToGain(value("Gain",b)));
        for(size_t i=0;i<response.size();++i){const double f=juce::jmin(rate*.49,20.*std::pow(1000.,(double)i/(response.size()-1)));float v=juce::Decibels::gainToDecibels((float)coeff->getMagnitudeForFrequency(f,rate));response[i]+=v;if(b==selected)bandResponse[i]=v;}
    }repaint();
}
void EqualizerEditor::paint(juce::Graphics& g){
    using namespace Theme;const auto well=graph.expanded(30,12);
    g.setColour(Colors::Background.darker(.25f));g.fillRoundedRectangle(well,7);
    g.setFont(Fonts::getBody().withHeight(10));
    for(float v:{-12.f,-6.f,0.f,6.f,12.f}){
        g.setColour(Colors::OnSurface.withAlpha(v==0?.22f:.07f));g.drawHorizontalLine((int)y(v),graph.getX(),graph.getRight());
        g.setColour(Colors::OnSurfaceVariant);g.drawText(juce::String((int)v),well.getX()+2,y(v)-7,25,14,juce::Justification::centred);
    }
    for(float f:{20.f,50.f,100.f,200.f,500.f,1000.f,2000.f,5000.f,10000.f,20000.f}){
        const float xx=x(f);g.setColour(Colors::OnSurface.withAlpha(.06f));g.drawVerticalLine((int)xx,graph.getY(),graph.getBottom());
        g.setColour(Colors::OnSurfaceVariant);g.drawText(f>=1000?juce::String(f/1000.f,0)+"k":juce::String((int)f),xx-18,graph.getBottom()+14,36,14,juce::Justification::centred);
    }
    { juce::Graphics::ScopedSaveState saved(g);g.reduceClipRegion(well.toNearestInt());
    auto path=[&](const auto& values){juce::Path p;for(size_t i=0;i<values.size();++i){float xx=graph.getX()+graph.getWidth()*(float)i/(values.size()-1);float yy=y(values[i]);if(i==0)p.startNewSubPath(xx,yy);else p.lineTo(xx,yy);}return p;};
    auto band=path(bandResponse);auto fill=band;fill.lineTo(graph.getRight(),y(0));fill.lineTo(graph.getX(),y(0));fill.closeSubPath();
    g.setColour(Colors::Secondary.withAlpha(.12f));g.fillPath(fill);g.setColour(Colors::Secondary.withAlpha(.6f));g.strokePath(band,juce::PathStrokeType(1));
    auto total=path(response);g.setColour(Colors::Primary.withAlpha(.08f));g.strokePath(total,juce::PathStrokeType(7));g.setColour(Colors::Primary);g.strokePath(total,juce::PathStrokeType(2));
    for(int i=0;i<EqualizerModule::NumBands;++i){auto pt=point(i);float radius=i==selected?7.f:5.f;
        g.setColour(i==selected?Colors::Primary:Colors::SurfaceVariant);g.fillEllipse(pt.x-radius,pt.y-radius,2*radius,2*radius);
        g.setColour(Colors::Primary.withAlpha(i==selected?1.f:.6f));g.drawEllipse(pt.x-radius,pt.y-radius,2*radius,2*radius,1.5f);
    }
    }
    g.setColour(Colors::OnSurfaceVariant);g.setFont(Fonts::getBody().withHeight(11));
    g.drawText("BAND "+juce::String(selected+1)+"  /  BELL",18,getHeight()-64,140,16,juce::Justification::left);
    g.drawText("Drag: frequency / gain     Scroll: Q     Double-click: add / reset     Ctrl: fine",170,getHeight()-64,getWidth()-185,16,juce::Justification::right);
}
}
