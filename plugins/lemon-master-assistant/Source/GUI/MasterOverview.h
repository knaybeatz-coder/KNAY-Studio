#pragma once
#include "Theme.h"
#include "../PluginProcessor.h"
#include "../Modules/EqualizerModule.h"
#include "../Modules/StereoImagerModule.h"
#include "../Modules/MaximizerModule.h"

// A compact mastering surface over the real, saved rack parameters. No second
// DSP or hidden macro state: reopening and assistant changes read the same values.
class MasterOverview final : public juce::Component, private juce::Timer
{
public:
    explicit MasterOverview(OxygenAudioProcessor& p) : processor(p)
    {
        for (auto& node : p.getModuleNodes()) {
            if (auto* m=dynamic_cast<oxygen::EqualizerModule*>(node->getProcessor())) eq=m;
            if (auto* m=dynamic_cast<oxygen::StereoImagerModule*>(node->getProcessor())) imager=m;
            if (auto* m=dynamic_cast<oxygen::MaximizerModule*>(node->getProcessor())) limiter=m;
        }
        for (int i=0;i<5;++i) {
            auto& k=knobs[(size_t)i]; addAndMakeVisible(k);
            k.setSliderStyle(juce::Slider::RotaryHorizontalVerticalDrag);
            k.setTextBoxStyle(juce::Slider::TextBoxBelow,false,92,22);
            k.setScrollWheelEnabled(false);
            k.setName(names[(size_t)i]);
            k.setRange(i<3 ? -12. : 0.,i<3 ? 12. : i==3 ? 200. : 60.,0.1);
            k.setTextValueSuffix(i==3 ? " %" : " dB");
            k.setDoubleClickReturnValue(true,i==3 ? 100. : 0.);
            k.setTooltip(i<3 ? "Broad tonal adjustment; preserves the assistant's finer EQ shape. Double-click for neutral."
                : i==3 ? "Stereo width above the bass band. 100% is unchanged; bass width is preserved."
                : "Limiter drive. More level means more limiting; compare at a similar listening volume.");
            k.onDragStart=[this,i]{gesture(i,true);};
            k.onDragEnd=[this,i]{gesture(i,false);};
            k.onValueChange=[this,i]{if(!refreshing) change(i,(float)knobs[(size_t)i].getValue());};
        }
        refresh();startTimerHz(20);
    }
    ~MasterOverview() override {stopTimer();}
    void resized() override {
        auto r=getLocalBounds().reduced(20);r.removeFromTop(42);
        graph=r.removeFromTop(juce::jmax(120,r.getHeight()-170)).toFloat().reduced(24,16);
        r.removeFromTop(20);
        const int w=r.getWidth()/5;
        for(auto& k:knobs) k.setBounds(r.removeFromLeft(w).reduced(9,20));
    }
    void paint(juce::Graphics& g) override {
        using namespace oxygen::Theme;
        g.setColour(Colors::SurfaceVariant);g.fillRoundedRectangle(getLocalBounds().toFloat().reduced(5),8);
        g.setColour(Colors::OnSurface);g.setFont(17.f);
        g.drawText("Shape your master",20,12,getWidth()-40,24,juce::Justification::centredLeft);
        g.setColour(Colors::OnSurfaceVariant);g.setFont(12.f);
        g.drawText("Run Master Assist, apply the result, then fine-tune. Drag the three tone handles.",20,38,getWidth()-40,20,juce::Justification::centredLeft);
        g.setColour(juce::Colour(0xff1c1c1c));g.fillRoundedRectangle(graph.expanded(12,4),5);
        for(float db:{-12.f,-6.f,0.f,6.f,12.f}) {
            g.setColour(Colors::OnSurface.withAlpha(db==0 ? .20f : .07f));
            g.drawHorizontalLine((int)y(db),graph.getX(),graph.getRight());
            g.setColour(Colors::OnSurfaceVariant);g.setFont(10.f);
            g.drawText(juce::String((int)db),graph.getX()+3,y(db)-14,24,12,juce::Justification::left);
        }
        for(float hz:{30.f,100.f,1000.f,10000.f}) {
            const float xx=x(hz);g.setColour(Colors::OnSurface.withAlpha(.08f));
            g.drawVerticalLine((int)xx,graph.getY(),graph.getBottom());
            g.setColour(Colors::OnSurfaceVariant);
            g.drawText(hz>=1000 ? juce::String((int)(hz/1000))+"k" : juce::String((int)hz),xx-15,graph.getBottom()+4,30,14,juce::Justification::centred);
        }
        juce::Path line;for(size_t i=0;i<response.size();++i) {
            const float xx=graph.getX()+graph.getWidth()*(float)i/(float)(response.size()-1);
            const float yy=y(response[i]);if(i==0)line.startNewSubPath(xx,yy);else line.lineTo(xx,yy);
        }
        g.setColour(Colors::Primary.withAlpha(.08f));auto fill=line;
        fill.lineTo(graph.getRight(),graph.getBottom());fill.lineTo(graph.getX(),graph.getBottom());fill.closeSubPath();g.fillPath(fill);
        g.setColour(Colors::Primary);g.strokePath(line,juce::PathStrokeType(2.f));
        for(int i=0;i<3;++i) {
            auto pt=handle(i);g.setColour(Colors::Background);g.fillEllipse(pt.x-7,pt.y-7,14,14);
            g.setColour(Colors::Primary);g.drawEllipse(pt.x-7,pt.y-7,14,14,2);
            g.setFont(12.f);g.drawText(names[(size_t)i],pt.x-35,pt.y-27,70,18,juce::Justification::centred);
        }
        for(int i=0;i<5;++i) {
            const auto r=knobs[(size_t)i].getBounds();g.setColour(Colors::OnSurface);g.setFont(13.f);
            g.drawText(names[(size_t)i],r.getX(),r.getY()-19,r.getWidth(),18,juce::Justification::centred);
        }
    }
    void mouseDown(const juce::MouseEvent& e) override {
        for(int i=0;i<3;++i) if(handle(i).getDistanceFrom(e.position)<24) {
            dragging=i;startY=e.position.y;startValue=value(i);gesture(i,true);return;
        }
    }
    void mouseDrag(const juce::MouseEvent& e) override {
        if(dragging>=0) {change(dragging,juce::jlimit(-12.f,12.f,startValue+(startY-e.position.y)*24.f/graph.getHeight()));refresh();}
    }
    void mouseUp(const juce::MouseEvent&) override {if(dragging>=0)gesture(dragging,false);dragging=-1;}
private:
    OxygenAudioProcessor& processor;
    oxygen::EqualizerModule* eq=nullptr;
    oxygen::StereoImagerModule* imager=nullptr;
    oxygen::MaximizerModule* limiter=nullptr;
    std::array<juce::Slider,5> knobs;
    const std::array<juce::String,5> names{"Bass","Body","Air","Width","Loudness"};
    std::array<float,256> response{};
    juce::Rectangle<float> graph;
    bool refreshing=false;int dragging=-1;float startY=0,startValue=0;
    std::vector<juce::RangedAudioParameter*> parameters(int i) const {
        std::vector<juce::RangedAudioParameter*> out;
        if(i<3 && eq) for(int b=0;b<15;++b) {
            const float hz=eq->apvts.getRawParameterValue("Freq"+juce::String(b))->load();
            const int group=hz<250.f?0:hz<3000.f?1:2;
            if(group==i)out.push_back(eq->apvts.getParameter("Gain"+juce::String(b)));
        }
        if(i==3 && imager)for(auto id:{"LowMidWidth","HighMidWidth","HighWidth"})out.push_back(imager->apvts.getParameter(id));
        if(i==4 && limiter)out.push_back(limiter->apvts.getParameter("Threshold"));
        return out;
    }
    float value(int i) const {
        auto ps=parameters(i);float sum=0;for(auto* p:ps)sum+=p->convertFrom0to1(p->getValue());
        const float v=ps.empty()?0:sum/(float)ps.size();return i==3?v*100:i==4?-v:v;
    }
    void gesture(int i,bool begin) {for(auto* p:parameters(i)) {if(begin)p->beginChangeGesture();else p->endChangeGesture();}}
    void change(int i,float v) {
        const float delta=i==3?(v-value(i))/100.f:i==4?-(v-value(i)):v-value(i);
        for(auto* p:parameters(i)) {
            const auto& range=p->getNormalisableRange();
            p->setValueNotifyingHost(p->convertTo0to1(juce::jlimit(range.start,range.end,p->convertFrom0to1(p->getValue())+delta)));
        }
        // A deliberate control move re-enables the corresponding processing module.
        auto* bypass=i<3&&eq?eq->apvts.getParameter("Bypass"):i==3&&imager?imager->apvts.getParameter("Bypass"):limiter?limiter->apvts.getParameter("Bypass"):nullptr;
        if(bypass)bypass->setValueNotifyingHost(0);
        refresh();
    }
    float x(float f) const {return graph.getX()+std::log(f/20.f)/std::log(1000.f)*graph.getWidth();}
    float y(float db) const {return graph.getCentreY()-juce::jlimit(-15.f,15.f,db)*graph.getHeight()/30.f;}
    juce::Point<float> handle(int i) const {
        const float hz=i==0?80.f:i==1?1000.f:10000.f;
        const auto index=(size_t)juce::jlimit(0,255,(int)(std::log(hz/20.f)/std::log(1000.f)*255.f));
        return {x(hz),y(response[index])};
    }
    void timerCallback() override {refresh();}
    void refresh() {
        refreshing=true;
        for(int i=0;i<5;++i)if(!knobs[(size_t)i].isMouseButtonDown())knobs[(size_t)i].setValue(value(i),juce::dontSendNotification);
        refreshing=false;
        response.fill(0);
        const double rate=processor.getSampleRate()>0?processor.getSampleRate():48000.;
        if(eq && eq->apvts.getRawParameterValue("Bypass")->load()<.5f)for(int b=0;b<15;++b) {
            const float db=eq->apvts.getRawParameterValue("Gain"+juce::String(b))->load();
            auto coefficients=juce::dsp::IIR::Coefficients<float>::makePeakFilter(rate,juce::jlimit(20.f,(float)rate*.45f,eq->apvts.getRawParameterValue("Freq"+juce::String(b))->load()),eq->apvts.getRawParameterValue("Q"+juce::String(b))->load(),juce::Decibels::decibelsToGain(db));
            for(size_t i=0;i<response.size();++i) {
                const double f=juce::jmin(rate*.49,20.*std::pow(1000.,(double)i/(response.size()-1)));
                response[i]+=juce::Decibels::gainToDecibels((float)coefficients->getMagnitudeForFrequency(f,rate));
            }
        }
        repaint();
    }
};
