// SPDX-License-Identifier: GPL-3.0-or-later
//! Portable font assets in the same theme file as the icon pack selection.

use base64::{Engine, engine::general_purpose::STANDARD};
use os_theme::fl;
use std::{io::Read, path::Path, sync::Arc};

pub const OPEN: &str = "Icons & Fonts...";
pub const FONT: &str = "LemonStudioStyle.ui-font";
pub const ICON: &str = "LemonStudioStyle.icon-family";
pub const DATA: &str = "LemonStudioFontData";
pub const NAME: &str = "LemonStudioFontName";
pub const FONTS: [&str; 5] = [
    "System default",
    "Cuprum",
    "Barlow",
    "Barlow Condensed",
    "Imported",
];
const MAX_FONT: usize = 512 * 1024;

#[derive(Default)]
pub struct FontAsset {
    source: Option<String>,
    theme: Option<String>,
    pub bytes: Option<Arc<[u8]>>,
}

impl FontAsset {
    pub fn refresh(&mut self, text: Option<&str>, font: u8) {
        if font != 4 {
            self.source = None;
            self.theme = None;
            self.bytes = None;
            return;
        }
        if self.theme.as_deref() == text {
            return;
        }
        self.theme = text.map(str::to_owned);
        let encoded = text
            .and_then(|t| fl::get_key(t, DATA))
            .filter(|s| s.len() <= MAX_FONT.div_ceil(3) * 4);
        if encoded == self.source.as_deref() {
            return;
        }
        self.source = encoded.map(str::to_owned);
        self.bytes = encoded
            .and_then(|s| STANDARD.decode(s).ok())
            .filter(|b| b.len() <= MAX_FONT)
            .map(Arc::from);
    }
}

pub fn import(text: &str, path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_FONT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FONT || !os_ui::Canvas::valid_theme_font(&bytes) {
        return Err("Choose a supported TrueType/OpenType font up to 512 KB.".into());
    }
    let name: String = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .filter(|c| !c.is_control())
        .take(80)
        .collect();
    let text = fl::set_key(text, DATA, &STANDARD.encode(bytes));
    let text = fl::set_key(&text, NAME, &name);
    Ok(fl::set_key(&text, FONT, "4"))
}
