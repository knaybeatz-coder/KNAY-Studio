// SPDX-License-Identifier: GPL-3.0-or-later
//! ARA 2 document hosting. UI-thread lifecycle; immutable sample readers may run on analysis threads.
#![allow(non_snake_case)]
use crate::Module;
use std::{
    collections::HashMap,
    ffi::{c_void, CStr, CString},
    mem::size_of,
    ptr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
};
use vst3_sys as vst3_com;
use vst3_sys::{
    base::{IPluginFactory, IUnknown},
    com_interface, VstPtr,
};
#[allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code
)]
pub mod raw {
    include!(concat!(env!("OUT_DIR"), "/ara.rs"));
}
use raw::*;
#[com_interface("DB2A1669-FAFD-42A5-A82F-864F7B6872EA")]
pub trait IMainFactory: IUnknown {
    unsafe fn get_factory(&self) -> *const ARAFactory;
}
#[com_interface("CD9A5913-C9EB-46D7-96CA-53ADD1DB89F5")]
pub trait IEntry2: IUnknown {
    unsafe fn bind(
        &self,
        doc: ARADocumentControllerRef,
        known: u32,
        assigned: u32,
    ) -> *const ARAPlugInExtensionInstance;
}
/// Only copy the prefix actually advertised by a versioned structure.
unsafe fn sized<T: Default>(p: *const T) -> Result<T, String> {
    if p.is_null() {
        return Err("ARA returned a null interface".into());
    }
    let n = ptr::read_unaligned(p.cast::<usize>());
    if n < size_of::<usize>() {
        return Err("ARA interface is truncated".into());
    }
    let mut out = T::default();
    ptr::copy_nonoverlapping(
        p.cast::<u8>(),
        (&mut out as *mut T).cast(),
        n.min(size_of::<T>()),
    );
    Ok(out)
}
fn text(p: *const i8) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}
struct Factory {
    api: ARAFactory,
    _owner: VstPtr<dyn IMainFactory>,
    _module: Arc<Module>,
}
// Factory lifecycle and model access occur on the UI thread. The registry is shared only
// to retain one initialized factory across documents; audio threads never mutate it.
unsafe impl Send for Factory {}
unsafe impl Sync for Factory {}
impl Drop for Factory {
    fn drop(&mut self) {
        unsafe {
            if let Some(f) = self.api.uninitializeARA {
                f();
            }
        }
    }
}
fn factory(module: &Arc<Module>, id: &str) -> Result<Arc<Factory>, String> {
    static LIVE: OnceLock<Mutex<HashMap<(std::path::PathBuf, String), Weak<Factory>>>> =
        OnceLock::new();
    let factory_id = module
        .ara_factory_id(id)
        .ok_or("Plugin does not advertise an unambiguous ARA factory")?;
    let key = (
        module
            .path()
            .canonicalize()
            .unwrap_or_else(|_| module.path().to_owned()),
        factory_id.to_owned(),
    );
    let mut live = LIVE
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "ARA factory registry unavailable")?;
    if let Some(f) = live.get(&key).and_then(Weak::upgrade) {
        return Ok(f);
    }
    unsafe {
        let mut cid = None;
        for i in 0..module.factory().count_classes() {
            let mut ci = std::mem::zeroed();
            if module.factory().get_class_info(i, &mut ci) == 0
                && crate::fuid_string(&ci.cid) == factory_id
            {
                cid = Some(ci.cid);
                break;
            }
        }
        let owner = module
            .create::<dyn IMainFactory>(&cid.ok_or("ARA class disappeared")?)
            .ok_or("ARA factory interface unavailable")?;
        let api = sized(owner.get_factory())?;
        if api.lowestSupportedApiGeneration > 4 || api.highestSupportedApiGeneration < 4 {
            return Err("Plugin does not support ARA 2.0 final".into());
        }
        if { api.uninitializeARA }.is_none() || { api.createDocumentControllerWithDocument }
            .is_none()
        {
            return Err("Incomplete ARA factory".into());
        }
        let config = ARAInterfaceConfiguration {
            structSize: size_of::<ARAInterfaceConfiguration>(),
            desiredApiGeneration: 4,
            assertFunctionAddress: ptr::null_mut(),
        };
        api.initializeARAWithConfiguration
            .ok_or("ARA initialization unavailable")?(&config);
        let f = Arc::new(Factory {
            api,
            _owner: owner,
            _module: module.clone(),
        });
        live.retain(|_, v| v.strong_count() > 0);
        live.insert(key, Arc::downgrade(&f));
        Ok(f)
    }
}
struct Host {
    sample: Arc<os_engine::sample::Sample>,
    fingerprint: u64,
    archive: Mutex<Vec<u8>>,
    archive_id: CString,
    failed: AtomicBool,
    changed: AtomicBool,
    audio: ARAAudioAccessControllerInterface,
    archiving: ARAArchivingControllerInterface,
    updates: ARAModelUpdateControllerInterface,
    playback: ARAPlaybackControllerInterface,
    playback_requests: Mutex<Vec<os_host::PlaybackRequest>>,
    cycle: Mutex<(Option<(f64,f64)>,bool)>,
}
// Queue host commands rather than reentering the app while the plugin mutex is held.
fn enqueue_playback(h: ARAPlaybackControllerHostRef, request: os_host::PlaybackRequest) {
    if h.is_null() {return;}
    let host=unsafe {&*h.cast::<Host>()};
    if let Ok(mut queue)=host.playback_requests.lock() {
        if queue.len()==32 {queue.remove(0);}
        queue.push(request);
    }
}
unsafe extern "C" fn playback_start(h: ARAPlaybackControllerHostRef) {enqueue_playback(h,os_host::PlaybackRequest::Start);}
unsafe extern "C" fn playback_stop(h: ARAPlaybackControllerHostRef) {enqueue_playback(h,os_host::PlaybackRequest::Stop);}
unsafe extern "C" fn playback_seek(h: ARAPlaybackControllerHostRef, seconds:f64) {
    if seconds.is_finite() {enqueue_playback(h,os_host::PlaybackRequest::Seek(seconds));}
}
unsafe extern "C" fn playback_cycle(h: ARAPlaybackControllerHostRef, start:f64, duration:f64) {
    if h.is_null()||!start.is_finite()||!duration.is_finite()||duration<=0.||!(start+duration).is_finite(){return;}
    if let Ok(mut cycle)=(&*h.cast::<Host>()).cycle.lock(){
        cycle.0=Some((start,start+duration));
        if cycle.1 {enqueue_playback(h,os_host::PlaybackRequest::Cycle(cycle.0));}
    }
}
unsafe extern "C" fn playback_cycle_enable(h: ARAPlaybackControllerHostRef, enable:ARABool) {
    if h.is_null(){return;}
    if let Ok(mut cycle)=(&*h.cast::<Host>()).cycle.lock(){
        cycle.1=enable!=0;
        enqueue_playback(h,os_host::PlaybackRequest::Cycle(if cycle.1 {cycle.0}else{None}));
    }
}
fn playback_interface()->ARAPlaybackControllerInterface {
    ARAPlaybackControllerInterface{structSize:size_of::<ARAPlaybackControllerInterface>(),requestStartPlayback:Some(playback_start),requestStopPlayback:Some(playback_stop),requestSetPlaybackPosition:Some(playback_seek),requestSetCycleRange:Some(playback_cycle),requestEnableCycle:Some(playback_cycle_enable)}
}

