#include <JuceHeader.h>
#include "ModuleRack.h"
#include "SpectrumAnalyzer.h"
#include "LevelMeter.h"
#include "../AI/InferenceEngine.h"
#include <optional>

class OxygenAudioProcessor;
class MasterOverview;

class MainComponent : public juce::Component,
                      public juce::Timer
{
public:
    enum class AssistantUiState
    {
        idle = 0,
        listeningMasterAssistant,
        listeningReference,
        analysing,
        awaitingConfirmation
    };

    MainComponent(OxygenAudioProcessor& p);
    ~MainComponent() override;

    void paint(juce::Graphics& g) override;
    void resized() override;
    void timerCallback() override;

    void drawStereoMeterValue(juce::Graphics& g,
                              float leftLevel,
                              float rightLevel,
                              juce::Rectangle<int> bounds);
    void drawBranding(juce::Graphics& g, juce::Rectangle<float> area);
    juce::String formatLevelText(float level) const;
    void startAssistantListening();
    void startReferenceWorkflow();
    void cancelAssistantListening();
    void completeAssistantListening();
    void showAssistantApplyConfirmation(const juce::String& windowTitle,
                                        const juce::String& messageText);
    void closeAssistantListenWindow();
    void setAssistantButtonsEnabled(bool shouldEnable);
    void beginListeningSession(AssistantUiState state,
                               const juce::String& activeButtonText,
                               const juce::String& windowTitle,
                               const juce::String& windowMessage);

private:
    OxygenAudioProcessor& audioProcessor;

    oxygen::SpectrumAnalyzer spectrumAnalyzer;
    oxygen::LevelMeter inputMeterL, inputMeterR;
    oxygen::LevelMeter outputMeterL, outputMeterR;

    std::unique_ptr<ModuleRack> moduleRack;
    std::unique_ptr<MasterOverview> overview;
    juce::TextButton detailButton {"Detailed controls"};
    bool details = false;

    // SVG Assets
    std::unique_ptr<juce::Drawable> iconDrawable;
    std::unique_ptr<juce::Drawable> referenceIconDrawable;

    std::unique_ptr<juce::TextButton> referenceButton;
    std::unique_ptr<juce::TextButton> masterAssistButton;
    AssistantUiState assistantUiState = AssistantUiState::idle;
    double assistantListenProgress = 0.0;
    double assistantListenStartMs = 0.0;
    double assistantListenDurationMs = 60000.0;
    std::optional<oxygen::MasteringParameters> pendingAssistantParameters;
    juce::String pendingAssistantWindowTitle;
    juce::Component::SafePointer<juce::AlertWindow> assistantListenWindow;
    juce::Component::SafePointer<juce::AlertWindow> assistantApplyWindow;
    uint64_t assistantUiGeneration = 0;
    std::unique_ptr<juce::FileChooser> referenceFileChooser;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(MainComponent)
};
