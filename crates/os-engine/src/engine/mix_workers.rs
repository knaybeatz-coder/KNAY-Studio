//! Persistent workers for independent hosted mixer insert chains. Jobs own all mutable
//! DSP state; only ordered routing/summing runs on the engine thread. No unsafe sharing.
use super::{FxSlot, TransportInfo, run_slot};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub(super) struct State {
    pub enabled: bool,
    pub done: Vec<bool>,
    pub incoming: Vec<bool>,
    pub cost: Vec<Duration>,
    pub candidates: Vec<usize>,
    pub spare: Vec<Job>,
    pub tx: mpsc::Sender<Job>,
    pub rx: mpsc::Receiver<Job>,
}
impl State {
    pub fn new() -> Self {
        let (tx,rx)=mpsc::channel();
        let enabled = available() && std::env::var_os("LEMON_AUDIO_SINGLE_CORE").is_none();
        Self { enabled,done:Vec::new(),incoming:Vec::new(),cost:Vec::new(),candidates:Vec::new(),spare:Vec::new(),tx,rx }
    }
}
pub(super) struct Job {
    pub track: usize,
    pub effects: Vec<FxSlot>,
    pub buf: Vec<f32>,
    pub rate: u32,
    pub song: TransportInfo,
    pub until: Option<Instant>,
    pub wait: Duration,
    pub export: bool,
    pub moved: bool,
    pub missed: u64,
    pub elapsed: Duration,
    dry: Vec<f32>, left: Vec<f32>, right: Vec<f32>, backup: Vec<f32>,
}
impl Default for Job {
    fn default() -> Self {
        Self { track:0,effects:Vec::new(),buf:Vec::new(),rate:48000,
            song:TransportInfo {playing:false,tempo:120.0,frame:0,beats:0.0,bar_start:0.0,bar:0,numerator:4,denominator:4,looping:None},
            until:None,wait:Duration::ZERO,export:false,moved:false,missed:0,elapsed:Duration::ZERO,
            dry:Vec::new(),left:Vec::new(),right:Vec::new(),backup:Vec::new() }
    }
}
impl Job {
    fn run(&mut self) {
        let start=Instant::now();
        self.moved=false; self.missed=0;
        self.backup.clear(); self.backup.extend_from_slice(&self.buf);
        let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let limit=os_dsp::glide::engine_limit(self.rate);
            for fx in &mut self.effects {
                run_slot(fx,&mut self.buf,self.rate,limit,&self.song,&mut self.dry,&mut self.left,&mut self.right,
                    (&mut self.until,self.wait,self.export,&mut self.missed),&mut self.moved,
                    (self.track,&[],&mut []),(None,None));
            }
        }));
        // A poisoned hosted instance will subsequently bypass through the existing lock guard.
        // Always return the job: an unwinding plugin must never strand the render thread.
        if result.is_err() { self.buf.copy_from_slice(&self.backup); self.missed+=1; }
        self.elapsed=start.elapsed();
    }
}
struct Work { job: Job, reply: mpsc::Sender<Job> }
struct Pool { tx: mpsc::Sender<Work>, workers: usize }
fn pool() -> &'static Pool {
    static POOL: OnceLock<Pool> = OnceLock::new();
    POOL.get_or_init(|| {
        let (tx,rx)=mpsc::channel::<Work>();
        let rx=Arc::new(Mutex::new(rx));
        let wanted=std::thread::available_parallelism().map_or(1,usize::from).saturating_sub(1).min(12);
        let mut workers=0;
        for i in 0..wanted {
            let rx=rx.clone();
            if std::thread::Builder::new().name(format!("Lemon mixer {}",i+1)).spawn(move || loop {
                let work=rx.lock().unwrap_or_else(|e|e.into_inner()).recv();
                let Ok(mut work)=work else { break };
                work.job.run();
                let _=work.reply.send(work.job);
            }).is_ok() { workers+=1; }
        }
        Pool {tx,workers}
    })
}
pub(super) fn available() -> bool { pool().workers>=2 }
pub(super) fn submit(job: Job, reply: mpsc::Sender<Job>) {
    if let Err(e)=pool().tx.send(Work {job,reply}) {
        let mut work=e.0; work.job.run(); let _=work.reply.send(work.job);
    }
}
