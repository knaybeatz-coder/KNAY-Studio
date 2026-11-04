#include "DynamicEqEditor.h"
#include "ControlSurface.h"
namespace oxygen {
DynamicEqEditor::DynamicEqEditor(DynamicEqModule& p,juce::AudioProcessorValueTreeState& v):AudioProcessorEditor(&p),audioProcessor(p),apvts(v){
    const char* prefixes[]={"Low","LowMid","HighMid","High"};
    for(int i=0;i<4;++i){auto& b=bands[(size_t)i];juce::String prefix=prefixes[i];
        b.threshold=std::make_unique<juce::Slider>();b.range=std::make_unique<juce::Slider>();
        addAndMakeVisible(*b.threshold);addAndMakeVisible(*b.range);
        b.thresholdAttachment=std::make_unique<juce::AudioProcessorValueTreeState::SliderAttachment>(apvts,prefix+"Thresh",*b.threshold);
        b.rangeAttachment=std::make_unique<juce::AudioProcessorValueTreeState::SliderAttachment>(apvts,prefix+"Range",*b.range);
        controls::dial(*b.threshold,apvts,prefix+"Thresh","Threshold"," dB");controls::dial(*b.range,apvts,prefix+"Range","Max reduction"," dB");
    }setSize(820,300);
}
DynamicEqEditor::~DynamicEqEditor()=default;
void DynamicEqEditor::resized(){for(int i=0;i<4;++i){auto r=controls::band(getLocalBounds().reduced(10,12),i,4);r.removeFromTop(80);r.removeFromBottom(26);int w=r.getWidth()/2;bands[(size_t)i].threshold->setBounds(r.removeFromLeft(w).reduced(2,0));bands[(size_t)i].range->setBounds(r.reduced(2,0));}}
void DynamicEqEditor::paint(juce::Graphics& g){
    const char* names[]={"LOW CLEAN","BODY","PRESENCE","AIR"};const char* freq[]={"140 Hz","380 Hz","3.2 kHz","9.6 kHz"};
    for(int i=0;i<4;++i){controls::card(g,controls::band(getLocalBounds().reduced(10,12),i,4),names[i],freq[i]);controls::label(g,*bands[(size_t)i].threshold);controls::label(g,*bands[(size_t)i].range);}
}
}
