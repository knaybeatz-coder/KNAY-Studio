#include "AI/MasteringParameterApplier.h"
#include "PluginProcessor.h"
#include "PluginEditor.h"
#include "Modules/GainModule.h"
#include "Modules/EqualizerModule.h"
#include "Modules/DynamicEqModule.h"
#include "Modules/MultibandCompressorModule.h"
#include "Modules/MaximizerModule.h"
#include "Modules/StereoImagerModule.h"
#include <algorithm>
#include <cmath>

namespace
{
    constexpr auto pluginStateType = "OXYGEN_STATE";
    constexpr auto moduleOrderStateType = "MODULE_ORDER";
    constexpr auto moduleStatesType = "MODULE_STATES";
    constexpr auto moduleStateType = "MODULE";
    constexpr double referenceAnalysisWindowSeconds = 60.0;

    void storeParameters(juce::ValueTree& moduleState, juce::AudioProcessor& processor)
    {
        for (auto* parameter : processor.getParameters())
        {
            if (auto* parameterWithId = dynamic_cast<juce::AudioProcessorParameterWithID*>(parameter))
                moduleState.setProperty(parameterWithId->paramID, parameterWithId->getValue(), nullptr);
        }
    }

    void restoreParameters(const juce::ValueTree& moduleState, juce::AudioProcessor& processor)
    {
        for (auto* parameter : processor.getParameters())
        {
            if (auto* parameterWithId = dynamic_cast<juce::AudioProcessorParameterWithID*>(parameter))
            {
                if (moduleState.hasProperty(parameterWithId->paramID))
                    parameterWithId->setValueNotifyingHost((float) moduleState.getProperty(parameterWithId->paramID));
                else if (dynamic_cast<oxygen::EqualizerModule*>(&processor) && (parameterWithId->paramID.startsWith("Freq") || parameterWithId->paramID.startsWith("Q")))
                    parameterWithId->setValueNotifyingHost(parameterWithId->getDefaultValue());
            }
        }
    }

    float getChannelMagnitude(const juce::AudioBuffer<float>& buffer, int channel)
    {
        if (channel < 0 || channel >= buffer.getNumChannels() || buffer.getNumSamples() == 0)
            return 0.0f;

        return buffer.getMagnitude(channel, 0, buffer.getNumSamples());
    }

    float getChannelMagnitude(const juce::AudioBuffer<double>& buffer, int channel)
    {
        if (channel < 0 || channel >= buffer.getNumChannels() || buffer.getNumSamples() == 0)
            return 0.0f;

        return (float) buffer.getMagnitude(channel, 0, buffer.getNumSamples());
    }

    template <typename SampleType>
    void writeAssistantHistory(juce::AudioBuffer<float>& assistantHistoryBuffer,
                               juce::SpinLock& assistantHistoryLock,
                               int& assistantHistoryWritePosition,
                               int& assistantHistoryNumValidSamples,
                               int assistantHistoryCapacity,
                               float& assistantCapturePeak,
                               const juce::AudioBuffer<SampleType>& buffer)
    {
        if (buffer.getNumChannels() == 0)
            return;

        const juce::SpinLock::ScopedTryLockType historyLock(assistantHistoryLock);
        if (!historyLock.isLocked())
            return;

        const auto* left = buffer.getReadPointer(0);
        const auto* right = (buffer.getNumChannels() > 1) ? buffer.getReadPointer(1) : left;

        for (int sample = 0; sample < buffer.getNumSamples(); ++sample)
        {
            const float storedL = (float) left[sample];
            const float storedR = (float) right[sample];

            assistantHistoryBuffer.setSample(0, assistantHistoryWritePosition, storedL);
            assistantHistoryBuffer.setSample(1, assistantHistoryWritePosition, storedR);
            assistantCapturePeak = juce::jmax(assistantCapturePeak, std::abs(storedL), std::abs(storedR));
            assistantHistoryWritePosition = (assistantHistoryWritePosition + 1) % assistantHistoryCapacity;
            assistantHistoryNumValidSamples = juce::jmin(assistantHistoryNumValidSamples + 1,
                                                         assistantHistoryCapacity);
        }
    }

    bool isSupportedReferenceFile(const juce::File& file)
    {
        return file.existsAsFile() && file.hasFileExtension("wav;flac");
    }

