//! Metadata-only diagnostic: no editor, audio device, or ARA initialization.
fn main() {
    let path=std::env::args_os().nth(1).expect("usage: ara_discovery <plugin.vst3>");
    match os_vst3::Module::load(std::path::Path::new(&path)) {
        Ok(module)=>for p in module.plugins() {
            println!("{} {}: ARA factory {}",p.name,p.version,module.ara_factory_id(&p.id).unwrap_or("not advertised"));
        },
        Err(e)=>{eprintln!("{e}");std::process::exit(1);}
    }
}
