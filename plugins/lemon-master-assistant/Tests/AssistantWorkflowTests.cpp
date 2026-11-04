#include <JuceHeader.h>
#include "../Source/PluginProcessor.h"
#include "../Source/GUI/MainComponent.h"
#include <chrono>
#include <iostream>
#include <stdexcept>

namespace
{
void require(bool condition, const char* message)
{
    if (!condition) throw std::runtime_error(message);
}
void pump(int milliseconds = 20)
{
    juce::MessageManager::getInstance()->runDispatchLoopUntil(milliseconds);
}
juce::AudioBuffer<float> makeAudio(int seconds = 3)
{
    juce::AudioBuffer<float> audio(2, seconds * 48000);
    for (int n = 0; n < audio.getNumSamples(); ++n)
    {
        const double phase = juce::MathConstants<double>::twoPi * n / 48000.0;
        audio.setSample(0, n, (float) (0.09 * std::sin(phase * 100.0) + 0.03 * std::sin(phase * 1300.0)));
        audio.setSample(1, n, (float) (0.08 * std::sin(phase * 100.0) + 0.04 * std::sin(phase * 2700.0)));
    }
    return audio;
}
void capture(OxygenAudioProcessor& processor, juce::AudioBuffer<float> audio)
{
    juce::MidiBuffer midi;
    for (int start = 0; start < audio.getNumSamples(); start += 512)
    {
        juce::AudioBuffer<float> block(audio.getArrayOfWritePointers(), 2, start,
            juce::jmin(512, audio.getNumSamples() - start));
        processor.processBlock(block, midi);
    }
}
juce::MemoryBlock state(OxygenAudioProcessor& processor)
{
    juce::MemoryBlock result;
    processor.getStateInformation(result);
    return result;
}
juce::AlertWindow* currentWindow()
{
    return dynamic_cast<juce::AlertWindow*>(juce::Component::getCurrentlyModalComponent());
}
void waitForApply()
{
    const auto deadline = juce::Time::getMillisecondCounterHiRes() + 15000.0;
    while (juce::Time::getMillisecondCounterHiRes() < deadline)
    {
        if (auto* dialog = currentWindow(); dialog != nullptr && dialog->getButton("Apply") != nullptr)
            return;
        pump();
    }
    throw std::runtime_error("Apply confirmation did not appear after analysis");
}
void testWorkflow(bool reference)
{
    OxygenAudioProcessor processor;
    processor.setPlayConfigDetails(2, 2, 48000.0, 512);
    processor.prepareToPlay(48000.0, 512);
    MainComponent editor(processor);
    juce::TemporaryFile referenceFile(".wav");
    const auto audio = makeAudio();
    if (reference)
    {
        juce::WavAudioFormat format;
        std::unique_ptr<juce::AudioFormatWriter> writer(format.createWriterFor(
            referenceFile.getFile().createOutputStream().release(), 48000.0, 2, 24, {}, 0));
        require(writer != nullptr && writer->writeFromAudioSampleBuffer(audio, 0, audio.getNumSamples()), "Reference fixture write failed");
        writer.reset();
        juce::String error;
        require(processor.loadReferenceAudioFile(referenceFile.getFile(), error), "Reference fixture load failed");
        editor.beginListeningSession(MainComponent::AssistantUiState::listeningReference,
            "LISTENING...", "Reference Listening", "Listening to test audio");
    }
    else
        editor.startAssistantListening();
    capture(processor, audio);
    const auto before = state(processor);
    auto* listening = currentWindow();
    require(listening != nullptr, "Listening dialog missing");
    editor.completeAssistantListening(); // Same transition as the 100% timer event.
    require(currentWindow() == listening, "Listening dialog vanished before analysis completed");
    waitForApply();
    pump(80); // Deliver the old modal dismissal callback before clicking Apply.
    auto* confirmation = currentWindow();
    require(confirmation != nullptr && confirmation->getButton("Apply") != nullptr, "Stale dismissal cancelled the proposal");
    require(state(processor) == before, "Settings changed before Apply");
    confirmation->getButton("Apply")->triggerClick();
    pump(150);
    require(state(processor) != before, "Apply did not write the proposed module parameters");
    require(currentWindow() == nullptr, "Confirmation remains modal after Apply");
    std::cout << "PASS " << (reference ? "Reference Match" : "Master Assistant")
              << ": capture completion -> confirmation -> Apply changes parameters\n";
}
void testCancellationAndStaleCallbacks()
{
    OxygenAudioProcessor processor;
    processor.setPlayConfigDetails(2, 2, 48000.0, 512);
    processor.prepareToPlay(48000.0, 512);
    MainComponent editor(processor);
    editor.startAssistantListening();
    currentWindow()->exitModalState(0); // Queue the first session's user cancellation.
    editor.cancelAssistantListening();
    editor.startAssistantListening();
    auto* newWindow = currentWindow();
    pump(80);
    require(newWindow != nullptr && currentWindow() == newWindow, "Old cancellation dismissed a new session");
    capture(processor, makeAudio());
    const auto before = state(processor);
    editor.completeAssistantListening();
    currentWindow()->getButton("Cancel")->triggerClick();
    pump(250);
    require(currentWindow() == nullptr, "Analysis Cancel left a modal open");
    require(state(processor) == before, "Cancelled analysis applied settings");

    // Complete a second pass and dismiss its confirmation without applying.
    editor.startAssistantListening();
    capture(processor, makeAudio());
    editor.completeAssistantListening();
    waitForApply();
    currentWindow()->getButton("Cancel")->triggerClick();
    pump(150);
    require(state(processor) == before, "Confirmation Cancel changed settings");
    std::cout << "PASS cancellation during analysis/confirmation and stale modal callbacks\n";
}
void testEditorClose()
{
    OxygenAudioProcessor processor;
    processor.setPlayConfigDetails(2, 2, 48000.0, 512);
    processor.prepareToPlay(48000.0, 512);
    auto editor = std::make_unique<MainComponent>(processor);
    editor->startAssistantListening();
    capture(processor, makeAudio());
    editor->completeAssistantListening();
    editor.reset();
    pump(250);
    require(currentWindow() == nullptr, "Closing the editor left an analysis dialog open");
    editor = std::make_unique<MainComponent>(processor);
    editor->startAssistantListening();
    capture(processor, makeAudio());
    editor->completeAssistantListening();
    waitForApply();
    editor.reset();
    pump(150);
    require(currentWindow() == nullptr, "Closing the editor left an Apply dialog open");
    std::cout << "PASS editor close during analysis and confirmation\n";
}
}

