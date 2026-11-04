#include "DynamicEqModule.h"
#include "../GUI/DynamicEqEditor.h"
#include <cmath>

namespace oxygen
{
    namespace
    {
        constexpr float softKneeWidthDb = 3.5f;
        constexpr float dynamicResponseRatio = 1.35f;
        constexpr std::array<float, DynamicEqModule::NumBands> defaultThresholds { -26.0f, -28.0f, -30.0f, -32.0f };
        constexpr std::array<float, DynamicEqModule::NumBands> attackTimesMs { 16.0f, 11.0f, 4.5f, 2.2f };
        constexpr std::array<float, DynamicEqModule::NumBands> releaseTimesMs { 170.0f, 130.0f, 90.0f, 65.0f };

        float computeTimeCoefficient(double sampleRate, float timeMs)
        {
            if (sampleRate <= 0.0 || timeMs <= 0.0f)
                return 0.0f;

            return std::exp(-1.0f / (0.001f * timeMs * (float) sampleRate));
        }
    }

    DynamicEqModule::DynamicEqModule()
        : MasteringModule("Dynamic EQ"),
          apvts(*this, nullptr, "Parameters", createParameterLayout())
    {
        for (int band = 0; band < NumBands; ++band)
        {
            lastAppliedCuts[(size_t) band] = std::numeric_limits<float>::quiet_NaN();
            runtimeStates[(size_t) band].thresholdDb = defaultThresholds[(size_t) band];
        }
    }

    DynamicEqModule::~DynamicEqModule() = default;

    juce::AudioProcessorValueTreeState::ParameterLayout DynamicEqModule::createParameterLayout()
    {
        juce::AudioProcessorValueTreeState::ParameterLayout layout;
        layout.add(std::make_unique<juce::AudioParameterBool>("Bypass", "Bypass", false));

        const char* bandNames[] = { "Low", "LowMid", "HighMid", "High" };
        for (int band = 0; band < NumBands; ++band)
        {
            const juce::String prefix = bandNames[band];
            layout.add(std::make_unique<juce::AudioParameterFloat>(
                prefix + "Thresh",
                prefix + " Threshold",
                juce::NormalisableRange<float>(-48.0f, -6.0f, 0.1f),
                defaultThresholds[(size_t) band]));
            layout.add(std::make_unique<juce::AudioParameterFloat>(
                prefix + "Range",
                prefix + " Range",
                juce::NormalisableRange<float>(0.0f, 12.0f, 0.1f),
                0.0f));
        }

        return layout;
    }

    void DynamicEqModule::prepareToPlay(double sampleRate, int samplesPerBlock)
    {
        juce::ignoreUnused(samplesPerBlock);
        controlPosition = 0;
        lastSampleRate = 0.0;
        for (int band = 0; band < NumBands; ++band)
        {
            const auto i = (size_t) band;
            isolationFilters[i].reset();
            dynamicFilters[i].reset();
            dynamicFilters[i].setCoefficients({}, 0);
            runtimeStates[i].envelope = 0.0f;
            runtimeStates[i].currentGainReductionDb = 0.0f;
            lastAppliedCuts[i] = 0.0f;
        }
        juce::ignoreUnused(sampleRate);
        updateFiltersAndParameters();
    }

    void DynamicEqModule::processBlock(juce::AudioBuffer<float>& buffer, juce::MidiBuffer&)
    {
        if (apvts.getRawParameterValue("Bypass")->load() > 0.5f)
            return;
        const int channels = juce::jmin(2, buffer.getNumChannels());
        if (channels == 0 || buffer.getNumSamples() == 0)
            return;
        updateFiltersAndParameters();
        const auto sampleRate = getSampleRate();
        for (int sample = 0; sample < buffer.getNumSamples(); ++sample)
        {
            std::array<float, 2> dry {}, output {};
            for (int ch = 0; ch < channels; ++ch)
                dry[(size_t) ch] = output[(size_t) ch] = buffer.getSample(ch, sample);
            for (int band = 0; band < NumBands; ++band)
            {
                const auto i = (size_t) band;
                auto& state = runtimeStates[i];
                float peak = 0.0f, energy = 0.0f;
                for (int ch = 0; ch < channels; ++ch)
                {
                    const float value = isolationFilters[i].processSample(ch, dry[(size_t) ch]);
                    peak = juce::jmax(peak, std::abs(value));
                    energy += value * value;
                }
                const float detector = juce::jmax(peak * 0.82f, std::sqrt(energy / (float) channels) * 1.20f);
                const float coefficient = detector > state.envelope ? state.attackCoeff : state.releaseCoeff;
                state.envelope = coefficient * state.envelope + (1.0f - coefficient) * detector;
                if (controlPosition == 0)
                {
                    const float over = juce::Decibels::gainToDecibels(state.envelope, -160.0f) - state.thresholdDb;
                    float cut = 0.0f;
                    if (over > softKneeWidthDb * 0.5f)
                        cut = over;
                    else if (over > -softKneeWidthDb * 0.5f)
                        cut = (over + softKneeWidthDb * 0.5f) * (over + softKneeWidthDb * 0.5f) / (2.0f * softKneeWidthDb);
                    cut = juce::jlimit(0.0f, state.maxCutDb, cut * dynamicResponseRatio);
                    state.currentGainReductionDb = cut;
                    if (std::abs(cut - lastAppliedCuts[i]) > 0.01f || (cut == 0.0f && lastAppliedCuts[i] != 0.0f))
                    {
                        dynamicFilters[i].setCoefficients(dsp::makePeakCoefficients(sampleRate,
                            juce::jmin(CenterFrequencies[i], (float) sampleRate * 0.45f), BandQValues[i], -cut), controlInterval);
                        lastAppliedCuts[i] = cut;
                    }
                }
                dynamicFilters[i].advance();
                for (int ch = 0; ch < channels; ++ch)
                    output[(size_t) ch] = dynamicFilters[i].processSample(ch, output[(size_t) ch]);
            }
            controlPosition = (controlPosition + 1) % controlInterval;
            for (int ch = 0; ch < channels; ++ch)
                buffer.setSample(ch, sample, std::isfinite(output[(size_t) ch]) ? output[(size_t) ch] : 0.0f);
        }
    }

    void DynamicEqModule::updateFiltersAndParameters()
    {
        const double sampleRate = getSampleRate();
        if (sampleRate <= 0.0)
            return;
        const bool sampleRateChanged = std::abs(sampleRate - lastSampleRate) > 1.0e-6;
        const char* names[] = { "Low", "LowMid", "HighMid", "High" };
        for (int band = 0; band < NumBands; ++band)
        {
            const auto i = (size_t) band;
            if (sampleRateChanged)
            {
                isolationFilters[i].setCoefficients(dsp::makeBandPassCoefficients(sampleRate,
                    juce::jmin(CenterFrequencies[i], (float) sampleRate * 0.45f), BandQValues[i]), 0);
                runtimeStates[i].attackCoeff = computeTimeCoefficient(sampleRate, attackTimesMs[i]);
                runtimeStates[i].releaseCoeff = computeTimeCoefficient(sampleRate, releaseTimesMs[i]);
            }
            const juce::String prefix = names[band];
            runtimeStates[i].thresholdDb = apvts.getRawParameterValue(prefix + "Thresh")->load();
            runtimeStates[i].maxCutDb = apvts.getRawParameterValue(prefix + "Range")->load();
        }
        lastSampleRate = sampleRate;
    }

    juce::AudioProcessorEditor* DynamicEqModule::createEditor()
    {
        return new DynamicEqEditor(*this, apvts);
    }
}
