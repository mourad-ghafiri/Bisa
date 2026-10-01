//! The GPU's load, for the footer.
//!
//! The one shell-out among the machine readers, and why: on macOS the
//! accelerator driver publishes its statistics as an IORegistry property
//! (`IOAccelerator` › `PerformanceStatistics`), and nothing in the
//! dependencies reads the IORegistry — `sysinfo` links IOKit for its own
//! reads but exposes none of it. `/usr/sbin/ioreg` is the OS's own,
//! unprivileged reader of that registry and answers in tens of
//! milliseconds; per-process GPU time needs elevated access and is not read.
//! Any other platform answers `None` and the footer draws no GPU read-out.
//!
//! The child runs under `probe`'s wall-clock budget and is ended past it: a
//! reader that hangs would pin the footer's in-flight mark and freeze every
//! read-out behind it.

use serde::Serialize;

/// What the accelerator reports about itself.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GpuLoad {
    /// Device utilisation, 0–100.
    pub util_percent: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renderer_percent: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiler_percent: Option<f32>,
    /// System memory the accelerator has in use, in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_used: Option<u64>,
}

/// The accelerator's statistics line, parsed: the busiest accelerator when
/// several report (an Intel Mac with two), `None` when none does.
pub fn parse(text: &str) -> Option<GpuLoad> {
    text.lines()
        .filter(|l| l.contains("\"PerformanceStatistics\""))
        .filter_map(parse_line)
        .max_by(|a, b| a.util_percent.total_cmp(&b.util_percent))
}

fn parse_line(line: &str) -> Option<GpuLoad> {
    let util = number(line, "Device Utilization %")?;
    Some(GpuLoad {
        util_percent: util as f32,
        renderer_percent: number(line, "Renderer Utilization %").map(|n| n as f32),
        tiler_percent: number(line, "Tiler Utilization %").map(|n| n as f32),
        mem_used: number(line, "In use system memory"),
    })
}

/// The integer after `"key"=` on a line, if the key is there.
fn number(line: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\"=");
    let start = line.find(&needle)? + needle.len();
    let digits: String = line[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// How long `ioreg` may take before it is given up on.
#[cfg(target_os = "macos")]
const BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

/// The GPU's load now, or `None` where there is no reader.
#[cfg(target_os = "macos")]
pub fn read() -> Option<GpuLoad> {
    let out = crate::probe::run(
        "/usr/sbin/ioreg",
        &["-r", "-d", "1", "-c", "IOAccelerator"],
        BUDGET,
    )?;
    parse(&out.stdout)
}

#[cfg(not(target_os = "macos"))]
pub fn read() -> Option<GpuLoad> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = r#"      "PerformanceStatistics" = {"In use system memory (driver)"=0,"Alloc system memory"=2166734848,"Tiler Utilization %"=20,"recoveryCount"=0,"lastRecoveryTime"=0,"Renderer Utilization %"=15,"TiledSceneBytes"=2129920,"Device Utilization %"=30,"SplitSceneCount"=0,"Allocated PB Size"=67371008,"In use system memory"=560513024}"#;

    #[test]
    fn the_statistics_line_parses_to_the_four_figures() {
        let text = format!("+-o AGXAcceleratorG14X  <class AGXAcceleratorG14X>\n    {{\n{LINE}\n      \"IOClass\" = \"AGXAcceleratorG14X\"\n    }}\n");
        assert_eq!(
            parse(&text),
            Some(GpuLoad {
                util_percent: 30.0,
                renderer_percent: Some(15.0),
                tiler_percent: Some(20.0),
                mem_used: Some(560_513_024),
            })
        );
    }

    #[test]
    fn no_accelerator_is_none_and_a_line_without_utilisation_is_skipped() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("+-o Nothing  <class IOService>\n"), None);
        assert_eq!(
            parse(r#""PerformanceStatistics" = {"Alloc system memory"=1}"#),
            None
        );
    }

    #[test]
    fn with_two_accelerators_the_busier_one_is_reported() {
        let quiet = r#""PerformanceStatistics" = {"Device Utilization %"=3}"#;
        let text = format!("{quiet}\n{LINE}\n");
        assert_eq!(parse(&text).map(|g| g.util_percent), Some(30.0));
        assert_eq!(
            parse(&format!("{LINE}\n{quiet}\n")).map(|g| g.util_percent),
            Some(30.0)
        );
    }
}
