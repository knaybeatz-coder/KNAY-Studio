// SPDX-License-Identifier: GPL-3.0-or-later
//! Included recordings are part of the binary. No original drive/catalogue is needed.
include!(concat!(env!("OUT_DIR"), "/included_library.rs"));
#[derive(serde::Deserialize)]
struct Record {
    group: String,
    name: String,
    root: f32,
    offset: usize,
    length: usize,
}
fn records() -> Result<(usize, Vec<Record>), String> {
    if PACK.is_empty() {
        return Ok((0, vec![]));
    }
    if PACK.len() < 8 || &PACK[..4] != b"LWPK" {
        return Err("Invalid included library".into());
    }
    let n = u32::from_le_bytes(PACK[4..8].try_into().unwrap()) as usize;
    let end = 8usize
        .checked_add(n)
        .filter(|e| *e <= PACK.len())
        .ok_or("Invalid library header")?;
    let r: Vec<Record> = serde_json::from_slice(&PACK[8..end]).map_err(|e| e.to_string())?;
    Ok((end, r))
}
pub fn entries() -> Vec<crate::sound_browser::Entry> {
    records()
        .map(|(_, r)| {
            r.into_iter()
                .enumerate()
                .map(|(i, r)| crate::sound_browser::Entry {
                    group: r.group,
                    name: r.name,
                    path: Default::default(),
                    root: r.root,
                    embedded: Some(i),
                })
                .collect()
        })
        .unwrap_or_default()
}
pub fn load(i: usize) -> Result<crate::sample::Sample, String> {
    let (start, r) = records()?;
    let r = r.get(i).ok_or("Included sound missing")?;
    let begin = start.checked_add(r.offset).ok_or("Invalid sample offset")?;
    let end = begin.checked_add(r.length).ok_or("Invalid sample length")?;
    let bytes = PACK.get(begin..end).ok_or("Truncated included sample")?;
    crate::sample::Sample::decode(bytes, format!("{} / {}", r.group, r.name), r.root)
}
#[cfg(test)]
mod tests {
    #[test]
    fn included_pack_loads_without_external_files() {
        let entries = super::entries();
        for (i, _) in entries.iter().enumerate() {
            super::load(i).unwrap().validate().unwrap();
        }
        if !super::PACK.is_empty() {
            assert_eq!(entries.len(), 260);
            assert!(entries.iter().any(|e| e.group.contains("Mini-Moog")));
        }
    }
}
