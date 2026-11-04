// SPDX-License-Identifier: GPL-3.0-or-later
//! Lemon Wave controls; appended models preserve existing parameter identities.
use lemon_small_fx::{Card, Control, Mark, Setting, Sound, Spec};
use os_ui::Rect;
pub const A_BANK: usize = 0;
pub const A_POSITION: usize = 1;
pub const A_LEVEL: usize = 2;
pub const A_TUNE: usize = 3;
pub const A_UNISON: usize = 4;
pub const A_DETUNE: usize = 5;
pub const A_PAN: usize = 6;
pub const A_WARP: usize = 7;
pub const B_BANK: usize = 8;
pub const B_POSITION: usize = 9;
pub const B_LEVEL: usize = 10;
pub const B_TUNE: usize = 11;
pub const B_UNISON: usize = 12;
pub const B_DETUNE: usize = 13;
pub const B_PAN: usize = 14;
pub const B_WARP: usize = 15;
pub const CUTOFF: usize = 16;
pub const RESONANCE: usize = 17;
pub const FILTER: usize = 18;
pub const DRIVE: usize = 19;
pub const SUB: usize = 20;
pub const NOISE: usize = 21;
pub const GAIN: usize = 22;
pub const SPREAD: usize = 23;
pub const ATTACK: usize = 24;
pub const DECAY: usize = 25;
pub const SUSTAIN: usize = 26;
pub const RELEASE: usize = 27;
pub const ENV_ATTACK: usize = 28;
pub const ENV_DECAY: usize = 29;
pub const ENV_SUSTAIN: usize = 30;
pub const ENV_RELEASE: usize = 31;
pub const ENV_AMOUNT: usize = 32;
pub const LFO_RATE: usize = 33;
pub const LFO_AMOUNT: usize = 34;
pub const LFO_SHAPE: usize = 35;
pub const LFO_TARGET: usize = 36;
pub const LFO_SYNC: usize = 37;
pub const CHORUS_RATE: usize = 38;
pub const CHORUS_DEPTH: usize = 39;
pub const CHORUS_MIX: usize = 40;
pub const DELAY_TIME: usize = 41;
pub const DELAY_FEEDBACK: usize = 42;
pub const DELAY_MIX: usize = 43;
pub const A_SOURCE: usize = 44;
pub const A_MODE: usize = 45;
pub const A_AMOUNT: usize = 46;
pub const B_SOURCE: usize = 47;
pub const B_MODE: usize = 48;
pub const B_AMOUNT: usize = 49;
pub const A_PHASE: usize = 50;
pub const A_RANDOM: usize = 51;
pub const A_BLEND: usize = 52;
pub const B_PHASE: usize = 53;
pub const B_RANDOM: usize = 54;
pub const B_BLEND: usize = 55;
pub const MOD1_SOURCE: usize = 56;
pub const MOD1_TARGET: usize = 57;
pub const MOD1_AMOUNT: usize = 58;
pub const MOD2_SOURCE: usize = 59;
pub const MOD2_TARGET: usize = 60;
pub const MOD2_AMOUNT: usize = 61;
pub const ENV_ENGINE: usize = 62;
pub const AMP_HOLD: usize = 63;
pub const AMP_ATK_CURVE: usize = 64;
pub const AMP_DEC_CURVE: usize = 65;
pub const AMP_REL_CURVE: usize = 66;
pub const ENV_HOLD: usize = 67;
pub const ENV_ATK_CURVE: usize = 68;
pub const ENV_DEC_CURVE: usize = 69;
pub const ENV_REL_CURVE: usize = 70;
pub const FILTER_MODEL: usize = 71;
pub const MOD3_SOURCE: usize = 72;
pub const MOD3_TARGET: usize = 73;
pub const MOD3_AMOUNT: usize = 74;
pub const MOD4_SOURCE: usize = 75;
pub const MOD4_TARGET: usize = 76;
pub const MOD4_AMOUNT: usize = 77;
pub const MOD5_SOURCE: usize = 78;
pub const MOD5_TARGET: usize = 79;
pub const MOD5_AMOUNT: usize = 80;
pub const MOD6_SOURCE: usize = 81;
pub const MOD6_TARGET: usize = 82;
pub const MOD6_AMOUNT: usize = 83;
pub const MOD7_SOURCE: usize = 84;
pub const MOD7_TARGET: usize = 85;
pub const MOD7_AMOUNT: usize = 86;
pub const MOD8_SOURCE: usize = 87;
pub const MOD8_TARGET: usize = 88;
pub const MOD8_AMOUNT: usize = 89;
pub const MACRO1: usize = 90;
pub const MACRO2: usize = 91;
pub const MACRO3: usize = 92;
pub const MACRO4: usize = 93;
pub const LFO_DRAW: usize = 94;
pub const LFO1_MODE: usize = 95;
pub const LFO1_STYLE: usize = 96;
pub const LFO2_RATE: usize = 97;
pub const LFO2_SYNC: usize = 98;
pub const LFO2_MODE: usize = 99;
pub const LFO2_STYLE: usize = 100;
pub const LFO3_RATE: usize = 101;
pub const LFO3_SYNC: usize = 102;
pub const LFO3_MODE: usize = 103;
pub const LFO3_STYLE: usize = 104;
pub const LFO4_RATE: usize = 105;
pub const LFO4_SYNC: usize = 106;
pub const LFO4_MODE: usize = 107;
pub const LFO4_STYLE: usize = 108;
pub const DIST_A: usize = 109;
pub const DIST_B: usize = 110;
pub const OTT_A: usize = 111;
pub const OTT_B: usize = 112;
pub const CHORUS_A: usize = 113;
pub const CHORUS_B: usize = 114;
pub const DELAY_A: usize = 115;
pub const DELAY_B: usize = 116;
pub const REVERB_A: usize = 117;
pub const REVERB_B: usize = 118;
pub const DIST_DRIVE: usize = 119;
pub const DIST_TONE: usize = 120;
pub const DIST_MIX: usize = 121;
pub const OTT_DEPTH: usize = 122;
pub const OTT_TIME: usize = 123;
pub const OTT_MIX: usize = 124;
pub const REVERB_SIZE: usize = 125;
pub const REVERB_DAMP: usize = 126;
pub const REVERB_MIX: usize = 127;
pub const RACK_MODE: usize = 128;
pub const RACK_START: usize = 129;
pub const RACK_SLOTS: usize = 16;
pub const RACK_STRIDE: usize = 12;
pub const COUNT: usize = RACK_START + RACK_SLOTS * RACK_STRIDE;
pub const FX_ROUTES: [[usize; 2]; 5] = [[DIST_A,DIST_B],[OTT_A,OTT_B],[CHORUS_A,CHORUS_B],[DELAY_A,DELAY_B],[REVERB_A,REVERB_B]];
pub const MOD_SOURCES: [&str; 5] = ["Off", "LFO 1", "Envelope 2", "Velocity", "Key track"];
pub const MOD_TARGETS: [&str; 8] = [
    "Pitch",
    "Filter cutoff",
    "A warp",
    "B warp",
    "A position",
    "B position",
    "Pan",
    "Amplitude",
];
pub const ROUTES: [usize; 8] = [
    MOD1_SOURCE,
    MOD2_SOURCE,
    MOD3_SOURCE,
    MOD4_SOURCE,
    MOD5_SOURCE,
    MOD6_SOURCE,
    MOD7_SOURCE,
    MOD8_SOURCE,
];
pub const ALL_MOD_SOURCES: [&str; 13] = [
    "Off",
    "LFO 1",
    "Envelope 2",
    "Velocity",
    "Key track",
    "LFO 2",
    "LFO 3",
    "LFO 4",
    "Macro 1",
    "Macro 2",
    "Macro 3",
    "Macro 4",
    "Envelope 1",
];
pub const ALL_MOD_TARGETS: [&str; 21] = [
    "Pitch",
    "Filter cutoff",
    "A warp",
    "B warp",
    "A position",
    "B position",
    "Pan",
    "Amplitude",
    "Resonance",
    "Drive",
    "A level",
    "B level",
    "Sub level",
    "Noise level",
    "Stereo width",
    "A tune",
    "B tune",
    "A detune",
    "B detune",
    "A blend",
    "B blend",
];
pub const EXTRA_TARGETS: [usize; 13] = [
    RESONANCE, DRIVE, A_LEVEL, B_LEVEL, SUB, NOISE, SPREAD, A_TUNE, B_TUNE, A_DETUNE, B_DETUNE,
    A_BLEND, B_BLEND,
];
pub const LFO_CONTROLS: [[usize; 4]; 4] = [
    [LFO_RATE, LFO_SYNC, LFO1_MODE, LFO1_STYLE],
    [LFO2_RATE, LFO2_SYNC, LFO2_MODE, LFO2_STYLE],
    [LFO3_RATE, LFO3_SYNC, LFO3_MODE, LFO3_STYLE],
    [LFO4_RATE, LFO4_SYNC, LFO4_MODE, LFO4_STYLE],
];
pub fn mod_target(k: usize) -> Option<usize> {
    match k {
        CUTOFF => Some(1),
        A_AMOUNT => Some(2),
        B_AMOUNT => Some(3),
        A_POSITION => Some(4),
        B_POSITION => Some(5),
        A_PAN | B_PAN => Some(6),
        GAIN => Some(7),
        _ => EXTRA_TARGETS.iter().position(|x| *x == k).map(|i| i + 8),
    }
}
pub const WARP_IDS: [u32; 11] = [0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15];
pub const WARP_NAMES: [&str; 11] = [
    "Off", "Bend +", "Bend -", "Bend +/-", "PWM", "Asym +", "Asym -", "Asym +/-", "Flip", "Mirror",
    "Quantize",
];
pub const SETTINGS: [Setting; COUNT] = [
    Setting {
        id: "a_bank",
        name: "A Bank",
        range: (0, 3),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "a_position",
        name: "A Position",
        range: (0, 1000),
        start: 650,
        magnet: None,
    },
    Setting {
        id: "a_level",
        name: "A Level",
        range: (0, 1000),
        start: 700,
        magnet: None,
    },
    Setting {
        id: "a_tune",
        name: "A Tune",
        range: (-24, 24),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "a_unison",
        name: "A Unison",
        range: (1, 7),
        start: 1,
        magnet: None,
    },
    Setting {
        id: "a_detune",
        name: "A Detune",
        range: (0, 100),
        start: 12,
        magnet: None,
    },
    Setting {
        id: "a_pan",
        name: "A Pan",
        range: (-100, 100),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "a_warp",
        name: "A Bend",
        range: (-90, 90),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "b_bank",
        name: "B Bank",
        range: (0, 3),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "b_position",
        name: "B Position",
        range: (0, 1000),
        start: 400,
        magnet: None,
    },
    Setting {
        id: "b_level",
        name: "B Level",
        range: (0, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "b_tune",
        name: "B Tune",
        range: (-24, 24),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "b_unison",
        name: "B Unison",
        range: (1, 7),
        start: 1,
        magnet: None,
    },
    Setting {
        id: "b_detune",
        name: "B Detune",
        range: (0, 100),
        start: 12,
        magnet: None,
    },
    Setting {
        id: "b_pan",
        name: "B Pan",
        range: (-100, 100),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "b_warp",
        name: "B Bend",
        range: (-90, 90),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "cutoff",
        name: "Cutoff",
        range: (0, 1000),
        start: 1000,
        magnet: None,
    },
    Setting {
        id: "resonance",
        name: "Resonance",
        range: (0, 1000),
        start: 70,
        magnet: None,
    },
    Setting {
        id: "filter",
        name: "Filter",
        range: (0, 3),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "drive",
        name: "Drive",
        range: (0, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "sub",
        name: "Sub",
        range: (0, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "noise",
        name: "Noise",
        range: (0, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "gain",
        name: "Output",
        range: (0, 1000),
        start: 650,
        magnet: None,
    },
    Setting {
        id: "spread",
        name: "Spread",
        range: (0, 1000),
        start: 800,
        magnet: None,
    },
    Setting {
        id: "attack",
        name: "Attack",
        range: (0, 1000),
        start: 80,
        magnet: None,
    },
    Setting {
        id: "decay",
        name: "Decay",
        range: (0, 1000),
        start: 460,
        magnet: None,
    },
    Setting {
        id: "sustain",
        name: "Sustain",
        range: (0, 1000),
        start: 800,
        magnet: None,
    },
    Setting {
        id: "release",
        name: "Release",
        range: (0, 1000),
        start: 400,
        magnet: None,
    },
    Setting {
        id: "env_attack",
        name: "Mod attack",
        range: (0, 1000),
        start: 80,
        magnet: None,
    },
    Setting {
        id: "env_decay",
        name: "Mod decay",
        range: (0, 1000),
        start: 450,
        magnet: None,
    },
    Setting {
        id: "env_sustain",
        name: "Mod sustain",
        range: (0, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "env_release",
        name: "Mod release",
        range: (0, 1000),
        start: 350,
        magnet: None,
    },
    Setting {
        id: "env_amount",
        name: "Env > cutoff",
        range: (-1000, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "lfo_rate",
        name: "LFO rate",
        range: (0, 1000),
        start: 450,
        magnet: None,
    },
    Setting {
        id: "lfo_amount",
        name: "LFO amount",
        range: (0, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "lfo_shape",
        name: "LFO shape",
        range: (0, 3),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "lfo_target",
        name: "LFO target",
        range: (0, 3),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "lfo_sync",
        name: "LFO sync",
        range: (0, 8),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "chorus_rate",
        name: "Chorus rate",
        range: (0, 1000),
        start: 350,
        magnet: None,
    },
    Setting {
        id: "chorus_depth",
        name: "Chorus depth",
        range: (0, 1000),
        start: 400,
        magnet: None,
    },
    Setting {
        id: "chorus_mix",
        name: "Chorus mix",
        range: (0, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "delay_time",
        name: "Delay time",
        range: (0, 1000),
        start: 400,
        magnet: None,
    },
    Setting {
        id: "delay_feedback",
        name: "Feedback",
        range: (0, 1000),
        start: 350,
        magnet: None,
    },
    Setting {
        id: "delay_mix",
        name: "Delay mix",
        range: (0, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "a_source",
        name: "A Source",
        range: (0, 1),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "a_mode",
        name: "A Warp mode",
        range: (0, 10),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "a_amount",
        name: "A Warp amount",
        range: (0, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "b_source",
        name: "B Source",
        range: (0, 1),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "b_mode",
        name: "B Warp mode",
        range: (0, 10),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "b_amount",
        name: "B Warp amount",
        range: (0, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "a_phase",
        name: "A Phase",
        range: (0, 360),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "a_random",
        name: "A Random",
        range: (0, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "a_blend",
        name: "A Blend",
        range: (0, 1000),
        start: 750,
        magnet: None,
    },
    Setting {
        id: "b_phase",
        name: "B Phase",
        range: (0, 360),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "b_random",
        name: "B Random",
        range: (0, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "b_blend",
        name: "B Blend",
        range: (0, 1000),
        start: 750,
        magnet: None,
    },
    Setting {
        id: "mod1_source",
        name: "Mod 1 Source",
        range: (0, 4),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod1_target",
        name: "Mod 1 Target",
        range: (0, 7),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod1_amount",
        name: "Mod 1 Amount",
        range: (-1000, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod2_source",
        name: "Mod 2 Source",
        range: (0, 4),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod2_target",
        name: "Mod 2 Target",
        range: (0, 7),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod2_amount",
        name: "Mod 2 Amount",
        range: (-1000, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "env_engine",
        name: "Envelope Engine",
        range: (0, 1),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "amp_hold",
        name: "Amp Hold",
        range: (0, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "amp_atk_curve",
        name: "Amp Attack Curve",
        range: (0, 1000),
        start: 500,
        magnet: None,
    },
    Setting {
        id: "amp_dec_curve",
        name: "Amp Decay Curve",
        range: (0, 1000),
        start: 500,
        magnet: None,
    },
    Setting {
        id: "amp_rel_curve",
        name: "Amp Release Curve",
        range: (0, 1000),
        start: 500,
        magnet: None,
    },
    Setting {
        id: "env_hold",
        name: "Env 2 Hold",
        range: (0, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "env_atk_curve",
        name: "Env 2 Attack Curve",
        range: (0, 1000),
        start: 500,
        magnet: None,
    },
    Setting {
        id: "env_dec_curve",
        name: "Env 2 Decay Curve",
        range: (0, 1000),
        start: 500,
        magnet: None,
    },
    Setting {
        id: "env_rel_curve",
        name: "Env 2 Release Curve",
        range: (0, 1000),
        start: 500,
        magnet: None,
    },
    Setting {
        id: "filter_model",
        name: "Filter Model",
        range: (0, 2),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod3_source",
        name: "Mod 3 Source",
        range: (0, 12),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod3_target",
        name: "Mod 3 Target",
        range: (0, 20),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod3_amount",
        name: "Mod 3 Amount",
        range: (-1000, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod4_source",
        name: "Mod 4 Source",
        range: (0, 12),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod4_target",
        name: "Mod 4 Target",
        range: (0, 20),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod4_amount",
        name: "Mod 4 Amount",
        range: (-1000, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod5_source",
        name: "Mod 5 Source",
        range: (0, 12),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod5_target",
        name: "Mod 5 Target",
        range: (0, 20),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod5_amount",
        name: "Mod 5 Amount",
        range: (-1000, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod6_source",
        name: "Mod 6 Source",
        range: (0, 12),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod6_target",
        name: "Mod 6 Target",
        range: (0, 20),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod6_amount",
        name: "Mod 6 Amount",
        range: (-1000, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod7_source",
        name: "Mod 7 Source",
        range: (0, 12),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod7_target",
        name: "Mod 7 Target",
        range: (0, 20),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod7_amount",
        name: "Mod 7 Amount",
        range: (-1000, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod8_source",
        name: "Mod 8 Source",
        range: (0, 12),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod8_target",
        name: "Mod 8 Target",
        range: (0, 20),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "mod8_amount",
        name: "Mod 8 Amount",
        range: (-1000, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "macro1",
        name: "Macro 1",
        range: (0, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "macro2",
        name: "Macro 2",
        range: (0, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "macro3",
        name: "Macro 3",
        range: (0, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "macro4",
        name: "Macro 4",
        range: (0, 1000),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "lfo_draw",
        name: "LFO 1 Shape Engine",
        range: (0, 1),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "lfo1_mode",
        name: "LFO 1 Trigger",
        range: (0, 2),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "lfo1_style",
        name: "LFO 1 Division",
        range: (0, 2),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "lfo2_rate",
        name: "LFO 2 Rate",
        range: (0, 1000),
        start: 350,
        magnet: None,
    },
    Setting {
        id: "lfo2_sync",
        name: "LFO 2 Sync",
        range: (0, 8),
        start: 5,
        magnet: None,
    },
    Setting {
        id: "lfo2_mode",
        name: "LFO 2 Trigger",
        range: (0, 2),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "lfo2_style",
        name: "LFO 2 Division",
        range: (0, 2),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "lfo3_rate",
        name: "LFO 3 Rate",
        range: (0, 1000),
        start: 350,
        magnet: None,
    },
    Setting {
        id: "lfo3_sync",
        name: "LFO 3 Sync",
        range: (0, 8),
        start: 5,
        magnet: None,
    },
    Setting {
        id: "lfo3_mode",
        name: "LFO 3 Trigger",
        range: (0, 2),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "lfo3_style",
        name: "LFO 3 Division",
        range: (0, 2),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "lfo4_rate",
        name: "LFO 4 Rate",
        range: (0, 1000),
        start: 350,
        magnet: None,
    },
    Setting {
        id: "lfo4_sync",
        name: "LFO 4 Sync",
        range: (0, 8),
        start: 5,
        magnet: None,
    },
    Setting {
        id: "lfo4_mode",
        name: "LFO 4 Trigger",
        range: (0, 2),
        start: 0,
        magnet: None,
    },
    Setting {
        id: "lfo4_style",
        name: "LFO 4 Division",
        range: (0, 2),
        start: 0,
        magnet: None,
    },
    Setting { id: "dist_a", name: "Dist oscillator A", range: (0, 1), start: 1, magnet: None },
    Setting { id: "dist_b", name: "Dist oscillator B", range: (0, 1), start: 1, magnet: None },
    Setting { id: "ott_a", name: "Ott oscillator A", range: (0, 1), start: 1, magnet: None },
    Setting { id: "ott_b", name: "Ott oscillator B", range: (0, 1), start: 1, magnet: None },
    Setting { id: "chorus_a", name: "Chorus oscillator A", range: (0, 1), start: 1, magnet: None },
    Setting { id: "chorus_b", name: "Chorus oscillator B", range: (0, 1), start: 1, magnet: None },
    Setting { id: "delay_a", name: "Delay oscillator A", range: (0, 1), start: 1, magnet: None },
    Setting { id: "delay_b", name: "Delay oscillator B", range: (0, 1), start: 1, magnet: None },
    Setting { id: "reverb_a", name: "Reverb oscillator A", range: (0, 1), start: 1, magnet: None },
    Setting { id: "reverb_b", name: "Reverb oscillator B", range: (0, 1), start: 1, magnet: None },
    Setting { id: "dist_drive", name: "Distortion drive", range: (0, 1000), start: 350, magnet: None },
    Setting { id: "dist_tone", name: "Distortion tone", range: (0, 1000), start: 600, magnet: None },
    Setting { id: "dist_mix", name: "Distortion mix", range: (0, 1000), start: 0, magnet: None },
    Setting { id: "ott_depth", name: "OTT depth", range: (0, 1000), start: 500, magnet: None },
    Setting { id: "ott_time", name: "OTT time", range: (0, 1000), start: 350, magnet: None },
    Setting { id: "ott_mix", name: "OTT mix", range: (0, 1000), start: 0, magnet: None },
    Setting { id: "reverb_size", name: "Reverb size", range: (0, 1000), start: 500, magnet: None },
    Setting { id: "reverb_damp", name: "Reverb damping", range: (0, 1000), start: 500, magnet: None },
    Setting { id: "reverb_mix", name: "Reverb mix", range: (0, 1000), start: 0, magnet: None },
    Setting {id:"fx_rack",name:"Modular FX rack",range:(0,1),start:0,magnet:None},
    Setting {id:"fx01_0",name:"FX 1 Type",range:(0,13),start:0,magnet:None},
    Setting {id:"fx01_1",name:"FX 1 Order",range:(0,15),start:0,magnet:None},
    Setting {id:"fx01_2",name:"FX 1 Enabled",range:(0,1),start:1,magnet:None},
    Setting {id:"fx01_3",name:"FX 1 Oscillator A",range:(0,1),start:1,magnet:None},
    Setting {id:"fx01_4",name:"FX 1 Oscillator B",range:(0,1),start:1,magnet:None},
    Setting {id:"fx01_5",name:"FX 1 Mode",range:(0,7),start:0,magnet:None},
    Setting {id:"fx01_6",name:"FX 1 Control 1",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx01_7",name:"FX 1 Control 2",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx01_8",name:"FX 1 Control 3",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx01_9",name:"FX 1 Control 4",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx01_10",name:"FX 1 Control 5",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx01_11",name:"FX 1 Mix",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx02_0",name:"FX 2 Type",range:(0,13),start:0,magnet:None},
    Setting {id:"fx02_1",name:"FX 2 Order",range:(0,15),start:1,magnet:None},
    Setting {id:"fx02_2",name:"FX 2 Enabled",range:(0,1),start:1,magnet:None},
    Setting {id:"fx02_3",name:"FX 2 Oscillator A",range:(0,1),start:1,magnet:None},
    Setting {id:"fx02_4",name:"FX 2 Oscillator B",range:(0,1),start:1,magnet:None},
    Setting {id:"fx02_5",name:"FX 2 Mode",range:(0,7),start:0,magnet:None},
    Setting {id:"fx02_6",name:"FX 2 Control 1",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx02_7",name:"FX 2 Control 2",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx02_8",name:"FX 2 Control 3",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx02_9",name:"FX 2 Control 4",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx02_10",name:"FX 2 Control 5",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx02_11",name:"FX 2 Mix",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx03_0",name:"FX 3 Type",range:(0,13),start:0,magnet:None},
    Setting {id:"fx03_1",name:"FX 3 Order",range:(0,15),start:2,magnet:None},
    Setting {id:"fx03_2",name:"FX 3 Enabled",range:(0,1),start:1,magnet:None},
    Setting {id:"fx03_3",name:"FX 3 Oscillator A",range:(0,1),start:1,magnet:None},
    Setting {id:"fx03_4",name:"FX 3 Oscillator B",range:(0,1),start:1,magnet:None},
    Setting {id:"fx03_5",name:"FX 3 Mode",range:(0,7),start:0,magnet:None},
    Setting {id:"fx03_6",name:"FX 3 Control 1",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx03_7",name:"FX 3 Control 2",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx03_8",name:"FX 3 Control 3",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx03_9",name:"FX 3 Control 4",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx03_10",name:"FX 3 Control 5",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx03_11",name:"FX 3 Mix",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx04_0",name:"FX 4 Type",range:(0,13),start:0,magnet:None},
    Setting {id:"fx04_1",name:"FX 4 Order",range:(0,15),start:3,magnet:None},
    Setting {id:"fx04_2",name:"FX 4 Enabled",range:(0,1),start:1,magnet:None},
    Setting {id:"fx04_3",name:"FX 4 Oscillator A",range:(0,1),start:1,magnet:None},
    Setting {id:"fx04_4",name:"FX 4 Oscillator B",range:(0,1),start:1,magnet:None},
    Setting {id:"fx04_5",name:"FX 4 Mode",range:(0,7),start:0,magnet:None},
    Setting {id:"fx04_6",name:"FX 4 Control 1",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx04_7",name:"FX 4 Control 2",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx04_8",name:"FX 4 Control 3",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx04_9",name:"FX 4 Control 4",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx04_10",name:"FX 4 Control 5",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx04_11",name:"FX 4 Mix",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx05_0",name:"FX 5 Type",range:(0,13),start:0,magnet:None},
    Setting {id:"fx05_1",name:"FX 5 Order",range:(0,15),start:4,magnet:None},
    Setting {id:"fx05_2",name:"FX 5 Enabled",range:(0,1),start:1,magnet:None},
    Setting {id:"fx05_3",name:"FX 5 Oscillator A",range:(0,1),start:1,magnet:None},
    Setting {id:"fx05_4",name:"FX 5 Oscillator B",range:(0,1),start:1,magnet:None},
    Setting {id:"fx05_5",name:"FX 5 Mode",range:(0,7),start:0,magnet:None},
    Setting {id:"fx05_6",name:"FX 5 Control 1",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx05_7",name:"FX 5 Control 2",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx05_8",name:"FX 5 Control 3",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx05_9",name:"FX 5 Control 4",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx05_10",name:"FX 5 Control 5",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx05_11",name:"FX 5 Mix",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx06_0",name:"FX 6 Type",range:(0,13),start:0,magnet:None},
    Setting {id:"fx06_1",name:"FX 6 Order",range:(0,15),start:5,magnet:None},
    Setting {id:"fx06_2",name:"FX 6 Enabled",range:(0,1),start:1,magnet:None},
    Setting {id:"fx06_3",name:"FX 6 Oscillator A",range:(0,1),start:1,magnet:None},
    Setting {id:"fx06_4",name:"FX 6 Oscillator B",range:(0,1),start:1,magnet:None},
    Setting {id:"fx06_5",name:"FX 6 Mode",range:(0,7),start:0,magnet:None},
    Setting {id:"fx06_6",name:"FX 6 Control 1",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx06_7",name:"FX 6 Control 2",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx06_8",name:"FX 6 Control 3",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx06_9",name:"FX 6 Control 4",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx06_10",name:"FX 6 Control 5",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx06_11",name:"FX 6 Mix",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx07_0",name:"FX 7 Type",range:(0,13),start:0,magnet:None},
    Setting {id:"fx07_1",name:"FX 7 Order",range:(0,15),start:6,magnet:None},
    Setting {id:"fx07_2",name:"FX 7 Enabled",range:(0,1),start:1,magnet:None},
    Setting {id:"fx07_3",name:"FX 7 Oscillator A",range:(0,1),start:1,magnet:None},
    Setting {id:"fx07_4",name:"FX 7 Oscillator B",range:(0,1),start:1,magnet:None},
    Setting {id:"fx07_5",name:"FX 7 Mode",range:(0,7),start:0,magnet:None},
    Setting {id:"fx07_6",name:"FX 7 Control 1",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx07_7",name:"FX 7 Control 2",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx07_8",name:"FX 7 Control 3",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx07_9",name:"FX 7 Control 4",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx07_10",name:"FX 7 Control 5",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx07_11",name:"FX 7 Mix",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx08_0",name:"FX 8 Type",range:(0,13),start:0,magnet:None},
    Setting {id:"fx08_1",name:"FX 8 Order",range:(0,15),start:7,magnet:None},
    Setting {id:"fx08_2",name:"FX 8 Enabled",range:(0,1),start:1,magnet:None},
    Setting {id:"fx08_3",name:"FX 8 Oscillator A",range:(0,1),start:1,magnet:None},
    Setting {id:"fx08_4",name:"FX 8 Oscillator B",range:(0,1),start:1,magnet:None},
    Setting {id:"fx08_5",name:"FX 8 Mode",range:(0,7),start:0,magnet:None},
    Setting {id:"fx08_6",name:"FX 8 Control 1",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx08_7",name:"FX 8 Control 2",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx08_8",name:"FX 8 Control 3",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx08_9",name:"FX 8 Control 4",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx08_10",name:"FX 8 Control 5",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx08_11",name:"FX 8 Mix",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx09_0",name:"FX 9 Type",range:(0,13),start:0,magnet:None},
    Setting {id:"fx09_1",name:"FX 9 Order",range:(0,15),start:8,magnet:None},
    Setting {id:"fx09_2",name:"FX 9 Enabled",range:(0,1),start:1,magnet:None},
    Setting {id:"fx09_3",name:"FX 9 Oscillator A",range:(0,1),start:1,magnet:None},
    Setting {id:"fx09_4",name:"FX 9 Oscillator B",range:(0,1),start:1,magnet:None},
    Setting {id:"fx09_5",name:"FX 9 Mode",range:(0,7),start:0,magnet:None},
    Setting {id:"fx09_6",name:"FX 9 Control 1",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx09_7",name:"FX 9 Control 2",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx09_8",name:"FX 9 Control 3",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx09_9",name:"FX 9 Control 4",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx09_10",name:"FX 9 Control 5",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx09_11",name:"FX 9 Mix",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx10_0",name:"FX 10 Type",range:(0,13),start:0,magnet:None},
    Setting {id:"fx10_1",name:"FX 10 Order",range:(0,15),start:9,magnet:None},
    Setting {id:"fx10_2",name:"FX 10 Enabled",range:(0,1),start:1,magnet:None},
    Setting {id:"fx10_3",name:"FX 10 Oscillator A",range:(0,1),start:1,magnet:None},
    Setting {id:"fx10_4",name:"FX 10 Oscillator B",range:(0,1),start:1,magnet:None},
    Setting {id:"fx10_5",name:"FX 10 Mode",range:(0,7),start:0,magnet:None},
    Setting {id:"fx10_6",name:"FX 10 Control 1",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx10_7",name:"FX 10 Control 2",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx10_8",name:"FX 10 Control 3",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx10_9",name:"FX 10 Control 4",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx10_10",name:"FX 10 Control 5",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx10_11",name:"FX 10 Mix",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx11_0",name:"FX 11 Type",range:(0,13),start:0,magnet:None},
    Setting {id:"fx11_1",name:"FX 11 Order",range:(0,15),start:10,magnet:None},
    Setting {id:"fx11_2",name:"FX 11 Enabled",range:(0,1),start:1,magnet:None},
    Setting {id:"fx11_3",name:"FX 11 Oscillator A",range:(0,1),start:1,magnet:None},
    Setting {id:"fx11_4",name:"FX 11 Oscillator B",range:(0,1),start:1,magnet:None},
    Setting {id:"fx11_5",name:"FX 11 Mode",range:(0,7),start:0,magnet:None},
    Setting {id:"fx11_6",name:"FX 11 Control 1",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx11_7",name:"FX 11 Control 2",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx11_8",name:"FX 11 Control 3",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx11_9",name:"FX 11 Control 4",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx11_10",name:"FX 11 Control 5",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx11_11",name:"FX 11 Mix",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx12_0",name:"FX 12 Type",range:(0,13),start:0,magnet:None},
    Setting {id:"fx12_1",name:"FX 12 Order",range:(0,15),start:11,magnet:None},
    Setting {id:"fx12_2",name:"FX 12 Enabled",range:(0,1),start:1,magnet:None},
    Setting {id:"fx12_3",name:"FX 12 Oscillator A",range:(0,1),start:1,magnet:None},
    Setting {id:"fx12_4",name:"FX 12 Oscillator B",range:(0,1),start:1,magnet:None},
    Setting {id:"fx12_5",name:"FX 12 Mode",range:(0,7),start:0,magnet:None},
    Setting {id:"fx12_6",name:"FX 12 Control 1",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx12_7",name:"FX 12 Control 2",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx12_8",name:"FX 12 Control 3",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx12_9",name:"FX 12 Control 4",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx12_10",name:"FX 12 Control 5",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx12_11",name:"FX 12 Mix",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx13_0",name:"FX 13 Type",range:(0,13),start:0,magnet:None},
    Setting {id:"fx13_1",name:"FX 13 Order",range:(0,15),start:12,magnet:None},
    Setting {id:"fx13_2",name:"FX 13 Enabled",range:(0,1),start:1,magnet:None},
    Setting {id:"fx13_3",name:"FX 13 Oscillator A",range:(0,1),start:1,magnet:None},
    Setting {id:"fx13_4",name:"FX 13 Oscillator B",range:(0,1),start:1,magnet:None},
    Setting {id:"fx13_5",name:"FX 13 Mode",range:(0,7),start:0,magnet:None},
    Setting {id:"fx13_6",name:"FX 13 Control 1",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx13_7",name:"FX 13 Control 2",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx13_8",name:"FX 13 Control 3",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx13_9",name:"FX 13 Control 4",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx13_10",name:"FX 13 Control 5",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx13_11",name:"FX 13 Mix",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx14_0",name:"FX 14 Type",range:(0,13),start:0,magnet:None},
    Setting {id:"fx14_1",name:"FX 14 Order",range:(0,15),start:13,magnet:None},
    Setting {id:"fx14_2",name:"FX 14 Enabled",range:(0,1),start:1,magnet:None},
    Setting {id:"fx14_3",name:"FX 14 Oscillator A",range:(0,1),start:1,magnet:None},
    Setting {id:"fx14_4",name:"FX 14 Oscillator B",range:(0,1),start:1,magnet:None},
    Setting {id:"fx14_5",name:"FX 14 Mode",range:(0,7),start:0,magnet:None},
    Setting {id:"fx14_6",name:"FX 14 Control 1",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx14_7",name:"FX 14 Control 2",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx14_8",name:"FX 14 Control 3",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx14_9",name:"FX 14 Control 4",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx14_10",name:"FX 14 Control 5",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx14_11",name:"FX 14 Mix",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx15_0",name:"FX 15 Type",range:(0,13),start:0,magnet:None},
    Setting {id:"fx15_1",name:"FX 15 Order",range:(0,15),start:14,magnet:None},
    Setting {id:"fx15_2",name:"FX 15 Enabled",range:(0,1),start:1,magnet:None},
    Setting {id:"fx15_3",name:"FX 15 Oscillator A",range:(0,1),start:1,magnet:None},
    Setting {id:"fx15_4",name:"FX 15 Oscillator B",range:(0,1),start:1,magnet:None},
    Setting {id:"fx15_5",name:"FX 15 Mode",range:(0,7),start:0,magnet:None},
    Setting {id:"fx15_6",name:"FX 15 Control 1",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx15_7",name:"FX 15 Control 2",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx15_8",name:"FX 15 Control 3",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx15_9",name:"FX 15 Control 4",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx15_10",name:"FX 15 Control 5",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx15_11",name:"FX 15 Mix",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx16_0",name:"FX 16 Type",range:(0,13),start:0,magnet:None},
    Setting {id:"fx16_1",name:"FX 16 Order",range:(0,15),start:15,magnet:None},
    Setting {id:"fx16_2",name:"FX 16 Enabled",range:(0,1),start:1,magnet:None},
    Setting {id:"fx16_3",name:"FX 16 Oscillator A",range:(0,1),start:1,magnet:None},
    Setting {id:"fx16_4",name:"FX 16 Oscillator B",range:(0,1),start:1,magnet:None},
    Setting {id:"fx16_5",name:"FX 16 Mode",range:(0,7),start:0,magnet:None},
    Setting {id:"fx16_6",name:"FX 16 Control 1",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx16_7",name:"FX 16 Control 2",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx16_8",name:"FX 16 Control 3",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx16_9",name:"FX 16 Control 4",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx16_10",name:"FX 16 Control 5",range:(0,1000),start:500,magnet:None},
    Setting {id:"fx16_11",name:"FX 16 Mix",range:(0,1000),start:500,magnet:None},
];
pub const CONTROLS: [Control; 50] = [
    Control::Knob {
        id: "a_bank",
        cx: 59.0,
        cy: 164.0,
        r: 21.0,
        caption: "Bank",
        bipolar: false,
    },
    Control::Knob {
        id: "a_position",
        cx: 146.0,
        cy: 164.0,
        r: 21.0,
        caption: "Position",
        bipolar: false,
    },
    Control::Knob {
        id: "a_level",
        cx: 233.0,
        cy: 164.0,
        r: 21.0,
        caption: "Level",
        bipolar: false,
    },
    Control::Knob {
        id: "a_tune",
        cx: 320.0,
        cy: 164.0,
        r: 21.0,
        caption: "Tune",
        bipolar: true,
    },
    Control::Knob {
        id: "a_unison",
        cx: 59.0,
        cy: 255.0,
        r: 21.0,
        caption: "Unison",
        bipolar: false,
    },
    Control::Knob {
        id: "a_detune",
        cx: 146.0,
        cy: 255.0,
        r: 21.0,
        caption: "Detune",
        bipolar: false,
    },
    Control::Knob {
        id: "a_pan",
        cx: 233.0,
        cy: 255.0,
        r: 21.0,
        caption: "Pan",
        bipolar: true,
    },
    Control::Knob {
        id: "a_warp",
        cx: 320.0,
        cy: 255.0,
        r: 21.0,
        caption: "Bend",
        bipolar: true,
    },
    Control::Knob {
        id: "b_bank",
        cx: 421.0,
        cy: 164.0,
        r: 21.0,
        caption: "Bank",
        bipolar: false,
    },
    Control::Knob {
        id: "b_position",
        cx: 508.0,
        cy: 164.0,
        r: 21.0,
        caption: "Position",
        bipolar: false,
    },
    Control::Knob {
        id: "b_level",
        cx: 595.0,
        cy: 164.0,
        r: 21.0,
        caption: "Level",
        bipolar: false,
    },
    Control::Knob {
        id: "b_tune",
        cx: 682.0,
        cy: 164.0,
        r: 21.0,
        caption: "Tune",
        bipolar: true,
    },
    Control::Knob {
        id: "b_unison",
        cx: 421.0,
        cy: 255.0,
        r: 21.0,
        caption: "Unison",
        bipolar: false,
    },
    Control::Knob {
        id: "b_detune",
        cx: 508.0,
        cy: 255.0,
        r: 21.0,
        caption: "Detune",
        bipolar: false,
    },
    Control::Knob {
        id: "b_pan",
        cx: 595.0,
        cy: 255.0,
        r: 21.0,
        caption: "Pan",
        bipolar: true,
    },
    Control::Knob {
        id: "b_warp",
        cx: 682.0,
        cy: 255.0,
        r: 21.0,
        caption: "Bend",
        bipolar: true,
    },
    Control::Knob {
        id: "cutoff",
        cx: 783.0,
        cy: 164.0,
        r: 21.0,
        caption: "Cutoff",
        bipolar: false,
    },
    Control::Knob {
        id: "resonance",
        cx: 870.0,
        cy: 164.0,
        r: 21.0,
        caption: "Resonance",
        bipolar: false,
    },
    Control::Knob {
        id: "filter",
        cx: 957.0,
        cy: 164.0,
        r: 21.0,
        caption: "Filter",
        bipolar: false,
    },
    Control::Knob {
        id: "drive",
        cx: 1044.0,
        cy: 164.0,
        r: 21.0,
        caption: "Drive",
        bipolar: false,
    },
    Control::Knob {
        id: "sub",
        cx: 783.0,
        cy: 255.0,
        r: 21.0,
        caption: "Sub",
        bipolar: false,
    },
    Control::Knob {
        id: "noise",
        cx: 870.0,
        cy: 255.0,
        r: 21.0,
        caption: "Noise",
        bipolar: false,
    },
    Control::Knob {
        id: "gain",
        cx: 957.0,
        cy: 255.0,
        r: 21.0,
        caption: "Output",
        bipolar: false,
    },
    Control::Knob {
        id: "spread",
        cx: 1044.0,
        cy: 255.0,
        r: 21.0,
        caption: "Spread",
        bipolar: false,
    },
    Control::Knob {
        id: "attack",
        cx: 59.0,
        cy: 414.0,
        r: 21.0,
        caption: "Attack",
        bipolar: false,
    },
    Control::Knob {
        id: "decay",
        cx: 146.0,
        cy: 414.0,
        r: 21.0,
        caption: "Decay",
        bipolar: false,
    },
    Control::Knob {
        id: "sustain",
        cx: 233.0,
        cy: 414.0,
        r: 21.0,
        caption: "Sustain",
        bipolar: false,
    },
    Control::Knob {
        id: "release",
        cx: 320.0,
        cy: 414.0,
        r: 21.0,
        caption: "Release",
        bipolar: false,
    },
    Control::Knob {
        id: "env_attack",
        cx: 405.0,
        cy: 414.0,
        r: 21.0,
        caption: "attack",
        bipolar: false,
    },
    Control::Knob {
        id: "env_decay",
        cx: 470.0,
        cy: 414.0,
        r: 21.0,
        caption: "decay",
        bipolar: false,
    },
    Control::Knob {
        id: "env_sustain",
        cx: 535.0,
        cy: 414.0,
        r: 21.0,
        caption: "sustain",
        bipolar: false,
    },
    Control::Knob {
        id: "env_release",
        cx: 600.0,
        cy: 414.0,
        r: 21.0,
        caption: "release",
        bipolar: false,
    },
    Control::Knob {
        id: "env_amount",
        cx: 665.0,
        cy: 414.0,
        r: 21.0,
        caption: "Env > cutoff",
        bipolar: true,
    },
    Control::Knob {
        id: "lfo_rate",
        cx: 770.0,
        cy: 414.0,
        r: 21.0,
        caption: "rate",
        bipolar: false,
    },
    Control::Knob {
        id: "lfo_amount",
        cx: 835.0,
        cy: 414.0,
        r: 21.0,
        caption: "amount",
        bipolar: false,
    },
    Control::Knob {
        id: "lfo_shape",
        cx: 900.0,
        cy: 414.0,
        r: 21.0,
        caption: "shape",
        bipolar: false,
    },
    Control::Knob {
        id: "lfo_target",
        cx: 965.0,
        cy: 414.0,
        r: 21.0,
        caption: "target",
        bipolar: false,
    },
    Control::Knob {
        id: "lfo_sync",
        cx: 1030.0,
        cy: 414.0,
        r: 21.0,
        caption: "sync",
        bipolar: false,
    },
    Control::Knob {
        id: "chorus_rate",
        cx: 85.0,
        cy: 590.0,
        r: 21.0,
        caption: "Chorus rate",
        bipolar: false,
    },
    Control::Knob {
        id: "chorus_depth",
        cx: 265.0,
        cy: 590.0,
        r: 21.0,
        caption: "Chorus depth",
        bipolar: false,
    },
    Control::Knob {
        id: "chorus_mix",
        cx: 445.0,
        cy: 590.0,
        r: 21.0,
        caption: "Chorus mix",
        bipolar: false,
    },
    Control::Knob {
        id: "delay_time",
        cx: 625.0,
        cy: 590.0,
        r: 21.0,
        caption: "Delay time",
        bipolar: false,
    },
    Control::Knob {
        id: "delay_feedback",
        cx: 805.0,
        cy: 590.0,
        r: 21.0,
        caption: "Feedback",
        bipolar: false,
    },
    Control::Knob {
        id: "delay_mix",
        cx: 985.0,
        cy: 590.0,
        r: 21.0,
        caption: "Delay mix",
        bipolar: false,
    },
    Control::Choice {
        id: "a_source",
        x: 40.,
        top: 730.,
        names: &["Wavetable / Sample", "Reference Saw"],
    },
    Control::Knob {
        id: "a_mode",
        cx: 220.,
        cy: 744.,
        r: 21.,
        caption: "Warp mode",
        bipolar: false,
    },
    Control::Knob {
        id: "a_amount",
        cx: 314.,
        cy: 744.,
        r: 21.,
        caption: "Amount",
        bipolar: false,
    },
    Control::Choice {
        id: "b_source",
        x: 402.,
        top: 730.,
        names: &["Wavetable / Sample", "Reference Saw"],
    },
    Control::Knob {
        id: "b_mode",
        cx: 582.,
        cy: 744.,
        r: 21.,
        caption: "Warp mode",
        bipolar: false,
    },
    Control::Knob {
        id: "b_amount",
        cx: 676.,
        cy: 744.,
        r: 21.,
        caption: "Amount",
        bipolar: false,
    },
];
const CARDS: [Card; 9] = [
    Card {
        x0: 16.,
        x1: 362.,
        y0: 36.,
        y1: 310.,
        heading: "OSCILLATOR A",
    },
    Card {
        x0: 378.,
        x1: 724.,
        y0: 36.,
        y1: 310.,
        heading: "OSCILLATOR B",
    },
    Card {
        x0: 740.,
        x1: 1084.,
        y0: 36.,
        y1: 310.,
        heading: "FILTER / MIXER",
    },
    Card {
        x0: 16.,
        x1: 362.,
        y0: 350.,
        y1: 480.,
        heading: "AMPLITUDE ENVELOPE",
    },
    Card {
        x0: 378.,
        x1: 724.,
        y0: 350.,
        y1: 480.,
        heading: "FILTER ENVELOPE",
    },
    Card {
        x0: 740.,
        x1: 1084.,
        y0: 350.,
        y1: 480.,
        heading: "LFO / MODULATION",
    },
    Card {
        x0: 16.,
        x1: 1084.,
        y0: 530.,
        y1: 660.,
        heading: "STEREO EFFECTS",
    },
    Card {
        x0: 16.,
        x1: 362.,
        y0: 690.,
        y1: 798.,
        heading: "A SOURCE / WARP",
    },
    Card {
        x0: 378.,
        x1: 724.,
        y0: 690.,
        y1: 798.,
        heading: "B SOURCE / WARP",
    },
];
pub const SPEC: &Spec = &Spec {
    name: "Lemon Wave",
    word: "WAVE",
    about: "Lemon Wave — wavetable synthesis / verified reference oscillator / embedded WAV tables",
    settings: &SETTINGS,
    version: Some(1),
    any_version: false,
    fields: &[],
    tail: &[],
    size: (1180, 820),
    cards: &CARDS,
    controls: &CONTROLS,
    display: None, // Per-instance waveforms are drawn by TableEditor.
    mark: Mark {
        x: 1084.,
        y: 828.,
        anchor: 1.,
        stacked: false,
    },
    hint: Some(Rect::new(16., 814., 780., 26.)),
    labels: &[(910., 76., "POLYPHONIC WAVETABLE SYNTH")],
    text,
    parse,
    sound: unused_sound,
    rings: true,
};
fn unused_sound(_: u32, _: &[i32]) -> Box<dyn Sound> {
    unreachable!("Wave owns its instrument engine")
}
pub fn seconds(v: f32) -> f32 {
    0.001 * 10000.0_f32.powf(v / 1000.)
}
pub fn cutoff(v: f32) -> f32 {
    20. * 1000.0_f32.powf(v / 1000.)
}
pub fn lfo_hz(v: f32) -> f32 {
    0.05 * 400.0_f32.powf(v / 1000.)
}
pub fn text(k: usize, v: i32) -> String {
    if ROUTES[2..].contains(&k) {
        return ALL_MOD_SOURCES[v.clamp(0, 12) as usize].into();
    }
    if ROUTES[2..].iter().any(|s| k == s + 1) {
        return ALL_MOD_TARGETS[v.clamp(0, 20) as usize].into();
    }
    if LFO_CONTROLS.iter().any(|s| k == s[2]) {
        return ["Note loop", "Free run", "One shot"][v.clamp(0, 2) as usize].into();
    }
    if LFO_CONTROLS.iter().any(|s| k == s[3]) {
        return ["Straight", "Dotted", "Triplet"][v.clamp(0, 2) as usize].into();
    }
    if LFO_CONTROLS[1..].iter().any(|s| k == s[1]) {
        return text(LFO_SYNC, v);
    }
    if LFO_CONTROLS[1..].iter().any(|s| k == s[0]) {
        return text(LFO_RATE, v);
    }
    if FX_ROUTES.iter().flatten().any(|i| *i == k) { return if v > 0 { "On" } else { "Off" }.into(); }
    match k {
        OTT_TIME => format!("{} ms", 20 + v / 2),
        LFO_DRAW => ["Classic shape", "Drawn shape"][v.clamp(0, 1) as usize].into(),
        ENV_ENGINE => ["Classic", "Curved AHDSR"][v.clamp(0, 1) as usize].into(),
        FILTER_MODEL => ["Classic", "SVF 12", "SVF 24"][v.clamp(0, 2) as usize].into(),
        AMP_HOLD | ENV_HOLD => format!("{:.0} ms", hold_seconds(v as f32) * 1000.),
        A_PHASE | B_PHASE => format!("{v} deg"),
        MOD1_SOURCE | MOD2_SOURCE => MOD_SOURCES[v.clamp(0, 4) as usize].into(),
        MOD1_TARGET | MOD2_TARGET => MOD_TARGETS[v.clamp(0, 7) as usize].into(),
        A_SOURCE | B_SOURCE => ["Wavetable / Sample", "Reference Saw"][v.clamp(0, 1) as usize].into(),
        A_MODE | B_MODE => WARP_NAMES[v.clamp(0, 10) as usize].into(),
        A_BANK | B_BANK => ["Basic", "Hollow", "Glass", "Vowel"][v.clamp(0, 3) as usize].into(),
        FILTER => ["Bypass", "Low pass", "Band pass", "High pass"][v.clamp(0, 3) as usize].into(),
        LFO_SHAPE => ["Sine", "Triangle", "Ramp", "Square"][v.clamp(0, 3) as usize].into(),
        LFO_TARGET => {
            ["Position / warp", "Cutoff", "Pitch", "Amplitude"][v.clamp(0, 3) as usize].into()
        }
        LFO_SYNC => [
            "Free", "4 bars", "2 bars", "1 bar", "1/2", "1/4", "1/8", "1/16", "1/32",
        ][v.clamp(0, 8) as usize]
            .into(),
        A_TUNE | B_TUNE => format!("{v:+} st"),
        A_UNISON | B_UNISON => format!("{v} voices"),
        A_DETUNE | B_DETUNE => format!("{v} cents"),
        CUTOFF => format!("{:.0} Hz", cutoff(v as f32)),
        ATTACK | DECAY | RELEASE | ENV_ATTACK | ENV_DECAY | ENV_RELEASE => {
            format!("{:.0} ms", seconds(v as f32) * 1000.)
        }
        LFO_RATE => format!("{:.2} Hz", lfo_hz(v as f32)),
        DELAY_TIME => format!("{} ms", 50 + v),
        CHORUS_RATE => format!("{:.2} Hz", 0.05 + v as f32 / 200.),
        _ => format!("{:.1}%", 100. * (v as f32 / SETTINGS[k].range.1 as f32)),
    }
}
fn parse(k: usize, s: &str) -> Option<i32> {
    let s = s.trim();
    if ROUTES[2..].iter().any(|s| k == *s || k == s + 1)
        || LFO_CONTROLS.iter().any(|s| s[1..].contains(&k))
        || k == LFO_DRAW
        || matches!(
            k,
            ENV_ENGINE
                | FILTER_MODEL
                | A_BANK
                | B_BANK
                | FILTER
                | LFO_SHAPE
                | LFO_TARGET
                | LFO_SYNC
                | A_SOURCE
                | B_SOURCE
                | A_MODE
                | B_MODE
                | MOD1_SOURCE
                | MOD2_SOURCE
                | MOD1_TARGET
                | MOD2_TARGET
        )
    {
        return (SETTINGS[k].range.0..=SETTINGS[k].range.1)
            .find(|&v| text(k, v).eq_ignore_ascii_case(s));
    }
    let x = s
        .split_whitespace()
        .next()?
        .trim_end_matches('%')
        .parse::<f32>()
        .ok()?;
    if !x.is_finite() {
        return None;
    }
    let v = match k {
        AMP_HOLD | ENV_HOLD => (x.max(0.) / 32000.).powf(0.2) * 1000.,
        ATTACK | DECAY | RELEASE | ENV_ATTACK | ENV_DECAY | ENV_RELEASE => {
            x.max(1.).log10() / 4. * 1000.
        }
        CUTOFF => (x.max(20.) / 20.).log10() / 3. * 1000.,
        LFO_RATE | LFO2_RATE | LFO3_RATE | LFO4_RATE => {
            (x.max(0.05) / 0.05).ln() / 400.0_f32.ln() * 1000.
        }
        OTT_TIME => (x - 20.) * 2.,
        CHORUS_RATE => (x - 0.05) * 200.,
        DELAY_TIME => x - 50.,
        A_TUNE | B_TUNE | A_UNISON | B_UNISON | A_DETUNE | B_DETUNE | A_PHASE | B_PHASE => x,
        _ => x / 100. * SETTINGS[k].range.1 as f32,
    };
    Some((v.round() as i32).clamp(SETTINGS[k].range.0, SETTINGS[k].range.1))
}

pub fn hold_seconds(v: f32) -> f32 {
    32. * (v / 1000.).powi(5)
}
