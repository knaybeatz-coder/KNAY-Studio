#include "MainComponent.h"
#include "MasterOverview.h"
#include "Theme.h"
#include "LemonLookAndFeel.h"
#include "../PluginProcessor.h"
#include <BinaryData.h>
#include <memory>

using namespace oxygen;

namespace
{
    constexpr int defaultGuiWidth = 960;
    constexpr int defaultGuiHeight = 740;
    constexpr int headerHeight = 96;
    constexpr int meterTextHeight = 30;
    constexpr int headerButtonWidth = 172;
    constexpr int headerButtonGap = 8;
    constexpr int dialogTextWidth = 360;
    constexpr int dialogTextHeight = 88;

    class DialogMessageComponent final : public juce::Component
    {
    public:
        explicit DialogMessageComponent(juce::String messageText)
            : text(std::move(messageText))
        {
            setInterceptsMouseClicks(false, false);
            setSize(dialogTextWidth, dialogTextHeight);
        }

        void setText(juce::String messageText)
        {
            text = std::move(messageText);
            repaint();
        }

        void paint(juce::Graphics& g) override
        {
            juce::AttributedString attributedText;
            attributedText.append(text,
                                  oxygen::Theme::Fonts::getBody().withHeight(14.0f),
                                  oxygen::Theme::Colors::OnSurface);
            attributedText.setJustification(juce::Justification::centred);
            attributedText.setWordWrap(juce::AttributedString::WordWrap::byWord);

            juce::TextLayout layout;
            const auto contentBounds = getLocalBounds().reduced(8, 8);
            layout.createLayout(attributedText, (float) contentBounds.getWidth());

            const auto layoutHeight = (int) std::ceil(layout.getHeight());
            const auto y = contentBounds.getY()
                         + juce::jmax(0, (contentBounds.getHeight() - layoutHeight) / 2);
            layout.draw(g, juce::Rectangle<float>((float) contentBounds.getX(),
                                                  (float) y,
                                                  (float) contentBounds.getWidth(),
                                                  (float) layoutHeight));
        }

    private:
        juce::String text;
    };

    juce::String shortenForDialog(const juce::String& text, int maxLength)
    {
        if (text.length() <= maxLength)
            return text;

        const auto headLength = juce::jmax(8, maxLength / 2 - 2);
        const auto tailLength = juce::jmax(8, maxLength - headLength - 3);
        return text.substring(0, headLength) + "..." + text.substring(text.length() - tailLength);
    }

    class AssistantDialog final : public juce::AlertWindow
    {
    public:
        AssistantDialog(const juce::String& title, const juce::String& text,
                        juce::MessageBoxIconType icon)
            : juce::AlertWindow(title, {}, icon), message(text)
        {
            // This modal can outlive its editor: own the theme rather than borrowing it.
            setLookAndFeel(&dialogLook);
            addCustomComponent(&message);
        }

        ~AssistantDialog() override { removeCustomComponent(0); setLookAndFeel(nullptr); }

        LemonLookAndFeel dialogLook;
        DialogMessageComponent message;
        // The modal may outlive the editor until its dismissal callback runs.
        double progress = 0.0;
    };

}