    int64_t findRepresentativeExcerptStart(juce::AudioFormatReader& reader, int64_t excerptLengthSamples)
    {
        if (reader.lengthInSamples <= excerptLengthSamples)
            return 0;

        const int numChannels = juce::jlimit(1, 2, (int) reader.numChannels);
        const int64_t scanBlockSize = juce::jmax<int64_t>(1, (int64_t) std::llround(reader.sampleRate));
        const int64_t windowBlocks = juce::jmax<int64_t>(1, (excerptLengthSamples + scanBlockSize - 1) / scanBlockSize);
        const int64_t numBlocks = (reader.lengthInSamples + scanBlockSize - 1) / scanBlockSize;

        juce::AudioBuffer<float> scanBuffer(numChannels, (int) scanBlockSize);
        std::vector<double> blockMeanSquares;
        blockMeanSquares.reserve((size_t) numBlocks);

        for (int64_t blockIndex = 0; blockIndex < numBlocks; ++blockIndex)
        {
            const int64_t startSample = blockIndex * scanBlockSize;
            const int samplesToRead = (int) juce::jmin<int64_t>(scanBlockSize, reader.lengthInSamples - startSample);
            scanBuffer.clear();

            if (!reader.read(&scanBuffer, 0, samplesToRead, startSample, true, true))
            {
                blockMeanSquares.push_back(0.0);
                continue;
            }

            double sumSquares = 0.0;
            for (int channel = 0; channel < numChannels; ++channel)
            {
                const auto* channelData = scanBuffer.getReadPointer(channel);
                for (int sample = 0; sample < samplesToRead; ++sample)
                    sumSquares += (double) channelData[sample] * channelData[sample];
            }

            const double normalization = (double) juce::jmax(1, samplesToRead * numChannels);
            blockMeanSquares.push_back(sumSquares / normalization);
        }

        double currentWindowEnergy = 0.0;
        double bestWindowEnergy = -1.0;
        int64_t bestWindowStartBlock = 0;

        for (size_t blockIndex = 0; blockIndex < blockMeanSquares.size(); ++blockIndex)
        {
            currentWindowEnergy += blockMeanSquares[blockIndex];
            if (blockIndex >= (size_t) windowBlocks)
                currentWindowEnergy -= blockMeanSquares[blockIndex - (size_t) windowBlocks];

            if (blockIndex + 1 >= (size_t) windowBlocks && currentWindowEnergy > bestWindowEnergy)
            {
                bestWindowEnergy = currentWindowEnergy;
                bestWindowStartBlock = (int64_t) blockIndex + 1 - windowBlocks;
            }
        }

        return juce::jlimit<int64_t>(0, juce::jmax<int64_t>(0, reader.lengthInSamples - excerptLengthSamples),
                                     bestWindowStartBlock * scanBlockSize);
    }

    bool loadReferenceAudioExcerpt(const juce::File& file,
                                   juce::AudioBuffer<float>& destinationBuffer,
                                   double& destinationSampleRate,
                                   juce::String& errorMessage)
    {
        if (!isSupportedReferenceFile(file))
        {
            errorMessage = "Only WAV and FLAC reference files are supported.";
            return false;
        }

        juce::AudioFormatManager formatManager;
        formatManager.registerFormat(new juce::WavAudioFormat(), true);
        formatManager.registerFormat(new juce::FlacAudioFormat(), true);

        std::unique_ptr<juce::AudioFormatReader> reader(formatManager.createReaderFor(file));
        if (reader == nullptr)
        {
            errorMessage = "Unable to open the selected reference file.";
            return false;
        }

        const int numChannels = juce::jlimit(1, 2, (int) reader->numChannels);
        if (numChannels <= 0 || reader->lengthInSamples <= 0)
        {
            errorMessage = "The selected reference file does not contain readable audio.";
            return false;
        }

        destinationSampleRate = reader->sampleRate;
        const juce::int64 requestedExcerptSamples = (juce::int64) std::llround(destinationSampleRate * referenceAnalysisWindowSeconds);
        const juce::int64 maxExcerptSamples = std::max<juce::int64>(1024, requestedExcerptSamples);
        const juce::int64 excerptLengthSamples = std::min<juce::int64>(reader->lengthInSamples, maxExcerptSamples);
        const juce::int64 excerptStartSample = findRepresentativeExcerptStart(*reader, excerptLengthSamples);
        const int samplesToRead = (int) excerptLengthSamples;

        juce::AudioBuffer<float> rawBuffer(numChannels, samplesToRead);
        rawBuffer.clear();
        if (!reader->read(&rawBuffer, 0, samplesToRead, excerptStartSample, true, true))
        {
            errorMessage = "Unable to read audio data from the selected reference file.";
            return false;
        }

        destinationBuffer.setSize(numChannels, samplesToRead);
        destinationBuffer.clear();
        destinationBuffer.copyFrom(0, 0, rawBuffer, 0, 0, samplesToRead);

        if (numChannels > 1)
            destinationBuffer.copyFrom(1, 0, rawBuffer, 1, 0, samplesToRead);

        return true;
    }

}

