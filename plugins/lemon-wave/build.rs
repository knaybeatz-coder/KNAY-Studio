// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::PathBuf;
fn main() {
    println!("cargo:rerun-if-env-changed=LEMON_WAVE_LIBRARY");
    let path = std::env::var_os("LEMON_WAVE_LIBRARY")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(|p| PathBuf::from(p).join("Lemon Studio/Lemon Wave/Included Library.lwpack"))
        });
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("included_library.rs");
    let text = if let Some(p) = path.filter(|p| p.is_file()) {
        println!("cargo:rerun-if-changed={}", p.display());
        let b = std::fs::read(&p).expect("Read included Wave library");
        assert!(
            b.starts_with(b"LWPK") && b.len() < 256 * 1024 * 1024,
            "Invalid included Wave library"
        );
        format!(
            "pub const PACK:&[u8]=include_bytes!({:?});",
            p.to_string_lossy()
        )
    } else {
        println!("cargo:warning=Building Wave without optional recorded-synth library");
        "pub const PACK:&[u8]=&[];".into()
    };
    std::fs::write(out, text).unwrap();
}