MainComponent::MainComponent(OxygenAudioProcessor& p)
    : audioProcessor(p),
      spectrumAnalyzer(p.getInputAudioBufferQueue(),
                       p.getOutputAudioBufferQueue(),
                       [&p] { return p.getSampleRate(); })
{
    // Rack
    moduleRack = std::make_unique<ModuleRack>(p);
    addChildComponent(moduleRack.get());
    overview=std::make_unique<MasterOverview>(p);addAndMakeVisible(*overview);
    addAndMakeVisible(detailButton);
    detailButton.onClick=[this] {
        details=!details;moduleRack->setVisible(details);overview->setVisible(!details);
        detailButton.setButtonText(details?"Simple controls":"Detailed controls");resized();
    };
    moduleRack->updateModuleList(); // Fix: Populate the rack on startup
    audioProcessor.setGraphChangedCallback([this]
    {
        if (moduleRack)
            moduleRack->updateModuleList();
    });

    // Spectrum Analyzer (Visualizer)
    addAndMakeVisible(spectrumAnalyzer);

    // Meters
    addAndMakeVisible(inputMeterL);
    addAndMakeVisible(inputMeterR);
    addAndMakeVisible(outputMeterL);
    addAndMakeVisible(outputMeterR);

    if (auto iconXml = juce::parseXML(juce::String::fromUTF8(BinaryData::oxygen_icon_svg,
                                                             BinaryData::oxygen_icon_svgSize)))
    {
        iconDrawable = juce::Drawable::createFromSVG(*iconXml);
    }

    if (auto referenceIconXml = juce::parseXML(juce::String::fromUTF8(BinaryData::reference_icon_svg,
                                                                      BinaryData::reference_icon_svgSize)))
    {
        referenceIconDrawable = juce::Drawable::createFromSVG(*referenceIconXml);
    }

    referenceButton = std::make_unique<juce::TextButton>("Reference match");

    referenceButton->setButtonText("REFERENCE");
    referenceButton->onClick = [this]
    {
        startReferenceWorkflow();
    };
    addAndMakeVisible(referenceButton.get());

    // Assistant Button
    masterAssistButton = std::make_unique<juce::TextButton>("Master Assist");

    masterAssistButton->setButtonText("MASTER ASSIST");
    masterAssistButton->setColour(juce::TextButton::textColourOnId, oxygen::Theme::Colors::Primary);
    masterAssistButton->onClick = [this]
    {
        startAssistantListening();
    };
    addAndMakeVisible(masterAssistButton.get());

    // Keep the full mastering chain visible without forcing viewport scroll.
    setSize(defaultGuiWidth, defaultGuiHeight);
    startTimerHz(60);   // Smoother 60fps update
}

MainComponent::~MainComponent()
{
    cancelAssistantListening();
    stopTimer();
    audioProcessor.setGraphChangedCallback({});
}

void MainComponent::timerCallback()
{
    inputMeterL.setLevel(audioProcessor.getInputLevel(0));
    inputMeterR.setLevel(audioProcessor.getInputLevel(1));
    outputMeterL.setLevel(audioProcessor.getOutputLevel(0));
    outputMeterR.setLevel(audioProcessor.getOutputLevel(1));

    if (assistantUiState == AssistantUiState::listeningMasterAssistant
        || assistantUiState == AssistantUiState::listeningReference)
    {
        const auto nowMs = juce::Time::getMillisecondCounterHiRes();
        const auto elapsedMs = nowMs - assistantListenStartMs;
        assistantListenProgress = juce::jlimit(0.0, 1.0, elapsedMs / assistantListenDurationMs);

        if (auto* window = dynamic_cast<AssistantDialog*>(assistantListenWindow.getComponent()))
        {
            window->progress = assistantListenProgress;
            window->repaint();
        }

        if (assistantListenProgress >= 1.0)
            completeAssistantListening();
    }
}

void MainComponent::resized()
{
    auto area = getLocalBounds();

    // Header
    auto headerArea = area.removeFromTop(headerHeight);

    auto buttonGroupArea = headerArea.removeFromRight((headerButtonWidth * 2) + headerButtonGap + 12).reduced(6, 10);
    buttonGroupArea.removeFromLeft(4);

    if (referenceButton.get() != nullptr)
    {
        auto referenceArea = buttonGroupArea.removeFromLeft(headerButtonWidth);
        referenceButton->setBounds(referenceArea.withSizeKeepingCentre(headerButtonWidth,44));
    }

    buttonGroupArea.removeFromLeft(headerButtonGap);

    if (masterAssistButton.get() != nullptr)
    {
        auto masterArea = buttonGroupArea.removeFromLeft(headerButtonWidth);
        masterAssistButton->setBounds(masterArea.withSizeKeepingCentre(headerButtonWidth,44));
    }

    // Spectrum Analyzer (Top 120px - Slightly more compact)
    auto visualizerArea = area.removeFromTop(120).reduced(10, 5);
    spectrumAnalyzer.setBounds(visualizerArea);

    // Meters Sidebar (Left/Right) - Adjusted to fit new layout
    auto leftSidebar = area.removeFromLeft(46);
    auto rightSidebar = area.removeFromRight(46);

    // Segmented Meters (Left)
    auto leftMetersArea = leftSidebar.removeFromTop(leftSidebar.getHeight() - meterTextHeight);
    inputMeterL.setBounds(leftMetersArea.removeFromLeft(23).reduced(2, 0));
    inputMeterR.setBounds(leftMetersArea.reduced(2, 0));

    // Segmented Meters (Right)
    auto rightMetersArea = rightSidebar.removeFromTop(rightSidebar.getHeight() - meterTextHeight);
    outputMeterL.setBounds(rightMetersArea.removeFromLeft(23).reduced(2, 0));
    outputMeterR.setBounds(rightMetersArea.reduced(2, 0));

    auto footer=area.removeFromBottom(34);
    detailButton.setBounds(footer.removeFromRight(150).reduced(3));
    moduleRack->setBounds(area.reduced(5));
    overview->setBounds(area.reduced(5));
}

