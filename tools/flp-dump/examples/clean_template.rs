// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
fn main() -> Result<(), Box<dyn std::error::Error>> {
 let a: Vec<String> = std::env::args().collect();
 if a.len() != 3 { return Err("usage: clean_template input.flp output.flp".into()); }
 let file = os_flp::FlpFile::parse(&std::fs::read(&a[1])?)?;
 let mut p = os_flp::project::load(&file)?;
 let slots = p.mixer.iter().flat_map(|m| m.slots.iter()).filter(|s| s.is_some()).count();
 for m in &mut p.mixer { m.slots = Default::default(); }
 for c in &mut p.channels {
   c.plugin = None; c.plugin_state = None;
   if c.kind == os_model::ChannelKind::Plugin { c.kind = os_model::ChannelKind::Sampler; c.sample_path = None; }
 }
 let out = os_flp::project::create(&p)?;
 let bytes = out.to_bytes()?;
 let back = os_flp::project::load(&os_flp::FlpFile::parse(&bytes)?)?;
 assert!(back.mixer.iter().all(|m| m.slots.iter().all(Option::is_none)));
 assert!(back.channels.iter().all(|c| c.plugin.is_none() && c.plugin_state.is_none()));
 for a in &p.mixer { let b = back.mixer.iter().find(|b| b.index == a.index).ok_or("Missing mixer track")?; assert_eq!(a.name, b.name); assert_eq!(a.color, b.color); assert_eq!(a.routes, b.routes); assert!((a.volume.unwrap_or(0.8)-b.volume.unwrap_or(0.8)).abs() < 0.001); }
 std::fs::write(&a[2], bytes)?;
 println!("Removed {slots} effect slots; retained {} mixer tracks, {} channels", back.mixer.len(), back.channels.len());
 Ok(())
}

