use std::{fs, env, path::Path, io::{self, Write},};
use rand::{Rng, SeedableRng, rngs::StdRng};

const RNG_SEED: u64 = 123456;
const SIZES: [u8; 4] = [4, 8, 16, 32];
const STATES: u8 = 255;

fn gen_func_transition(mut w: impl Write, size: u8, n: u8, rng: &mut StdRng) -> io::Result<()> {
    writeln!(w, "pub unsafe fn fsm_pos{}(f: *mut *const c_void, state: *mut u64, data: u64) {{", n)?;
    writeln!(w, "let state = &mut *(state as *mut [u64; {}]);", size)?;
    for i in 0..size {
        let val: u64 = rng.gen_range(1, std::u64::MAX);
        match rng.gen_range(0, 5) {
            0 => writeln!(w, "state[{}] = state[{}].wrapping_add({}u64.wrapping_add(data));", i, i, val)?,
            1 => writeln!(w, "state[{}] = state[{}].wrapping_sub({}u64.wrapping_add(data));", i, i, val)?,
            2 => writeln!(w, "state[{}] = state[{}].wrapping_mul({}u64.wrapping_add(data));", i, i, val)?,
            3 => writeln!(w, "state[{}] = state[{}].wrapping_div({}u64.wrapping_add(data));", i, i, val)?,
            _ => (),
        }
    }
    writeln!(w, "let fp = FTABLE.get_unchecked((state.iter().sum::<u64>()%{}) as usize);", STATES as u64 + 1)?;
    w.write_all(b"std::ptr::write(f, *fp  as *const FsmTrans as *const c_void);\n}")?;
    Ok(())
}

fn gen_func_fsm(size: u8, rng: &mut StdRng) -> io::Result<()> {
    let file_name = format!("func_fsm_{}.rs", size);
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join(&file_name);
    let mut w = io::BufWriter::new(fs::File::create(&dest_path)?);
    w.write_all(b"use core::ffi::c_void;")?;
    writeln!(w, "const SIZE: usize = {};", size)?;
    w.write_all(b"
        use crate::{FsmState, FsmTrans};
        use rand::Rng;
        pub fn init(mut rng: impl Rng) -> FsmState {
            let mut state = Box::new([0u64; SIZE]);
            for v in state.iter_mut() { *v = rng.gen(); }
            let state = Box::leak(state) as *mut [u64; SIZE] as *mut u64;
            let f = fsm_pos0 as *const FsmTrans as *const c_void;
            FsmState { f, state }
        }
    ")?;

    writeln!(w, "static FTABLE: &[unsafe fn(*mut *const c_void, *mut u64, u64); {}] = &[", STATES as u64 + 1)?;
    for n in 0..STATES {
        writeln!(w, "fsm_pos{},", n)?;
    }
    w.write_all(b"fsm_pos_trap,")?;
    w.write_all(b"];\n")?;
    for n in 0..STATES {
        gen_func_transition(&mut w, size, n, rng)?;
    }
    w.write_all(b"pub unsafe fn fsm_pos_trap(_: *mut *const c_void, _: *mut u64, _: u64) {}")?;

    Ok(())
}

fn gen_func2_fsm(size: u8) -> io::Result<()> {
    let file_name = format!("func2_fsm_{}.rs", size);
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join(&file_name);
    let mut w = io::BufWriter::new(fs::File::create(&dest_path)?);
    w.write_all(b"use core::ffi::c_void;")?;
    writeln!(w, "const SIZE: usize = {};", size)?;
    writeln!(w, "use crate::func_fsm::s{}::fsm_pos0;", size)?;
    w.write_all(b"
        use crate::{Fsm, FsmTrans};
        use rand::Rng;

        struct State {
            f: *const c_void,
            state: [u64; SIZE],
        }

        impl Fsm for State {
            fn drive(&mut self, data: u64) { unsafe {
                let f = core::mem::transmute::<*const c_void, FsmTrans>(self.f);
                let f_mut = (&mut self.f) as *mut *const c_void;
                f(f_mut, self.state.as_mut_ptr(), data);
            }}
        }

        pub fn init(mut rng: impl Rng) -> Box<dyn Fsm> {
            let mut state = [0u64; SIZE];
            for v in state.iter_mut() { *v = rng.gen(); }
            let f = fsm_pos0 as *const FsmTrans as *const c_void;
            Box::new(State{ f, state }) as Box<dyn Fsm>
        }
    ")?;

    Ok(())
}

fn gen_enum_branch(mut w: impl Write, size: u8, n: u8, rng: &mut StdRng) -> io::Result<()> {
    if n == STATES {
        return w.write_all(b"_ => (),");
    }

    writeln!(w, "State(n @ {}, s) => {{", n)?;
    for i in 0..size {
        let val: u64 = rng.gen_range(1, std::u64::MAX);
        match rng.gen_range(0, 5) {
            0 => writeln!(w, "s[{}] = s[{}].wrapping_add({}u64.wrapping_add(data));", i, i, val)?,
            1 => writeln!(w, "s[{}] = s[{}].wrapping_sub({}u64.wrapping_add(data));", i, i, val)?,
            2 => writeln!(w, "s[{}] = s[{}].wrapping_mul({}u64.wrapping_add(data));", i, i, val)?,
            3 => writeln!(w, "s[{}] = s[{}].wrapping_div({}u64.wrapping_add(data));", i, i, val)?,
            _ => (),
        }
    }
    writeln!(w, "*n = (s.iter().sum::<u64>()%{}) as u8\n}}\n", STATES as u64 + 1)?;
    Ok(())
}

fn gen_enum_fsm(size: u8, rng: &mut StdRng) -> io::Result<()> {
    let file_name = format!("enum_fsm_{}.rs", size);
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join(&file_name);
    let mut w = io::BufWriter::new(fs::File::create(&dest_path)?);
    writeln!(w, "const SIZE: usize = {};", size)?;
    w.write_all(b"
        use crate::Fsm;
        use rand::Rng;
        pub fn init(mut rng: impl Rng) -> Box<dyn Fsm> {
            let mut state = [0u64; SIZE];
            for v in state.iter_mut() { *v = rng.gen(); }
            Box::new(State(0, state)) as Box<dyn Fsm>
        }
    ")?;

    writeln!(w, "pub struct State(u8, [u64; {}]);", size)?;

    w.write_all(b"
        impl Fsm for State {\n\
            fn drive(&mut self, data: u64) {\n\
                match self {\n\
    ")?;
    for n in 0..=STATES {
        gen_enum_branch(&mut w, size, n, rng)?;
    }
    w.write_all(b"}\n}\n}\n")?;

    Ok(())
}

fn main() {
    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    for &size in &SIZES {
        gen_func_fsm(size, &mut rng).unwrap();
        gen_func2_fsm(size).unwrap();
    }

    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    for &size in &SIZES {
        gen_enum_fsm(size, &mut rng).unwrap();
    }
    println!("cargo:rerun-if-changed=build.rs");
}
