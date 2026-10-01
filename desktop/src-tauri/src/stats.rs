//! The machine's load and every process's share of it, for the footer bar's
//! CPU, GPU, memory and disk read-outs and the overlays behind them.
//!
//! Two read-only commands. `host_info` says what the machine is, once: its
//! name, OS, CPU and logical core count. `resource_usage` says what it is
//! doing now — CPU, load, memory, swap, the GPU where `gpu.rs` has a reader —
//! and what each process the app can name takes of it: this app, the node,
//! every terminal's shell and every harness session the node reports, with
//! their whole process trees, each attributed to the nearest of those roots
//! (`attribution.rs`).
//!
//! The process table is kept between reads (`Table`) and refreshed once per
//! read: a CPU or disk figure is the delta between two samples, and with a
//! table read fresh every time the first sample was always this read's own,
//! which is why the old read slept 200 ms in the middle and still saw no disk
//! activity. Kept, the two samples are the last read and this one — the
//! footer's own cadence — and an executable is resolved once per process
//! lifetime rather than once per tick. The one shell-out among these readers
//! is `gpu.rs`'s, and it says why.

use crate::sync::Locked;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use sysinfo::{
    CpuRefreshKind, MemoryRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind,
    System, UpdateKind,
};
use tauri::State;

use crate::attribution::{Root, Roots};
use crate::gpu::{self, GpuLoad};
use crate::sidecar::NodeState;
use crate::terminal::TerminalRegistry;

/// What the machine is — read once.
#[derive(Debug, Clone, Serialize)]
pub struct HostInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    /// Logical cores — what a process's `cpu_percent` (per core) is divided
    /// by to stack beside the host's 0–100 figure.
    pub cores: usize,
    /// The CPU's brand, `Apple M2 Max` — also the GPU's name on a machine
    /// whose accelerator is on the same chip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<String>,
}

/// What the machine is doing now.
#[derive(Debug, Clone, Serialize)]
pub struct HostLoad {
    /// System-wide CPU usage, 0–100.
    pub cpu_percent: f32,
    /// The load averages over one, five and fifteen minutes.
    pub load: [f64; 3],
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub uptime_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu: Option<GpuLoad>,
}

/// One process the app can name, and what it takes.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessShare {
    pub pid: u32,
    /// The executable's file name, else the short name the OS keeps.
    pub name: String,
    pub root: Root,
    /// Per core, so a busy tree can exceed 100.
    pub cpu_percent: f32,
    pub mem_bytes: u64,
    /// Bytes read and written since the last read — the interval's activity,
    /// never a cumulative count.
    pub read_bytes: u64,
    pub written_bytes: u64,
    pub run_secs: u64,
}

/// One read: the host and every named process's share.
#[derive(Debug, Clone, Serialize)]
pub struct ResourceUsage {
    pub host: HostLoad,
    pub processes: Vec<ProcessShare>,
    /// Seconds between this read and the last — what the deltas span.
    pub interval_secs: f32,
    /// How long the read took.
    pub read_ms: u32,
}

/// The process table kept between reads.
pub struct Table {
    sys: Mutex<Sampled>,
}

struct Sampled {
    system: System,
    last: Option<std::time::Instant>,
}

/// A percentage to one decimal, as the footer draws it.
fn tenths(v: f32) -> f32 {
    (v * 10.0).round() / 10.0
}

/// The CPU's brand as sysinfo names it — trimmed, and none when it is blank.
fn brand_of(brand: Option<&str>) -> Option<String> {
    brand
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .map(str::to_string)
}

/// A process's name: its executable's file name, else the short name the OS keeps.
fn name_of(exe: Option<&std::path::Path>, fallback: &str) -> String {
    exe.and_then(|e| e.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| fallback.to_string())
}

/// The seconds a read's deltas span: since the last read, else the minimum sysinfo needs for a first sample.
fn interval_of(last: Option<std::time::Instant>, now: std::time::Instant) -> f32 {
    last.map(|t| now.duration_since(t).as_secs_f32())
        .unwrap_or(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.as_secs_f32())
}

/// Milliseconds as the wire carries them, never wrapping.
fn capped_ms(elapsed: std::time::Duration) -> u32 {
    elapsed.as_millis().min(u32::MAX as u128) as u32
}

fn process_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::new()
        .with_memory()
        .with_cpu()
        .with_disk_usage()
        .with_exe(UpdateKind::OnlyIfNotSet)
}

impl Table {
    pub fn new() -> Self {
        let system = System::new_with_specifics(
            RefreshKind::new()
                .with_memory(MemoryRefreshKind::everything())
                .with_cpu(CpuRefreshKind::everything())
                .with_processes(process_kind()),
        );
        Self {
            sys: Mutex::new(Sampled { system, last: None }),
        }
    }