struct Reader {
    sample: Arc<os_engine::sample::Sample>,
    double: bool,
}
unsafe extern "C" fn reader(
    h: ARAAudioAccessControllerHostRef,
    _: ARAAudioSourceHostRef,
    double: ARABool,
) -> ARAAudioReaderHostRef {
    if h.is_null() {
        return ptr::null_mut();
    }
    let host = &*h.cast::<Host>();
    Box::into_raw(Box::new(Reader {
        sample: host.sample.clone(),
        double: double != 0,
    }))
    .cast()
}
unsafe extern "C" fn read(
    _: ARAAudioAccessControllerHostRef,
    r: ARAAudioReaderHostRef,
    pos: i64,
    count: i64,
    b: *const *mut c_void,
) -> ARABool {
    if r.is_null() || b.is_null() || count < 0 || count > i32::MAX as i64 {
        return 0;
    }
    let r = &*r.cast::<Reader>();
    for (ch, samples) in [&r.sample.left, &r.sample.right].into_iter().enumerate() {
        let dst = *b.add(ch);
        if dst.is_null() {
            return 0;
        }
        for i in 0..count as usize {
            let v = pos
                .checked_add(i as i64)
                .and_then(|p| usize::try_from(p).ok())
                .and_then(|p| samples.get(p))
                .copied()
                .unwrap_or(0.);
            if r.double {
                *dst.cast::<f64>().add(i) = v as f64;
            } else {
                *dst.cast::<f32>().add(i) = v;
            }
        }
    }
    1
}
unsafe extern "C" fn destroy_reader(_: ARAAudioAccessControllerHostRef, r: ARAAudioReaderHostRef) {
    if !r.is_null() {
        drop(Box::from_raw(r.cast::<Reader>()));
    }
}
unsafe extern "C" fn archive_size(
    h: ARAArchivingControllerHostRef,
    _: ARAArchiveReaderHostRef,
) -> usize {
    (&*h.cast::<Host>()).archive.lock().map_or(0, |v| v.len())
}
unsafe extern "C" fn archive_read(
    h: ARAArchivingControllerHostRef,
    _: ARAArchiveReaderHostRef,
    pos: usize,
    n: usize,
    b: *mut u8,
) -> ARABool {
    let host = &*h.cast::<Host>();
    let Ok(v) = host.archive.lock() else {
        host.failed.store(true, Ordering::Relaxed);
        return 0;
    };
    let Some(end) = pos.checked_add(n) else {
        host.failed.store(true, Ordering::Relaxed);
        return 0;
    };
    if end > v.len() || (n > 0 && b.is_null()) {
        host.failed.store(true, Ordering::Relaxed);
        return 0;
    }
    if n > 0 {
        ptr::copy_nonoverlapping(v.as_ptr().add(pos), b, n);
    }
    1
}
unsafe extern "C" fn archive_write(
    h: ARAArchivingControllerHostRef,
    _: ARAArchiveWriterHostRef,
    pos: usize,
    n: usize,
    b: *const u8,
) -> ARABool {
    let host = &*h.cast::<Host>();
    let Some(end) = pos.checked_add(n).filter(|&end| end <= 512 * 1024 * 1024) else {
        host.failed.store(true, Ordering::Relaxed);
        return 0;
    };
    if n > 0 && b.is_null() {
        host.failed.store(true, Ordering::Relaxed);
        return 0;
    }
    let Ok(mut v) = host.archive.lock() else {
        return 0;
    };
    if end > v.len() {
        let extra = end - v.len();
        if v.try_reserve(extra).is_err() {
            host.failed.store(true, Ordering::Relaxed);
            return 0;
        }
        v.resize(end, 0);
    }
    if n > 0 {
        ptr::copy_nonoverlapping(b, v.as_mut_ptr().add(pos), n);
    }
    1
}
unsafe extern "C" fn archive_progress(_: ARAArchivingControllerHostRef, _: f32) {}
unsafe extern "C" fn archive_id(
    h: ARAArchivingControllerHostRef,
    _: ARAArchiveReaderHostRef,
) -> *const i8 {
    (&*h.cast::<Host>()).archive_id.as_ptr()
}
unsafe extern "C" fn changed(h: ARAModelUpdateControllerHostRef) {
    (&*h.cast::<Host>()).changed.store(true, Ordering::Release);
}
unsafe extern "C" fn source_changed(
    h: ARAModelUpdateControllerHostRef,
    _: ARAAudioSourceHostRef,
    _: *const ARAContentTimeRange,
    _: i32,
) {
    changed(h);
}
unsafe extern "C" fn mod_changed(
    h: ARAModelUpdateControllerHostRef,
    _: ARAAudioModificationHostRef,
    _: *const ARAContentTimeRange,
    _: i32,
) {
    changed(h);
}
unsafe extern "C" fn region_changed(
    h: ARAModelUpdateControllerHostRef,
    _: ARAPlaybackRegionHostRef,
    _: *const ARAContentTimeRange,
    _: i32,
) {
    changed(h);
}
unsafe extern "C" fn progress(
    _: ARAModelUpdateControllerHostRef,
    _: ARAAudioSourceHostRef,
    _: i32,
    _: f32,
) {
}

