use std::time::{Instant, Duration};
use rand::{SeedableRng, Rng, rngs::StdRng};
use core::ffi::c_void;

const BENCH_WARMUPS: usize = 1<<8;
const BENCH_RUNS: usize = 1<<12;

const HEAP_RUNS_PER_BENCH: usize = 1<<16;
const FSM_COUNT: usize = 256;
const SEED: u64 = 42;


pub mod func_fsm;
pub mod func2_fsm;
pub mod enum_fsm;

pub type FsmTrans = unsafe fn(*mut *const c_void, *mut u64, u64);

pub struct FsmState {
    f: *const c_void,
    state: *mut u64,
}

impl Fsm for FsmState {
    fn drive(&mut self, data: u64) {
        unsafe {
            let f = core::mem::transmute::<*const c_void, FsmTrans>(self.f);
            let f_ptr = (&mut self.f) as *mut *const c_void;
            f(f_ptr, self.state, data);
        }
    }
}

pub trait Fsm {
    fn drive(&mut self, data: u64);
}

macro_rules! single_run {
    ($mod:ident, $rng:ident) => {{
        let mut states = Vec::new();
        let data: Vec<u64> = (0..HEAP_RUNS_PER_BENCH)
            .map(|_| $rng.gen())
            .collect();
        for _ in 0..FSM_COUNT {
            match $rng.gen_range(0, 4) {
                0 => states.push($mod::s4::init(&mut $rng)),
                1 => states.push($mod::s8::init(&mut $rng)),
                2 => states.push($mod::s16::init(&mut $rng)),
                _ => states.push($mod::s32::init(&mut $rng)),
            }
        }
        let idxs: Vec<usize> = (0..HEAP_RUNS_PER_BENCH)
            .map(|_| $rng.gen_range(0, FSM_COUNT))
            .collect();
        let t = Instant::now();
        for (&idx, d) in idxs.iter().zip(data) {
            states[idx].drive(d);
        }
        t.elapsed()
    }};
}

macro_rules! bench_run {
    ($mod:ident, $name:expr) => {{
        let mut rng = StdRng::seed_from_u64(SEED);
        for _ in 0..BENCH_WARMUPS {
            single_run!($mod, rng);
        }
        let func_durs: Vec<Duration> = (0..BENCH_RUNS)
            .map(|_| single_run!($mod, rng))
            .collect();
        let (m, s) = calc_mean_std(&func_durs);
        println!("{}: {:.1}±{:.1} us", $name, m, s);
    }};
}

fn calc_mean_std(durs: &[Duration]) -> (f32, f32) {
    let n = durs.len() as f32;
    let sum: f32 = durs
        .iter()
        .map(|d| d.as_secs_f32())
        .sum();
    let mean = sum/n;
    let std2: f32 = durs.iter()
        .map(|d| {
            let v = d.as_secs_f32() - mean;
            v*v
        })
        .sum();
    (1e6*mean, 1e6*(std2/n).sqrt())
}

fn main() {
    bench_run!(func_fsm, "function");
    bench_run!(func2_fsm, "hybrid");
    bench_run!(enum_fsm, "enum");
}
