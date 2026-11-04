#pragma once
#include <JuceHeader.h>

// Expose the rack's real parameters to the host. Upstream stores the child
// states, but does not forward child edits/gestures to its outer processor.
// This makes Lemon's dirty tracking and automation see the actual controls.
class ForwardParameter final : public juce::AudioProcessorParameterWithID,
                               private juce::AudioProcessorParameter::Listener
{
public:
    ForwardParameter(const juce::String& id, const juce::String& name, juce::RangedAudioParameter& p)
        : AudioProcessorParameterWithID(juce::ParameterID(id, 1), name,
              juce::AudioProcessorParameterWithIDAttributes().withLabel(p.getLabel())), child(&p)
    { child->addListener(this); }
    ~ForwardParameter() override { disconnect(); }
    void disconnect() { if (child) child->removeListener(this); child = nullptr; }
    float getValue() const override { return child ? child->getValue() : 0; }
    void setValue(float value) override {
        if (!child) return;
        fromHost.store(true);
        child->setValueNotifyingHost(value);
        fromHost.store(false);
    }
    float getDefaultValue() const override { return child ? child->getDefaultValue() : 0; }
    int getNumSteps() const override { return child ? child->getNumSteps() : 0; }
    bool isDiscrete() const override { return child && child->isDiscrete(); }
    bool isBoolean() const override { return child && child->isBoolean(); }
    juce::String getText(float value, int length) const override { return child ? child->getText(value,length) : juce::String(); }
    float getValueForText(const juce::String& text) const override { return child ? child->getValueForText(text) : 0; }
private:
    void parameterValueChanged(int, float value) override {
        if (!fromHost.load()) sendValueChangedMessageToListeners(value);
    }
    void parameterGestureChanged(int, bool starting) override {
        if (starting) beginChangeGesture(); else endChangeGesture();
    }
    juce::RangedAudioParameter* child;
    std::atomic<bool> fromHost {false};
};
