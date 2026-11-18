//! Idle resource probe (SC-004): starts VT Tray, lets it settle, samples its memory and CPU
//! through the Windows process APIs, then closes it the same way its Exit command does.
//!
//!   cargo run --release --example memprobe -- [seconds] [path-to-VTray.exe]
//!
//! Exits with code 1 when the average working set exceeds 25 MB or idle CPU exceeds 1%.
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{CloseHandle, FILETIME};
use windows::Win32::System::ProcessStatus::{
    GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

const MAX_WORKING_SET_MB: f64 = 25.0;
const MAX_IDLE_CPU_PERCENT: f64 = 1.0;
const MB: f64 = 1024.0 * 1024.0;

fn ticks(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seconds: u64 = args
        .first()
        .and_then(|value| value.parse().ok())
        .unwrap_or(60);
    let exe = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "target/release/VTray.exe".to_string());

    let mut child = std::process::Command::new(&exe)
        .spawn()
        .unwrap_or_else(|error| panic!("could not start {exe}: {error}"));
    std::thread::sleep(Duration::from_secs(40)); // warm-up, past the 30 s idle trim
    if let Ok(Some(status)) = child.try_wait() {
        eprintln!("VT Tray exited early ({status}); is another copy already running?");
        std::process::exit(2);
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, child.id()) }
        .expect("open VT Tray process");
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as f64;

    let cpu_ticks = || {
        let (mut created, mut exited, mut kernel, mut user) = Default::default();
        unsafe { GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) }
            .expect("process times");
        ticks(kernel) + ticks(user)
    };
    let memory = || {
        let mut counters = PROCESS_MEMORY_COUNTERS_EX::default();
        unsafe {
            GetProcessMemoryInfo(
                process,
                &mut counters as *mut _ as *mut PROCESS_MEMORY_COUNTERS,
                std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            )
        }
        .expect("memory info");
        counters
    };

    let (start_cpu, start) = (cpu_ticks(), Instant::now());
    let (mut working_sum, mut working_max, mut private_max, mut samples) = (0.0, 0.0f64, 0.0f64, 0);
    while start.elapsed() < Duration::from_secs(seconds) {
        let counters = memory();
        let working = counters.WorkingSetSize as f64 / MB;
        working_sum += working;
        working_max = working_max.max(working);
        private_max = private_max.max(counters.PrivateUsage as f64 / MB);
        samples += 1;
        std::thread::sleep(Duration::from_secs(1));
    }
    // FILETIME ticks are 100 ns.
    let busy = (cpu_ticks() - start_cpu) as f64 / 10_000_000.0;
    let cpu = busy / start.elapsed().as_secs_f64() / cores * 100.0;
    let peak = memory().PeakWorkingSetSize as f64 / MB;
    unsafe {
        let _ = CloseHandle(process);
    }
    vt_tray::platform::windows::ipc::close_running(Duration::from_secs(5));
    let _ = child.wait();

    let average = working_sum / f64::from(samples.max(1));
    println!("VT Tray idle over {seconds} s ({samples} samples, {cores} cores):");
    println!("  working set  avg {average:.1} MB, max {working_max:.1} MB, peak {peak:.1} MB");
    println!("  private      max {private_max:.1} MB");
    println!("  CPU          {cpu:.3} % of the machine ({busy:.2} CPU-seconds)");
    let ok = average <= MAX_WORKING_SET_MB && cpu <= MAX_IDLE_CPU_PERCENT;
    println!(
        "  budget       {} (<= {MAX_WORKING_SET_MB} MB, <= {MAX_IDLE_CPU_PERCENT} %)",
        if ok { "PASS" } else { "FAIL" }
    );
    std::process::exit(if ok { 0 } else { 1 });
}
