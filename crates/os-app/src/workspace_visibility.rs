// SPDX-License-Identifier: GPL-3.0-or-later

//! Visibility filters the presentation without rewriting toolbar layouts or projects.

use crate::settings_file;

pub(crate) const GROUPS: &[(&str, &[(&str, &str)])] = &[
    ("Toolbar panels", &[
        ("Hint panel", "visible_hint"),
        ("Time display", "visible_time"),
        ("Output monitor", "visible_monitor"),
        ("Recording options", "visible_recording"),
        ("Mode buttons", "visible_modes"),
        ("Utility buttons", "visible_utilities"),
        ("Main volume", "visible_volume"),
        ("Varispeed", "visible_pitch"),
        ("Song position", "visible_song-position"),
        ("Global snap", "visible_snap"),
        ("Pattern picker", "visible_pattern"),
        ("Toolbar logo", "visible_logo"),
    ]),
    ("Toolbar window shortcuts", &[
        ("Playlist shortcut", "visible_win-playlist"),
        ("Piano Roll shortcut", "visible_win-piano"),
        ("Channel Rack shortcut", "visible_win-rack"),
        ("Mixer shortcut", "visible_win-mixer"),
        ("Browser shortcut", "visible_win-browser"),
        ("Project Picker shortcut", "visible_win-project"),
        ("Plugin Picker shortcut", "visible_win-plugin"),
        ("Tempo Tapper shortcut", "visible_win-tapper"),
        ("Touch Controller shortcut", "visible_win-touch"),
    ]),
    ("Toolbar extras", &[
        ("CPU and live monitor", "visible_cpu"),
        ("Vintage CPU meter", "visible_vintage_cpu"),
        ("Vintage output scope", "visible_vintage_scope"),
        ("Vintage sync and MIDI lights", "visible_vintage_activity"),
        ("AI control button when inactive", "visible_ai"),
        ("Tesla shortcut", "visible_tesla"),
        ("Update button", "visible_update"),
        ("Donate button", "visible_donate"),
        ("Report a Bug button", "visible_feedback"),
    ]),
    ("Browser", &[
        ("Community Samples shortcuts", "visible_browser-community"),
        ("Plugin Downloads tab", "visible_browser-downloads"),
        ("Feature Voting tab", "visible_browser-voting"),
        ("Favorites tab", "visible_browser-favorites"),
        ("Sample preview panel", "visible_browser-preview"),
        ("File information panel", "visible_browser-info"),
    ]),
    ("Playlist", &[
        ("Arrangement mini-map", "visible_playlist-map"),
        ("Bounce audio menu", crate::track_render::VISIBILITY),
        ("Audio clip waveforms", "visible_playlist-waves"),
        ("Stretch button", "visible_playlist-stretch"),
        ("Comping button", "visible_playlist-comp"),
        ("Clips picker button", "visible_playlist-clips"),
        ("Add instrument button", "visible_playlist-add-instrument"),
        ("Clip color picker and custom swatches", "visible_playlist-custom-swatches"),
        ("Vocal Align command", "visible_playlist-vocal-align"),
        ("Vocal Align review markers", "visible_playlist-vocal-align-review"),
    ]),
    ("Piano Roll", &[
        ("Vertical zoom control", "visible_piano-row-zoom"),
        ("Zoom shortcut setting", "visible_piano-zoom-keys"),
        ("Note mini-map", "visible_piano-map"),
        ("Ghost notes from other channels", "visible_piano-ghost"),
        ("Tools menu shortcut", "visible_piano-tools"),
        ("Chord Workspace shortcut", "visible_piano-chords"),
        ("Undo button", "visible_piano-undo"),
        ("Place Pattern to Playlist button", "visible_piano-place-pattern"),
    ]),
    ("Channel Rack", &[
        ("Loop mode button", "visible_rack-loop"),
        ("Graph editor shortcut", "visible_rack-graph"),
    ]),
    ("Mixer", &[
        ("Mixer channel lock buttons", "visible_mixer-channel-lock"),
        ("Mixer section shortcuts", crate::views::mixer::SECTION_SHORTCUTS),
        ("Stereo separation on Full Mixer strips", crate::views::mixer::STEREO_SEPARATION),
        ("Scroll to Drums button", crate::mixer::Section::Drums.visibility()),
        ("Scroll to Instruments button", crate::mixer::Section::Instruments.visibility()),
        ("Scroll to Vocals button", crate::mixer::Section::Vocals.visibility()),
        ("Scroll to Effects button", crate::mixer::Section::Effects.visibility()),
        ("Scroll to Buses button", crate::mixer::Section::Buses.visibility()),
        ("Routing cables", "visible_mixer-cables"),
        ("Channel meters", "visible_mixer-meters"),
        ("Mini EQ panel", "visible_mixer-eq"),
        ("Mini EQ bypass button", crate::mini_eq::BYPASS_VISIBILITY),
    ]),
    ("Workspace", &[
        ("Background logo and version", "visible_watermark"),
        ("Vintage 0 dB markers", crate::vintage::GAIN_MARKS),
        ("Vintage startup sound", crate::startup_sound::VINTAGE_SOUND),
    ]),
    ("Settings tabs", &[
        ("Theme tab", "visible_settings-theme"),
        ("Project tab", "visible_settings-project"),
        ("Info tab", "visible_settings-info"),
        ("Debug tab", "visible_settings-debug"),
        ("About tab", "visible_settings-about"),
        ("Menus tab and customization shortcut", crate::menu_layout::VISIBILITY),
    ]),
    ("Lemon EQ", &[
        ("Band controls card", lemon_parametric_eq_2::visibility::BAND_CARD),
    ]),
    ("Lemon Wave", &[
        ("Right-click modulation controls", lemon_wave::visibility::MODULATION_MENU),
        ("Phase FM and ring modulation controls", lemon_wave::visibility::HARMONIC_CONTROLS),
    ]),
];

pub(crate) const RESET: &str = "Restore default visibility";

pub(crate) fn key(label: &str) -> Option<&'static str> {
    GROUPS.iter().flat_map(|(_, rows)| *rows)
        .find_map(|&(name, key)| (label == name).then_some(key))
}

pub(crate) fn shown(key: &str) -> bool {
    settings_file::flag(key, true)
}

pub(crate) fn toggle(key: &str) {
    settings_file::set_flag(key, !shown(key));
}

pub(crate) fn reset() {
    let defaults = GROUPS.iter().flat_map(|(_, rows)| *rows)
        .map(|&(_, key)| (key.into(), String::new())).collect::<Vec<_>>();
    settings_file::write_all(&defaults);
}

pub(crate) const WINDOW_BUTTONS: [&str; 9] = [
    "visible_win-playlist", "visible_win-piano", "visible_win-rack", "visible_win-mixer",
    "visible_win-browser", "visible_win-project", "visible_win-plugin", "visible_win-tapper",
    "visible_win-touch",
];