pub struct Document {
    api: ARADocumentControllerInterface,
    pub(crate) doc: ARADocumentControllerRef,
    source: ARAAudioSourceRef,
    modification: ARAAudioModificationRef,
    region: ARAPlaybackRegionRef,
    sequence: ARARegionSequenceRef,
    context: ARAMusicalContextRef,
    host: Box<Host>,
    _host_instance: Box<ARADocumentControllerHostInstance>,
    factory: Arc<Factory>,
    extension: Option<ARAPlugInExtensionInstance>,
    editor_registered: bool,
}
unsafe impl Send for Document {}
impl Document {
    pub fn new(
        module: &Arc<Module>,
        id: &str,
        sample: Arc<os_engine::sample::Sample>,
        archive: Option<&[u8]>,
    ) -> Result<Self, String> {
        Self::with_archive(module, id, sample, archive, None)
    }
    fn with_archive(
        module: &Arc<Module>,
        id: &str,
        sample: Arc<os_engine::sample::Sample>,
        archive: Option<&[u8]>,
        saved_archive_id: Option<&str>,
    ) -> Result<Self, String> {
        unsafe {
            if sample.rate == 0 || sample.frames() == 0 {
                return Err("ARA requires nonempty audio with a valid sample rate".into());
            }
            let factory = factory(module, id)?;
            let fingerprint = sample_fingerprint(&sample);
            let mut host = Box::new(Host {
                sample,
                fingerprint,
                archive: Mutex::new(archive.unwrap_or_default().to_vec()),
                archive_id: CString::new(
                    saved_archive_id
                        .map(str::to_owned)
                        .unwrap_or_else(|| text(factory.api.documentArchiveID)),
                )
                .map_err(|_| "Invalid archive ID")?,
                playback: playback_interface(),
                playback_requests: Mutex::new(Vec::new()),
                cycle: Mutex::new((None,false)),
                failed: AtomicBool::new(false),
                changed: AtomicBool::new(false),
                audio: ARAAudioAccessControllerInterface {
                    structSize: size_of::<ARAAudioAccessControllerInterface>(),
                    createAudioReaderForSource: Some(reader),
                    readAudioSamples: Some(read),
                    destroyAudioReader: Some(destroy_reader),
                },
                archiving: ARAArchivingControllerInterface {
                    structSize: size_of::<ARAArchivingControllerInterface>(),
                    getArchiveSize: Some(archive_size),
                    readBytesFromArchive: Some(archive_read),
                    writeBytesToArchive: Some(archive_write),
                    notifyDocumentArchivingProgress: Some(archive_progress),
                    notifyDocumentUnarchivingProgress: Some(archive_progress),
                    getDocumentArchiveID: Some(archive_id),
                },
                updates: ARAModelUpdateControllerInterface {
                    structSize: size_of::<ARAModelUpdateControllerInterface>(),
                    notifyAudioSourceAnalysisProgress: Some(progress),
                    notifyAudioSourceContentChanged: Some(source_changed),
                    notifyAudioModificationContentChanged: Some(mod_changed),
                    notifyPlaybackRegionContentChanged: Some(region_changed),
                    notifyDocumentDataChanged: Some(changed),
                },
            });
            let h = (&mut *host as *mut Host).cast::<c_void>();
            let host_instance = Box::new(ARADocumentControllerHostInstance {
                structSize: size_of::<ARADocumentControllerHostInstance>(),
                audioAccessControllerHostRef: h.cast(),
                audioAccessControllerInterface: &host.audio,
                archivingControllerHostRef: h.cast(),
                archivingControllerInterface: &host.archiving,
                modelUpdateControllerHostRef: h.cast(),
                modelUpdateControllerInterface: &host.updates,
                playbackControllerHostRef: h.cast(),
                playbackControllerInterface: &host.playback,
                ..Default::default()
            });
            let props = ARADocumentProperties {
                structSize: size_of::<ARADocumentProperties>(),
                name: c"Lemon Studio audio clip".as_ptr(),
            };
            let inst = sized((factory.api.createDocumentControllerWithDocument.unwrap())(
                &*host_instance,
                &props,
            ))?;
            if inst.documentControllerRef.is_null() {
                return Err("ARA document creation failed".into());
            }
            let api = sized(inst.documentControllerInterface)?;
            let mut d = Self {
                api,
                doc: inst.documentControllerRef,
                source: ptr::null_mut(),
                modification: ptr::null_mut(),
                region: ptr::null_mut(),
                sequence: ptr::null_mut(),
                context: ptr::null_mut(),
                host,
                _host_instance: host_instance,
                factory,
                extension: None,
                editor_registered: false,
            };
            d.create_model()?;
            if archive.is_some_and(|a| !a.is_empty()) {
                d.api.beginEditing.ok_or("ARA begin editing unavailable")?(d.doc);
                let ok = d
                    .api
                    .restoreObjectsFromArchive
                    .ok_or("ARA restore unavailable")?(
                    d.doc, h.cast(), ptr::null()
                );
                d.api.endEditing.unwrap()(d.doc);
                if ok == 0 || d.host.failed.load(Ordering::Relaxed) {
                    return Err("ARA archive restoration failed".into());
                }
            }
            d.api
                .enableAudioSourceSamplesAccess
                .ok_or("ARA sample access unavailable")?(d.doc, d.source, 1);
            Ok(d)
        }
    }
    unsafe fn create_model(&mut self) -> Result<(), String> {
        let a = self.api;
        let d = self.doc;
        let h = (&*self.host as *const Host as *mut Host).cast::<c_void>();
        // Validate required operations before entering an editing transaction.
        if { a.beginEditing }.is_none()
            || { a.endEditing }.is_none()
            || { a.createMusicalContext }.is_none()
            || { a.createRegionSequence }.is_none()
            || { a.createAudioSource }.is_none()
            || { a.createAudioModification }.is_none()
            || { a.createPlaybackRegion }.is_none()
            || { a.destroyDocumentController }.is_none()
            || { a.destroyMusicalContext }.is_none()
            || { a.destroyRegionSequence }.is_none()
            || { a.destroyAudioSource }.is_none()
            || { a.destroyAudioModification }.is_none()
            || { a.destroyPlaybackRegion }.is_none()
            || { a.enableAudioSourceSamplesAccess }.is_none()
            || { a.restoreObjectsFromArchive }.is_none()
            || { a.storeObjectsToArchive }.is_none()
        {
            return Err("Incomplete ARA document interface".into());
        }
        a.beginEditing.unwrap()(d);
        self.context = a.createMusicalContext.unwrap()(
            d,
            h.cast(),
            &ARAMusicalContextProperties {
                structSize: size_of::<ARAMusicalContextProperties>(),
                name: c"Audio clip".as_ptr(),
                ..Default::default()
            },
        );
        if self.context.is_null() {
            a.endEditing.unwrap()(d);
            return Err("ARA failed to create context".into());
        }
        self.sequence = a.createRegionSequence.unwrap()(
            d,
            h.cast(),
            &ARARegionSequenceProperties {
                structSize: size_of::<ARARegionSequenceProperties>(),
                name: c"Audio clip".as_ptr(),
                musicalContextRef: self.context,
                ..Default::default()
            },
        );
        if self.sequence.is_null() {
            a.endEditing.unwrap()(d);
            return Err("ARA failed to create sequence".into());
        }
        let s = &self.host.sample;
        self.source = a.createAudioSource.unwrap()(
            d,
            h.cast(),
            &ARAAudioSourceProperties {
                structSize: size_of::<ARAAudioSourceProperties>(),
                name: c"Original audio".as_ptr(),
                persistentID: c"source-1".as_ptr(),
                sampleCount: s.frames() as i64,
                sampleRate: s.rate as f64,
                channelCount: 2,
                ..Default::default()
            },
        );
        if self.source.is_null() {
            a.endEditing.unwrap()(d);
            return Err("ARA failed to create source".into());
        }
        self.modification = a.createAudioModification.unwrap()(
            d,
            self.source,
            h.cast(),
            &ARAAudioModificationProperties {
                structSize: size_of::<ARAAudioModificationProperties>(),
                name: c"Edits".as_ptr(),
                persistentID: c"modification-1".as_ptr(),
            },
        );
        if self.modification.is_null() {
            a.endEditing.unwrap()(d);
            return Err("ARA failed to create modification".into());
        }
        let duration = s.frames() as f64 / s.rate as f64;
        self.region = a.createPlaybackRegion.unwrap()(
            d,
            self.modification,
            h.cast(),
            &ARAPlaybackRegionProperties {
                structSize: size_of::<ARAPlaybackRegionProperties>(),
                durationInModificationTime: duration,
                durationInPlaybackTime: duration,
                regionSequenceRef: self.sequence,
                name: c"Audio clip".as_ptr(),
                ..Default::default()
            },
        );
        a.endEditing.unwrap()(d);
        if self.context.is_null()
            || self.sequence.is_null()
            || self.source.is_null()
            || self.modification.is_null()
            || self.region.is_null()
        {
            return Err("ARA could not create the audio model".into());
        }
        Ok(())
    }
    pub(crate) unsafe fn bind(
        &mut self,
        component: &VstPtr<dyn vst3_sys::vst::IComponent>,
    ) -> Result<(), String> {
        let entry = component
            .cast::<dyn IEntry2>()
            .ok_or("Plugin lacks the ARA 2 binding interface")?;
        let ext = sized(entry.bind(self.doc, 7, 7))?;
        let renderer = sized(ext.playbackRendererInterface)?;
        let add = renderer
            .addPlaybackRegion
            .ok_or("ARA renderer cannot accept regions")?;
        if { renderer.removePlaybackRegion }.is_none() {
            return Err("ARA renderer cannot remove regions".into());
        }
        self.extension = Some(ext);
        add(ext.playbackRendererRef, self.region);
        if !ext.editorRendererInterface.is_null() {
            let r = sized(ext.editorRendererInterface)?;
            if let Some(add) = r.addPlaybackRegion {
                add(ext.editorRendererRef, self.region);
                self.editor_registered = true;
            }
        }
        if !ext.editorViewInterface.is_null() {
            let view = sized(ext.editorViewInterface)?;
            if let Some(select) = view.notifySelection {
                select(
                    ext.editorViewRef,
                    &ARAViewSelection {
                        structSize: size_of::<ARAViewSelection>(),
                        playbackRegionRefsCount: 1,
                        playbackRegionRefs: &self.region,
                        ..Default::default()
                    },
                );
            }
        }
        self.extension = Some(ext);
        Ok(())
    }
    pub fn take_playback_requests(&self)->Vec<os_host::PlaybackRequest> {
        self.host.playback_requests.lock().map(|mut q|std::mem::take(&mut *q)).unwrap_or_default()
    }
    pub fn poll(&self) {
        unsafe {
            if let Some(f) = self.api.notifyModelUpdates {
                f(self.doc);
            }
        }
    }
    pub fn archive(&self) -> Result<Vec<u8>, String> {
        unsafe {
            self.poll();
            self.host.failed.store(false, Ordering::Relaxed);
            self.host
                .archive
                .lock()
                .map_err(|_| "ARA archive unavailable")?
                .clear();
            let ok = self
                .api
                .storeObjectsToArchive
                .ok_or("ARA archive storage unavailable")?(
                self.doc,
                (&*self.host as *const Host as *mut Host).cast(),
                ptr::null(),
            );
            if ok == 0 || self.host.failed.load(Ordering::Relaxed) {
                return Err("ARA archive could not be saved".into());
            }
            Ok(self
                .host
                .archive
                .lock()
                .map_err(|_| "ARA archive unavailable")?
                .clone())
        }
    }
    pub fn archive_id(&self) -> String {
        text(self.factory.api.documentArchiveID)
    }
    pub fn factory_id(&self) -> String {
        text(self.factory.api.factoryID)
    }
    pub fn fingerprint(&self) -> u64 {
        self.host.fingerprint
    }
    pub fn duration(&self) -> f64 {
        self.frames() as f64 / self.sample_rate() as f64
    }
    pub fn frames(&self) -> usize {
        self.host.sample.frames()
    }
    pub fn sample_rate(&self) -> u32 {
        self.host.sample.rate
    }
    pub fn take_changed(&self) -> bool {
        self.host.changed.swap(false, Ordering::AcqRel)
    }
    pub fn licensed(&self) -> Option<bool> {
        unsafe {
            self.api
                .isLicensedForCapabilities
                .map(|f| f(self.doc, 0, 0, ptr::null(), 0) != 0)
        }
    }
    pub(crate) fn unbind(&mut self) {
        unsafe {
            if let Some(ext) = self.extension.take() {
                if let Some(r) = sized(ext.editorRendererInterface)
                    .ok()
                    .filter(|_| self.editor_registered)
                {
                    if let Some(remove) = r.removePlaybackRegion {
                        remove(ext.editorRendererRef, self.region);
                    }
                }
                if let Ok(r) = sized(ext.playbackRendererInterface) {
                    if let Some(remove) = r.removePlaybackRegion {
                        remove(ext.playbackRendererRef, self.region);
                    }
                }
            }
        }
    }
}
impl Drop for Document {
    fn drop(&mut self) {
        unsafe {
            self.unbind();
            let a = self.api;
            let d = self.doc;
            if !self.source.is_null() {
                if let Some(f) = a.enableAudioSourceSamplesAccess {
                    f(d, self.source, 0);
                }
            }
            if let (Some(begin), Some(_)) = (a.beginEditing, a.endEditing) {
                begin(d);
            }
            if !self.region.is_null() {
                if let Some(f) = a.destroyPlaybackRegion {
                    f(d, self.region);
                }
            }
            if !self.modification.is_null() {
                if let Some(f) = a.destroyAudioModification {
                    f(d, self.modification);
                }
            }
            if !self.source.is_null() {
                if let Some(f) = a.destroyAudioSource {
                    f(d, self.source);
                }
            }
            if !self.sequence.is_null() {
                if let Some(f) = a.destroyRegionSequence {
                    f(d, self.sequence);
                }
            }
            if !self.context.is_null() {
                if let Some(f) = a.destroyMusicalContext {
                    f(d, self.context);
                }
            }
            if let (Some(_), Some(end)) = (a.beginEditing, a.endEditing) {
                end(d);
            }
            if let Some(f) = a.destroyDocumentController {
                f(d);
            }
        }
    }
}

