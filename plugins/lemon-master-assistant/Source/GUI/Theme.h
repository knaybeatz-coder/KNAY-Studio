#pragma once
#include <JuceHeader.h>

namespace oxygen
{
    namespace Theme
    {
        // Color Palette (Material Design 3 inspired Dark Theme)
        namespace Colors
        {
            const juce::Colour Background    = juce::Colour::fromString("FF252525"); // Surface
            const juce::Colour SurfaceVariant= juce::Colour::fromString("FF303030"); // Slightly lighter background for modules
            const juce::Colour OnSurface     = juce::Colour::fromString("FFE8E6DC"); // Text
            const juce::Colour OnSurfaceVariant = juce::Colour::fromString("FFAAA9A1"); // Secondary Text

            const juce::Colour Primary       = juce::Colour::fromString("FFFFDE70"); // Cyan Neon (Brand)
            const juce::Colour Secondary     = juce::Colour::fromString("FF8DBBB4"); // Magenta Neon (Brand)

            const juce::Colour Error         = juce::Colour::fromString("FFCF6679");

            const juce::Colour ButtonFill    = SurfaceVariant.brighter(0.1f);
            const juce::Colour ButtonHover   = ButtonFill.brighter(0.1f);
            const juce::Colour ButtonPress   = ButtonFill.darker(0.1f);
        }

        // Typography
        namespace Fonts
        {
            static const juce::Font getHeading() { return juce::Font("Segoe UI", 24.0f, juce::Font::bold); }
            static const juce::Font getSubheading() { return juce::Font("Segoe UI", 18.0f, juce::Font::plain); }
            static const juce::Font getBody() { return juce::Font("Segoe UI", 14.0f, juce::Font::plain); }
        }

        // Layout Constants
        namespace Dimens
        {
            constexpr float CornerRadius = 5.0f;
            constexpr float SmallCornerRadius = 6.0f;
            constexpr float Padding = 10.0f;
            constexpr float Margin = 5.0f;
        }
    }
}
