//! Read-only installed Serum parameter display comparison. No audio device or GUI.
use os_host::Library;
fn main() {
    let lib = os_vst3::Module::load(std::path::Path::new(
        "C:/Program Files/Common Files/VST3/Serum2.vst3",
    ))
    .unwrap();
    let d = lib
        .plugins()
        .iter()
        .find(|d| d.name == "Serum 2")
        .unwrap()
        .clone();
    let instance = lib.create(&d.id).unwrap();
    let mut p = instance.lock().unwrap();
    p.set_offline(true);
    assert!(p.activate(48000));
    let mut result = Vec::new();
    for lfo in 0..4 {
        for v in [0., 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.] {
            let id = 4000000 + lfo * 1000;
            assert!(p.set_param(id, v));
            result.push(serde_json::json!({"lfo":lfo+1,"id":id,"normalized":v,"display":p.param_text(id,v)}));
        }
    }
    std::fs::write(
        std::env::args().nth(1).unwrap(),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    println!("Read 36 reference LFO tempo-rate displays; no modulated-audio equivalence claimed.");
}
