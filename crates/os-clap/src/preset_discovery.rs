// SPDX-License-Identifier: GPL-3.0-or-later
//! CLAP provider locations, filtered to presets explicitly advertised for this plugin.
use super::*;
use clap_sys::ext::preset_load::*;
use clap_sys::factory::preset_discovery::*;
use clap_sys::universal_plugin_id::clap_universal_plugin_id;
type Indexer = clap_preset_discovery_indexer;
type Receiver = clap_preset_discovery_metadata_receiver;

pub(super) struct Preset {
    pub name: String,
    key: Option<CString>,
    path: Option<CString>,
}
struct Bank<'a> {
    target: &'a str,
    path: Option<CString>,
    entries: Vec<(Preset, bool)>,
}

#[derive(Default)]
struct Locations {
    embedded: bool,
    paths: Vec<PathBuf>,
    extensions: Vec<String>,
}
unsafe extern "C" fn location(
    i: *const Indexer,
    loc: *const clap_preset_discovery_location,
) -> bool {
    unsafe {
        let Some(loc) = loc.as_ref() else {
            return false;
        };
        let declared = &mut *(*i).indexer_data.cast::<Locations>();
        match loc.kind {
            CLAP_PRESET_DISCOVERY_LOCATION_PLUGIN => declared.embedded = true,
            CLAP_PRESET_DISCOVERY_LOCATION_FILE if !loc.location.is_null() => {
                let path = PathBuf::from(cstr(loc.location));
                if !path.is_absolute() {
                    return false;
                }
                declared.paths.push(path);
            }
            _ => return false,
        }
    }
    true
}
unsafe extern "C" fn filetype(
    i: *const Indexer,
    ft: *const clap_preset_discovery_filetype,
) -> bool {
    unsafe {
        let Some(ft) = ft.as_ref() else { return false };
        let ext = cstr(ft.file_extension);
        if ext.is_empty() {
            return false;
        }
        (*(*i).indexer_data.cast::<Locations>())
            .extensions
            .push(ext);
    }
    true
}
impl Locations {
    fn files(&self) -> std::collections::BTreeSet<PathBuf> {
        let mut files = std::collections::BTreeSet::new();
        let mut pending = self.paths.clone();
        let mut seen = std::collections::HashSet::new();
        while let Some(path) = pending.pop() {
            if !seen.insert(path.clone()) || seen.len() > 65536 {
                continue;
            }
            let Ok(meta) = path.symlink_metadata() else {
                continue;
            };
            if meta.is_dir() {
                if let Ok(children) = path.read_dir() {
                    pending.extend(children.flatten().map(|entry| entry.path()));
                }
            } else if meta.is_file()
                && (self.paths.contains(&path)
                    || path
                        .extension()
                        .and_then(|x| x.to_str())
                        .is_some_and(|ext| {
                            self.extensions
                                .iter()
                                .any(|known| ext.eq_ignore_ascii_case(known))
                        }))
            {
                files.insert(path);
            }
        }
        files
    }
}
unsafe extern "C" fn pack(_: *const Indexer, _: *const clap_preset_discovery_soundpack) -> bool {
    true
}
unsafe extern "C" fn extension(_: *const Indexer, _: *const c_char) -> *const c_void {
    ptr::null()
}
unsafe extern "C" fn begin(r: *const Receiver, name: *const c_char, key: *const c_char) -> bool {
    unsafe {
        let bank = &mut *(*r).receiver_data.cast::<Bank>();
        if bank.entries.len() >= 16384 {
            return false;
        }
        bank.entries.push((
            Preset {
                name: if name.is_null() {
                    bank.path
                        .as_ref()
                        .and_then(|p| Path::new(p.to_str().ok()?).file_stem()?.to_str())
                        .unwrap_or("Preset")
                        .to_owned()
                } else {
                    cstr(name)
                },
                path: bank.path.clone(),
                key: (!key.is_null()).then(|| CStr::from_ptr(key).to_owned()),
            },
            false,
        ));
    }
    true
}
unsafe extern "C" fn plugin_id(r: *const Receiver, p: *const clap_universal_plugin_id) {
    unsafe {
        let bank = &mut *(*r).receiver_data.cast::<Bank>();
        if !p.is_null() && cstr((*p).abi) == "clap" && cstr((*p).id) == bank.target {
            if let Some((_, matched)) = bank.entries.last_mut() {
                *matched = true;
            }
        }
    }
}
unsafe extern "C" fn string(_: *const Receiver, _: *const c_char) {}
unsafe extern "C" fn flags(_: *const Receiver, _: u32) {}
unsafe extern "C" fn times(_: *const Receiver, _: u64, _: u64) {}
unsafe extern "C" fn extra(_: *const Receiver, _: *const c_char, _: *const c_char) {}
unsafe extern "C" fn error(_: *const Receiver, _: i32, _: *const c_char) {}
fn loader(plugin: &Plugin) -> Option<&clap_plugin_preset_load> {
    unsafe {
        let get = (*plugin.plugin).get_extension?;
        [CLAP_EXT_PRESET_LOAD, CLAP_EXT_PRESET_LOAD_COMPAT]
            .into_iter()
            .find_map(|id| {
                get(plugin.plugin, id.as_ptr())
                    .cast::<clap_plugin_preset_load>()
                    .as_ref()
            })
    }
}
pub(super) fn load(plugin: &Plugin, p: &Preset) -> bool {
    loader(plugin)
        .and_then(|ext| ext.from_location)
        .is_some_and(|f| unsafe {
            f(
                plugin.plugin,
                if p.path.is_some() {
                    CLAP_PRESET_DISCOVERY_LOCATION_FILE
                } else {
                    CLAP_PRESET_DISCOVERY_LOCATION_PLUGIN
                },
                p.path.as_ref().map_or(ptr::null(), |path| path.as_ptr()),
                p.key.as_ref().map_or(ptr::null(), |k| k.as_ptr()),
            )
        })
}
pub(super) fn discover(plugin: &Plugin) -> Vec<Preset> {
    if loader(plugin).is_none() {
        return Vec::new();
    }
    let mut bank = Bank {
        target: &plugin.desc.id,
        path: None,
        entries: Vec::new(),
    };
    let receiver = Receiver {
        receiver_data: (&raw mut bank).cast(),
        on_error: Some(error),
        begin_preset: Some(begin),
        add_plugin_id: Some(plugin_id),
        set_soundpack_id: Some(string),
        set_flags: Some(flags),
        add_creator: Some(string),
        set_description: Some(string),
        set_timestamps: Some(times),
        add_feature: Some(string),
        add_extra_info: Some(extra),
    };
    unsafe {
        let Some(get) = (*plugin._lib.entry).get_factory else {
            return Vec::new();
        };
        let Some(factory) = [
            CLAP_PRESET_DISCOVERY_FACTORY_ID,
            CLAP_PRESET_DISCOVERY_FACTORY_ID_COMPAT,
        ]
        .into_iter()
        .find_map(|id| {
            get(id.as_ptr())
                .cast::<clap_preset_discovery_factory>()
                .as_ref()
        }) else {
            return Vec::new();
        };
        let (Some(count), Some(descriptor), Some(create)) =
            (factory.count, factory.get_descriptor, factory.create)
        else {
            return Vec::new();
        };
        for i in 0..count(factory).min(1024) {
            let Some(desc) = descriptor(factory, i).as_ref() else {
                continue;
            };
            let mut declared = Locations::default();
            let indexer = Indexer {
                clap_version: CLAP_VERSION,
                name: c"Lemon Studio".as_ptr(),
                vendor: c"Lemon Studio".as_ptr(),
                url: c"".as_ptr(),
                version: c"1".as_ptr(),
                indexer_data: (&raw mut declared).cast(),
                declare_filetype: Some(filetype),
                declare_location: Some(location),
                declare_soundpack: Some(pack),
                get_extension: Some(extension),
            };
            let Some(provider) = create(factory, &indexer, desc.id).as_ref() else {
                continue;
            };
            if provider.init.is_some_and(|f| f(provider))
                && let Some(metadata) = provider.get_metadata
            {
                bank.path = None;
                if declared.embedded {
                    metadata(
                        provider,
                        CLAP_PRESET_DISCOVERY_LOCATION_PLUGIN,
                        ptr::null(),
                        &receiver,
                    );
                }
                for path in declared.files() {
                    let Ok(location) = CString::new(path.to_string_lossy().as_bytes()) else {
                        continue;
                    };
                    bank.path = Some(location.clone());
                    metadata(
                        provider,
                        CLAP_PRESET_DISCOVERY_LOCATION_FILE,
                        location.as_ptr(),
                        &receiver,
                    );
                }
            }
            if let Some(destroy) = provider.destroy {
                destroy(provider);
            }
        }
    }
    bank.entries
        .into_iter()
        .filter_map(|(preset, matched)| matched.then_some(preset))
        .collect()
}
