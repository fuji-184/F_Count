use std::sync::Mutex;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;
use core::mem::MaybeUninit;

#[cfg(not(any(feature = "time", feature = "rdtscp", feature = "software", feature = "hardware")))]
compile_error!("Select one feature: hardware, software, rdtscp, or time");

#[cfg(all(feature = "hardware", feature = "software"))]
compile_error!("Features hardware and software are mutually exclusive");

#[cfg(all(feature = "hardware", feature = "rdtscp"))]
compile_error!("Features hardware and rdtscp are mutually exclusive");

#[cfg(all(feature = "hardware", feature = "time"))]
compile_error!("Features hardware and time are mutually exclusive");

#[cfg(all(feature = "software", feature = "rdtscp"))]
compile_error!("Features software and rdtscp are mutually exclusive");

#[cfg(all(feature = "software", feature = "time"))]
compile_error!("Features software and time are mutually exclusive");

#[cfg(all(feature = "rdtscp", feature = "time"))]
compile_error!("Features rdtscp and time are mutually exclusive");

#[cfg(any(feature = "hardware", feature = "software"))]
use perf_event::{Builder, Group, Counter};
#[cfg(feature = "hardware")]
use perf_event::events::Hardware;
#[cfg(feature = "software")]
use perf_event::events::Software;
#[cfg(feature = "time")]
use std::time::Instant;

#[cfg(feature = "hardware")]
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

#[cfg(feature = "time")]
struct BenchState {
    timer: Option<Instant>,
}

#[cfg(feature = "rdtscp")]
struct BenchState {
    start_ticks: Option<u64>,
}

fn fmt_num(mut n: u64, buf: &mut [MaybeUninit<u8>; 26]) -> &str {
    let mut tmp = [MaybeUninit::<u8>::uninit(); 20];
    let mut tmp_len = 0usize;

    if n == 0 {
        buf[0].write(b'0');
        return unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(buf[0].as_ptr(), 1)) };
    }

    while n > 0 {
        let q = n / 10;
        let r = n - q * 10;
        tmp[tmp_len].write(b'0' + r as u8);
        tmp_len += 1;
        n = q;
    }

    let mut out_len = 0usize;
    let mut next_dot = tmp_len - (tmp_len / 3) * 3;
    if next_dot == 0 { next_dot = 3; }
    let mut count = 0usize;

    for i in (0..tmp_len).rev() {
        if count == next_dot && i != tmp_len - 1 {
            buf[out_len].write(b'.');
            out_len += 1;
            next_dot += 3;
        }
        buf[out_len].write(unsafe { tmp[i].assume_init() });
        out_len += 1;
        count += 1;
    }

    unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(buf[0].as_ptr(), out_len)) }
}

#[cfg(feature = "rdtscp")]
unsafe fn rdtscp() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!(
            "rdtscp",
            out("eax") lo,
            out("edx") hi,
            out("ecx") _,
            options(nostack, nomem),
        );
    }
    ((hi as u64) << 32) | (lo as u64)
}

#[cfg(feature = "rdtscp")]
unsafe fn lfence() {
    unsafe {
        core::arch::asm!(
            "lfence",
            options(nostack, nomem),
        );
    }
}

static BENCH_STATE: OnceLock<Mutex<Option<BenchState>>> = OnceLock::new();

