// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    assets::{MAX_STATE, Sources, WaveParams},
    controls::*,
    factory_index::FACTORY,
};
use nih_plug::{prelude::*, wrapper::state::ParamValue};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub const CATEGORIES: [&str; 10] = [
    "Init", "Bass", "Arps", "Keys", "Pads", "Leads", "Plucks", "Textures", "FX", "Drums",
];
pub const FIELD: &str = "lemon-wave-program-v1";
const HEADER: &[u8] =
    b"OSPRESET 1\nplugin=Lemon Wave\tid=clap:org.lemonstudio.lemon-wave\tformat=clap\n";
const LIMIT: usize = MAX_STATE + 8 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct Origin {
    pub bank: String,
    pub category: String,
    pub name: String,
}
pub fn root() -> PathBuf {
    crate::table_tools::root().with_file_name("User Presets")
}
fn check_name(s: &str) -> Result<(), String> {
    let reserved = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    if s.trim() != s
        || s.is_empty()
        || s.chars().count() > 52
        || s.ends_with('.')
        || s.chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || ["CON", "PRN", "AUX", "NUL"].contains(&reserved.as_str())
        || (reserved.len() == 4
            && (reserved.starts_with("COM") || reserved.starts_with("LPT"))
            && reserved.as_bytes()[3].is_ascii_digit())
    {
        return Err("Choose a name of 1–52 characters without file-name symbols.".into());
    }
    Ok(())
}
pub fn path_for(root: &Path, category: &str, name: &str) -> Result<PathBuf, String> {
    check_name(name)?;
    if !CATEGORIES.contains(&category) {
        return Err("Choose a preset category.".into());
    }
    Ok(root.join(category).join(format!("{name}.ospreset")))
}
pub fn users(root: &Path, category: &str) -> Result<Vec<String>, String> {
    let dir = path_for(root, category, "Example")?
        .parent()
        .unwrap()
        .to_path_buf();
    let files = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.to_string()),
    };
    let mut names = Vec::new();
    for entry in files {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if entry.file_type().map_err(|e| e.to_string())?.is_file()
            && path.extension().is_some_and(|e| e == "ospreset")
        {
            if let Some(name) = path
                .file_stem()
                .and_then(|n| n.to_str())
                .filter(|n| check_name(n).is_ok())
            {
                names.push(name.to_owned());
            }
        }
    }
    names.sort_by_key(|n| n.to_lowercase());
    Ok(names)
}
fn decode(bytes: &[u8]) -> Result<PluginState, String> {
    let body = bytes
        .strip_prefix(HEADER)
        .ok_or("Choose a Lemon Wave preset.")?;
    let size = u64::from_le_bytes(
        body.get(..8)
            .ok_or("Incomplete preset.")?
            .try_into()
            .unwrap(),
    );
    if size != body[8..].len() as u64 || size > LIMIT as u64 {
        return Err("Incomplete or oversized preset.".into());
    }
    let mut state: PluginState = serde_json::from_slice(&body[8..]).map_err(|e| e.to_string())?;
    crate::LemonWave::filter_state(&mut state);
    Ok(state)
}
pub fn factory(index: usize) -> PluginState {
    let (category, name, data) = FACTORY[index % FACTORY.len()];
    let mut state = decode(data).expect("verified factory program");
    let origin = Origin {
        bank: "Factory".into(),
        category: category.into(),
        name: name.into(),
    };
    state
        .fields
        .insert(FIELD.into(), serde_json::to_string(&origin).unwrap());
    state
}
pub fn capture(params: &WaveParams, origin: &Origin) -> PluginState {
    let mut state = factory(0);
    let values: Vec<_> = (0..COUNT).map(|k| params.base.v(k)).collect();
    for (k, setting) in SETTINGS.iter().enumerate() {
        state
            .params
            .insert(setting.id.into(), ParamValue::I32(values[k]));
    }
    state.fields = params.serialize_fields();
    let raw = lemon_small_fx::params::to_fl(SPEC, &values, &params.base.raw_copy());
    let hex: String = raw.iter().map(|v| format!("{v:02x}")).collect();
    state
        .fields
        .insert("state".into(), serde_json::to_string(&hex).unwrap());
    state
        .fields
        .insert(FIELD.into(), serde_json::to_string(origin).unwrap());
    state
}
fn checked(state: &PluginState) -> Result<(), String> {
    let raw = state
        .fields
        .get("state")
        .and_then(|encoded| serde_json::from_str::<String>(encoded).ok())
        .ok_or("Missing Wave patch data.")?;
    if raw.len() != 8 + COUNT * 8
        || !raw.starts_with("01000000")
        || !raw.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err("Damaged Wave patch data.".into());
    }
    for s in &SETTINGS {
        match state.params.get(s.id) {
            Some(ParamValue::I32(v)) if (s.range.0..=s.range.1).contains(v) => (),
            _ => return Err(format!("Invalid preset parameter {}", s.name)),
        }
    }
    if let Some(s) = state.fields.get(crate::assets::FIELD) {
        if s.len() > MAX_STATE {
            return Err("Embedded assets exceed the Wave size limit.".into());
        }
        let a: Sources = serde_json::from_str(s).map_err(|e| e.to_string())?;
        for table in a.oscillators.iter().flatten() {
            table.validate()?;
        }
        for sample in a.samples.iter().flatten().chain(a.noise.iter()) {
            sample.validate()?;
        }
    }
    if let Some(s) = state.fields.get(crate::modulation::FIELD) {
        crate::modulation::Curves::decode(s)?;
    }
    Ok(())
}
pub fn read_user(root: &Path, category: &str, name: &str) -> Result<PluginState, String> {
    let file = fs::File::open(path_for(root, category, name)?).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take((LIMIT + HEADER.len() + 9) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let mut state = decode(&bytes)?;
    checked(&state)?;
    state.fields.insert(
        FIELD.into(),
        serde_json::to_string(&Origin {
            bank: "User".into(),
            category: category.into(),
            name: name.into(),
        })
        .unwrap(),
    );
    Ok(state)
}
pub fn save(
    root: &Path,
    params: &WaveParams,
    origin: &Origin,
    replace: bool,
) -> Result<(), String> {
    let path = path_for(root, &origin.category, &origin.name)?;
    let state = capture(params, origin);
    checked(&state)?;
    let data = serde_json::to_vec(&state).map_err(|e| e.to_string())?;
    if data.len() > LIMIT {
        return Err("Preset exceeds the file size limit.".into());
    }
    let dir = path.parent().unwrap();
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let tmp = dir.join(format!(
        ".wave-{}-{}.tmp",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    let result = (|| -> std::io::Result<()> {
        file.write_all(HEADER)?;
        file.write_all(&(data.len() as u64).to_le_bytes())?;
        file.write_all(&data)?;
        file.sync_all()?;
        drop(file);
        if replace {
            fs::rename(&tmp, &path)?;
        } else {
            fs::hard_link(&tmp, &path)?;
        }
        Ok(())
    })();
    let _ = fs::remove_file(tmp);
    result.map_err(|e| e.to_string())?;
    *params.origin.lock().unwrap() = Some(origin.clone());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dsp::Engine, performance::Player};
    #[test]
    fn factory_bank_is_distinct_audible_and_valid() {
        use std::collections::HashSet;
        let mut signatures = HashSet::new();
        let mut names = HashSet::new();
        let mut max_peak = 0f32;
        let params = WaveParams::default();
        let mut engine = Engine::new(24000.);
        let mut player = Player::default();
        for (i, (category, name, _)) in FACTORY.iter().enumerate() {
            assert!(names.insert(*name), "Duplicate {name}");
            let state = factory(i);
            checked(&state).unwrap();
            params.deserialize_fields(&state.fields);
            let patch = std::array::from_fn(|k| match state.params[SETTINGS[k].id] {
                ParamValue::I32(v) => v as f32,
                _ => unreachable!(),
            });
            engine.custom = params.tables.read().unwrap().clone();
            engine.curves = *params.curves.read().unwrap();
            engine.set(patch);
            engine.reset();
            player.reset();
            player.configure(&mut engine, &patch);
            player.on(
                &mut engine,
                if *category == "Drums" { 48 } else { 60 },
                0.8,
                0,
                None,
                false,
            );
            let mut l = [0.; 512];
            let mut r = l;
            let mut peak = 0f32;
            let mut signature = Vec::new();
            for _ in 0..24 {
                player.render(&mut engine, &mut l, &mut r);
                for v in l.iter().chain(&r) {
                    assert!(v.is_finite(), "{name}");
                    peak = peak.max(v.abs());
                }
                signature.extend(l.iter().step_by(31).map(|v| (v * 100000.).round() as i32));
            }
            assert!(peak > 0.0002, "Silent {name} ({peak})");
            max_peak = max_peak.max(peak);
            if i >= 14 {
                assert!(signatures.insert(signature), "Duplicate sound {name}");
            }
        }
        assert_eq!(signatures.len(), 144);
        println!(
            "{} factory programs rendered. Maximum sampled peak {max_peak:.3}",
            FACTORY.len()
        );
    }
}