void runAssistantWorkflowTests()
{
    testWorkflow(false);
    testWorkflow(true);
    testCancellationAndStaleCallbacks();
    testEditorClose();
}

void runAssistantCancellationTests()
{
    const auto audio = makeAudio();
    oxygen::InferenceEngine engine;
    int checkpoints = 0;
    engine.predict(audio, 48000.0, {}, [&] { ++checkpoints; return false; });
    require(checkpoints > 20, "Missing analysis cancellation checkpoints");
    for (int stopAt : { 1, checkpoints / 4, checkpoints / 2, checkpoints * 3 / 4, checkpoints - 2 })
    {
        int count = 0;
        bool cancelled = false;
        try { engine.predict(audio, 48000.0, {}, [&] { return ++count >= stopAt; }); }
        catch (const oxygen::AnalysisCancelled&) { cancelled = true; }
        require(cancelled, "Analysis ignored cancellation");
    }
    bool referenceCancelled = false;
    int count = 0;
    try { engine.matchReference(audio, 48000.0, audio, 48000.0, [&] { return ++count > 100; }); }
    catch (const oxygen::AnalysisCancelled&) { referenceCancelled = true; }
    require(referenceCancelled, "Reference matching ignored cancellation");

    auto processor = std::make_unique<OxygenAudioProcessor>();
    processor->setPlayConfigDetails(2, 2, 48000.0, 512);
    processor->prepareToPlay(48000.0, 512);
    capture(*processor, makeAudio(8));
    auto callbackCalled = std::make_shared<std::atomic<bool>>(false);
    processor->createAssistantSuggestionAsync(false, [callbackCalled](bool, oxygen::MasteringParameters, juce::String)
    {
        callbackCalled->store(true);
    });
    juce::Thread::sleep(30); // Let the worker enter analysis before teardown.
    const auto start = std::chrono::steady_clock::now();
    processor.reset();
    const double milliseconds = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - start).count();
    require(milliseconds < 1000.0, "Processor shutdown waited for the full analysis");
    require(!callbackCalled->load(), "Cancelled processor delivered a result");
    std::cout << "PASS cancellation throughout analysis; active processor shutdown: " << milliseconds << " ms\n";
}
