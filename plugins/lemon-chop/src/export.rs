// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline pad rendering uses the same voice as live playback, never a raw source copy.
use crate::{audio::Audio, dsp::Voice, params::*};
use std::{
    io::{Seek, SeekFrom, Write},
    sync::Arc,
};
pub fn render<W: Write + Seek>(
    out: &mut W,
    a: Arc<Audio>,
    d: &Document,
    values: &[f32],
    tempo: f64,
    cancel: &dyn Fn() -> bool,
) -> Result<u64, String> {
    let cue = &d.cues[d.selected];
    if !cue.enabled || cue.end <= cue.start {
        return Err("Select a populated pad to export".into());
    }
    let rate = a.source.rate;
    let mut voice = Voice::new(rate);
    let values = values.to_vec(); // Hold exports act as a held note; Trigger retains its envelope.
    let note = if values[KEYBOARD] > 0.5 {
        60
    } else {
        36 + d.selected as u8
    };
    voice.start(a, d, &values, d.selected, note, 0, 1., tempo, None);
    let gains = std::array::from_fn(|i| values[12 + i]);
    let master = 10f32.powf(values[MASTER] / 20.);
    os_engine::wav::write_f32_stereo_header(out, rate, 0).map_err(|e| e.to_string())?;
    let mut count = 0u64;
    while voice.active {
        if count % 4096 == 0 && cancel() {
            return Err("Export cancelled".into());
        }
        if count >= (u32::MAX as u64 - 64) / 8 {
            return Err("Rendered pad exceeds the WAV size limit".into());
        }
        for v in voice.tick(gains) {
            out.write_all(&(v * master).to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        count += 1;
    }
    out.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    os_engine::wav::write_f32_stereo_header(out, rate, count).map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())?;
    Ok(count)
}

/// Atomically replace the chosen destination only after a successful complete render.
pub fn commit(temp: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    #[cfg(not(windows))]
    {
        std::fs::rename(temp, dest)
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };
        let a: Vec<_> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<_> = dest.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: both UTF-16 paths are NUL-terminated and remain alive during the call.
        if unsafe {
            MoveFileExW(
                a.as_ptr(),
                b.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn completed_export_replaces_destination_atomically() {
        let dir = std::env::temp_dir();
        let a = dir.join(format!("chop-export-{}.tmp", std::process::id()));
        let b = a.with_extension("wav");
        std::fs::write(&a, b"new complete wave").unwrap();
        std::fs::write(&b, b"old wave").unwrap();
        super::commit(&a, &b).unwrap();
        assert!(!a.exists());
        assert_eq!(std::fs::read(&b).unwrap(), b"new complete wave");
        assert!(super::commit(&a, &b).is_err());
        assert_eq!(std::fs::read(&b).unwrap(), b"new complete wave");
        let _ = std::fs::remove_file(b);
    }
}