using namespace oxygen;

OxygenAudioProcessor::OxygenAudioProcessor()
#ifndef JucePlugin_PreferredChannelConfigurations
     : AudioProcessor (BusesProperties()
                     .withInput  ("Input",  juce::AudioChannelSet::stereo(), true)
                     .withOutput ("Output", juce::AudioChannelSet::stereo(), true)
                       ),
       mainProcessorGraph(std::make_unique<juce::AudioProcessorGraph>()),
       inferenceEngine(std::make_unique<oxygen::InferenceEngine>())
#endif
{
    initialiseGraph();
    for (auto& node : moduleNodes) {
        auto* module = node->getProcessor();
        for (auto* parameter : module->getParameters()) {
            if (auto* ranged = dynamic_cast<juce::RangedAudioParameter*>(parameter)) {
                if (dynamic_cast<oxygen::EqualizerModule*>(module) && (ranged->paramID.startsWith("Freq") || ranged->paramID.startsWith("Q"))) continue;
                auto* forwarded = new ForwardParameter(module->getName() + "_" + ranged->paramID,
                    module->getName() + " / " + ranged->getName(64), *ranged);
                addParameter(forwarded);
                forwardedParameters.push_back(forwarded);
            }
        }
    }
    for (auto& node : moduleNodes) if (auto* eq=dynamic_cast<oxygen::EqualizerModule*>(node->getProcessor()))
        for (auto* parameter:eq->getParameters()) if(auto* ranged=dynamic_cast<juce::RangedAudioParameter*>(parameter))
            if(ranged->paramID.startsWith("Freq") || ranged->paramID.startsWith("Q")){
                auto* forwarded=new ForwardParameter(eq->getName()+"_"+ranged->paramID,eq->getName()+" / "+ranged->getName(64),*ranged);
                addParameter(forwarded);forwardedParameters.push_back(forwarded);
            }

}

OxygenAudioProcessor::~OxygenAudioProcessor()
{
    cancelAssistantAnalysis();
    assistantJobs.removeAllJobs(true, -1);
    for (auto* parameter : forwardedParameters) parameter->disconnect();
}

const juce::String OxygenAudioProcessor::getName() const
{
    return JucePlugin_Name;
}

bool OxygenAudioProcessor::acceptsMidi() const
{
   #if JucePlugin_WantsMidiInput
    return true;
   #else
    return false;
   #endif
}

bool OxygenAudioProcessor::producesMidi() const
{
   #if JucePlugin_ProducesMidiOutput
    return true;
   #else
    return false;
   #endif
}

bool OxygenAudioProcessor::isMidiEffect() const
{
   #if JucePlugin_IsMidiEffect
    return true;
   #else
    return false;
   #endif
}

double OxygenAudioProcessor::getTailLengthSeconds() const
{
    return 0.0;
}

int OxygenAudioProcessor::getNumPrograms()
{
    return 1;   // NB: some hosts don't cope very well if you tell them there are 0 programs,
                // so this should be at least 1, even if you're not really implementing programs.
}

int OxygenAudioProcessor::getCurrentProgram()
{
    return 0;
}

void OxygenAudioProcessor::setCurrentProgram (int index)
{
}

const juce::String OxygenAudioProcessor::getProgramName (int index)
{
    return {};
}

void OxygenAudioProcessor::changeProgramName (int index, const juce::String& newName)
{
}

void OxygenAudioProcessor::prepareToPlay (double sampleRate, int samplesPerBlock)
{
    mainProcessorGraph->setProcessingPrecision(getProcessingPrecision());
    mainProcessorGraph->setPlayConfigDetails(getMainBusNumInputChannels(),
                                             getMainBusNumOutputChannels(),
                                             sampleRate, samplesPerBlock);
    mainProcessorGraph->prepareToPlay(sampleRate, samplesPerBlock);

    {
        const juce::SpinLock::ScopedLockType historyLock(assistantHistoryLock);
        assistantHistorySampleRate = sampleRate;
        assistantHistoryChannels = juce::jlimit(1, 2, getMainBusNumInputChannels());
        assistantHistoryCapacity = juce::jmax(4096, juce::roundToInt(assistantHistorySampleRate * assistantAnalysisWindowSeconds));
        assistantHistoryBuffer.setSize(2, assistantHistoryCapacity);
        assistantHistoryBuffer.clear();
        assistantHistoryWritePosition = 0;
        assistantHistoryNumValidSamples = 0;
        assistantCapturePeak = 0.0f;
    }

    inputAudioBufferQueue.clear();
    outputAudioBufferQueue.clear();

    // Refresh connections to match the new layout
    updateGraphConnections();
    setLatencySamples(mainProcessorGraph->getLatencySamples());
}

