#pragma once
#include "InferenceEngine.h"
#include "../Modules/EqualizerModule.h"
#include "../Modules/DynamicEqModule.h"
#include "../Modules/MultibandCompressorModule.h"
#include "../Modules/StereoImagerModule.h"
#include "../Modules/GainModule.h"
#include "../Modules/MaximizerModule.h"

namespace oxygen
{
// Shared by the live rack and the offline assistant preview so calibration uses
// exactly the same parameter mapping and quantization as the accepted proposal.
inline void applyParametersToModule(juce::AudioProcessor* processor, const MasteringParameters& params)
{
    auto setParameter = [] (juce::AudioProcessorValueTreeState& apvts, const juce::String& id, float value)
    {
        if (auto* parameter = apvts.getParameter(id))
            parameter->setValueNotifyingHost(parameter->convertTo0to1(value));
    };

    if (auto* eqModule = dynamic_cast<oxygen::EqualizerModule*>(processor))
    {
        for (int band = 0; band < oxygen::MasteringParameters::eqBandCount; ++band)
        {
            setParameter(eqModule->apvts, "Freq" + juce::String(band), EqualizerModule::Frequencies[band]);
            setParameter(eqModule->apvts, "Q" + juce::String(band), EqualizerModule::BandQValues[(size_t)band]);
            setParameter(eqModule->apvts, "Gain" + juce::String(band), params.eqBandGains[(size_t) band]);
        }
    }
    else if (auto* dynamicEqModule = dynamic_cast<oxygen::DynamicEqModule*>(processor))
    {
        setParameter(dynamicEqModule->apvts, "LowThresh", params.dynamicEqThresholds[0]);
        setParameter(dynamicEqModule->apvts, "LowRange", params.dynamicEqRanges[0]);
        setParameter(dynamicEqModule->apvts, "LowMidThresh", params.dynamicEqThresholds[1]);
        setParameter(dynamicEqModule->apvts, "LowMidRange", params.dynamicEqRanges[1]);
        setParameter(dynamicEqModule->apvts, "HighMidThresh", params.dynamicEqThresholds[2]);
        setParameter(dynamicEqModule->apvts, "HighMidRange", params.dynamicEqRanges[2]);
        setParameter(dynamicEqModule->apvts, "HighThresh", params.dynamicEqThresholds[3]);
        setParameter(dynamicEqModule->apvts, "HighRange", params.dynamicEqRanges[3]);
    }
    else if (auto* compModule = dynamic_cast<oxygen::MultibandCompressorModule*>(processor))
    {
        setParameter(compModule->apvts, "LowMidX", params.lowMidX);
        setParameter(compModule->apvts, "MidHighX", params.midHighX);
        setParameter(compModule->apvts, "HighX", params.highX);

        setParameter(compModule->apvts, "LowThresh", params.lowThresh);
        setParameter(compModule->apvts, "LowRatio", params.lowRatio);
        setParameter(compModule->apvts, "LowAttack", params.lowAttack);
        setParameter(compModule->apvts, "LowRelease", params.lowRelease);
        setParameter(compModule->apvts, "LowGain", 0.0f);

        setParameter(compModule->apvts, "LowMidThresh", params.lowMidThresh);
        setParameter(compModule->apvts, "LowMidRatio", params.lowMidRatio);
        setParameter(compModule->apvts, "LowMidAttack", params.lowMidAttack);
        setParameter(compModule->apvts, "LowMidRelease", params.lowMidRelease);
        setParameter(compModule->apvts, "LowMidGain", 0.0f);

        setParameter(compModule->apvts, "HighMidThresh", params.highMidThresh);
        setParameter(compModule->apvts, "HighMidRatio", params.highMidRatio);
        setParameter(compModule->apvts, "HighMidAttack", params.highMidAttack);
        setParameter(compModule->apvts, "HighMidRelease", params.highMidRelease);
        setParameter(compModule->apvts, "HighMidGain", 0.0f);

        setParameter(compModule->apvts, "HighThresh", params.highThresh);
        setParameter(compModule->apvts, "HighRatio", params.highRatio);
        setParameter(compModule->apvts, "HighAttack", params.highAttack);
        setParameter(compModule->apvts, "HighRelease", params.highRelease);
        setParameter(compModule->apvts, "HighGain", 0.0f);
    }
    else if (auto* imagerModule = dynamic_cast<oxygen::StereoImagerModule*>(processor))
    {
        setParameter(imagerModule->apvts, "LowMidX", params.lowMidX);
        setParameter(imagerModule->apvts, "MidHighX", params.midHighX);
        setParameter(imagerModule->apvts, "HighX", params.highX);
        setParameter(imagerModule->apvts, "LowWidth", params.lowWidth);
        setParameter(imagerModule->apvts, "LowMidWidth", params.lowMidWidth);
        setParameter(imagerModule->apvts, "HighMidWidth", params.highMidWidth);
        setParameter(imagerModule->apvts, "HighWidth", params.highWidth);
    }
    else if (auto* gainModule = dynamic_cast<oxygen::GainModule*>(processor))
    {
        if (auto* gainParameter = dynamic_cast<juce::RangedAudioParameter*>(gainModule->getParameters()[0]))
            gainParameter->setValueNotifyingHost(gainParameter->convertTo0to1(params.outputGain));
    }
    else if (auto* maxModule = dynamic_cast<oxygen::MaximizerModule*>(processor))
    {
        setParameter(maxModule->apvts, "Threshold", params.maximizerThreshold);
        setParameter(maxModule->apvts, "Ceiling", params.maximizerCeiling);
        setParameter(maxModule->apvts, "Release", params.maximizerRelease);
        setParameter(maxModule->apvts, "Mode", (float) params.maximizerMode);
    }
}
}
