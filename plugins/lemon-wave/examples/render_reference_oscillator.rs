//! Offline synthetic reconstruction check; never opens an audio device.
use lemon_wave::reference_oscillator::{LevelSmoother, Oscillator, Table, Warp};
use std::{path::Path, sync::Arc};
fn main() {
    let path = std::env::args().nth(1).expect("cases JSON path");
    let path = Path::new(&path);
    let cases: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let output = path.parent().unwrap().join("reconstructed");
    std::fs::create_dir_all(&output).unwrap();
    let table = Arc::new(Table::init_saw());
    for case in cases.as_array().unwrap() {
        let label = case["label"].as_str().unwrap();
        let rate = case["sample_rate"].as_u64().unwrap_or(48000) as f32;
        let note = case["note"].as_u64().unwrap_or(69) as f64;
        let mut mode = 0;
        let mut amount = 0.;
        let mut level = 0.75;
        for item in case["parameters"].as_array().unwrap() {
            match item[0].as_u64().unwrap() {
                1000035 => mode = (item[1].as_f64().unwrap() * 69.).round() as u32,
                1000033 => amount = item[1].as_f64().unwrap() as f32,
                1000001 => level = item[1].as_f64().unwrap() as f32,
                _ => {}
            }
        }
        let warp = Warp::from_reference_id(mode).unwrap();
        let mut oscillator = Oscillator::new(table.clone(), rate);
        let mut smoother = LevelSmoother::new(rate, level);
        let mut bytes = Vec::new();
        let frequency = 440. * 2_f64.powf((note - 69.) / 12.);
        for frame in 0..case["frames"].as_u64().unwrap() {
            if let Some(step) = case.get("step") {
                if frame == step["frame"].as_u64().unwrap() {
                    assert_eq!(
                        step["id"].as_u64().unwrap(),
                        1000001,
                        "Only level steps supported"
                    );
                    level = step["value"].as_f64().unwrap() as f32;
                }
            }
            let value = oscillator.tick(frequency, warp, amount, smoother.tick(level))
                * std::f32::consts::SQRT_2
                * 0.25
                * 0.25;
            assert!(value.is_finite());
            bytes.extend(value.to_le_bytes());
        }
        std::fs::write(output.join(format!("{label}.f32")), bytes).unwrap();
        println!("Rendered {label}");
    }
}