void MainComponent::paint(juce::Graphics& g)
{
    // Modern Dark Gradient Background
    g.fillAll(oxygen::Theme::Colors::Background);

    // Subtle header separator or glow
    g.setColour(oxygen::Theme::Colors::OnSurfaceVariant.withAlpha(0.1f));
    g.drawHorizontalLine(headerHeight, 0.0f, (float) getWidth());

    drawBranding(g, getLocalBounds().removeFromTop(headerHeight).reduced(10, 5).toFloat());

    // Draw Grid for Visualizer (Background)
    auto visBounds = spectrumAnalyzer.getBounds().toFloat();
    if (!visBounds.isEmpty())
    {
        g.setColour(oxygen::Theme::Colors::OnSurfaceVariant.withAlpha(0.1f));
        g.drawRect(visBounds);

        // Frequency Lines (Logarithmic approx)
        float freqs[] = { 100.0f, 1000.0f, 10000.0f };
        for (float f : freqs)
        {
            float normX = std::log10(f / 20.0f) / std::log10(20000.0f / 20.0f);
            if (normX >= 0.0f && normX <= 1.0f)
                g.drawVerticalLine((int)(visBounds.getX() + normX * visBounds.getWidth()), visBounds.getY(), visBounds.getBottom());
        }

        // dB Lines
        float dbs[] = { -6.0f, -12.0f, -24.0f };
        for (float db : dbs)
        {
            // Assuming simplified linear mapping for now or map to analyzer range
             float normY = std::abs(db) / 60.0f; // Approx range 0 to -60
             if (normY <= 1.0f)
                 g.drawHorizontalLine((int)(visBounds.getY() + normY * visBounds.getHeight()), visBounds.getX(), visBounds.getRight());
        }
    }

    // Draw Meter Text Values
    drawStereoMeterValue(g,
                         audioProcessor.getInputLevel(0),
                         audioProcessor.getInputLevel(1),
                         { inputMeterL.getX(),
                           inputMeterL.getBottom() + 4,
                           inputMeterR.getRight() - inputMeterL.getX(),
                           meterTextHeight - 4 });
    drawStereoMeterValue(g,
                         audioProcessor.getOutputLevel(0),
                         audioProcessor.getOutputLevel(1),
                         { outputMeterL.getX(),
                           outputMeterL.getBottom() + 4,
                           outputMeterR.getRight() - outputMeterL.getX(),
                           meterTextHeight - 4 });
}

void MainComponent::drawStereoMeterValue(juce::Graphics& g,
                                         float leftLevel,
                                         float rightLevel,
                                         juce::Rectangle<int> bounds)
{
    g.setColour(oxygen::Theme::Colors::OnSurfaceVariant);
    g.setFont(oxygen::Theme::Fonts::getBody().withHeight(10.0f));

    auto topLine = bounds.removeFromTop(bounds.getHeight() / 2);
    g.drawText("L " + formatLevelText(leftLevel), topLine, juce::Justification::centred, false);
    g.drawText("R " + formatLevelText(rightLevel), bounds, juce::Justification::centred, false);
}

