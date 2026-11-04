#include <JuceHeader.h>
#include "PluginProcessor.h"
#include "GUI/MainComponent.h"
#include "GUI/LemonLookAndFeel.h"

class OxygenAudioProcessorEditor  : public juce::AudioProcessorEditor
{
public:
    OxygenAudioProcessorEditor (OxygenAudioProcessor&);
    ~OxygenAudioProcessorEditor() override;

    void paint (juce::Graphics&) override;
    void resized() override;

private:
    OxygenAudioProcessor& audioProcessor;
    LemonLookAndFeel lemonLook;
    std::unique_ptr<MainComponent> mainComponent;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR (OxygenAudioProcessorEditor)
};
