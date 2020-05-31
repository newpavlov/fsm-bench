# Benchmark of Finite State Machine approaches

Asynchronous functions and generators can be viewed as an ergonomic way to define [Finite State Machines](https://en.wikipedia.org/wiki/Finite-state_machine) (FSM). Today in Rust asynchronous functions are implemented on top of generators, which in turn get compiled into FSMs represented in the form of a big `enum`. Its variants contain FSM data between yield points, while tag encodes current FSM position. In a certain sense, the `enum`  is a pseudo-stack of a "green thread", maximum size of which is known at compile time.

Each time when you poll a `Future` or drive a generator, generated code will match on the enum to jump to code responsile for processing current FSM state. Such matches usually compiled down to [jump tables](https://en.wikipedia.org/wiki/Branch_table) and fairly efficient. But there is a catch: in practice we usually work with heterogeneous futures, so we box top-level (and sometimes intermediary) futures and work with `Box<dyn Future>`. It means that (as far as I understand it) compiler inserts triple indirection in the worst case scenario, first we follow pointer to a [Vitrual Method Table](https://en.wikipedia.org/wiki/Virtual_method_table) (vtable), from which we jump to a function (trait method) processing the current FSM type, and finally we use jump table to finally reach code driving our FSM forward. Theoretically it is possible to omit the vtable for single method traits and jump directly to the target function, thus reducing number of indirections from 3 to 2, but I am not sure if Rust currently implements such optimization.

But even two indirections is not the best that we can do. Instead of using `enum`'s tag to encode FSM position, we can encode it via a function pointer. It means that instead of the `(pointer to enum tag + data, static vtable)` pair which we get with the current approach, we could store `(pointer to data, dynamic function pointer)`. If after driving FSM forward its position has changed, then the function pointer will be updated to a function responsible for driving forward in the next position. In other words, instead of the static jump table behind static vtable we will use dynamic function pointers. Of course, users will not deal with those pointers directly, tracking which function responsible for which FSM states and ensuring validity of function pointer updates will be handled entirely by compiler.

## Code examples

Let's use some code examples to make things a bit more clear.

First, "enum" approach, which roughly emulates how generators currently work:

```rust
// FSM tag and data
struct State(u8, [u64; SIZE]);

// You can view `Fsm` as a simplified version of the `Poll` trait
impl Fsm for State {
    fn drive(&mut self, data: u64) {
        match self {
            State(n @ 0, state) => {
                modify_fsm_data0(state, data); // modify FSM data
                *n = get_next_pos0(state, data); // set next FSM position
            }
            State(n @ 1, s) => { .. },
            State(n @ 2, s) => { .. },
            // ...
            _ => (), // trap position
        }
    }
}

//user code
let mut fsm: Box<dyn Fsm> = get_fsm();
fsm.drive(data1);
fsm.drive(data2);
```

The alternative "function" approach can look like this:

```rust
// Function type which drives FSM forward, since we can't use recursive definitions,
// we use `*const c_void` instead of `*const FsmTrans`
type FsmTrans = unsafe fn(*mut *const c_void, *mut [u64; SIZE], u64);

struct FsmState {
    // Pointer to a function which is responsible for driving FSM forward
    // at the current position, read type as `*const FsmTrans`
    f: *const c_void,
    // Pointer to a heap memory containing FSM data. In practical code
    // it will be a generated `union` containing variants for each
    // FSM position, not a fixed type.
    state: *mut [u64; SIZE],
}

// Starting FSM position
fn fsm_pos0(f: *mut *const c_void, state: *mut [u64; SIZE], data: u64) {
    modify_fsm_data0(state, data); // modify FSM data
    // find function for driving FSM in the new state
    let next_fp: *const FsmTrans = get_next_pos0(state, data);
    // overwrite function pointer on the stack with new new one
    core::ptr::write(f, next_fp as *const c_void);
}

unsafe fn fsm_pos1(f: *mut *const c_void, state: *mut [u64; SIZE], data: u64) { .. }

// trap position
unsafe fn fsm_pos_trap(_: *mut *const c_void, _: *mut u64, _: u64) {}

impl Fsm for FsmState {
    fn drive(&mut self, data: u64) {
        unsafe {
            let f = core::mem::transmute::<*const c_void, FsmTrans>(self.f);
            let f_ptr = (&mut self.f) as *mut *const c_void;
            f(f_ptr, self.state, data);
        }
    }
}

// user code
let mut fsm: FsmState = get_fsm();
fsm.drive(data1);
fsm.drive(data2);
```

Additionally we can introduce a "hybrid" approach:
```rust
struct State {
    f: *const c_void,
    state: [u64; SIZE],
}

impl Fsm for State {
    fn drive(&mut self, data: u64) {
        unsafe {
            let f = core::mem::transmute::<*const c_void, FsmTrans>(self.f);
            let f_mut = (&mut self.f) as *mut *const c_void;
            f(f_mut, self.state.as_mut_ptr(), data);
        }
    }
}

//user code
let mut fsm: Box<dyn Fsm> = get_fsm();
fsm.drive(data1);
fsm.drive(data2);
```

## Benchmarks
The code in this repository generates 4 FSMs with different state sizes as defined in the `SIZES` constant (by default: 4, 8, 16, and 32 `u64`s) for each approach. Each FSM contains number of positions equal to `STATES` (by default 255) plus the trap position. At each position FSM performs simple arithmetic actions on its state dependent on `data`, those actions are generated randomly at compile time and different for each position. FSMs of a same size encoded using different approaches are functionally equivalent to each other.

Next, for each approach generated vector of FSMs of randomly selected types with a length equal to `FSM_COUNT` (by default 256). After a warmup, code performs `HEAP_RUNS_PER_BENCH` FSM drives (by default 4096), i.e. at each iteration it selects random FSM from the vector and drives it with a random `data`. The loop is repeated `BENCH_RUNS` times and completion timing of each run is recorded (excluding vectors initialization and warmup), based on which mean and standard deviation is calculated.

## Results and Discussion

On my PC (Linux, AMD 2700x) I get the following benchmark results:
```
function: 1129.9±53.9 us
hybrid: 1423.4±54.0 us
enum: 1579.9±39.6 us
```

As expected less indirection results in a better performance, for the simple state manipulations used in this benchmark the "function" approach results in an almost 30% speed-up. A bit surprisingly the "hybrid" approach also outperforms the baseline, although by a smaller margin equal to 10%. Since in the latter case we essentially measure a difference between jump tables and raw function pointers, it looks like function pointers are a bit more suitable for implementing relatively large FSMs.

Thus we can say that the current Rust implementation of futures and generators is not strictly speaking zero-cost and can be potentially improved. Is it possible to implement the function pointer based approach in a backward compatible way? In theory it could be done by special-casing `Box<dyn Future>` and `Box<dyn Generator>` on compiler level and adjusting code generation accordingly. This will mean that memory layout  of generators and futures will slightly differ between stack and heap (i.e. the function pointer will stay on stack, as part of the `dyn` type). Unfortunately I am not familiar enough with the Rust compiler inner workings to judge whether such change would be practical or not.

It is worth to note that the presented results should be taken with a grain of salt. Firstly, overhead of the current "enum" approach may be negligibly low in practice, for example FSM with a state size equal to 32 `u64`s uses state transition functions which contain only ~200 instructions. Secondly, function pointers may inhibit some optimizations since they are generally opaque for compiler, especially it may be important for embeded applications which use stack or statically allocated generators and futures. And finally, it looks like bigger gains can be achieved by improving code generation for generators without migration from enums (e.g. by using smarter layout optimizations).