juce::String MainComponent::formatLevelText(float level) const
{
    const float db = juce::Decibels::gainToDecibels(level);
    return (db < -60.0f) ? "-inf" : juce::String(db, 1) + " dB";
}

void MainComponent::startAssistantListening()
{
    if (assistantUiState != AssistantUiState::idle)
        return;

    beginListeningSession(AssistantUiState::listeningMasterAssistant,
                          "LISTENING...",
                          "Master Assistant Listening",
                          "Listening to your mix now.\n"
                          "Loudness may rise after processing.\n"
                          "Play the loudest section if needed.");
}

void MainComponent::startReferenceWorkflow()
{
    if (assistantUiState != AssistantUiState::idle)
        return;

    referenceFileChooser = std::make_unique<juce::FileChooser>("Choose a reference audio file",
                                                               juce::File(),
                                                               "*.wav;*.flac");

    juce::Component::SafePointer<MainComponent> safeThis(this);
    referenceFileChooser->launchAsync(juce::FileBrowserComponent::openMode
                                      | juce::FileBrowserComponent::canSelectFiles,
                                      [safeThis] (const juce::FileChooser& chooser)
                                      {
                                          if (safeThis == nullptr)
                                              return;

                                          const juce::File selectedFile = chooser.getResult();
                                          safeThis->referenceFileChooser.reset();

                                          if (selectedFile == juce::File())
                                              return;

                                          juce::MouseCursor::showWaitCursor();
                                          juce::String errorMessage;
                                          if (!safeThis->audioProcessor.loadReferenceAudioFile(selectedFile, errorMessage))
                                          {
                                              juce::MouseCursor::hideWaitCursor();
                                              juce::AlertWindow::showMessageBoxAsync(juce::AlertWindow::WarningIcon,
                                                                                     "Reference Match",
                                                                                     errorMessage);
                                              return;
                                          }
                                          juce::MouseCursor::hideWaitCursor();

                                         safeThis->beginListeningSession(AssistantUiState::listeningReference,
                                                                         "LISTENING...",
                                                                         "Reference Listening",
                                                                         "Reference ready.\n"
                                                                         "Listening to your mix now.\n"
                                                                         "Play the loudest section.");
                                      });
}

void MainComponent::cancelAssistantListening()
{
    ++assistantUiGeneration;
    audioProcessor.cancelAssistantAnalysis();
    if (auto* window = assistantApplyWindow.getComponent())
        window->exitModalState(0);
    assistantApplyWindow = nullptr;
    assistantUiState = AssistantUiState::idle;
    assistantListenProgress = 0.0;
    assistantListenStartMs = 0.0;
    setAssistantButtonsEnabled(true);
    if (masterAssistButton != nullptr)
        masterAssistButton->setButtonText("MASTER ASSIST");
    if (referenceButton != nullptr)
        referenceButton->setButtonText("REFERENCE");
    pendingAssistantParameters.reset();
    pendingAssistantWindowTitle.clear();
    audioProcessor.clearLoadedReferenceAudio();

    closeAssistantListenWindow();
}