void OxygenAudioProcessor::releaseResources()
{
    mainProcessorGraph->releaseResources();
}

bool OxygenAudioProcessor::isBusesLayoutSupported (const BusesLayout& layouts) const
{
  #if JucePlugin_IsMidiEffect
    juce::ignoreUnused (layouts);
    return true;
  #else
    if (layouts.getMainOutputChannelSet() != juce::AudioChannelSet::mono()
     && layouts.getMainOutputChannelSet() != juce::AudioChannelSet::stereo())
        return false;

   #if ! JucePlugin_IsSynth
    if (layouts.getMainOutputChannelSet() != layouts.getMainInputChannelSet())
        return false;
   #endif

    return true;
  #endif
}

void OxygenAudioProcessor::processBlock (juce::AudioBuffer<float>& buffer, juce::MidiBuffer& midiMessages)
{
    juce::ScopedNoDenormals noDenormals;
    auto totalNumInputChannels  = getTotalNumInputChannels();
    auto totalNumOutputChannels = getTotalNumOutputChannels();

    for (auto i = totalNumInputChannels; i < totalNumOutputChannels; ++i)
        buffer.clear (i, 0, buffer.getNumSamples());

    writeAssistantHistory(assistantHistoryBuffer,
                          assistantHistoryLock,
                          assistantHistoryWritePosition,
                          assistantHistoryNumValidSamples,
                          assistantHistoryCapacity,
                          assistantCapturePeak,
                          buffer);

    const float inL = getChannelMagnitude(buffer, 0);
    const float inR = (buffer.getNumChannels() > 1) ? getChannelMagnitude(buffer, 1) : inL;
    inputLevelL.store(inL);
    inputLevelR.store(inR);

    if (buffer.getNumChannels() > 0)
        inputAudioBufferQueue.push(buffer);

    // Process audio through the graph
    mainProcessorGraph->processBlock(buffer, midiMessages);

    const float outL = getChannelMagnitude(buffer, 0);
    const float outR = (buffer.getNumChannels() > 1) ? getChannelMagnitude(buffer, 1) : outL;
    outputLevelL.store(outL);
    outputLevelR.store(outR);

    // Feed the analyser with a mono sum of the processed output.
    if (buffer.getNumChannels() > 0)
        outputAudioBufferQueue.push(buffer);
}

void OxygenAudioProcessor::processBlock (juce::AudioBuffer<double>& buffer, juce::MidiBuffer& midiMessages)
{
    juce::ScopedNoDenormals noDenormals;
    auto totalNumInputChannels  = getTotalNumInputChannels();
    auto totalNumOutputChannels = getTotalNumOutputChannels();

    for (auto i = totalNumInputChannels; i < totalNumOutputChannels; ++i)
        buffer.clear(i, 0, buffer.getNumSamples());

    writeAssistantHistory(assistantHistoryBuffer,
                          assistantHistoryLock,
                          assistantHistoryWritePosition,
                          assistantHistoryNumValidSamples,
                          assistantHistoryCapacity,
                          assistantCapturePeak,
                          buffer);

    const float inL = getChannelMagnitude(buffer, 0);
    const float inR = (buffer.getNumChannels() > 1) ? getChannelMagnitude(buffer, 1) : inL;
    inputLevelL.store(inL);
    inputLevelR.store(inR);

    if (buffer.getNumChannels() > 0)
        inputAudioBufferQueue.push(buffer);

    mainProcessorGraph->processBlock(buffer, midiMessages);

    const float outL = getChannelMagnitude(buffer, 0);
    const float outR = (buffer.getNumChannels() > 1) ? getChannelMagnitude(buffer, 1) : outL;
    outputLevelL.store(outL);
    outputLevelR.store(outR);

    if (buffer.getNumChannels() > 0)
        outputAudioBufferQueue.push(buffer);
}

