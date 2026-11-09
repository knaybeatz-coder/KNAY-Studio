// SPDX-License-Identifier: GPL-3.0-or-later
//! Coordinated rendering recipes over the app's semantic vector glyphs.

use crate::Color;

pub struct Pack {
    pub name: &'static str,
    pub weight: f32,
    pub scale: f32,
    pub round: bool,
    pub badge: u8,
    pub shadow: bool,
    pub tint: u32,
}

pub const PACKS: [Pack; os_theme::appearance::CUSTOM_ICON_PACKS as usize + 1] = [
    Pack {
        name: "Default",
        weight: 1.0,
        scale: 1.0,
        round: true,
        badge: 0,
        shadow: false,
        tint: 0xf4e4b6,
    },
    Pack {
        name: "Workshop",
        weight: 1.0,
        scale: 1.0,
        round: true,
        badge: 0,
        shadow: false,
        tint: 0xf4e4b6,
    },
    Pack {
        name: "Draft",
        weight: 0.70,
        scale: 1.03,
        round: true,
        badge: 0,
        shadow: false,
        tint: 0xd3efff,
    },
    Pack {
        name: "Broadcast",
        weight: 1.54,
        scale: 0.94,
        round: false,
        badge: 0,
        shadow: true,
        tint: 0xf3d0ac,
    },
    Pack {
        name: "Console",
        weight: 0.92,
        scale: 0.76,
        round: false,
        badge: 1,
        shadow: false,
        tint: 0x7fddff,
    },
    Pack {
        name: "Pixel",
        weight: 1.35,
        scale: 0.76,
        round: false,
        badge: 2,
        shadow: true,
        tint: 0xff8bd9,
    },
    Pack {
        name: "Orbit",
        weight: 0.88,
        scale: 0.75,
        round: true,
        badge: 3,
        shadow: false,
        tint: 0xcabaff,
    },
    Pack {
        name: "Stamp",
        weight: 1.2,
        scale: 0.85,
        round: true,
        badge: 0,
        shadow: true,
        tint: 0xffe1a6,
    },
    Pack {
        name: "Circuit",
        weight: 1.16,
        scale: 0.82,
        round: false,
        badge: 4,
        shadow: false,
        tint: 0xffbf54,
    },
    Pack {
        name: "Score",
        weight: 0.76,
        scale: 0.84,
        round: false,
        badge: 5,
        shadow: false,
        tint: 0xb4f7e0,
    },
    Pack {
        name: "Stream",
        weight: 1.22,
        scale: 0.73,
        round: true,
        badge: 6,
        shadow: true,
        tint: 0x9ac0ff,
    },
    Pack {
        name: "Deco",
        weight: 0.84,
        scale: 1.0,
        round: false,
        badge: 0,
        shadow: false,
        tint: 0xf4d5a0,
    },
    Pack {
        name: "Foundry",
        weight: 1.16,
        scale: 1.0,
        round: false,
        badge: 0,
        shadow: true,
        tint: 0xf3ae75,
    },
    Pack {
        name: "Origami",
        weight: 0.92,
        scale: 1.0,
        round: false,
        badge: 0,
        shadow: false,
        tint: 0xb8dfe6,
    },
    Pack {
        name: "Beacon",
        weight: 1.0,
        scale: 1.0,
        round: true,
        badge: 0,
        shadow: false,
        tint: 0xf7df9b,
    },
    Pack {
        name: "Tile",
        weight: 1.0,
        scale: 1.0,
        round: true,
        badge: 0,
        shadow: false,
        tint: 0xaddbea,
    },
    Pack {
        name: "Ribbon",
        weight: 1.0,
        scale: 1.0,
        round: true,
        badge: 0,
        shadow: false,
        tint: 0xe6b9da,
    },
    Pack { name: "Quartz", weight: 0.92, scale: 1.0, round: true, badge: 0, shadow: false, tint: 0xd1e4f2 },
    Pack { name: "Capsule", weight: 1.05, scale: 1.0, round: true, badge: 0, shadow: false, tint: 0xbbe8db },
    Pack { name: "Compass", weight: 1.0, scale: 1.0, round: true, badge: 0, shadow: false, tint: 0xf2d58c },
    Pack { name: "Studio Ink", weight: 1.18, scale: 1.0, round: true, badge: 0, shadow: false, tint: 0xf4e8cf },
    Pack { name: "Precision", weight: 0.82, scale: 1.0, round: true, badge: 0, shadow: false, tint: 0xb5d4ff },
    Pack { name: "Classic Clean", weight: 1.0, scale: 1.0, round: true, badge: 0, shadow: false, tint: 0xece8dd },
    Pack { name: "Classic Soft", weight: 1.0, scale: 1.0, round: true, badge: 0, shadow: false, tint: 0xece8dd },
    Pack { name: "Classic Square", weight: 1.0, scale: 1.0, round: false, badge: 0, shadow: false, tint: 0xece8dd },
    Pack { name: "Classic Solid", weight: 1.16, scale: 1.0, round: true, badge: 0, shadow: false, tint: 0xece8dd },
    Pack { name: "Classic Open", weight: 1.0, scale: 1.0, round: true, badge: 0, shadow: false, tint: 0xece8dd },
    Pack { name: "2005 Desktop", weight: 1.08, scale: 1.0, round: false, badge: 0, shadow: false, tint: 0xe2e7d8 },
];

pub fn get(id: u8) -> &'static Pack {
    PACKS.get(usize::from(id)).unwrap_or(&PACKS[0])
}

pub fn ink(id: u8, color: Color) -> Color {
    if id == 0 {
        color
    } else {
        color
            .mix(Color::rgb(get(id).tint), 0.17)
            .with_alpha(color.a)
    }
}
