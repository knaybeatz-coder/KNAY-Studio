#include "MultibandCompressorEditor.h"
#include "ControlSurface.h"
namespace oxygen {
MultibandCompressorEditor::MultibandCompressorEditor(MultibandCompressorModule& p,juce::AudioProcessorValueTreeState& v):AudioProcessorEditor(&p),audioProcessor(p),apvts(v){
    auto setup=[&](std::unique_ptr<juce::Slider>& s,std::unique_ptr<juce::AudioProcessorValueTreeState::SliderAttachment>& a,juce::String id,juce::String label,juce::String unit){s=std::make_unique<juce::Slider>();addAndMakeVisible(*s);a=std::make_unique<juce::AudioProcessorValueTreeState::SliderAttachment>(apvts,id,*s);controls::dial(*s,apvts,id,label,unit);};
    const char* prefixes[]={"Low","LowMid","HighMid","High"};
    for(int i=0;i<4;++i){auto& b=bands[(size_t)i];juce::String prefix=prefixes[i];
        setup(b.thresh,b.threshAtt,prefix+"Thresh","Threshold"," dB");setup(b.ratio,b.ratioAtt,prefix+"Ratio","Ratio",":1");
        setup(b.attack,b.attackAtt,prefix+"Attack","Attack"," ms");setup(b.release,b.releaseAtt,prefix+"Release","Release"," ms");setup(b.gain,b.gainAtt,prefix+"Gain","Makeup"," dB");
    }
    setup(lowMidX,lowMidXAtt,"LowMidX","Low / mid crossover"," Hz");setup(midHighX,midHighXAtt,"MidHighX","Mid crossover"," Hz");setup(highX,highXAtt,"HighX","Mid / high crossover"," Hz");
    for(auto* s:{lowMidX.get(),midHighX.get(),highX.get()})s->setTextBoxStyle(juce::Slider::TextBoxRight,false,100,23);
    const char* names[]={"Low","Low mid","High mid","High"};
    for(int i=0;i<4;++i){auto& b=bandButtons[(size_t)i];addAndMakeVisible(b);b.setButtonText(names[i]);b.onClick=[this,i]{selectedBand=i;resized();repaint();};}
    setSize(820,350);
}
MultibandCompressorEditor::~MultibandCompressorEditor()=default;
void MultibandCompressorEditor::resized(){
    auto r=getLocalBounds().reduced(14,8);auto top=r.removeFromTop(78);int w=top.getWidth()/3;
    for(auto* s:{lowMidX.get(),midHighX.get(),highX.get()}){auto cell=top.removeFromLeft(w).reduced(12,0);cell.removeFromTop(20);s->setBounds(cell);}
    r.removeFromTop(12);auto tabs=r.removeFromTop(32);const int tabWidth=tabs.getWidth()/4;
    for(int i=0;i<4;++i){bandButtons[(size_t)i].setBounds(tabs.removeFromLeft(tabWidth).reduced(4,0));bandButtons[(size_t)i].setToggleState(i==selectedBand,juce::dontSendNotification);}
    r.removeFromTop(48);r.removeFromBottom(20);const int width=r.getWidth()/5;
    for(int i=0;i<4;++i){auto& b=bands[(size_t)i];auto row=r;for(auto* s:{b.thresh.get(),b.ratio.get(),b.attack.get(),b.release.get(),b.gain.get()}){s->setVisible(i==selectedBand);s->setBounds(row.removeFromLeft(width).reduced(10,0));}}
}
void MultibandCompressorEditor::paint(juce::Graphics& g){
    for(auto* s:{lowMidX.get(),midHighX.get(),highX.get()})controls::label(g,*s);
    auto r=getLocalBounds().reduced(14,8);r.removeFromTop(142);g.setColour(Theme::Colors::Background);g.fillRoundedRectangle(r.toFloat(),8);
    auto& b=bands[(size_t)selectedBand];for(auto* s:{b.thresh.get(),b.ratio.get(),b.attack.get(),b.release.get(),b.gain.get()})controls::label(g,*s);
}
}
