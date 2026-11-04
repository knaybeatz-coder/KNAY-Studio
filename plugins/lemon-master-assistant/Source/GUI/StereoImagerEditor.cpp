#include "StereoImagerEditor.h"
#include "ControlSurface.h"
namespace oxygen {
StereoImagerEditor::StereoImagerEditor(StereoImagerModule& p,juce::AudioProcessorValueTreeState& v):AudioProcessorEditor(&p),audioProcessor(p),apvts(v){
    auto setup=[&](juce::Slider& s,std::unique_ptr<SliderAttachment>& a,const juce::String& id){addAndMakeVisible(s);a=std::make_unique<SliderAttachment>(apvts,id,s);controls::dial(s,apvts,id,"Stereo width","");
        s.textFromValueFunction=[](double value){return juce::String(value*100.,0)+" %";};s.valueFromTextFunction=[](const juce::String& text){return text.getDoubleValue()/100.;};s.updateText();
    };
    setup(lowWidthSlider,lowAttachment,"LowWidth");setup(lowMidWidthSlider,lowMidAttachment,"LowMidWidth");setup(highMidWidthSlider,highMidAttachment,"HighMidWidth");setup(highWidthSlider,highAttachment,"HighWidth");setSize(820,300);
}
StereoImagerEditor::~StereoImagerEditor()=default;
void StereoImagerEditor::resized(){int i=0;for(auto* s:{&lowWidthSlider,&lowMidWidthSlider,&highMidWidthSlider,&highWidthSlider}){auto r=controls::band(getLocalBounds().reduced(10,12),i++,4);r.removeFromTop(80);r.removeFromBottom(26);s->setBounds(r.reduced(24,0));}}
void StereoImagerEditor::paint(juce::Graphics& g){const char* names[]={"LOW","LOW MID","HIGH MID","HIGH"};int i=0;for(auto* s:{&lowWidthSlider,&lowMidWidthSlider,&highMidWidthSlider,&highWidthSlider}){controls::card(g,controls::band(getLocalBounds().reduced(10,12),i,4),names[i],"0% mono  /  100% original");controls::label(g,*s);++i;}}
}
