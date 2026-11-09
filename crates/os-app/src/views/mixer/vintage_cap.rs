// SPDX-License-Identifier: GPL-3.0-or-later
//! Molded, colored caps for the Original console. The ivory pointer is painted
//! separately so turning it does not rotate the lighting or the cast shadow.
use os_ui::Color;
use std::sync::OnceLock;

pub(super) struct Sprite {
    pub(super) size: usize,
    pub(super) pixels: Vec<u8>,
}

// Left: existing control color; right: the cap face sampled from the concept.
const CAPS: [(u32, u32); 11] = [
    (0xc8c2ad, 0x918c70),
    (0xd1a77e, 0xb18259),
    (0xc98b73, 0xa25842),
    (0x89b593, 0x8c9c66),
    (0x84a7c1, 0x788d9a),
    (0xcbb578, 0xcca53c),
    (0x91b9c9, 0x789da6),
    (0xb9a0d1, 0x9c85af),
    (0xb5b7a7, 0xb4b39a),
    (0xc2c7c9, 0xa3a59a),
    (0x9cae84, 0x93a17b),
];

pub(super) fn for_color(color: Color) -> &'static Sprite {
    static CAPS_CACHE: [OnceLock<Sprite>; CAPS.len()] = [const { OnceLock::new() }; CAPS.len()];
    let key = (u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b);
    let index = CAPS.iter().position(|&(src, _)| src == key).unwrap_or(8);
    CAPS_CACHE[index].get_or_init(|| render(Color::rgb(CAPS[index].1)))
}

fn render(cap: Color) -> Sprite {
    const SIZE: usize = 96;
    let mut pixels = Vec::with_capacity(SIZE * SIZE * 4);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let nx = ((x as f32 + 0.5) / SIZE as f32 - 0.5) * 2.5;
            let ny = ((y as f32 + 0.5) / SIZE as f32 - 0.5) * 2.5;
            let skirt = nx.hypot(ny - 0.04);
            let top = nx.hypot((ny + 0.155) / 0.96);
            let footprint = skirt.min(top / 0.90);
            if footprint > 1. {
                let shadow = (nx - 0.045).hypot((ny - 0.14) * 0.97);
                let alpha = (1. - (shadow - 0.93) / 0.27).clamp(0., 1.).powi(2) * 170.;
                pixels.extend([8, 8, 6, alpha as u8]);
                continue;
            }
            let angle = ny.atan2(nx);
            let light = (-nx * 0.44 - ny * 0.63 + 0.73).clamp(0., 1.65);
            let (gain, shine) = if top <= 0.90 {
                let dome = (1. - (top / 0.90).powi(2)).max(0.).sqrt();
                let bevel = ((0.90 - top) / 0.10).clamp(0., 1.);
                let spec = (-((nx + 0.37) / 0.39).powi(2) - ((ny + 0.56) / 0.14).powi(2)).exp();
                (
                    0.50 + 0.31 * light + 0.14 * dome + 0.065 * bevel,
                    spec * 23.,
                )
            } else {
                let ribs = (23. * angle).cos() * 0.037;
                (0.28 + 0.22 * light + ribs, 0.)
            };
            let ch = |v: u8| (f32::from(v) * gain + shine).clamp(0., 255.) as u8;
            pixels.extend([ch(cap.r), ch(cap.g), ch(cap.b), 255]);
        }
    }
    Sprite { size: SIZE, pixels }
}