void MainComponent::completeAssistantListening()
{
    if (assistantUiState != AssistantUiState::listeningMasterAssistant
        && assistantUiState != AssistantUiState::listeningReference)
        return;

    const auto completedState = assistantUiState;
    if (!audioProcessor.hasAssistantCapturedSignal())
    {
        cancelAssistantListening();
        juce::AlertWindow::showMessageBoxAsync(juce::AlertWindow::WarningIcon,
                                               completedState == AssistantUiState::listeningReference
                                                   ? "Reference Match"
                                                   : "Master Assistant",
                                               "No audio signal was detected.\n"
                                               "Play the loudest section of your mix and try again.");
        if (completedState == AssistantUiState::listeningReference)
            audioProcessor.clearLoadedReferenceAudio();
        return;
    }

    assistantUiState = AssistantUiState::analysing;
    assistantListenProgress = -1.0; // Indeterminate while the captured audio is analysed.
    if (auto* window = dynamic_cast<AssistantDialog*>(assistantListenWindow.getComponent()))
    {
        window->progress = assistantListenProgress;
        window->message.setText("Listening complete.\nAnalyzing audio and preparing your master...\nPlease wait for the Apply confirmation.");
        window->repaint();
    }
    setAssistantButtonsEnabled(false);
    if (masterAssistButton != nullptr)
        masterAssistButton->setButtonText(completedState == AssistantUiState::listeningMasterAssistant
                                             ? "ANALYSING..." : "MASTER ASSIST");
    if (referenceButton != nullptr)
        referenceButton->setButtonText(completedState == AssistantUiState::listeningReference
                                          ? "ANALYSING..." : "REFERENCE");
    const bool reference = completedState == AssistantUiState::listeningReference;
    const auto referenceName = audioProcessor.getLoadedReferenceName();
    juce::Component::SafePointer<MainComponent> safeThis(this);
    const auto generation = assistantUiGeneration;
    audioProcessor.createAssistantSuggestionAsync(reference,
        [safeThis, reference, referenceName, generation](bool success, oxygen::MasteringParameters suggestion, juce::String error)
        {
            if (safeThis == nullptr || safeThis->assistantUiGeneration != generation
                || safeThis->assistantUiState != AssistantUiState::analysing)
                return;
            safeThis->closeAssistantListenWindow();
            safeThis->assistantUiState = AssistantUiState::awaitingConfirmation;
            if (safeThis->masterAssistButton != nullptr)
                safeThis->masterAssistButton->setButtonText("MASTER ASSIST");
            if (safeThis->referenceButton != nullptr)
                safeThis->referenceButton->setButtonText("REFERENCE");
            const juce::String title = reference ? "Reference Match" : "Master Assistant";
            if (!success)
            {
                safeThis->assistantUiState = AssistantUiState::idle;
                safeThis->setAssistantButtonsEnabled(true);
                juce::AlertWindow::showMessageBoxAsync(juce::AlertWindow::WarningIcon, title,
                    error.isNotEmpty() ? error : "Unable to build the mastering suggestion.\nTry listening again with a stronger signal.");
                if (reference) safeThis->audioProcessor.clearLoadedReferenceAudio();
                return;
            }
            safeThis->pendingAssistantParameters = suggestion;
            safeThis->pendingAssistantWindowTitle = title;
            safeThis->showAssistantApplyConfirmation(title,
                reference ? "Reference analysis complete.\nApply the matched settings from \""
                            + shortenForDialog(referenceName, 28) + "\"?"
                          : "Analysis complete.\nApply the recommended mastering settings to the modules?");
        });
}

void MainComponent::showAssistantApplyConfirmation(const juce::String& windowTitle,
                                                   const juce::String& messageText)
{
    if (!pendingAssistantParameters.has_value())
        return;

    auto* confirmationWindow = new AssistantDialog(windowTitle, messageText, juce::AlertWindow::QuestionIcon);
    confirmationWindow->addButton("Apply", 1);
    confirmationWindow->addButton("Cancel", 0, juce::KeyPress(juce::KeyPress::escapeKey));
    confirmationWindow->setAlwaysOnTop(true);
    confirmationWindow->centreAroundComponent(this, confirmationWindow->getWidth(), confirmationWindow->getHeight() + 16);

    juce::Component::SafePointer<MainComponent> safeThis(this);
    assistantApplyWindow = confirmationWindow;
    const auto generation = assistantUiGeneration;
    confirmationWindow->enterModalState(true,
                                        juce::ModalCallbackFunction::create([safeThis, generation](int result)
                                        {
                                            if (safeThis == nullptr || safeThis->assistantUiGeneration != generation)
                                                return;

                                            safeThis->assistantApplyWindow = nullptr;
                                            safeThis->assistantUiState = AssistantUiState::idle;
                                            safeThis->setAssistantButtonsEnabled(true);

                                            if (result == 1 && safeThis->pendingAssistantParameters.has_value())
                                            {
                                                if (!safeThis->audioProcessor.applyMasterAssistantSuggestion(*safeThis->pendingAssistantParameters))
                                                {
                                                    juce::AlertWindow::showMessageBoxAsync(juce::AlertWindow::WarningIcon,
                                                                                           safeThis->pendingAssistantWindowTitle,
                                                                                           "Unable to apply settings.\n"
                                                                                           "No valid audio profile is available.");
                                                }
                                            }

                                            safeThis->audioProcessor.clearLoadedReferenceAudio();
                                            safeThis->pendingAssistantParameters.reset();
                                            safeThis->pendingAssistantWindowTitle.clear();
                                        }),
                                        true);
}