/// An ARA document archive is distinct from a VST3 component preset. The envelope
/// travels in the audio channel's plugin_state, while sample_path remains editable/portable.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct State {
    pub plugin_path: std::path::PathBuf,
    pub class_id: String,
    pub source_path: String,
    pub factory_id: String,
    pub archive_id: String,
    pub frames: usize,
    pub rate: u32,
    pub fingerprint: u64,
    #[serde(skip)]
    pub archive: Vec<u8>,
}
impl State {
    const MAGIC: &'static [u8] = b"LEMONARA1\n";
    pub fn is(bytes: &[u8]) -> bool {
        bytes.starts_with(Self::MAGIC)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let bytes = bytes
            .strip_prefix(Self::MAGIC)
            .ok_or("Not a Lemon ARA document")?;
        let n = u32::from_le_bytes(
            bytes
                .get(..4)
                .ok_or("Truncated ARA header")?
                .try_into()
                .unwrap(),
        ) as usize;
        if n > 65536 {
            return Err("ARA header is too large".into());
        }
        let body = bytes.get(4..).ok_or("Truncated ARA document")?;
        let json = body.get(..n).ok_or("Truncated ARA header")?;
        let archive = body.get(n..).ok_or("Truncated ARA archive")?;
        if archive.len() > 512 * 1024 * 1024 {
            return Err("ARA archive exceeds the supported size".into());
        }
        let metadata: serde_json::Value =
            serde_json::from_slice(json).map_err(|e| format!("Invalid ARA document: {e}"))?;
        let hash = metadata
            .get("archive_hash")
            .and_then(|v| v.as_u64())
            .ok_or("Missing ARA archive checksum")?;
        if archive_hash(archive) != hash {
            return Err("ARA archive is damaged or incomplete".into());
        }
        let mut out: Self =
            serde_json::from_value(metadata).map_err(|e| format!("Invalid ARA document: {e}"))?;
        out.archive = archive.to_vec();
        Ok(out)
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let mut metadata = serde_json::to_value(self).map_err(|e| e.to_string())?;
        metadata["archive_hash"] = serde_json::json!(archive_hash(&self.archive));
        let json = serde_json::to_vec(&metadata).map_err(|e| e.to_string())?;
        if json.len() > 65536 || self.archive.len() > 512 * 1024 * 1024 {
            return Err("ARA document exceeds the supported size".into());
        }
        let mut out = Self::MAGIC.to_vec();
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(&json);
        out.extend_from_slice(&self.archive);
        Ok(out)
    }
}
impl Module {
    /// Creates an ARA instrument using immutable decoded source audio and a checked archive.
    /// All plugin calls, including document construction, use the ordinary crash boundary.
    pub fn create_ara(
        self: &Arc<Self>,
        id: &str,
        source: &std::path::Path,
        saved: Option<&State>,
    ) -> Result<os_host::SharedPlugin, String> {
        let sample = os_engine::sample::load_shared(source).map_err(|e| e.to_string())?;
        let guard = os_host::guard::module_of(self.code as *const c_void);
        let plugin=os_host::guard::call(&guard,||->Result<crate::Plugin,String>{
   let f=factory(self,id)?;
   if let Some(s)=saved {
    if s.archive.is_empty(){return Err("The saved ARA archive is empty".into());}
    if s.class_id!=id || s.factory_id!=text(f.api.factoryID){return Err("The saved ARA editor identity does not match this plugin".into());}
    let mut compatible=s.archive_id==text(f.api.documentArchiveID);
    if f.api.compatibleDocumentArchiveIDsCount>0 && !f.api.compatibleDocumentArchiveIDs.is_null(){
     for i in 0..f.api.compatibleDocumentArchiveIDsCount {if unsafe{text(*f.api.compatibleDocumentArchiveIDs.add(i as usize))}==s.archive_id{compatible=true;}}
    }
    if !compatible{return Err("This editor cannot restore the saved ARA archive version".into());}
    if sample.frames()!=s.frames || sample.rate!=s.rate || sample_fingerprint(&sample)!=s.fingerprint{return Err("The source audio differs from the saved ARA document; restore its original audio file".into());}
   }
   let doc=Document::with_archive(self,id,sample,saved.map(|s|s.archive.as_slice()),saved.map(|s|s.archive_id.as_str()))?;
   let mut p=crate::Plugin::construct(self,id,Some(doc))?;
   p.ara_source=Some(source.to_string_lossy().into_owned());Ok(p)
  }).map_err(|c|os_host::guard::stopped_text("ARA editor",c))??;
        Ok(os_host::share(plugin, guard))
    }
}

