// SPDX-License-Identifier: GPL-3.0-or-later
//! Embedded oscillator assets. Old tables are reclaimed on a state/UI thread.
use crate::{
    controls,
    wavetable::{Source, Table},
};
use lemon_small_fx::params::FxParams;
use nih_plug::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, RwLock},
};
pub const FIELD: &str = "lemon-wave-tables-v1";
pub const MAX_STATE: usize = 64 * 1024 * 1024;
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sources {
    #[serde(default)]
    pub noise: Option<crate::sample::Sample>,
    pub oscillators: [Option<Source>; 2],
    #[serde(default)]
    pub samples: [Option<crate::sample::Sample>; 2],
}
#[derive(Default)]
pub struct Tables {
    pub noise: Option<crate::sample::Playback>,
    pub oscillators: [Option<Table>; 2],
    pub samples: [Option<crate::sample::Playback>; 2],
}
pub struct WaveParams {
    pub base: Arc<FxParams>,
    pub page: std::sync::atomic::AtomicUsize,
    pub env_tab: std::sync::atomic::AtomicUsize,
    pub filter_tab: std::sync::atomic::AtomicUsize,
    pub fx_scroll: std::sync::atomic::AtomicUsize,
    pub audition:crate::morph::PreviewBus,
    pub keyboard: crate::performance::Keyboard,
    pub origin:Mutex<Option<crate::presets::Origin>>,
    pub details: std::sync::atomic::AtomicUsize,
    pub fx_add: std::sync::atomic::AtomicBool,
    pub lfo_tab: std::sync::atomic::AtomicUsize,
    pub curves: RwLock<crate::modulation::Curves>,
    pub telemetry: crate::modulation::Telemetry,
    pub tables: RwLock<Arc<Tables>>,
    retired: Mutex<Vec<Arc<Tables>>>,
    pub status: Mutex<String>,
}
impl Default for WaveParams {
    fn default() -> Self {
        Self { origin:Default::default(), audition:Default::default(), keyboard:Default::default(), details:Default::default(), filter_tab:Default::default(), fx_scroll:Default::default(), fx_add:Default::default(), env_tab:Default::default(), lfo_tab:Default::default(), page:Default::default(), curves:RwLock::default(), telemetry:Default::default(), base:Arc::new(FxParams::new(controls::SPEC)), tables:RwLock::new(Arc::default()), retired:Mutex::default(), status:Mutex::new("Choose cycle length, then Import WAV. Imported tables travel with saved projects and presets.".into()) }
    }
}
impl WaveParams {
    pub fn sources(&self) -> Sources {
        let tables = self.tables.read().unwrap();
        Sources {
            noise: tables.noise.as_ref().map(|s|s.source.clone()),
            samples: std::array::from_fn(|o| tables.samples[o].as_ref().map(|t| t.source.clone())),
            oscillators: std::array::from_fn(|o| {
                tables.oscillators[o].as_ref().map(|t| t.source.clone())
            }),
        }
    }
    pub fn replace(&self, sources: Sources) -> Result<(), String> {
        let [a, b] = sources.oscillators;
        let [sa, sb] = sources.samples;
        let tables = Arc::new(Tables {
            noise: sources.noise.map(crate::sample::Playback::compile).transpose()?,
            samples: [
                sa.map(crate::sample::Playback::compile).transpose()?,
                sb.map(crate::sample::Playback::compile).transpose()?,
            ],
            oscillators: [
                a.map(Table::compile).transpose()?,
                b.map(Table::compile).transpose()?,
            ],
        });
        let mut current = self.tables.write().unwrap();
        let old = std::mem::replace(&mut *current, tables);
        let mut retired = self.retired.lock().unwrap();
        retired.retain(|x| Arc::strong_count(x) > 1);
        retired.push(old);
        Ok(())
    }
    pub fn message(&self, s: impl Into<String>) {
        *self.status.lock().unwrap() = s.into();
    }
}
// SAFETY: parameter pointers are owned by base, which lives as long as this Params object.
unsafe impl Params for WaveParams {
    fn param_map(&self) -> Vec<(String, ParamPtr, String)> {
        self.base.param_map()
    }
    fn serialize_fields(&self) -> BTreeMap<String, String> {
        let mut fields = self.base.serialize_fields();
        if let Some(origin)=&*self.origin.lock().unwrap(){fields.insert(crate::presets::FIELD.into(),serde_json::to_string(origin).unwrap());}
        fields.insert(
            crate::modulation::FIELD.into(),
            self.curves.read().unwrap().encode(),
        );
        fields.insert(
            FIELD.into(),
            serde_json::to_string(&self.sources()).unwrap(),
        );
        fields
    }
    fn deserialize_fields(&self, fields: &BTreeMap<String, String>) {
        self.base.live.clear();
        self.base.deserialize_fields(fields);
        *self.origin.lock().unwrap()=fields.get(crate::presets::FIELD).filter(|s|s.len()<512).and_then(|s|serde_json::from_str(s).ok());
        let curves = fields
            .get(crate::modulation::FIELD)
            .map(|s| crate::modulation::Curves::decode(s))
            .transpose();
        *self.curves.write().unwrap() = curves.unwrap_or_default().unwrap_or_default();
        let result = match fields.get(FIELD) {
            None => self.replace(Sources::default()), // Older/factory patches must clear imported tables.
            Some(s) if s.len() > MAX_STATE => Err("Embedded table state exceeds size limit".into()),
            Some(s) => serde_json::from_str::<Sources>(s)
                .map_err(|e| e.to_string())
                .and_then(|s| self.replace(s)),
        };
        match result {
            Ok(()) => self.message("Tables recalled with this patch"),
            Err(e) => {
                let _ = self.replace(Sources::default());
                self.message(format!("Table recall failed: {e}"));
            }
        }
    }
}