    /// The names of every process running now — what the network facts read
    /// a VPN provider's daemon off. Its own refresh, names only.
    pub fn process_names(&self) -> Vec<String> {
        let mut guard = self.sys.locked();
        guard.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::new(),
        );
        guard
            .system
            .processes()
            .values()
            .map(|p| p.name().to_string_lossy().into_owned())
            .collect()
    }

    /// Refresh once and read. The very first read has only its own sample
    /// to compare against, so it waits the minimum sysinfo needs and samples
    /// again; every later read compares against the last.
    fn read(&self, roots: &Roots) -> ResourceUsage {
        let started = std::time::Instant::now();
        let mut guard = self.sys.locked();
        let Sampled { system, last } = &mut *guard;
        if last.is_none() {
            std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        }
        system.refresh_memory();
        system.refresh_cpu_usage();
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, process_kind());
        let now = std::time::Instant::now();
        let interval_secs = interval_of(*last, now);
        *last = Some(now);

        let parent_of = |pid: u32| {
            system
                .process(Pid::from_u32(pid))
                .and_then(|p| p.parent())
                .map(|p| p.as_u32())
        };
        let mut processes: Vec<ProcessShare> = system
            .processes()
            .iter()
            .filter_map(|(pid, p)| {
                let root = roots.chain(pid.as_u32(), &parent_of).nearest?;
                let io = p.disk_usage();
                Some(ProcessShare {
                    pid: pid.as_u32(),
                    name: name_of(p.exe(), &p.name().to_string_lossy()),
                    root,
                    cpu_percent: tenths(p.cpu_usage()),
                    mem_bytes: p.memory(),
                    read_bytes: io.read_bytes,
                    written_bytes: io.written_bytes,
                    run_secs: p.run_time(),
                })
            })
            .collect();
        processes.sort_by_key(|p| p.pid);

        let load = System::load_average();
        ResourceUsage {
            host: HostLoad {
                cpu_percent: tenths(system.global_cpu_usage()),
                load: [load.one, load.five, load.fifteen],
                mem_used: system.used_memory(),
                mem_total: system.total_memory(),
                swap_used: system.used_swap(),
                swap_total: system.total_swap(),
                uptime_secs: System::uptime(),
                gpu: gpu::read(),
            },
            processes,
            interval_secs,
            read_ms: capped_ms(started.elapsed()),
        }
    }
}

/// What the machine is.
#[tauri::command]
pub fn host_info(table: State<'_, Arc<Table>>) -> HostInfo {
    let guard = table.sys.locked();
    HostInfo {
        hostname: System::host_name(),
        os: System::long_os_version(),
        cores: guard.system.cpus().len().max(1),
        cpu: brand_of(guard.system.cpus().first().map(|c| c.brand())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{Duration, Instant};

    #[test]
    fn a_percentage_is_drawn_to_one_decimal_and_a_brand_is_trimmed_or_none() {
        assert_eq!(tenths(12.345), 12.3);
        assert_eq!(tenths(99.96), 100.0);
        assert_eq!(tenths(0.0), 0.0);
        assert_eq!(
            brand_of(Some("  Apple M2 Max ")),
            Some("Apple M2 Max".to_string())
        );
        assert_eq!(brand_of(Some("   ")), None);
        assert_eq!(brand_of(None), None);
    }

    #[test]
    fn a_process_is_named_by_its_executable_else_the_short_name_the_os_keeps() {
        assert_eq!(name_of(Some(Path::new("/usr/bin/zsh")), "zsh"), "zsh");
        assert_eq!(
            name_of(
                Some(Path::new("/Applications/Bisa.app/Contents/MacOS/bisa")),
                "bisa-node"
            ),
            "bisa"
        );
        assert_eq!(name_of(None, "kernel_task"), "kernel_task");
        assert_eq!(
            name_of(Some(Path::new("/")), "root"),
            "root",
            "a path with no file name falls back"
        );
    }

    #[test]
    fn the_interval_is_the_time_since_the_last_read_and_a_first_read_spans_the_minimum_sample() {
        let now = Instant::now();
        let last = now - Duration::from_millis(5000);
        assert!((interval_of(Some(last), now) - 5.0).abs() < 0.01);
        assert_eq!(
            interval_of(None, now),
            sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.as_secs_f32()
        );
        assert_eq!(capped_ms(Duration::from_millis(42)), 42);
        assert_eq!(
            capped_ms(Duration::from_secs(u64::MAX / 1000)),
            u32::MAX,
            "never wraps"
        );
    }

    #[test]
    fn a_read_names_this_process_under_the_desktop_root_with_sane_host_figures() {
        // The one read against the machine: sysinfo alone, nothing spawned.
        let table = Table::new();
        let roots = Roots::new(
            Some(std::process::id()),
            None,
            Vec::<(String, u32)>::new(),
            Vec::<u32>::new(),
        );
        let usage = table.read(&roots);
        assert!(usage.host.mem_total > 0);
        assert!(usage.host.cpu_percent >= 0.0);
        assert!(usage.interval_secs > 0.0);
        let me = usage
            .processes
            .iter()
            .find(|p| p.pid == std::process::id())
            .expect("this process is a named root");
        assert_eq!(me.root, Root::Desktop);
        assert!(!me.name.is_empty());
        assert!(
            usage.processes.windows(2).all(|w| w[0].pid < w[1].pid),
            "sorted by pid"
        );
        // A second read compares against the first: its interval is the gap, not the minimum.
        let again = table.read(&roots);
        assert!(again.interval_secs >= 0.0);
        assert!(!table.process_names().is_empty());
    }
}

/// The machine's load and every named process's share of it.
///
/// `roots` are the harness session pids the webview gathers from the roster;
/// this process, the node and every live terminal's shell come from the
/// shell's own state.
#[tauri::command]
pub async fn resource_usage(
    roots: Vec<u32>,
    node: State<'_, NodeState>,
    registry: State<'_, Arc<TerminalRegistry>>,
    table: State<'_, Arc<Table>>,
) -> Result<ResourceUsage, String> {
    let roots = Roots::new(
        Some(std::process::id()),
        node.status().pid,
        registry.pids(),
        roots,
    );
    let table = Arc::clone(&table);
    tauri::async_runtime::spawn_blocking(move || table.read(&roots))
        .await
        .map_err(|e| format!("the resource read did not finish: {e}"))
}
