#include "StereoImagerModule.h"
#include "../GUI/StereoImagerEditor.h"
#include <cmath>

namespace oxygen
{
    namespace
    {
        constexpr float minCrossoverSpacingHz = 20.0f;

        struct CrossoverSet
        {
            float lowMid = 200.0f;
            float midHigh = 2000.0f;
            float high = 8000.0f;
        };

        CrossoverSet sanitizeCrossovers(float sampleRate, float lowMid, float midHigh, float high)
        {
            const float nyquist = juce::jmax(60.0f, sampleRate * 0.45f);
            const float lowMidMax = juce::jmax(20.0f, nyquist - (2.0f * minCrossoverSpacingHz));

            CrossoverSet sanitized;
            sanitized.lowMid = juce::jlimit(20.0f, lowMidMax, lowMid);

            const float midHighMin = juce::jmin(nyquist - minCrossoverSpacingHz, sanitized.lowMid + minCrossoverSpacingHz);
            const float midHighMax = juce::jmax(midHighMin, nyquist - minCrossoverSpacingHz);
            sanitized.midHigh = juce::jlimit(midHighMin, midHighMax, midHigh);

            const float highMin = juce::jmin(nyquist, sanitized.midHigh + minCrossoverSpacingHz);
            sanitized.high = juce::jlimit(highMin, nyquist, high);

            return sanitized;
        }
    }

    StereoImagerModule::StereoImagerModule()
        : MasteringModule("Stereo Imager"),
          apvts(*this, nullptr, "Parameters", createParameterLayout())
    {
    }

    StereoImagerModule::~StereoImagerModule()
    {
    }

    juce::AudioProcessorValueTreeState::ParameterLayout StereoImagerModule::createParameterLayout()
    {
        juce::AudioProcessorValueTreeState::ParameterLayout layout;

        // Crossovers
        layout.add(std::make_unique<juce::AudioParameterFloat>("LowMidX", "Low-Mid Xover",
            juce::NormalisableRange<float>(20.0f, 1000.0f, 1.0f, 0.5f), 200.0f));
        layout.add(std::make_unique<juce::AudioParameterFloat>("MidHighX", "Mid-High Xover",
            juce::NormalisableRange<float>(500.0f, 5000.0f, 1.0f, 0.5f), 2000.0f));
        layout.add(std::make_unique<juce::AudioParameterFloat>("HighX", "High Xover",
            juce::NormalisableRange<float>(2000.0f, 15000.0f, 1.0f, 0.5f), 8000.0f));

        // Width per band (0.0 = Mono, 1.0 = Normal, 2.0 = Extra Wide)
        layout.add(std::make_unique<juce::AudioParameterFloat>("LowWidth", "Low Width",
            juce::NormalisableRange<float>(0.0f, 2.0f, 0.01f), 1.0f));
        layout.add(std::make_unique<juce::AudioParameterFloat>("LowMidWidth", "Low-Mid Width",
            juce::NormalisableRange<float>(0.0f, 2.0f, 0.01f), 1.0f));
        layout.add(std::make_unique<juce::AudioParameterFloat>("HighMidWidth", "High-Mid Width",
            juce::NormalisableRange<float>(0.0f, 2.0f, 0.01f), 1.0f));
        layout.add(std::make_unique<juce::AudioParameterFloat>("HighWidth", "High Width",
            juce::NormalisableRange<float>(0.0f, 2.0f, 0.01f), 1.0f));

        layout.add(std::make_unique<juce::AudioParameterBool>("Bypass", "Bypass", false));

        return layout;
    }

    void StereoImagerModule::prepareToPlay(double sampleRate, int samplesPerBlock)
    {
        juce::dsp::ProcessSpec spec;
        spec.sampleRate = sampleRate;
        spec.maximumBlockSize = samplesPerBlock;
        spec.numChannels = getTotalNumOutputChannels();

        crossover.prepare(spec);

        bufferLow.setSize(spec.numChannels, samplesPerBlock);
        bufferLowMid.setSize(spec.numChannels, samplesPerBlock);
        bufferHighMid.setSize(spec.numChannels, samplesPerBlock);
        bufferHigh.setSize(spec.numChannels, samplesPerBlock);

        detectorCoefficient = std::exp(-1.0 / (sampleRate * 0.100));
        gainCoefficient = (float) std::exp(-1.0 / (sampleRate * 0.010));
        for (auto& state : widthStates)
            state = {};
        updateParameters();
    }

