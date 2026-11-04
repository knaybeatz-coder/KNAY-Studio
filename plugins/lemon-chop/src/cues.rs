// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only Serato Markers2 interoperability. Format reference: Holzhaus/serato-tags docs.
use crate::params::{Cue, Document};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
const LIMIT: usize = 16 * 1024 * 1024;
#[derive(Clone, Debug)]
pub struct ImportedCue {
    pub index: usize,
    pub start: f64,
    pub end: Option<f64>,
    pub name: String,
    pub color: u32,
}
fn serato32(b: &[u8]) -> Option<u32> {
    if b.len() != 4 || b.iter().any(|x| *x > 127) {
        return None;
    }
    Some(((b[0] as u32 & 7) << 21) | ((b[1] as u32) << 14) | ((b[2] as u32) << 7) | b[3] as u32)
}
fn legacy(data: &[u8]) -> Option<Vec<ImportedCue>> {
    if !data.starts_with(&[2, 5]) {
        return None;
    }
    let count = be(data.get(2..6)?)?;
    if count > 32 || data.len() < 6 + count * 22 + 4 {
        return None;
    }
    let mut out = Vec::new();
    for (i, b) in data[6..6 + count * 22].chunks_exact(22).enumerate() {
        if b[0] != 0 || ![1, 3].contains(&b[20]) {
            continue;
        }
        let is_loop = b[20] == 3;
        let index = if is_loop { 16 + i.saturating_sub(5) } else { i };
        if index >= 32 {
            continue;
        }
        out.push(ImportedCue {
            index,
            start: serato32(&b[1..5])? as f64 / 1000.,
            end: if b[5] == 0 {
                Some(serato32(&b[6..10])? as f64 / 1000.)
            } else {
                None
            },
            color: serato32(&b[16..20])?,
            name: String::new(),
        });
    }
    Some(out)
}
fn be(b: &[u8]) -> Option<usize> {
    Some(u32::from_be_bytes(b.get(..4)?.try_into().ok()?) as usize)
}
fn sync(b: &[u8]) -> Option<usize> {
    if b.len() < 4 || b[..4].iter().any(|x| *x > 127) {
        return None;
    }
    Some(b[..4].iter().fold(0, |n, x| n * 128 + *x as usize))
}
fn decode(b: &[u8]) -> Option<Vec<u8>> {
    if b.len() > LIMIT {
        return None;
    }
    let (mut bits, mut n) = (0u32, 0u32);
    let mut out = Vec::new();
    for &x in b.iter().take_while(|x| **x != 0) {
        if x.is_ascii_whitespace() {
            continue;
        }
        if x == b'=' {
            break;
        }
        let v = match x {
            b'A'..=b'Z' => x - b'A',
            b'a'..=b'z' => x - b'a' + 26,
            b'0'..=b'9' => x - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        bits = (bits << 6) | v as u32;
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push((bits >> n) as u8);
            bits &= (1 << n) - 1;
        }
    }
    Some(out)
}
pub fn markers(data: &[u8]) -> Option<Vec<ImportedCue>> {
    let data = if data.starts_with(&[1, 1]) || data.starts_with(&[2, 5]) {
        data.to_vec()
    } else {
        decode(data)?
    };
    // FLAC/MP4 wrap the tag in a GEOB-like MIME header.
    let at = data
        .windows(16)
        .position(|w| w == b"Serato Markers2\0" || w == b"Serato Markers_\0")
        .map(|i| i + 16)
        .unwrap_or(0);
    let tag = data.get(at..)?;
    if tag.starts_with(&[2, 5]) {
        return legacy(tag);
    }
    if !tag.starts_with(&[1, 1]) {
        return None;
    }
    let payload = decode(&tag[2..])?;
    parse_payload(&payload)
}
fn parse_payload(payload: &[u8]) -> Option<Vec<ImportedCue>> {
    if !payload.starts_with(&[1, 1]) {
        return None;
    }
    let (mut at, mut out) = (2, Vec::new());
    while at < payload.len() && payload[at] != 0 {
        let end = at + payload[at..].iter().position(|x| *x == 0)?;
        let size = be(payload.get(end + 1..end + 5)?)?;
        let next = (end + 5).checked_add(size)?;
        let b = payload.get(end + 5..next)?;
        if &payload[at..end] == b"CUE" && b.len() >= 13 {
            let index = b[1] as usize;
            let time = be(&b[2..6])? as f64 / 1000.;
            let name = String::from_utf8_lossy(b[12..].split(|x| *x == 0).next()?)
                .chars()
                .take(64)
                .collect();
            if index < 32 {
                out.push(ImportedCue {
                    index,
                    start: time,
                    end: None,
                    name,
                    color: ((b[7] as u32) << 16) | ((b[8] as u32) << 8) | b[9] as u32,
                });
            }
        }
        if &payload[at..end] == b"LOOP" && b.len() >= 21 && b[1] < 16 {
            out.push(ImportedCue {
                index: 16 + b[1] as usize,
                start: be(&b[2..6])? as f64 / 1000.,
                end: Some(be(&b[6..10])? as f64 / 1000.),
                color: be(&b[14..18])? as u32 & 0xffffff,
                name: String::from_utf8_lossy(b[20..].split(|x| *x == 0).next()?)
                    .chars()
                    .take(64)
                    .collect(),
            });
        }
        at = next;
    }
    Some(out)
}
#[cfg(test)]
fn id3(data: &[u8]) -> Option<Vec<u8>> {
    id3_named(data, false)
}
fn id3_named(data: &[u8], old: bool) -> Option<Vec<u8>> {
    if data.len() < 10 || data.get(..3)? != b"ID3" {
        return None;
    }
    let version = data[3];
    if !(3..=4).contains(&version) || data[5] & 0xC0 != 0 {
        return None;
    }
    let end = (10 + sync(&data[6..10])?).min(data.len());
    let mut at = 10;
    while at + 10 <= end {
        let size = if version == 4 {
            sync(&data[at + 4..at + 8])?
        } else {
            be(&data[at + 4..at + 8])?
        };
        if size == 0 {
            break;
        }
        let next = (at + 10).checked_add(size)?;
        if next > end {
            return None;
        }
        let frame = data.get(at + 10..next)?;
        if &data[at..at + 4] == b"GEOB" && data[at + 8] == 0 && data[at + 9] == 0 {
            if let Some(i) = frame.windows(16).position(|w| {
                w == if old {
                    b"Serato Markers_\0"
                } else {
                    b"Serato Markers2\0"
                }
            }) {
                return Some(frame[i + 16..].to_vec());
            }
        }
        at = next;
    }
    None
}
fn read_at(f: &mut File, at: u64, n: usize) -> Option<Vec<u8>> {
    if n > LIMIT {
        return None;
    }
    f.seek(SeekFrom::Start(at)).ok()?;
    let mut b = vec![0; n];
    f.read_exact(&mut b).ok()?;
    Some(b)
}
fn mp4(f: &mut File, start: u64, end: u64, depth: usize, old: bool) -> Option<Vec<u8>> {
    if depth > 8 {
        return None;
    }
    let mut at = start;
    while at + 8 <= end {
        let h = read_at(f, at, 8)?;
        let size = be(&h)? as u64;
        if size < 8 || at.checked_add(size)? > end {
            return None;
        }
        let kind = &h[4..8];
        if kind == b"----" {
            let b = read_at(f, at + 8, (size - 8) as usize)?;
            if b.windows(13).any(|w| w == b"com.serato.dj")
                && if old {
                    b.windows(7).any(|w| w == b"markers")
                        && !b.windows(9).any(|w| w == b"markersv2")
                } else {
                    b.windows(9).any(|w| w == b"markersv2")
                }
            {
                let mut i = 0;
                while i + 8 <= b.len() {
                    let n = be(&b[i..])?;
                    if n < 8 || i + n > b.len() {
                        return None;
                    }
                    if &b[i + 4..i + 8] == b"data" && n >= 16 {
                        return Some(b[i + 16..i + n].to_vec());
                    }
                    i += n;
                }
            }
        } else if [b"moov", b"udta", b"meta", b"ilst"].contains(&kind.try_into().ok()?) {
            if let Some(b) = mp4(
                f,
                at + 8 + if kind == b"meta" { 4 } else { 0 },
                at + size,
                depth + 1,
                old,
            ) {
                return Some(b);
            }
        }
        at += size;
    }
    None
}
fn tag(path: &Path, old: bool) -> Option<Vec<u8>> {
    let mut f = File::open(path).ok()?;
    let length = f.metadata().ok()?.len();
    let head = read_at(&mut f, 0, 12)?;
    if &head[..3] == b"ID3" {
        let n = 10 + sync(&head[6..10])?;
        return id3_named(&read_at(&mut f, 0, n)?, old);
    }
    if &head[..4] == b"RIFF" || &head[..4] == b"FORM" {
        let le = &head[..4] == b"RIFF";
        let mut at = 12u64;
        while at + 8 <= length {
            let h = read_at(&mut f, at, 8)?;
            let n = if le {
                u32::from_le_bytes(h[4..8].try_into().ok()?) as usize
            } else {
                be(&h[4..8])?
            };
            if h[..4].eq_ignore_ascii_case(b"id3 ") {
                return id3_named(&read_at(&mut f, at + 8, n)?, old);
            }
            at = at.checked_add(8 + n as u64 + (n % 2) as u64)?;
        }
    } else if &head[..4] == b"fLaC" && !old {
        let mut at = 4;
        loop {
            let h = read_at(&mut f, at, 4)?;
            let n = ((h[1] as usize) << 16) | ((h[2] as usize) << 8) | h[3] as usize;
            if h[0] & 127 == 4 {
                let b = read_at(&mut f, at + 4, n)?;
                let le = |p: usize| -> Option<usize> {
                    Some(u32::from_le_bytes(b.get(p..p + 4)?.try_into().ok()?) as usize)
                };
                let mut i = 4 + le(0)?;
                let count = le(i)?;
                i += 4;
                for _ in 0..count.min(65536) {
                    let n = le(i)?;
                    i += 4;
                    let value = b.get(i..i + n)?;
                    if value.starts_with(b"SERATO_MARKERS_V2=") {
                        return Some(value[18..].to_vec());
                    }
                    i += n;
                }
            }
            at += 4 + n as u64;
            if h[0] & 128 != 0 {
                break;
            }
        }
    } else if &head[4..8] == b"ftyp" {
        return mp4(&mut f, 0, length, 0, old);
    }
    None
}
pub fn import(path: &Path, doc: &mut Document, duration: f64) -> Result<usize, String> {
    let mut cues = tag(path, false)
        .and_then(|raw| markers(&raw))
        .unwrap_or_default();
    if let Some(old) = tag(path, true).and_then(|raw| markers(&raw)) {
        for mut cue in old {
            if let Some(i) = cues.iter().position(|c| c.index == cue.index) {
                cue.name = cues[i].name.clone();
                cues[i] = cue;
            } else {
                cues.push(cue);
            }
        }
    }
    if cues.is_empty() {
        return Err("No supported Serato cues found (Markers2 or legacy Markers_)".into());
    }
    let mut count = 0;
    for cue in &cues {
        let (i, start, name) = (cue.index, cue.start, &cue.name);
        if doc.cues[i].favorite || start >= duration {
            continue;
        }
        let end = cues
            .iter()
            .filter(|c| c.start > start)
            .map(|c| c.start)
            .min_by(f64::total_cmp)
            .unwrap_or(duration)
            .min(duration);
        let end = cue.end.unwrap_or(end).min(duration);
        if end <= start {
            continue;
        }
        doc.cues[i] = Cue {
            start,
            end,
            enabled: true,
            name: if name.is_empty() {
                format!("DJ cue {}", i + 1)
            } else {
                name.clone()
            },
            favorite: false,
            color: Some(cue.color),
        };
        count += 1;
    }
    if count == 0 {
        return Err("No usable cues imported; existing favorite pads are protected".into());
    }
    Ok(count)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_positions_colors_and_saved_loop_regions() {
        let enc = |n: u32| {
            [
                (n >> 21) as u8 & 7,
                (n >> 14) as u8 & 127,
                (n >> 7) as u8 & 127,
                n as u8 & 127,
            ]
        };
        let mut data = vec![2, 5];
        data.extend(14u32.to_be_bytes());
        for i in 0..14 {
            let mut b = [127u8; 22];
            if i == 0 || i == 5 {
                b[0] = 0;
                b[1..5].copy_from_slice(&enc(1500));
                b[16..20].copy_from_slice(&enc(0xcc0011));
                b[20] = if i == 0 { 1 } else { 3 };
                if i == 5 {
                    b[5] = 0;
                    b[6..10].copy_from_slice(&enc(2500));
                }
            }
            data.extend(b);
        }
        data.extend([0; 4]);
        let parsed = markers(&data).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].start, 1.5);
        assert_eq!(parsed[0].color, 0xcc0011);
        assert_eq!(parsed[1].index, 16);
        assert_eq!(parsed[1].end, Some(2.5));
        for len in 0..data.len() {
            assert!(legacy(&data[..len]).is_none());
        }
        let mut payload = vec![1, 1];
        payload.extend(b"LOOP\0");
        payload.extend(25u32.to_be_bytes());
        let mut b = vec![0, 2];
        b.extend(500u32.to_be_bytes());
        b.extend(2000u32.to_be_bytes());
        b.extend([255; 4]);
        b.extend(0x112233u32.to_be_bytes());
        b.extend([0, 0]);
        b.extend(b"Loop\0");
        payload.extend(b);
        payload.push(0);
        let loops = parse_payload(&payload).unwrap();
        assert_eq!(loops[0].index, 18);
        assert_eq!(loops[0].end, Some(2.));
        assert_eq!(loops[0].name, "Loop");
    }
    #[test]
    fn truncated_id3_headers_do_not_panic() {
        for n in 3..10 {
            let mut bytes = b"ID3\x03\0\0\0\0\0\0".to_vec();
            bytes.truncate(n);
            assert!(id3(&bytes).is_none());
        }
    }
    fn encode(bytes: &[u8]) -> Vec<u8> {
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = Vec::new();
        for c in bytes.chunks(3) {
            let n = ((c[0] as u32) << 16)
                | ((c.get(1).copied().unwrap_or(0) as u32) << 8)
                | c.get(2).copied().unwrap_or(0) as u32;
            out.push(alphabet[(n >> 18) as usize]);
            out.push(alphabet[((n >> 12) & 63) as usize]);
            if c.len() > 1 {
                out.push(alphabet[((n >> 6) & 63) as usize]);
            }
            if c.len() > 2 {
                out.push(alphabet[(n & 63) as usize]);
            }
        }
        out
    }
    #[test]
    fn tagged_import_preserves_favorites_and_source_bytes() {
        let mut payload = vec![1, 1];
        for (index, ms, name) in [(0, 500u32, "Kick"), (1, 1500, "Snare")] {
            let mut cue = vec![0, index];
            cue.extend(ms.to_be_bytes());
            cue.extend([0, 255, 0, 0, 0, 0]);
            cue.extend(name.as_bytes());
            cue.push(0);
            payload.extend(b"CUE\0");
            payload.extend((cue.len() as u32).to_be_bytes());
            payload.extend(cue);
        }
        payload.push(0);
        let mut marker = vec![1, 1];
        marker.extend(encode(&payload));
        let mut frame = b"\0application/octet-stream\0\0Serato Markers2\0".to_vec();
        frame.extend(marker.clone());
        let mut frames = b"GEOB".to_vec();
        frames.extend((frame.len() as u32).to_be_bytes());
        frames.extend([0, 0]);
        frames.extend(frame);
        let n = frames.len();
        let mut tag = b"ID3\x03\0\0".to_vec();
        tag.extend([
            (n >> 21) as u8 & 127,
            (n >> 14) as u8 & 127,
            (n >> 7) as u8 & 127,
            n as u8 & 127,
        ]);
        tag.extend(frames);
        let path = std::env::temp_dir().join(format!("chop-cues-{}.mp3", std::process::id()));
        std::fs::write(&path, &tag).unwrap();
        let mut doc = Document::default();
        doc.cues[0].favorite = true;
        doc.cues[0].name = "Protected".into();
        assert_eq!(import(&path, &mut doc, 4.).unwrap(), 1);
        assert_eq!(doc.cues[0].name, "Protected");
        assert_eq!(doc.cues[1].name, "Snare");
        assert_eq!(doc.cues[1].start, 1.5);
        assert_eq!(doc.cues[1].color, Some(0xff0000));
        let saved = serde_json::to_string(&doc).unwrap();
        assert_eq!(
            serde_json::from_str::<Document>(&saved).unwrap().cues[1].color,
            Some(0xff0000)
        );
        assert_eq!(std::fs::read(&path).unwrap(), tag);
        let mut wrapped = b"application/octet-stream\0\0Serato Markers2\0".to_vec();
        wrapped.extend(marker);
        assert_eq!(markers(&encode(&wrapped)).unwrap().len(), 2);
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn bounded_cue_parser_rejects_truncation() {
        let mut payload = vec![1, 1];
        payload.extend(b"CUE\0");
        let mut cue = vec![0, 3];
        cue.extend(1500u32.to_be_bytes());
        cue.extend([0, 255, 0, 0, 0, 0]);
        cue.extend(b"Hook\0");
        payload.extend((cue.len() as u32).to_be_bytes());
        payload.extend(&cue);
        payload.push(0);
        assert_eq!(
            parse_payload(&payload)
                .unwrap()
                .into_iter()
                .map(|c| (c.index, c.start, c.name))
                .collect::<Vec<_>>(),
            vec![(3, 1.5, "Hook".into())]
        );
        for n in 3..payload.len() - 1 {
            assert!(parse_payload(&payload[..n]).is_none());
        }
        assert!(parse_payload(&[1, 1, b'C', 0, 255, 255, 255, 255]).is_none());
        assert_eq!(decode(b"AQE="), Some(vec![1, 1]));
    }
}