#[unsafe(no_mangle)]
pub extern "C" fn init() -> i32 {
    let mutex = BENCH_STATE.get_or_init(|| Mutex::new(None));
    let mut guard = mutex.lock().unwrap();
    if guard.is_some() { return 0; }

    #[cfg(feature = "time")]
    {
        *guard = Some(BenchState { timer: None });
        return 0;
    }

    #[cfg(feature = "rdtscp")]
    {
        *guard = Some(BenchState { start_ticks: None });
        return 0;
    }

    #[cfg(feature = "hardware")]
    {
        let mut group = match Group::new() {
            Ok(g) => g,
            Err(_) => return -1,
        };

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
        return 0;
    }

    #[cfg(feature = "software")]
    {
        let mut group = match Group::new() {
            Ok(g) => g,
            Err(_) => return -1,
        };

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
        return 0;
    }

    #[allow(unreachable_code)]
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn start() {
    thread::sleep(Duration::from_secs(1));

    if let Some(mutex) = BENCH_STATE.get() {
        let mut guard = mutex.lock().unwrap();
        if let Some(ref mut state) = *guard {
            #[cfg(feature = "time")]
            {
                state.timer = Some(Instant::now());
            }

            #[cfg(feature = "rdtscp")]
            unsafe {
                lfence();
                state.start_ticks = Some(rdtscp());
            }

            #[cfg(any(feature = "hardware", feature = "software"))]
            {
                let _ = state.group.reset();
                let _ = state.group.enable();
            }
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn stop_and_print() {
    if let Some(mutex) = BENCH_STATE.get() {
        let mut guard = mutex.lock().unwrap();
        if let Some(ref mut state) = *guard {
            #[cfg(feature = "time")]
            {
                if let Some(elapsed) = state.timer.take() {
                    println!("\n[PERF REPORT]");
                    println!("  Wall Time : {:.3} ms", elapsed.elapsed().as_secs_f64() * 1000.0);
                }
            }

            #[cfg(feature = "rdtscp")]
            unsafe {
                if let Some(start) = state.start_ticks.take() {
                    let end = rdtscp();
                    lfence();
                    let mut buf = [MaybeUninit::<u8>::uninit(); 26];
                    println!("\n[PERF REPORT]");
                    println!("  RDTSCP Ticks : {} ticks", fmt_num(end - start, &mut buf));
                }
            }

            #[cfg(feature = "hardware")]
            {
                let _ = state.group.disable();

                if let Ok(counts) = state.group.read() {
                    let total_cycles = counts[&state.cycles];
                    let total_instr = counts[&state.instr];
                    let total_cache = counts[&state.cache_misses];
                    let total_branch = counts[&state.branch_misses];
                    let ipc = if total_cycles > 0 { total_instr as f64 / total_cycles as f64 } else { 0.0 };

                    let mut buf = [MaybeUninit::<u8>::uninit(); 26];
                    println!("\n[PERF REPORT]");
                    println!("  CPU Core Cycles        : {} cycles", fmt_num(total_cycles, &mut buf));
                    let mut buf = [MaybeUninit::<u8>::uninit(); 26];
                    println!("  Instructions           : {} instructions", fmt_num(total_instr, &mut buf));
                    println!("  Instructions Per Cycle : {:.3}", ipc);
                    let mut buf = [MaybeUninit::<u8>::uninit(); 26];
                    println!("  Cache Misses           : {} events", fmt_num(total_cache, &mut buf));
                    let mut buf = [MaybeUninit::<u8>::uninit(); 26];
                    println!("  Branch Misses          : {} events", fmt_num(total_branch, &mut buf));
                }
            }

            #[cfg(feature = "software")]
            {
                let _ = state.group.disable();

                if let Ok(counts) = state.group.read() {
                    let clock = counts[&state.task_clock];
                    let faults = counts[&state.page_faults];
                    let ctx_switches = counts[&state.context_switches];
                    let migrations = counts[&state.cpu_migrations];

                    let mut buf = [MaybeUninit::<u8>::uninit(); 26];
                    println!("\n[PERF REPORT]");
                    println!("  Task Clock (Duration) : {:.3} ms", clock as f64 / 1_000_000.0);
                    println!("  Page Faults           : {} events", fmt_num(faults, &mut buf));
                    let mut buf = [MaybeUninit::<u8>::uninit(); 26];
                    println!("  Context Switches      : {} events", fmt_num(ctx_switches, &mut buf));
                    let mut buf = [MaybeUninit::<u8>::uninit(); 26];
                    println!("  CPU Migrations        : {} events", fmt_num(migrations, &mut buf));
                }
            }
        }
    } else {
        println!("[PERF ERROR] Shared library not initialized!");
    }
}
