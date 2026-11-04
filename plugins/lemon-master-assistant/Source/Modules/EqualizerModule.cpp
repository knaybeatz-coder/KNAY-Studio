#include "EqualizerModule.h"
#include "../GUI/EqualizerEditor.h"
#include <cmath>
#include <limits>

namespace oxygen
{

    EqualizerModule::EqualizerModule()
        : MasteringModule("Graphic EQ"),
          apvts(*this, nullptr, "Parameters", createParameterLayout())
    {
        for (int i = 0; i < NumBands; ++i)
        {
            filters.push_back(std::make_unique<Filter>());
            lastGains[i] = lastFreqs[i] = lastQs[i] = std::numeric_limits<float>::quiet_NaN();
            smoothedGains[(size_t) i].setCurrentAndTargetValue(0.0f);
        }
    }

    EqualizerModule::~EqualizerModule()
    {
    }

    juce::AudioProcessorValueTreeState::ParameterLayout EqualizerModule::createParameterLayout()
    {
        juce::AudioProcessorValueTreeState::ParameterLayout layout;

        layout.add(std::make_unique<juce::AudioParameterBool>("Bypass", "Bypass", false));

        for (int i = 0; i < NumBands; ++i)
        {
            juce::String freqStr;
            if (Frequencies[i] >= 1000.0f)
                freqStr = juce::String(Frequencies[i] / 1000.0f, 1) + "k";
            else
                freqStr = juce::String((int)Frequencies[i]);

            layout.add(std::make_unique<juce::AudioParameterFloat>(
                "Gain" + juce::String(i),
                freqStr + "Hz",
                juce::NormalisableRange<float>(-12.0f, 12.0f, 0.1f),
                0.0f));
        }

        for (int i = 0; i < NumBands; ++i) {
            auto range = juce::NormalisableRange<float>(20.f, 20000.f, 0.1f);
            range.setSkewForCentre(1000.f);
            layout.add(std::make_unique<juce::AudioParameterFloat>("Freq" + juce::String(i), "Band frequency", range, Frequencies[i]));
            layout.add(std::make_unique<juce::AudioParameterFloat>("Q" + juce::String(i), "Band Q", juce::NormalisableRange<float>(0.1f, 12.f, 0.01f), BandQValues[(size_t)i]));
        }
        return layout;
    }

    void EqualizerModule::prepareToPlay(double sampleRate, int samplesPerBlock)
    {
        juce::dsp::ProcessSpec spec;
        spec.sampleRate = sampleRate;
        spec.maximumBlockSize = samplesPerBlock;
        spec.numChannels = getTotalNumOutputChannels();

        for (auto& filter : filters)
        {
            filter->prepare(spec);
            filter->reset();
        }

        for (int i = 0; i < NumBands; ++i)
        {
            smoothedFreqs[(size_t)i].reset(sampleRate, parameterSmoothingSeconds);
            smoothedQs[(size_t)i].reset(sampleRate, parameterSmoothingSeconds);
            smoothedFreqs[(size_t)i].setCurrentAndTargetValue(apvts.getRawParameterValue("Freq" + juce::String(i))->load());
            smoothedQs[(size_t)i].setCurrentAndTargetValue(apvts.getRawParameterValue("Q" + juce::String(i))->load());
            smoothedGains[(size_t) i].reset(sampleRate, parameterSmoothingSeconds);
            smoothedGains[(size_t) i].setCurrentAndTargetValue(apvts.getRawParameterValue("Gain" + juce::String(i))->load());
        }

        lastSampleRate = 0.0;
        updateFilters(samplesPerBlock);
    }

    void EqualizerModule::processBlock(juce::AudioBuffer<float>& buffer, juce::MidiBuffer&)
    {
        if (apvts.getRawParameterValue("Bypass")->load() > 0.5f)
            return;

        updateFilters(buffer.getNumSamples());

        juce::dsp::AudioBlock<float> block(buffer);
        juce::dsp::ProcessContextReplacing<float> context(block);

        // Process serially
        for (auto& filter : filters)
            filter->process(context);
    }

    void EqualizerModule::updateFilters(int numSamples)
    {
        const auto currentSampleRate = getSampleRate();
        if (currentSampleRate <= 0.0)
            return;

        const bool sampleRateChanged = std::abs(currentSampleRate - lastSampleRate) > 1.0e-6;
        const float maxFreq = (float) currentSampleRate * 0.45f;
        if (maxFreq <= 20.0f)
            return;

        for (int i = 0; i < NumBands; ++i)
        {
            auto& smoothedGain = smoothedGains[(size_t) i];
            smoothedGain.setTargetValue(apvts.getRawParameterValue("Gain" + juce::String(i))->load());
            const float gainDb = (numSamples > 0)
                               ? smoothedGain.skip(numSamples)
                               : smoothedGain.getCurrentValue();

            auto& sf=smoothedFreqs[(size_t)i]; auto& sq=smoothedQs[(size_t)i];
            sf.setTargetValue(apvts.getRawParameterValue("Freq" + juce::String(i))->load());
            sq.setTargetValue(apvts.getRawParameterValue("Q" + juce::String(i))->load());
            const float freq=numSamples>0?sf.skip(numSamples):sf.getCurrentValue();
            const float q=numSamples>0?sq.skip(numSamples):sq.getCurrentValue();
            if (sampleRateChanged || !std::isfinite(lastGains[i]) || std::abs(gainDb - lastGains[i]) > 0.0025f
                || freq != lastFreqs[i] || q != lastQs[i])
            {
                lastGains[i] = gainDb;
                const float gain = std::isfinite(gainDb) ? juce::Decibels::decibelsToGain(gainDb) : 1.0f;
                lastFreqs[i]=freq; lastQs[i]=q;
                const float safeFreq = juce::jlimit(20.0f, maxFreq, freq);

                auto coeffs = juce::dsp::IIR::ArrayCoefficients<float>::makePeakFilter(currentSampleRate, safeFreq, q, gain);
                *filters[i]->state = coeffs;
            }
        }

        lastSampleRate = currentSampleRate;
    }

    juce::AudioProcessorEditor* EqualizerModule::createEditor()
    {
        return new EqualizerEditor(*this, apvts);
    }

}
