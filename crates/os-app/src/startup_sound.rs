//! Original, quiet completion chime. Independent of project routing and transport.
#[cfg(any(windows, test))]
const CHIME: &[u8] = include_bytes!("../assets/startup.wav");

pub const VINTAGE_SOUND: &str = "visible_vintage_startup_sound";

pub fn play_launch(_vintage: bool) {
    #[cfg(all(windows, not(test)))]
    play(_vintage, true);
}

pub fn play_vintage() {
    #[cfg(all(windows, not(test)))]
    play(true, false);
}

#[cfg(all(windows, not(test)))]
fn play(vintage: bool, launch: bool) {
    if crate::prefs::check(&crate::prefs::SILENT_STARTUP)
        || (vintage && !crate::workspace_visibility::shown(VINTAGE_SOUND))
    {
        return;
    }
    let original = vintage.then(vintage_wave).flatten();
    let Some(wave) = original.or_else(|| launch.then_some(CHIME)) else { return; };
    #[link(name = "winmm")]
    unsafe extern "system" {
        fn PlaySoundW(sound: *const u16, module: *mut core::ffi::c_void, flags: u32) -> i32;
    }
    // SND_MEMORY | SND_ASYNC | SND_NODEFAULT. Both sources live for the process.
    // The vintage cue can replace the regular chime if the theme is switched just after launch.
    let flags = 0x4 | 0x1 | 0x2 | if original.is_none() { 0x10 } else { 0 };
    unsafe { PlaySoundW(wave.as_ptr().cast(), core::ptr::null_mut(), flags); }
}

#[cfg(any(windows, test))]
fn vintage_wave() -> Option<&'static [u8]> {
    static WAVE: std::sync::OnceLock<Option<Vec<u8>>> = std::sync::OnceLock::new();
    WAVE.get_or_init(|| crate::vintage_skin::directories().into_iter().find_map(|dir| {
        // The original uses mu-law. Decode with the sample loader and pass
        // standard PCM to Windows, independent of installed audio codecs.
        let sample = os_engine::sample::load(&dir.join("StartSnd.wav")).ok()?;
        (sample.frames() > 0).then(|| crate::export::pcm16_stereo(&sample).ok()).flatten()
    })).as_deref()
}

#[cfg(test)]
mod tests {
    #[test]
    fn vintage_startup_decodes_local_fl_sound_as_pcm() {
        let available = crate::vintage_skin::directories().into_iter()
            .any(|dir| dir.join("StartSnd.wav").is_file());
        if available {
            let wave = super::vintage_wave().expect("decode installed FL startup WAV");
            assert_eq!(&wave[..4], b"RIFF");
            assert_eq!(&wave[20..22], &1u16.to_le_bytes());
            assert!(wave[44..].iter().any(|&v| v != 0));
        }
    }

    #[test]
    fn startup_chime_is_short_quiet_and_faded() {
        let w = super::CHIME;
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(&w[8..12], b"WAVE");
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), 44100);
        let samples: Vec<i16> = w[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        assert_eq!(samples.len(), 39690);
        assert!(samples.iter().any(|x| x.abs() > 3000));
        assert!(samples.iter().all(|x| x.abs() < 4000));
        assert_eq!(samples[0], 0);
        assert!(samples[samples.len()-100..].iter().all(|x| x.abs() < 10));
    }
}