void OxygenAudioProcessor::initialiseGraph()
{
    mainProcessorGraph->clear();
    moduleNodes.clear();

    // Create AudioGraphIOProcessors for input and output
    audioInputNode  = mainProcessorGraph->addNode(std::make_unique<juce::AudioProcessorGraph::AudioGraphIOProcessor>(juce::AudioProcessorGraph::AudioGraphIOProcessor::audioInputNode));
    audioOutputNode = mainProcessorGraph->addNode(std::make_unique<juce::AudioProcessorGraph::AudioGraphIOProcessor>(juce::AudioProcessorGraph::AudioGraphIOProcessor::audioOutputNode));

    // Add Modules (Initial Order)
    moduleNodes.push_back(mainProcessorGraph->addNode(std::make_unique<oxygen::EqualizerModule>()));
    moduleNodes.push_back(mainProcessorGraph->addNode(std::make_unique<oxygen::DynamicEqModule>()));
    moduleNodes.push_back(mainProcessorGraph->addNode(std::make_unique<oxygen::MultibandCompressorModule>()));
    moduleNodes.push_back(mainProcessorGraph->addNode(std::make_unique<oxygen::StereoImagerModule>()));
    moduleNodes.push_back(mainProcessorGraph->addNode(std::make_unique<oxygen::GainModule>()));
    moduleNodes.push_back(mainProcessorGraph->addNode(std::make_unique<oxygen::MaximizerModule>()));

    updateGraphConnections();
    notifyGraphChanged();
}

void OxygenAudioProcessor::updateGraphConnections()
{
    // Clear all existing connections properly
    while (!mainProcessorGraph->getConnections().empty())
        mainProcessorGraph->removeConnection(mainProcessorGraph->getConnections().front());

    int numInputChannels = getMainBusNumInputChannels();
    int numOutputChannels = getMainBusNumOutputChannels();
    const int numChannelsToProcess = juce::jmin(juce::jmin(numInputChannels, numOutputChannels), 2);

    auto addConnectionIfLegal = [this](juce::AudioProcessorGraph::NodeID srcNode, int srcChannel,
                                       juce::AudioProcessorGraph::NodeID dstNode, int dstChannel)
    {
        const juce::AudioProcessorGraph::Connection connection { { srcNode, srcChannel }, { dstNode, dstChannel } };
        if (mainProcessorGraph->isConnectionLegal(connection))
            mainProcessorGraph->addConnection(connection);
    };

    if (moduleNodes.empty())
    {
        for (int channel = 0; channel < numChannelsToProcess; ++channel)
            addConnectionIfLegal(audioInputNode->nodeID, channel, audioOutputNode->nodeID, channel);
        setLatencySamples(mainProcessorGraph->getLatencySamples());
        return;
    }

    // Connect Input -> First Module
    // Safety check for empty graph processing
    if (numChannelsToProcess == 0)
        return;

    for (int channel = 0; channel < numChannelsToProcess; ++channel)
        addConnectionIfLegal(audioInputNode->nodeID, channel, moduleNodes[0]->nodeID, channel);

    // Connect Modules in sequence
    for (size_t i = 1; i < moduleNodes.size(); ++i)
    {
        for (int channel = 0; channel < numChannelsToProcess; ++channel)
            addConnectionIfLegal(moduleNodes[i - 1]->nodeID, channel, moduleNodes[i]->nodeID, channel);
    }

    // Connect Last Module -> Output
    for (int channel = 0; channel < numChannelsToProcess; ++channel)
        addConnectionIfLegal(moduleNodes.back()->nodeID, channel, audioOutputNode->nodeID, channel);

    setLatencySamples(mainProcessorGraph->getLatencySamples());
}

void OxygenAudioProcessor::moveModuleUp(int index)
{
    if (index > 0 && index < (int)moduleNodes.size())
    {
        std::swap(moduleNodes[index], moduleNodes[index - 1]);
        updateGraphConnections();
        notifyGraphChanged();
    }
}

void OxygenAudioProcessor::moveModuleDown(int index)
{
    if (index >= 0 && index < (int)moduleNodes.size() - 1)
    {
        std::swap(moduleNodes[index], moduleNodes[index + 1]);
        updateGraphConnections();
        notifyGraphChanged();
    }
}

void OxygenAudioProcessor::cancelAssistantAnalysis()
{
    if (assistantCancellation != nullptr)
        assistantCancellation->store(true, std::memory_order_relaxed);
    // Remove queued work without blocking the message thread. Running analysis
    // observes the same token at bounded checkpoints and unwinds normally.
    assistantJobs.removeAllJobs(true, 0);
}

