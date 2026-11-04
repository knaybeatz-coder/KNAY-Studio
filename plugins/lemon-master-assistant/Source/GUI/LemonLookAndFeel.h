#pragma once
#include "Theme.h"

// Lemon Studio's charcoal panels, cream lettering and tactile rotary controls.
// The audio processors and assistant remain the attributed oXygen engine.
class LemonLookAndFeel final : public juce::LookAndFeel_V4
{
public:
    LemonLookAndFeel()
    {
        using namespace oxygen::Theme;
        setColour(juce::ResizableWindow::backgroundColourId, Colors::Background);
        setColour(juce::TextButton::buttonColourId, juce::Colour(0xff3b3b3b));
        setColour(juce::TextButton::buttonOnColourId, juce::Colour(0xff655b32));
        setColour(juce::TextButton::textColourOffId, Colors::OnSurface);
        setColour(juce::TextButton::textColourOnId, Colors::OnSurface);
        setColour(juce::ComboBox::backgroundColourId, Colors::SurfaceVariant);
        setColour(juce::ComboBox::textColourId, Colors::OnSurface);
        setColour(juce::ComboBox::arrowColourId, Colors::Primary);
        setColour(juce::PopupMenu::backgroundColourId, Colors::Background);
        setColour(juce::PopupMenu::textColourId, Colors::OnSurface);
        setColour(juce::PopupMenu::highlightedBackgroundColourId, juce::Colour(0xff655b32));
        setColour(juce::AlertWindow::backgroundColourId, Colors::Background);
        setColour(juce::AlertWindow::textColourId, Colors::OnSurface);
        setColour(juce::AlertWindow::outlineColourId, Colors::OnSurface.withAlpha(0.18f));
        setColour(juce::ProgressBar::backgroundColourId, Colors::Background.darker(0.3f));
        setColour(juce::ProgressBar::foregroundColourId, Colors::Primary);
        setColour(juce::Slider::textBoxTextColourId, Colors::OnSurface);
        setColour(juce::Slider::textBoxBackgroundColourId, Colors::Background);
        setColour(juce::Slider::textBoxOutlineColourId, juce::Colours::transparentBlack);
    }
    juce::Font getAlertWindowTitleFont() override { return oxygen::Theme::Fonts::getHeading().withHeight(21.f); }
    juce::Font getAlertWindowMessageFont() override { return oxygen::Theme::Fonts::getBody(); }
    juce::Font getAlertWindowFont() override { return oxygen::Theme::Fonts::getBody(); }
    void drawProgressBar(juce::Graphics& g,juce::ProgressBar&,int width,int height,double progress,const juce::String& text) override
    {
        using namespace oxygen::Theme;
        const auto r=juce::Rectangle<float>(0,0,(float)width,(float)height).reduced(1);
        g.setColour(Colors::Background.darker(.3f));g.fillRoundedRectangle(r,5);
        const auto track=r.reduced(3);
        if(progress>=0 && progress<=1){
            g.setColour(Colors::Primary);g.fillRoundedRectangle(track.withWidth(track.getWidth()*(float)progress),3);
        }else{
            const float phase=(float)(juce::Time::getMillisecondCounter()%1800)/1800.f;
            const float travel=phase<.5f?phase*2.f:(1.f-phase)*2.f;
            g.setColour(Colors::Primary.withAlpha(.8f));
            g.fillRoundedRectangle({track.getX()+travel*track.getWidth()*.7f,track.getY(),track.getWidth()*.3f,track.getHeight()},3);
        }
        g.setColour(Colors::OnSurface.withAlpha(.15f));g.drawRoundedRectangle(r,5,1);
        // Keep status text on a dark inset so it stays readable over any fill amount.
        const juce::String label=progress>=0 && progress<=1?text:juce::String("Analysing...");
        if(label.isNotEmpty()){
            auto badge=r.withSizeKeepingCentre(104.f,juce::jmin(20.f,r.getHeight()));
            g.setColour(Colors::Background.withAlpha(.92f));g.fillRoundedRectangle(badge,3);
            g.setColour(Colors::OnSurface);g.setFont(Fonts::getBody().withHeight(12));g.drawText(label,badge,juce::Justification::centred);
        }
    }
    void drawButtonBackground(juce::Graphics& g, juce::Button& b, const juce::Colour& colour, bool hover, bool down) override
    {
        auto r = b.getLocalBounds().toFloat().reduced(1.0f);
        auto c = colour.withMultipliedAlpha(b.isEnabled() ? 1.0f : 0.4f);
        if (hover) c = c.brighter(0.12f);
        if (down) c = c.darker(0.15f);
        g.setColour(juce::Colour(0xff171717)); g.fillRoundedRectangle(r.translated(0, 1), 4);
        g.setGradientFill(juce::ColourGradient(c.brighter(0.10f), r.getX(), r.getY(), c, r.getX(), r.getBottom(), false));
        g.fillRoundedRectangle(r, 4);
        g.setColour(juce::Colour(0xff666666).withAlpha(0.45f)); g.drawRoundedRectangle(r, 4, 1);
    }
    void drawRotarySlider(juce::Graphics& g, int x, int y, int w, int h, float value,
                          float start, float end, juce::Slider& slider) override
    {
        auto r = juce::Rectangle<float>((float)x, (float)y, (float)w, (float)h).reduced(6);
        const float radius = juce::jmin(r.getWidth(), r.getHeight()) * 0.5f;
        const auto c = r.getCentre();
        const float angle = start + value * (end - start);
        juce::Path arc; arc.addCentredArc(c.x, c.y, radius, radius, 0, start, end, true);
        g.setColour(juce::Colour(0xff191919)); g.strokePath(arc, juce::PathStrokeType(3));
        juce::Path active; active.addCentredArc(c.x, c.y, radius, radius, 0, start, angle, true);
        g.setColour(slider.isEnabled() ? oxygen::Theme::Colors::Primary : juce::Colour(0xff686868));
        g.strokePath(active, juce::PathStrokeType(2.5f));
        auto face = juce::Rectangle<float>(radius * 1.65f, radius * 1.65f).withCentre(c);
        g.setColour(juce::Colour(0xff181818)); g.fillEllipse(face.translated(0, 2));
        g.setGradientFill(juce::ColourGradient(juce::Colour(0xff626262), c.x, face.getY(), juce::Colour(0xff353535), c.x, face.getBottom(), false));
        g.fillEllipse(face); g.setColour(juce::Colour(0xff737373)); g.drawEllipse(face, 1);
        g.setColour(oxygen::Theme::Colors::OnSurface);
        g.drawLine(c.x + std::sin(angle)*radius*0.38f, c.y - std::cos(angle)*radius*0.38f,
                   c.x + std::sin(angle)*radius*0.69f, c.y - std::cos(angle)*radius*0.69f, 2);
    }
};
