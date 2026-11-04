#pragma once
#include <JuceHeader.h>
#include "../Modules/EqualizerModule.h"
namespace oxygen {
class EqualizerEditor : public juce::AudioProcessorEditor, private juce::Timer {
public:
    EqualizerEditor(EqualizerModule&, juce::AudioProcessorValueTreeState&);
    ~EqualizerEditor() override;
    void paint(juce::Graphics&) override;
    void resized() override;
    void mouseDown(const juce::MouseEvent&) override;
    void mouseDrag(const juce::MouseEvent&) override;
    void mouseUp(const juce::MouseEvent&) override;
    void mouseDoubleClick(const juce::MouseEvent&) override;
    void mouseWheelMove(const juce::MouseEvent&, const juce::MouseWheelDetails&) override;
private:
    EqualizerModule& audioProcessor;
    juce::AudioProcessorValueTreeState& apvts;
    juce::Rectangle<float> graph;
    juce::Slider frequency, gain, bandwidth;
    juce::TextButton reset {"Reset band"};
    std::array<std::unique_ptr<juce::AudioProcessorValueTreeState::SliderAttachment>,3> attachments;
    int selected=7, dragging=-1;
    juce::Point<float> origin;
    float initialFreq=0, initialGain=0;
    std::array<float,384> response{}, bandResponse{};
    float value(const char*,int) const;
    float x(float) const;
    float y(float) const;
    float hz(float) const;
    float db(float) const;
    juce::Point<float> point(int) const;
    int nearest(juce::Point<float>) const;
    void select(int);
    void set(const char*,int,float);
    void gesture(bool);
    void timerCallback() override;
    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(EqualizerEditor)
};
}
