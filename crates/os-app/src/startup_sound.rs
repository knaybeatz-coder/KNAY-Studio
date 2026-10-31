//! Original, quiet completion chime. Independent of project routing and transport.
#[cfg(any(windows, test))]
const CHIME: &[u8] = include_bytes!("../assets/startup.wav");

pub fn play() {
    #[cfg(all(windows, not(test)))]
    if !crate::prefs::check(&crate::prefs::SILENT_STARTUP) {
        #[link(name = "winmm")]
        unsafe extern "system" {
            fn PlaySoundW(sound: *const u16, module: *mut core::ffi::c_void, flags: u32) -> i32;
        }
        // SND_MEMORY | SND_ASYNC | SND_NODEFAULT | SND_NOSTOP.
        // Static embedded bytes remain alive until asynchronous playback completes.
        // Do not interrupt another PlaySound notification, or beep on failure.
        unsafe { PlaySoundW(CHIME.as_ptr().cast(), core::ptr::null_mut(), 0x4 | 0x1 | 0x2 | 0x10); }
    }
}

#[cfg(test)]
mod tests {
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