void OxygenAudioProcessor::createAssistantSuggestionAsync(bool reference, AssistantCallback callback)
{
    cancelAssistantAnalysis();
    auto cancelled = std::make_shared<std::atomic<bool>>(false);
    assistantCancellation = cancelled;
    assistantJobs.addJob([this, reference, callback = std::move(callback), cancelled]
    {
        const oxygen::InferenceEngine::CancellationCheck shouldCancel = [cancelled]
        {
            return cancelled->load(std::memory_order_relaxed);
        };
        if (shouldCancel()) return;
        oxygen::MasteringParameters params;
        juce::String error;
        bool success = false;
        try
        {
            success = reference ? createReferenceMatchSuggestion(params, error, shouldCancel)
                                : createMasterAssistantSuggestion(params, shouldCancel);
        }
        catch (const oxygen::AnalysisCancelled&)
        {
            return;
        }
        catch (const std::exception&)
        {
            error = "Unable to finish analysis. Try a shorter capture.";
        }
        if (shouldCancel()) return;
        juce::MessageManager::callAsync([callback, success, params, error, cancelled]
        {
            // Cancellation can happen after completion was queued, including
            // when the editor/processor closes or a new request replaces this one.
            if (!cancelled->load(std::memory_order_relaxed))
                callback(success, params, error);
        });
    });
}

bool OxygenAudioProcessor::triggerMasterAssistant()
{
    oxygen::MasteringParameters params;
    if (!createMasterAssistantSuggestion(params))
        return false;

    return applyMasterAssistantSuggestion(params);
}

bool OxygenAudioProcessor::createMasterAssistantSuggestion(oxygen::MasteringParameters& params,
    const oxygen::InferenceEngine::CancellationCheck& shouldCancel)
{
    const float minSignalGain = juce::Decibels::decibelsToGain(assistantMinSignalDb);
    float capturedPeak = 0.0f;
    {
        const juce::SpinLock::ScopedLockType historyLock(assistantHistoryLock);
        capturedPeak = assistantCapturePeak;
    }

    if (capturedPeak < minSignalGain)
        return false;

    double capturedSampleRate = 44100.0;
    auto analysisBuffer = buildAssistantAnalysisBuffer(&capturedSampleRate);
    if (analysisBuffer.getNumSamples() < 1024)
        return false;

    params = (inferenceEngine != nullptr)
           ? inferenceEngine->predict(analysisBuffer, capturedSampleRate, {}, shouldCancel)
           : oxygen::MasteringParameters {};

    return params.usedAnalysis;
}

bool OxygenAudioProcessor::applyMasterAssistantSuggestion(const oxygen::MasteringParameters& suggestion)
{
    if (!suggestion.usedAnalysis)
        return false;

    applyMasteringParameters(suggestion);

    return true;
}

bool OxygenAudioProcessor::loadReferenceAudioFile(const juce::File& referenceFile, juce::String& errorMessage)
{
    juce::AudioBuffer<float> referenceBuffer;
    double referenceSampleRate = 44100.0;
    if (!loadReferenceAudioExcerpt(referenceFile, referenceBuffer, referenceSampleRate, errorMessage))
        return false;

    const juce::SpinLock::ScopedLockType referenceLock(loadedReferenceLock);
    loadedReferenceAudioBuffer = std::move(referenceBuffer);
    loadedReferenceSampleRate = referenceSampleRate;
    loadedReferenceFileName = referenceFile.getFileName();
    return true;
}

void OxygenAudioProcessor::clearLoadedReferenceAudio()
{
    const juce::SpinLock::ScopedLockType referenceLock(loadedReferenceLock);
    loadedReferenceAudioBuffer.setSize(0, 0);
    loadedReferenceSampleRate = 44100.0;
    loadedReferenceFileName.clear();
}

bool OxygenAudioProcessor::hasLoadedReferenceAudio() const
{
    const juce::SpinLock::ScopedLockType referenceLock(loadedReferenceLock);
    return loadedReferenceAudioBuffer.getNumSamples() > 0;
}

juce::String OxygenAudioProcessor::getLoadedReferenceName() const
{
    const juce::SpinLock::ScopedLockType referenceLock(loadedReferenceLock);
    return loadedReferenceFileName;
}

