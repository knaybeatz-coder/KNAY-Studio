//! Offline reference-case runner. No editor or hardware audio access.
use os_host::Library;
use std::{path::Path, sync::Arc};
fn main() {
    let path = std::env::args().nth(1).expect("cases JSON path");
    let path = Path::new(&path);
    let root = path.parent().unwrap();
    let cases: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let lib: Arc<dyn Library> =
        os_vst3::Module::load(Path::new("C:/Program Files/Common Files/VST3/Serum2.vst3")).unwrap();
    let desc = lib
        .plugins()
        .iter()
        .find(|d| d.name == "Serum 2" && d.is_instrument())
        .unwrap()
        .clone();
    let mut results = Vec::new();
    for c in cases.as_array().unwrap() {
        let label = c["label"].as_str().unwrap();
        let rate = c["sample_rate"].as_u64().unwrap_or(48000) as u32;
        let frames = c["frames"].as_u64().unwrap() as usize;
        let note = c["note"].as_u64().unwrap_or(69) as u8;
        let shared = lib.clone().create(&desc.id).unwrap();
        let mut p = shared.lock().unwrap();
        p.set_offline(true);
        assert!(p.activate(rate));
        for (id, v) in [
            (1000000, 1.),
            (1001000, 0.),
            (1002000, 0.),
            (1003000, 0.),
            (1004000, 0.),
            (1000035, 0.),
            (1000038, 0.),
            (1000041, 0.),
            (1000042, 0.),
            (1000030, 0.),
            (1000025, 0.),
            (0, 0.25),
            (1000001, 0.75),
        ] {
            assert!(p.set_param(id, v));
        }
        let mut values = Vec::new();
        for item in c["parameters"].as_array().unwrap() {
            let id = item[0].as_u64().unwrap() as u32;
            let v = item[1].as_f64().unwrap();
            assert!(p.set_param(id, v));
            values.push(serde_json::json!({"id":id,"normalized":v,"display":p.param_text(id,v)}));
        }
        let (mut l, mut r) = ([0f32; 256], [0f32; 256]);
        for _ in 0..((rate * 2 + 255) / 256) {
            p.process(rate, &mut l, &mut r);
        }
        if c["save_state"].as_bool().unwrap_or(false) {
            std::fs::write(
                root.join(format!("{label}.state")),
                p.save_state().expect("state export"),
            )
            .unwrap();
        }
        p.reset();
        p.note_on(0, note, 0.8);
        let mut bytes = Vec::with_capacity(frames * 8);
        let mut done = 0;
        let mut peak = 0f32;
        while done < frames {
            let mut n = (frames - done).min(256);
            if let Some(step) = c.get("step") {
                let at = step["frame"].as_u64().unwrap() as usize;
                if done < at {
                    n = n.min(at - done);
                } else if done == at {
                    assert!(p.set_param(
                        step["id"].as_u64().unwrap() as u32,
                        step["value"].as_f64().unwrap()
                    ));
                }
            }
            if let Some(off) = c["off"].as_u64() {
                let at = off as usize;
                if done < at {
                    n = n.min(at - done);
                } else if done == at {
                    p.note_off(0, note);
                }
            }
            if let Some(on) = c["retrigger"].as_u64() {
                let at = on as usize;
                if done < at {
                    n = n.min(at - done);
                } else if done == at {
                    p.note_on(0, note, 0.8);
                }
            }
            p.process(rate, &mut l[..n], &mut r[..n]);
            for (&a, &b) in l[..n].iter().zip(&r[..n]) {
                assert!(a.is_finite() && b.is_finite());
                peak = peak.max(a.abs()).max(b.abs());
                bytes.extend(a.to_le_bytes());
                bytes.extend(b.to_le_bytes());
            }
            done += n;
        }
        p.note_off(0, note);
        let file = format!("{label}.f32");
        std::fs::write(root.join(&file), bytes).unwrap();
        results.push(serde_json::json!({"label":label,"parameters":values,"peak":peak,"frames":frames,"sample_rate":rate,"note":note,"file":file}));
        println!("Rendered {label}");
    }
    std::fs::write(
        root.join("render-results.json"),
        serde_json::to_vec_pretty(&results).unwrap(),
    )
    .unwrap();
}
