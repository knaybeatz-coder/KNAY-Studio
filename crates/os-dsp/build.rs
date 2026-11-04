fn main() {
 let dir="vendor/snaptune";
 println!("cargo:rerun-if-changed={dir}");
 cc::Build::new().cpp(true).std("c++17").include(dir)
 .file(format!("{dir}/AutoTuneEngine.cpp"))
 .file(format!("{dir}/PitchDetector.cpp"))
 .file(format!("{dir}/bridge.cpp"))
 .flag_if_supported("/EHsc").compile("lemon_snaptune");
 // KNAY VariAudio: Signalsmith Stretch (MIT), the pitch / formant engine Giannitune uses.
 let ss="vendor/signalsmith";
 println!("cargo:rerun-if-changed={ss}");
 cc::Build::new().cpp(true).std("c++17").include(ss)
 .file(format!("{ss}/bridge.cpp"))
 .flag_if_supported("/EHsc").flag_if_supported("/bigobj").compile("knay_signalsmith");
}