void MainComponent::setAssistantButtonsEnabled(bool shouldEnable)
{
    if (masterAssistButton != nullptr)
        masterAssistButton->setEnabled(shouldEnable);

    if (referenceButton != nullptr)
        referenceButton->setEnabled(shouldEnable);
}

void MainComponent::beginListeningSession(AssistantUiState state,
                                          const juce::String& activeButtonText,
                                          const juce::String& windowTitle,
                                          const juce::String& windowMessage)
{
    closeAssistantListenWindow();

    ++assistantUiGeneration;
    audioProcessor.resetAssistantAnalysisCapture();
    assistantUiState = state;
    assistantListenProgress = 0.0;
    assistantListenStartMs = juce::Time::getMillisecondCounterHiRes();

    if (masterAssistButton != nullptr)
        masterAssistButton->setButtonText(state == AssistantUiState::listeningMasterAssistant
                                              ? activeButtonText
                                              : "MASTER ASSIST");
    if (referenceButton != nullptr)
        referenceButton->setButtonText(state == AssistantUiState::listeningReference
                                           ? activeButtonText
                                           : "REFERENCE");
    setAssistantButtonsEnabled(false);

    auto* listenWindow = new AssistantDialog(windowTitle, windowMessage, juce::AlertWindow::NoIcon);
    listenWindow->addProgressBarComponent(listenWindow->progress);
    listenWindow->addButton("Cancel", 0, juce::KeyPress(juce::KeyPress::escapeKey));
    listenWindow->setAlwaysOnTop(true);
    // Keep JUCE's measured layout: forcing a shorter window clips its already-positioned Cancel button.
    listenWindow->centreAroundComponent(this, listenWindow->getWidth(), listenWindow->getHeight() + 16);

    juce::Component::SafePointer<MainComponent> safeThis(this);
    const auto generation = assistantUiGeneration;
    listenWindow->enterModalState(true,
                                  juce::ModalCallbackFunction::create([safeThis, generation](int result)
                                  {
                                      if (safeThis == nullptr || safeThis->assistantUiGeneration != generation)
                                          return;

                                      if (safeThis->assistantUiState != AssistantUiState::idle && result == 0)
                                          safeThis->cancelAssistantListening();
                                  }),
                                  true);
    assistantListenWindow = listenWindow;
}

void MainComponent::closeAssistantListenWindow()
{
    auto* listenWindow = assistantListenWindow.getComponent();
    assistantListenWindow = nullptr;

    if (listenWindow == nullptr)
        return;

    if (listenWindow->isCurrentlyModal())
        listenWindow->exitModalState(1); // Programmatic completion is not Cancel.
    else
        delete listenWindow;
}

void MainComponent::drawBranding(juce::Graphics& g, juce::Rectangle<float> area)
{
    auto text = area.reduced(12, 8);
    g.setColour(oxygen::Theme::Colors::Primary);
    g.setFont(juce::Font("Segoe UI", 25.0f, juce::Font::bold));
    g.drawText("LEMON MASTER", text.removeFromTop(30), juce::Justification::centredLeft);
    g.setColour(oxygen::Theme::Colors::OnSurface);
    g.setFont(juce::Font("Segoe UI", 18.0f, juce::Font::bold));
    g.drawText("ASSISTANT  /  MASTERING SUITE", text.removeFromTop(24), juce::Justification::centredLeft);
    g.setColour(oxygen::Theme::Colors::OnSurfaceVariant);
    g.setFont(12.0f);
    g.drawText("Lemon Studio / oXygen engine", text, juce::Justification::centredLeft);
}
