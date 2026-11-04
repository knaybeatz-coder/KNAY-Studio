#include "MaximizerEditor.h"
#include "ControlSurface.h"
namespace oxygen {
MaximizerEditor::MaximizerEditor(MaximizerModule& p,juce::AudioProcessorValueTreeState& v):AudioProcessorEditor(&p),audioProcessor(p),apvts(v){
    auto setup=[&](juce::Slider& s,std::unique_ptr<SliderAttachment>& a,const juce::String& id,const juce::String& label,const juce::String& unit){addAndMakeVisible(s);a=std::make_unique<SliderAttachment>(apvts,id,s);controls::dial(s,apvts,id,label,unit);};
    setup(threshSlider,threshAttachment,"Threshold","Threshold"," dB");setup(ceilingSlider,ceilingAttachment,"Ceiling","Ceiling"," dB");setup(releaseSlider,releaseAttachment,"Release","Release"," ms");setSize(820,300);
}
MaximizerEditor::~MaximizerEditor()=default;
void MaximizerEditor::resized(){int i=0;for(auto* s:{&threshSlider,&ceilingSlider,&releaseSlider}){auto r=controls::band(getLocalBounds().reduced(10,12),i++,3);r.removeFromTop(80);r.removeFromBottom(24);s->setBounds(r.withSizeKeepingCentre(154,r.getHeight()));}}
void MaximizerEditor::paint(juce::Graphics& g){const char* names[]={"LOUDNESS","OUTPUT LIMIT","RECOVERY"};const char* descriptions[]={"Lower for more limiting","Maximum output level","Return after each peak"};int i=0;for(auto* s:{&threshSlider,&ceilingSlider,&releaseSlider}){controls::card(g,controls::band(getLocalBounds().reduced(10,12),i,3),names[i],descriptions[i]);++i;controls::label(g,*s);}}
}
