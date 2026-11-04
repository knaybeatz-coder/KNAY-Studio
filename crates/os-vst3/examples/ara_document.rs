//! Synthetic, headless ARA lifecycle test. No device, editor, microphone or user project.
fn main() -> Result<(), String> {
    let path = std::env::args_os().nth(1).ok_or("plugin path required")?;
    let module = os_vst3::Module::load(std::path::Path::new(&path))?;
    let id = &module
        .plugins()
        .iter()
        .find(|p| module.ara_factory_id(&p.id).is_some())
        .ok_or("no ARA plugin")?
        .id;
    let temp = std::env::temp_dir().join(format!("lemon-ara-synthetic-{}.wav", std::process::id()));
    struct TemporaryAudio(std::path::PathBuf);
    impl Drop for TemporaryAudio {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _cleanup = TemporaryAudio(temp.clone());
    let rate = 44100;
    let audio: Vec<f32> = (0..rate * 2)
        .flat_map(|i| {
            let v = (i as f32 * 440. * std::f32::consts::TAU / rate as f32).sin() * 0.1;
            [v, v]
        })
        .collect();
    os_engine::wav::write_f32_stereo(
        &mut std::fs::File::create(&temp).map_err(|e| e.to_string())?,
        rate,
        &audio,
    )
    .map_err(|e| e.to_string())?;
    let sample = os_engine::sample::load_shared(&temp).map_err(|e| e.to_string())?;
    println!("Creating and binding ARA playback/editor roles");
    let mut plugin = os_vst3::Plugin::new_ara(&module, id, sample.clone(), None)?;
    println!(
        "Bound document; licensed: {:?}",
        plugin.ara_document().unwrap().licensed()
    );
    if !plugin.activate(rate) {
        return Err("Could not activate ARA renderer".into());
    }
    for _ in 0..40 {
        plugin.idle();
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::*;
            let mut message = std::mem::zeroed();
            while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let mut left = vec![0.; rate as usize];
    let mut right = left.clone();
    plugin.render_ara_at(rate, 0, &mut left, &mut right)?;
    let peak = left.iter().chain(&right).fold(0f32, |v, x| v.max(x.abs()));
    println!("Rendered peak {peak}");
    if !peak.is_finite() || peak < 0.001 {
        return Err("ARA playback is silent or invalid".into());
    }
    println!("Archiving bound document");
    let archive = plugin.ara_document().unwrap().archive()?;
    println!("Archived {} bytes", archive.len());
    drop(plugin);
    let mut restored = os_vst3::Plugin::new_ara(&module, id, sample, Some(&archive))?;
    if !restored.activate(rate) {
        return Err("Could not activate restored ARA renderer".into());
    }
    restored.render_ara_at(rate, 0, &mut left, &mut right)?;
    let restored_peak = left.iter().chain(&right).fold(0f32, |v, x| v.max(x.abs()));
    println!("Restored peak {restored_peak}");
    drop(restored);
    println!("Teardown complete");
    if !restored_peak.is_finite() || restored_peak < 0.001 {
        return Err("Restored ARA playback is silent or invalid".into());
    }
    Ok(())
}
