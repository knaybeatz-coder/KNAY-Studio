#pragma once
#include "Theme.h"
namespace oxygen::controls {
inline void dial(juce::Slider& s,juce::AudioProcessorValueTreeState& state,const juce::String& id,const juce::String& label,const juce::String& unit){
    s.setName(label);s.setSliderStyle(juce::Slider::RotaryVerticalDrag);
    s.setTextBoxStyle(juce::Slider::TextBoxBelow,false,92,23);
    s.setMouseDragSensitivity(180);s.setScrollWheelEnabled(false);
    s.setTextValueSuffix(unit);
    s.setTooltip(label+". Drag vertically, or type an exact value. Double-click to restore the default.");
    auto* p=state.getParameter(id);s.setDoubleClickReturnValue(true,p->convertFrom0to1(p->getDefaultValue()));
}
inline void label(juce::Graphics& g,const juce::Slider& s){
    g.setColour(Theme::Colors::OnSurfaceVariant);g.setFont(Theme::Fonts::getBody().withHeight(12));
    g.drawText(s.getName(),s.getX()-3,s.getY()-19,s.getWidth()+6,18,juce::Justification::centred);
}
inline void card(juce::Graphics& g,juce::Rectangle<int> r,const juce::String& title,const juce::String& subtitle={}){
    g.setColour(Theme::Colors::Background);g.fillRoundedRectangle(r.toFloat(),8);
    g.setColour(Theme::Colors::OnSurface.withAlpha(.07f));g.drawRoundedRectangle(r.toFloat().reduced(.5f),8,1);
    g.setColour(Theme::Colors::Primary);g.setFont(Theme::Fonts::getBody().boldened().withHeight(13));
    g.drawText(title,r.getX()+8,r.getY()+10,r.getWidth()-16,20,juce::Justification::centred);
    if(subtitle.isNotEmpty()){g.setColour(Theme::Colors::OnSurfaceVariant);g.setFont(Theme::Fonts::getBody().withHeight(11));g.drawText(subtitle,r.getX()+8,r.getY()+31,r.getWidth()-16,18,juce::Justification::centred);}
}
inline juce::Rectangle<int> band(juce::Rectangle<int> r,int i,int count){const int w=r.getWidth()/count;return {r.getX()+i*w+4,r.getY(),w-8,r.getHeight()};}
}
