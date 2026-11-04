// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{assets::*, dsp, wavetable::*};
use nih_plug::prelude::Params;
use std::{f32::consts::TAU, sync::Arc};

fn wav(bits: u16, channels: u16, float: bool, samples: &[f32]) -> Vec<u8> {
    let mut data = Vec::new();
    for &x in samples {
        for _ in 0..channels {
            match (float, bits) {
                (true, 32) => data.extend(x.to_le_bytes()),
                (_, 8) => data.push((x * 127. + 128.) as u8),
                (_, 16) => data.extend(((x * 32767.) as i16).to_le_bytes()),
                (_, 24) => data.extend(&((x * 8388607.) as i32).to_le_bytes()[..3]),
                (_, 32) => data.extend(((x as f64 * 2147483647.) as i32).to_le_bytes()),
                _ => unreachable!(),
            }
        }
    }
    let align = channels * bits / 8;
    let mut out = b"RIFF".to_vec();
    out.extend((36 + data.len() as u32).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend((if float { 3u16 } else { 1u16 }).to_le_bytes());
    out.extend(channels.to_le_bytes());
    out.extend(48000u32.to_le_bytes());
    out.extend((48000 * align as u32).to_le_bytes());
    out.extend(align.to_le_bytes());
    out.extend(bits.to_le_bytes());
    out.extend(b"data");
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(data);
    out
}
fn sine(size: usize, cycles: usize) -> Vec<f32> {
    (0..size)
        .map(|i| 0.6 * (TAU * i as f32 * cycles as f32 / size as f32).sin())
        .collect()
}

#[test]
fn wav_formats_bounds_and_fourier_resampling() {
    for bits in [8, 16, 24, 32] {
        for ch in [1, 2] {
            for cycle in CYCLES {
                let x = sine(cycle, 1);
                let source = decode(&wav(bits, ch, false, &x), cycle, "test".into()).unwrap();
                let expected = sine(SIZE, 1);
                assert!(
                    source
                        .samples
                        .iter()
                        .zip(expected)
                        .all(|(a, b)| (a - b).abs() < 0.015),
                    "{bits}/{ch}/{cycle}"
                );
            }
        }
    }
    let mut input = sine(2048, 1);
    input.extend(sine(2048, 5));
    let bytes = wav(32, 2, true, &input);
    let source = decode(&bytes, 2048, "two frames".into()).unwrap();
    assert_eq!(source.frames(), 2);
    for n in 0..bytes.len() {
        assert!(decode(&bytes[..n], 2048, "bad".into()).is_err());
    }
    for x in [f32::NAN, f32::INFINITY, 100.] {
        assert!(decode(&wav(32, 1, true, &vec![x; 1024]), 1024, "bad".into()).is_err());
    }
    assert!(decode(&wav(16, 1, false, &vec![0.; 1024]), 1024, "silent".into()).is_err());
    assert!(decode(&wav(16, 1, false, &sine(1023, 1)), 1024, "partial".into()).is_err());
    assert!(
        decode(
            &wav(16, 1, false, &sine(64 * 129, 129)),
            64,
            "too many".into()
        )
        .is_err()
    );
    let high = sine(4096, 700);
    let source = decode(&wav(32, 1, true, &high), 4096, "high".into());
    if let Ok(source) = source {
        assert!(source.samples.iter().all(|x| x.abs() < 0.001));
    }
}

#[test]
fn frames_mips_and_embedded_recall_without_original_file() {
    let mut x = sine(2048, 1);
    x.extend(sine(2048, 5));
    let path = std::env::temp_dir().join(format!("lemon-wave-import-{}.wav", std::process::id()));
    std::fs::write(&path, wav(32, 1, true, &x)).unwrap();
    let source = import(&path, 2048).unwrap();
    std::fs::remove_file(&path).unwrap();
    let table = Table::compile(source.clone()).unwrap();
    for i in 0..100 {
        let phase = i as f32 / 100.;
        let a = table.sample(0., phase, 100., 48000.);
        let b = table.sample(1., phase, 100., 48000.);
        assert!((table.sample(0.5, phase, 100., 48000.) - (a + b) * 0.5).abs() < 1e-6);
        assert!(
            table.sample(1., phase, 10000., 48000.).abs() < 0.0001,
            "fifth harmonic must be removed"
        );
    }
    let params = WaveParams::default();
    params
        .replace(Sources {
            samples: Default::default(),
            oscillators: [Some(source.clone()), Some(source)],
        })
        .unwrap();
    let saved = params.serialize_fields();
    assert!(saved[FIELD].contains("samples"));
    let restored = WaveParams::default();
    restored.deserialize_fields(&saved);
    assert_eq!(
        params.sources().oscillators[0].as_ref().unwrap().samples,
        restored.sources().oscillators[0].as_ref().unwrap().samples
    );
    let render = |params: &WaveParams| {
        let mut e = dsp::Engine::new(48000.);
        e.custom = params.tables.read().unwrap().clone();
        e.on(60, 0.8, 0, None);
        let mut l = vec![0.; 4096];
        let mut r = l.clone();
        e.render(&mut l, &mut r);
        l
    };
    let before = render(&params);
    let after = render(&restored);
    assert_eq!(before, after);
    assert!(before.iter().any(|x| x.abs() > 0.001));
    let mut legacy = saved.clone();
    legacy.remove(FIELD);
    restored.deserialize_fields(&legacy);
    assert!(restored.sources().oscillators.iter().all(Option::is_none));
    assert_ne!(before, render(&restored));
    let retained = params.tables.read().unwrap().clone();
    params.replace(Sources::default()).unwrap();
    assert!(
        Arc::strong_count(&retained) > 1,
        "retired asset remains owned outside audio"
    );
    let mut invalid = saved;
    invalid.insert(
        FIELD.into(),
        "{\"oscillators\":[{\"name\":\"bad\",\"samples\":[1]},null]}".into(),
    );
    restored.deserialize_fields(&invalid);
    assert!(restored.status.lock().unwrap().contains("failed"));
}

#[test]
fn import_editor_layout_and_builtin_restore() {
    use lemon_small_fx::editor::{Btn, Extension, Logic, Mods};
    let params = Arc::new(WaveParams::default());
    let source = decode(
        &wav(32, 1, true, &sine(2048, 1)),
        2048,
        "Imported test table".into(),
    )
    .unwrap();
    params
        .replace(Sources {
            samples: Default::default(),
            oscillators: [Some(source), None],
        })
        .unwrap();
    let extension = Arc::new(crate::table_editor::TableEditor::new(params.clone()));
    let shared = Arc::new(lemon_small_fx::Shared {
        extension: Some(extension.clone()),
        ..Default::default()
    });
    let mut logic = Logic::new(crate::controls::SPEC, params.base.clone(), shared, None);
    for scale in [1., 1.5] {
        let mut c = os_ui::Canvas::new(1180, 820, scale).unwrap();
        logic.paint(&mut c, &os_plugkit::palette());
        for o in 0..2 {
            for r in crate::table_editor::TableEditor::buttons(o) {
                assert!(r.x >= 0. && r.right() <= 1180. && r.y >= 0. && r.bottom() <= 820.);
                assert!(!logic.hits.iter().any(|h| h.r.contains(r.cx(), r.cy())));
            }
        }
        if let Ok(dir) = std::env::var("LEMON_WAVE_PREVIEW") {
            std::fs::create_dir_all(&dir).unwrap();
            c.pixmap
                .save_png(format!("{dir}/wave-import-{scale}.png"))
                .unwrap();
        }
    }
    let r = crate::table_editor::TableEditor::buttons(0)[2];
    logic.down(r.cx(), r.cy(), Btn::Left, Mods::default(), false);
    assert!(params.sources().oscillators[0].is_none());
    assert!(!extension.click(1000., 782., None));
}

#[test]
fn recorded_samples_decode_embed_clear_and_render_consistently() {
    use crate::sample::Sample;
    let bytes = wav(16, 1, false, &sine(48000, 440));
    let sample = Sample::decode(&bytes, "A4 recorded".into(), 69.).unwrap();
    assert_eq!(sample.rate, 48000);
    for n in [0, 8, 36, bytes.len() - 1] {
        assert!(Sample::decode(&bytes[..n], "bad".into(), 60.).is_err());
    }
    let params = WaveParams::default();
    params
        .replace(Sources {
            samples: [Some(sample), None],
            ..Default::default()
        })
        .unwrap();
    let fields = params.serialize_fields();
    let restored = WaveParams::default();
    restored.deserialize_fields(&fields);
    let run = |block: usize| {
        let mut e = dsp::Engine::new(48000.);
        e.custom = restored.tables.read().unwrap().clone();
        e.on(69, 0.8, 0, None);
        let mut l = vec![0.; 8192];
        let mut r = l.clone();
        for (a, b) in l.chunks_mut(block).zip(r.chunks_mut(block)) {
            e.render(a, b);
        }
        l
    };
    let a = run(1);
    assert_eq!(a, run(127));
    assert!(a.iter().any(|x| x.abs() > 0.001));
    assert!(a.iter().all(|x| x.is_finite()));
    restored.deserialize_fields(&Default::default());
    assert!(restored.sources().samples[0].is_none());
}

#[test]
fn sound_browser_basic_choices_activate_and_save() {
    use lemon_small_fx::editor::Extension;
    let params = Arc::new(WaveParams::default());
    let editor = crate::table_editor::TableEditor::new(params.clone());
    for i in 0..6 {
        let r = crate::sound_browser::button(0);
        assert!(editor.click(r.cx(), r.cy(), None));
        assert!(editor.click(r.x + 20., r.bottom() + 16. + i as f32 * 26., None));
        let s = params.sources();
        assert_eq!(
            s.oscillators[0].as_ref().unwrap().name,
            crate::sound_browser::BASIC[i]
        );
        assert!(s.samples[0].is_none());
        assert_eq!(params.base.v(crate::controls::A_SOURCE), 0);
        let state = params.serialize_fields();
        let restored = WaveParams::default();
        restored.deserialize_fields(&state);
        assert_eq!(
            restored.sources().oscillators[0].as_ref().unwrap().name,
            crate::sound_browser::BASIC[i]
        );
    }
}

#[test]
fn blend_affects_classic_unison_and_single_voice_is_unchanged() {
    let render = |count: f32, blend: f32| {
        let mut e = dsp::Engine::new(48000.);
        let mut p = dsp::defaults();
        p[crate::controls::A_UNISON] = count;
        p[crate::controls::A_BLEND] = blend;
        e.set(p);
        e.reset();
        e.on(60, 0.8, 0, None);
        let mut l = vec![0.; 4096];
        let mut r = l.clone();
        e.render(&mut l, &mut r);
        l
    };
    assert_eq!(render(1., 0.), render(1., 1000.));
    let a = render(5., 0.);
    let b = render(5., 1000.);
    assert!(a.iter().zip(&b).any(|(a, b)| (a - b).abs() > 0.001));
    assert!(a.iter().chain(&b).all(|x| x.is_finite()));
}

#[test]
fn included_moog_dropdown_stays_in_editor_and_generic_selectors_work() {
    use lemon_small_fx::editor::{Btn, Extension, Logic, Mods};
    let params = Arc::new(WaveParams::default());
    let editor = Arc::new(crate::table_editor::TableEditor::new(params.clone()));
    let shared = Arc::new(lemon_small_fx::Shared {
        extension: Some(editor.clone()),
        ..Default::default()
    });
    let mut logic = Logic::new(crate::controls::SPEC, params.base.clone(), shared, None);
    let mut canvas = os_ui::Canvas::new(1180, 820, 1.).unwrap();
    logic.paint(&mut canvas, &os_plugkit::palette());
    // A parameter selector opens in-place, then commits the chosen value through existing host gestures.
    let selector = logic
        .hits
        .iter()
        .find(|h| h.t.id() == crate::controls::SETTINGS[crate::controls::FILTER].id)
        .unwrap()
        .r;
    logic.down(
        selector.cx(),
        selector.cy(),
        Btn::Left,
        Mods::default(),
        false,
    );
    editor.click(selector.x + 15., selector.bottom() + 16. + 2. * 26., None);
    assert_eq!(params.base.v(crate::controls::FILTER), 2);
    if crate::included_library::PACK.is_empty() {
        return;
    }
    let r = crate::sound_browser::button(0);
    editor.click(r.cx(), r.cy(), None);
    editor.click(r.x + 15., r.bottom() + 16. + 11. * 26., None);
    let r = crate::sound_browser::button(0);
    editor.click(r.x + 15., r.bottom() + 16., None);
    let r = crate::sound_browser::button(0);
    editor.click(r.x + 15., r.bottom() + 16., None);
    assert!(
        params.sources().samples[0]
            .as_ref()
            .unwrap()
            .name
            .starts_with("Mini-Moog /")
    );
    logic.paint(&mut canvas, &os_plugkit::palette());
    if let Ok(dir) = std::env::var("LEMON_WAVE_PREVIEW") {
        canvas
            .pixmap
            .save_png(format!("{dir}/included-moog.png"))
            .unwrap();
    }
    editor.click(r.cx(), r.cy(), None);
    logic.paint(&mut canvas, &os_plugkit::palette());
    assert!(logic.hits.is_empty());
    if let Ok(dir) = std::env::var("LEMON_WAVE_PREVIEW") {
        canvas
            .pixmap
            .save_png(format!("{dir}/inline-samples-dropdown.png"))
            .unwrap();
    }
    editor.click(1170., 790., None);
    logic.paint(&mut canvas, &os_plugkit::palette());
    assert!(!logic.hits.is_empty());
}

#[test]
fn workshop_draft_apply_and_embedded_conversion_recall() {
    use lemon_small_fx::editor::{Btn, Extension, Logic, Mods};
    let params = Arc::new(WaveParams::default());
    let editor = Arc::new(crate::table_editor::TableEditor::new(params.clone()));
    let shared = Arc::new(lemon_small_fx::Shared {
        extension: Some(editor.clone()),
        ..Default::default()
    });
    let mut logic = Logic::new(crate::controls::SPEC, params.base.clone(), shared, None);
    let mut c = os_ui::Canvas::new(1180, 820, 1.).unwrap();
    logic.paint(&mut c, &os_plugkit::palette());
    let r = crate::instrument_view::edit_table_button(0);
    logic.down(r.cx(), r.cy(), Btn::Left, Mods::default(), false);
    assert_eq!(params.page.load(std::sync::atomic::Ordering::Relaxed), 2);
    let before = params.serialize_fields();
    let r = crate::wavetable_editor::DRAW;
    editor.click(r.x + 1., r.cy(), None);
    assert!(editor.dragging());
    editor.moved(r.cx(), r.y + 8.);
    editor.release(None);
    assert_eq!(
        params.serialize_fields(),
        before,
        "draft does not replace playing sound"
    );
    editor.click(800., 628., None);
    editor.character('\u{1}');
    for ch in "Workshop Test".chars() {
        editor.character(ch);
    }
    editor.character('\r');
    editor.click(400., 108., None);
    let applied = params.sources().oscillators[0].clone().unwrap();
    assert_eq!(applied.name, "Workshop Test");
    assert_ne!(params.serialize_fields(), before);
    let saved = params.serialize_fields();
    let recalled = WaveParams::default();
    recalled.deserialize_fields(&saved);
    assert_eq!(
        recalled.sources().oscillators[0].as_ref().unwrap().samples,
        applied.samples
    );
    let sound = |p: &WaveParams| {
        let mut e = dsp::Engine::new(48000.);
        e.custom = p.tables.read().unwrap().clone();
        e.on(60, 0.8, 0, None);
        let mut a = vec![0.; 4096];
        let mut b = a.clone();
        e.render(&mut a, &mut b);
        a
    };
    assert_eq!(sound(&params), sound(&recalled));
    assert!(sound(&params).iter().any(|x| x.abs() > 0.001));
    if !crate::included_library::PACK.is_empty() {
        let sample = crate::included_library::load(0).unwrap();
        params
            .replace(Sources {
                samples: [Some(sample), None],
                ..Default::default()
            })
            .unwrap();
        editor.click(260., 108., None);
        editor.click(90., 692., None);
        editor.click(400., 108., None);
        assert_eq!(
            params.sources().oscillators[0].as_ref().unwrap().frames(),
            16
        );
        assert!(params.sources().samples[0].is_none());
    }
    for scale in [1., 1.5] {
        let mut c = os_ui::Canvas::new(1180, 820, scale).unwrap();
        logic.paint(&mut c, &os_plugkit::palette());
        assert!(logic.hits.is_empty());
        if let Ok(dir) = std::env::var("LEMON_WAVE_PREVIEW") {
            std::fs::create_dir_all(&dir).unwrap();
            c.pixmap
                .save_png(format!("{dir}/complete-workshop-{scale}.png"))
                .unwrap();
        }
    }
}

#[test]
fn oscillator_channel_faders_follow_vertical_travel() {
    use lemon_small_fx::editor::{Btn, Logic, Mods};
    use lemon_small_fx::view::Target;
    let params=Arc::new(WaveParams::default());
    let editor=Arc::new(crate::table_editor::TableEditor::new(params.clone()));
    let shared=Arc::new(lemon_small_fx::Shared { extension:Some(editor), ..Default::default() });
    let mut logic=Logic::new(crate::controls::SPEC,params.base.clone(),shared,None);
    let mut canvas=os_ui::Canvas::new(1180,820,1.).unwrap();
    logic.paint(&mut canvas,&os_plugkit::palette());
    for (level,pan) in [(crate::controls::A_LEVEL,crate::controls::A_PAN),(crate::controls::B_LEVEL,crate::controls::B_PAN)] {
        let id=crate::controls::SETTINGS[level].id;
        let h=logic.hits.iter().find(|h|h.t.id()==id).unwrap().clone();
        let Target::Fader(_,travel)=h.t else {panic!("level must be a fader")};
        let p=logic.hits.iter().find(|h|h.t.id()==crate::controls::SETTINGS[pan].id).unwrap();
        assert!(p.r.bottom()<h.r.y);
        logic.down(h.r.cx(),h.r.cy(),Btn::Left,Mods::default(),false);
        logic.delta(0.,-travel,Mods {shift:true,..Mods::default()});
        logic.up(Btn::Left);
        assert_eq!(params.base.v(level),crate::controls::SETTINGS[level].range.1);
        logic.down(h.r.cx(),h.r.cy(),Btn::Left,Mods::default(),false);
        logic.delta(0.,travel,Mods {shift:true,..Mods::default()});
        logic.up(Btn::Left);
        assert_eq!(params.base.v(level),crate::controls::SETTINGS[level].range.0);
    }
}