    void StereoImagerModule::processBlock(juce::AudioBuffer<float>& buffer, juce::MidiBuffer&)
    {
        if (apvts.getRawParameterValue("Bypass")->load() > 0.5f)
            return;

        updateParameters();

        const int numChannels = buffer.getNumChannels();
        const int numSamples = buffer.getNumSamples();
        if (numSamples == 0)
            return;

        if (numChannels < 2) return; // Stereo only

        // Ensure buffers
        if (bufferLow.getNumChannels() != numChannels || bufferLow.getNumSamples() < numSamples)
        {
            bufferLow.setSize(numChannels, numSamples, true, true, true);
            bufferLowMid.setSize(numChannels, numSamples, true, true, true);
            bufferHighMid.setSize(numChannels, numSamples, true, true, true);
            bufferHigh.setSize(numChannels, numSamples, true, true, true);
        }

        // Split Bands
        for (int ch = 0; ch < numChannels; ++ch)
        {
            auto* inData = buffer.getReadPointer(ch);
            auto* lowData = bufferLow.getWritePointer(ch);
            auto* lowMidData = bufferLowMid.getWritePointer(ch);
            auto* highMidData = bufferHighMid.getWritePointer(ch);
            auto* highData = bufferHigh.getWritePointer(ch);

            for (int i = 0; i < numSamples; ++i)
            {
                std::array<float, 4> bands {};
                crossover.processSample(ch, inData[i], bands);
                lowData[i] = bands[0];
                lowMidData[i] = bands[1];
                highMidData[i] = bands[2];
                highData[i] = bands[3];
            }
        }

        // Process Width per band
        processBand(bufferLow, apvts.getRawParameterValue("LowWidth")->load(), widthStates[0], numSamples);
        processBand(bufferLowMid, apvts.getRawParameterValue("LowMidWidth")->load(), widthStates[1], numSamples);
        processBand(bufferHighMid, apvts.getRawParameterValue("HighMidWidth")->load(), widthStates[2], numSamples);
        processBand(bufferHigh, apvts.getRawParameterValue("HighWidth")->load(), widthStates[3], numSamples);

        // Sum back
        buffer.clear();
        for (int ch = 0; ch < numChannels; ++ch)
        {
            buffer.addFrom(ch, 0, bufferLow, ch, 0, numSamples);
            buffer.addFrom(ch, 0, bufferLowMid, ch, 0, numSamples);
            buffer.addFrom(ch, 0, bufferHighMid, ch, 0, numSamples);
            buffer.addFrom(ch, 0, bufferHigh, ch, 0, numSamples);

            auto* out = buffer.getWritePointer(ch);
            for (int i = 0; i < numSamples; ++i)
            {
                if (!std::isfinite(out[i]))
                    out[i] = 0.0f;
            }
        }
    }

    void StereoImagerModule::processBand(juce::AudioBuffer<float>& bandBuffer, float width,
                                          WidthState& state, int numSamples)
    {
        width = juce::jlimit(0.0f, 2.0f, width);
        const float requestedSideScale = width <= 1.0f ? width : 1.0f + (width - 1.0f) * 2.0f;
        auto* left = bandBuffer.getWritePointer(0);
        auto* right = bandBuffer.getWritePointer(1);
        const auto follow = [this](double previous, double value)
        {
            return detectorCoefficient * previous + (1.0 - detectorCoefficient) * value;
        };

        // Continuous energy and gain envelopes make safety decisions independent
        // of the host block size and avoid abrupt level changes on drum hits.
        for (int i = 0; i < numSamples; ++i)
        {
            const float l = left[i], r = right[i];
            const float mid = 0.5f * (l + r), side = 0.5f * (l - r);
            state.l2 = follow(state.l2, (double) l * l);
            state.r2 = follow(state.r2, (double) r * r);
            state.lr = follow(state.lr, (double) l * r);
            state.mid2 = follow(state.mid2, (double) mid * mid);
            state.side2 = follow(state.side2, (double) side * side);
            float scale = requestedSideScale;
            float compensation = 1.0f;
            if (scale > 1.0f)
            {
                const double denominator = std::sqrt(state.l2 * state.r2);
                const double correlation = denominator > 1.0e-20 ? state.lr / denominator : 1.0;
                if (correlation < 0.15 || state.side2 > state.mid2 * 0.64)
                    scale = juce::jmin(scale, 1.10f);
                else if (correlation < 0.35 || state.side2 > state.mid2 * 0.36)
                    scale = juce::jmin(scale, 1.25f);
                const double energy = state.mid2 + state.side2;
                if (energy > 1.0e-20)
                    compensation = (float) std::sqrt(energy / (state.mid2 + state.side2 * scale * scale));
            }
            state.midGain = gainCoefficient * state.midGain + (1.0f - gainCoefficient) * compensation;
            state.sideGain = gainCoefficient * state.sideGain + (1.0f - gainCoefficient) * scale * compensation;
            left[i] = mid * state.midGain + side * state.sideGain;
            right[i] = mid * state.midGain - side * state.sideGain;
        }
    }

    void StereoImagerModule::updateParameters()
    {
        const auto crossovers = sanitizeCrossovers((float) getSampleRate(),
                                                   apvts.getRawParameterValue("LowMidX")->load(),
                                                   apvts.getRawParameterValue("MidHighX")->load(),
                                                   apvts.getRawParameterValue("HighX")->load());

        crossover.setCrossovers(crossovers.lowMid, crossovers.midHigh, crossovers.high);
    }

    juce::AudioProcessorEditor* StereoImagerModule::createEditor()
    {
        return new StereoImagerEditor(*this, apvts);
    }

}