bool OxygenAudioProcessor::createReferenceMatchSuggestion(oxygen::MasteringParameters& params, juce::String& errorMessage,
    const oxygen::InferenceEngine::CancellationCheck& shouldCancel)
{
    const float minSignalGain = juce::Decibels::decibelsToGain(assistantMinSignalDb);
    float capturedPeak = 0.0f;
    {
        const juce::SpinLock::ScopedLockType historyLock(assistantHistoryLock);
        capturedPeak = assistantCapturePeak;
    }

    if (capturedPeak < minSignalGain)
    {
        errorMessage = "No mix signal was captured for matching.";
        return false;
    }

    juce::AudioBuffer<float> referenceBuffer;
    double referenceSampleRate = 44100.0;
    {
        const juce::SpinLock::ScopedLockType referenceLock(loadedReferenceLock);
        if (loadedReferenceAudioBuffer.getNumSamples() < 1024)
        {
            errorMessage = "No valid reference file is loaded.";
            return false;
        }

        referenceBuffer = loadedReferenceAudioBuffer;
        referenceSampleRate = loadedReferenceSampleRate;
    }

    double capturedSampleRate = 44100.0;
    auto sourceBuffer = buildAssistantAnalysisBuffer(&capturedSampleRate);
    if (sourceBuffer.getNumSamples() < 1024)
    {
        errorMessage = "Unable to capture enough mix audio for matching.";
        return false;
    }

    params = (inferenceEngine != nullptr)
           ? inferenceEngine->matchReference(sourceBuffer, capturedSampleRate,
                                             referenceBuffer, referenceSampleRate, shouldCancel)
           : oxygen::MasteringParameters {};

    if (!params.usedAnalysis)
    {
        errorMessage = "The mix or reference does not contain enough valid audio.";
        return false;
    }
    return true;
}

void OxygenAudioProcessor::applyMasteringParameters(const oxygen::MasteringParameters& params)
{
    for (auto& node : mainProcessorGraph->getNodes())
        if (auto* processor = node->getProcessor())
            oxygen::applyParametersToModule(processor, params);
}

bool OxygenAudioProcessor::hasEditor() const
{
    return true; // (change this to false if you choose to not supply an editor)
}

juce::AudioProcessorEditor* OxygenAudioProcessor::createEditor()
{
    return new OxygenAudioProcessorEditor (*this);
}

void OxygenAudioProcessor::getStateInformation (juce::MemoryBlock& destData)
{
    juce::ValueTree root(pluginStateType);
    root.setProperty("version", 2, nullptr);

    juce::ValueTree moduleOrder(moduleOrderStateType);
    for (const auto& name : getCurrentModuleOrder())
    {
        juce::ValueTree child(moduleStateType);
        child.setProperty("name", name, nullptr);
        moduleOrder.addChild(child, -1, nullptr);
    }

    juce::ValueTree moduleStates(moduleStatesType);
    for (const auto& node : moduleNodes)
    {
        if (auto* processor = node->getProcessor())
        {
            juce::ValueTree moduleState(moduleStateType);
            moduleState.setProperty("name", processor->getName(), nullptr);
            storeParameters(moduleState, *processor);
            moduleStates.addChild(moduleState, -1, nullptr);
        }
    }

    root.addChild(moduleOrder, -1, nullptr);
    root.addChild(moduleStates, -1, nullptr);

    if (auto xml = root.createXml())
        copyXmlToBinary(*xml, destData);
}

void OxygenAudioProcessor::setStateInformation (const void* data, int sizeInBytes)
{
    std::unique_ptr<juce::XmlElement> xmlState(getXmlFromBinary(data, sizeInBytes));
    if (xmlState == nullptr)
        return;

    const auto root = juce::ValueTree::fromXml(*xmlState);
    if (!root.isValid() || !root.hasType(pluginStateType))
        return;

    if (auto orderState = root.getChildWithName(moduleOrderStateType); orderState.isValid())
    {
        juce::StringArray savedOrder;
        for (const auto& child : orderState)
        {
            if (child.hasProperty("name"))
                savedOrder.add(child["name"].toString());
        }

        applyModuleOrder(savedOrder);
    }

    if (auto states = root.getChildWithName(moduleStatesType); states.isValid())
    {
        for (const auto& moduleState : states)
        {
            const auto moduleName = moduleState["name"].toString();
            auto it = std::find_if(moduleNodes.begin(), moduleNodes.end(),
                                   [&moduleName](const juce::AudioProcessorGraph::Node::Ptr& node)
                                   {
                                       return node != nullptr
                                           && node->getProcessor() != nullptr
                                           && node->getProcessor()->getName() == moduleName;
                                   });

            if (it != moduleNodes.end())
                if (auto* processor = (*it)->getProcessor())
                    restoreParameters(moduleState, *processor);
        }
    }
}

