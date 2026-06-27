use perf_event::{Builder, Group, Counter};
#[cfg(not(feature = "software"))]
use perf_event::events::Hardware;
#[cfg(feature = "software")]
use perf_event::events::Software;
use std::sync::Mutex;
use std::sync::OnceLock;

#[cfg(not(feature = "software"))]
struct BenchState {
    group: Group,
    cycles: Counter,
    instr: Counter,
    cache_misses: Counter,
    branch_misses: Counter,
}

#[cfg(feature = "software")]
struct BenchState {
    group: Group,
    task_clock: Counter,
    page_faults: Counter,
    context_switches: Counter,
    cpu_migrations: Counter,
}

static BENCH_STATE: OnceLock<Mutex<Option<BenchState>>> = OnceLock::new();

#[unsafe(no_mangle)]
pub extern "C" fn init() -> i32 {
    let mutex = BENCH_STATE.get_or_init(|| Mutex::new(None));
    let mut guard = mutex.lock().unwrap();
    if guard.is_some() { return 0; }

    let mut group = match Group::new() {
        Ok(g) => g,
        Err(_) => return -1,
    };

    #[cfg(not(feature = "software"))]
    {
        let cycles = match { let mut b = Builder::new(); b.exclude_kernel(true); b }.group(&mut group).kind(Hardware::CPU_CYCLES).build() {
            Ok(c) => c,
            Err(_) => return -2,
        };

        let instr = match { let mut b = Builder::new(); b.exclude_kernel(true); b }.group(&mut group).kind(Hardware::INSTRUCTIONS).build() {
            Ok(i) => i,
            Err(_) => return -3,
        };

        let cache_misses = match { let mut b = Builder::new(); b.exclude_kernel(true); b }.group(&mut group).kind(Hardware::CACHE_MISSES).build() {
            Ok(c) => c,
            Err(_) => return -4,
        };

        let branch_misses = match { let mut b = Builder::new(); b.exclude_kernel(true); b }.group(&mut group).kind(Hardware::BRANCH_MISSES).build() {
            Ok(b) => b,
            Err(_) => return -5,
        };

        *guard = Some(BenchState { group, cycles, instr, cache_misses, branch_misses });
    }

    #[cfg(feature = "software")]
    {
        let task_clock = match Builder::new().group(&mut group).kind(Software::TASK_CLOCK).build() {
            Ok(c) => c,
            Err(_) => return -2,
        };

        let page_faults = match Builder::new().group(&mut group).kind(Software::PAGE_FAULTS).build() {
            Ok(i) => i,
            Err(_) => return -3,
        };

        let context_switches = match Builder::new().group(&mut group).kind(Software::CONTEXT_SWITCHES).build() {
            Ok(c) => c,
            Err(_) => return -4,
        };

        let cpu_migrations = match Builder::new().group(&mut group).kind(Software::CPU_MIGRATIONS).build() {
            Ok(b) => b,
            Err(_) => return -5,
        };

        *guard = Some(BenchState { group, task_clock, page_faults, context_switches, cpu_migrations });
    }

    0
}

#[unsafe(no_mangle)]
pub extern "C" fn start() {
    if let Some(mutex) = BENCH_STATE.get() {
        let mut guard = mutex.lock().unwrap();
        if let Some(ref mut state) = *guard {
            let _ = state.group.reset();
            let _ = state.group.enable();
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn stop_and_print() {
    if let Some(mutex) = BENCH_STATE.get() {
        let mut guard = mutex.lock().unwrap();
        if let Some(ref mut state) = *guard {
            let _ = state.group.disable();

            if let Ok(counts) = state.group.read() {
                #[cfg(not(feature = "software"))]
                {
                    let total_cycles = counts[&state.cycles];
                    let total_instr = counts[&state.instr];
                    let total_cache = counts[&state.cache_misses];
                    let total_branch = counts[&state.branch_misses];
                    
                    let ipc = if total_cycles > 0 { total_instr as f64 / total_cycles as f64 } else { 0.0 };

                    println!("\n[PERF REPORT]");
                    println!("  CPU Core Cycles : {} cycles", total_cycles);
                    println!("  Instructions    : {} instructions", total_instr);
                    println!("  Instructions Per Cycle : {:.3}", ipc);
                    println!("  Cache Misses    : {} events", total_cache);
                    println!("  Branch Misses   : {} events", total_branch);
                }

                #[cfg(feature = "software")]
                {
                    let clock = counts[&state.task_clock];
                    let faults = counts[&state.page_faults];
                    let ctx_switches = counts[&state.context_switches];
                    let migrations = counts[&state.cpu_migrations];
                    
                    println!("\n[PERF REPORT]");
                    println!("  Task Clock (Duration) : {:.3} ms", clock as f64 / 1_000_000.0);
                    println!("  Page Faults           : {} events", faults);
                    println!("  Context Switches      : {} events", ctx_switches);
                    println!("  CPU Migrations        : {} events", migrations);
                }
            }
        }
    } else {
        println!("[PERF ERROR] Shared library not initialized!");
    }
}