fn archive_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(14695981039346656037u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(1099511628211)
    })
}

fn sample_fingerprint(sample: &os_engine::Sample) -> u64 {
    let mut h = 14695981039346656037u64;
    for s in sample.left.iter().chain(sample.right.iter()) {
        for b in s.to_bits().to_le_bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(1099511628211);
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    fn host() -> Box<Host> {
        let sample = Arc::new(os_engine::Sample {
            rate: 48000,
            left: vec![0.25, 0.5],
            right: vec![-0.5, -0.25],
        });
        Box::new(Host {
            sample,
            fingerprint: 0,
            archive: Mutex::new(Vec::new()),
            archive_id: CString::new("test").unwrap(),
            failed: AtomicBool::new(false),
            changed: AtomicBool::new(false),
            audio: Default::default(),
            archiving: Default::default(),
            updates: Default::default(),
            playback: playback_interface(),
            playback_requests: Mutex::new(Vec::new()),
            cycle: Mutex::new((None,false)),
        })
    }
    #[test]
    fn ara_playback_callbacks_queue_valid_commands() {
        let mut host=host();let h=(&mut *host as *mut Host).cast();
        unsafe {
            playback_start(h);playback_seek(h,1.25);playback_seek(h,f64::NAN);
            playback_cycle(h,0.5,2.);playback_cycle_enable(h,1);playback_stop(h);playback_cycle_enable(h,0);
        }
        use os_host::PlaybackRequest::*;
        assert_eq!(*host.playback_requests.lock().unwrap(),vec![Start,Seek(1.25),Cycle(Some((0.5,2.5))),Stop,Cycle(None)]);
    }
    #[test]
    fn ara_callback_readers_stereo_bounds_and_lifetime() {
        unsafe {
            let mut host = host();
            let h = (&mut *host as *mut Host).cast();
            let r = reader(h, ptr::null_mut(), 0);
            let d = reader(h, ptr::null_mut(), 1);
            drop(host);
            let mut l = [9f32; 4];
            let mut rr = l;
            let buffers = [l.as_mut_ptr().cast(), rr.as_mut_ptr().cast()];
            assert_eq!(read(ptr::null_mut(), r, -1, 4, buffers.as_ptr()), 1);
            assert_eq!(l, [0., 0.25, 0.5, 0.]);
            assert_eq!(rr, [0., -0.5, -0.25, 0.]);
            assert_eq!(read(ptr::null_mut(), r, i64::MAX, 4, buffers.as_ptr()), 1);
            assert_eq!(l, [0.; 4]);
            assert_eq!(read(ptr::null_mut(), r, 0, -1, buffers.as_ptr()), 0);
            let mut l = [9f64; 2];
            let mut rr = l;
            let buffers = [l.as_mut_ptr().cast(), rr.as_mut_ptr().cast()];
            assert_eq!(read(ptr::null_mut(), d, 0, 2, buffers.as_ptr()), 1);
            assert_eq!(l, [0.25, 0.5]);
            assert_eq!(rr, [-0.5, -0.25]);
            destroy_reader(ptr::null_mut(), r);
            destroy_reader(ptr::null_mut(), d);
        }
    }
    #[test]
    fn ara_archive_callbacks_random_io_and_overflow() {
        unsafe {
            let mut host = host();
            let h = (&mut *host as *mut Host).cast();
            let bytes = [3u8, 4];
            assert_eq!(archive_write(h, ptr::null_mut(), 3, 2, bytes.as_ptr()), 1);
            assert_eq!(*host.archive.lock().unwrap(), [0, 0, 0, 3, 4]);
            let mut b = [0; 2];
            assert_eq!(archive_read(h, ptr::null_mut(), 3, 2, b.as_mut_ptr()), 1);
            assert_eq!(b, bytes);
            assert_eq!(archive_read(h, ptr::null_mut(), 4, 2, b.as_mut_ptr()), 0);
            assert_eq!(
                archive_write(h, ptr::null_mut(), usize::MAX, 2, bytes.as_ptr()),
                0
            );
            assert!(host.failed.load(Ordering::Relaxed));
            assert_eq!(archive_size(h, ptr::null_mut()), 5);
        }
    }
    #[test]
    fn ara_state_envelope_roundtrip_and_truncated_header() {
        let s = State {
            plugin_path: "plugin.vst3".into(),
            class_id: "class".into(),
            source_path: "audio.wav".into(),
            factory_id: "factory".into(),
            archive_id: "archive".into(),
            frames: 2,
            rate: 48000,
            fingerprint: 55,
            archive: vec![0, 255, 1, 42],
        };
        let bytes = s.encode().unwrap();
        assert_eq!(State::decode(&bytes).unwrap(), s);
        for n in 0..State::MAGIC.len() + 4 {
            assert!(State::decode(&bytes[..n]).is_err());
        }
        let mut damaged = bytes.clone();
        *damaged.last_mut().unwrap() ^= 1;
        assert!(State::decode(&damaged).is_err());
        let mut invalid = State::MAGIC.to_vec();
        invalid.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(State::decode(&invalid).is_err());
    }
}