float OxygenAudioProcessor::getInputLevel(int channel) const
{
    return (channel <= 0) ? inputLevelL.load() : inputLevelR.load();
}

float OxygenAudioProcessor::getOutputLevel(int channel) const
{
    return (channel <= 0) ? outputLevelL.load() : outputLevelR.load();
}

void OxygenAudioProcessor::resetAssistantAnalysisCapture()
{
    const juce::SpinLock::ScopedLockType historyLock(assistantHistoryLock);
    assistantHistoryBuffer.clear();
    assistantHistoryWritePosition = 0;
    assistantHistoryNumValidSamples = 0;
    assistantCapturePeak = 0.0f;
}

bool OxygenAudioProcessor::hasAssistantCapturedSignal() const
{
    const juce::SpinLock::ScopedLockType historyLock(assistantHistoryLock);
    return assistantCapturePeak >= juce::Decibels::decibelsToGain(assistantMinSignalDb);
}

void OxygenAudioProcessor::setGraphChangedCallback(std::function<void()> callback)
{
    graphChangedCallback = std::move(callback);
}

juce::StringArray OxygenAudioProcessor::getCurrentModuleOrder() const
{
    juce::StringArray order;
    for (const auto& node : moduleNodes)
    {
        if (node != nullptr)
            if (auto* processor = node->getProcessor())
                order.add(processor->getName());
    }

    return order;
}

void OxygenAudioProcessor::applyModuleOrder(const juce::StringArray& savedOrder)
{
    if (savedOrder.isEmpty())
        return;

    std::vector<juce::AudioProcessorGraph::Node::Ptr> remaining(moduleNodes.begin(), moduleNodes.end());
    std::vector<juce::AudioProcessorGraph::Node::Ptr> reordered;
    reordered.reserve(moduleNodes.size());

    for (const auto& name : savedOrder)
    {
        auto it = std::find_if(remaining.begin(), remaining.end(),
                               [&name](const juce::AudioProcessorGraph::Node::Ptr& node)
                               {
                                   return node != nullptr
                                       && node->getProcessor() != nullptr
                                       && node->getProcessor()->getName() == name;
                               });

        if (it != remaining.end())
        {
            reordered.push_back(*it);
            remaining.erase(it);
        }
    }

    reordered.insert(reordered.end(), remaining.begin(), remaining.end());
    moduleNodes = std::move(reordered);
    updateGraphConnections();
    notifyGraphChanged();
}

void OxygenAudioProcessor::notifyGraphChanged()
{
    if (graphChangedCallback)
        graphChangedCallback();
}

juce::AudioBuffer<float> OxygenAudioProcessor::buildAssistantAnalysisBuffer(double* capturedSampleRate)
{
    juce::AudioBuffer<float> analysisBuffer;
    const juce::SpinLock::ScopedLockType historyLock(assistantHistoryLock);

    if (capturedSampleRate != nullptr)
        *capturedSampleRate = assistantHistorySampleRate;

    if (assistantHistoryNumValidSamples <= 0)
        return analysisBuffer;

    analysisBuffer.setSize(assistantHistoryChannels, assistantHistoryNumValidSamples);
    const int startPosition = (assistantHistoryWritePosition - assistantHistoryNumValidSamples + assistantHistoryCapacity)
                            % assistantHistoryCapacity;

    for (int channel = 0; channel < assistantHistoryChannels; ++channel)
    {
        if (startPosition + assistantHistoryNumValidSamples <= assistantHistoryCapacity)
        {
            analysisBuffer.copyFrom(channel, 0,
                                    assistantHistoryBuffer, channel, startPosition,
                                    assistantHistoryNumValidSamples);
        }
        else
        {
            const int firstBlockSize = assistantHistoryCapacity - startPosition;
            const int secondBlockSize = assistantHistoryNumValidSamples - firstBlockSize;

            analysisBuffer.copyFrom(channel, 0,
                                    assistantHistoryBuffer, channel, startPosition,
                                    firstBlockSize);
            analysisBuffer.copyFrom(channel, firstBlockSize,
                                    assistantHistoryBuffer, channel, 0,
                                    secondBlockSize);
        }
    }

    return analysisBuffer;
}

// This creates new instances of the plugin..
juce::AudioProcessor* JUCE_CALLTYPE createPluginFilter()
{
    return new OxygenAudioProcessor();
}
