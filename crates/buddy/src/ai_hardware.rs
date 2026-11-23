//! A best-effort look at what this machine can run. Every probe is optional: a missing
//! tool, a broken driver or an unusual layout leaves a field empty and must never stop
//! Buddy from starting.

use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::process::Command;
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gpu {
    pub name: String,
    pub vendor: String,
    pub vram_gb: f64,
    pub driver: String,
    pub compute: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hardware {
    pub cpu: String,
    pub cores: usize,
    pub avx2: bool,
    pub ram_gb: f64,
    pub ram_available_gb: f64,
    pub gpu: Option<Gpu>,
    pub free_disk_gb: f64,
    pub system_ollama: bool,
}

/// `MemTotal` from `/proc/meminfo`, in GiB.
pub fn parse_meminfo(text: &str) -> Option<f64> {
    let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024.0 / 1024.0)
}

/// `MemAvailable` from `/proc/meminfo`, in GiB.
pub fn parse_mem_available(text: &str) -> Option<f64> {
    let line = text.lines().find(|l| l.starts_with("MemAvailable:"))?;
    let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024.0 / 1024.0)
}

/// The first `model name`, the number of `processor` entries, and whether AVX2 is present.
pub fn parse_cpuinfo(text: &str) -> (String, usize, bool) {
    let cpu = text
        .lines()
        .find_map(|l| l.strip_prefix("model name"))
        .and_then(|rest| rest.split_once(':').map(|(_, v)| v.trim().to_string()))
        .unwrap_or_default();
    let cores = text.lines().filter(|l| l.starts_with("processor")).count();
    let avx2 = text.lines().any(|l| l.starts_with("flags") && l.split_whitespace().any(|f| f == "avx2"));
    (cpu, cores, avx2)
}

/// `nvidia-smi --query-gpu=name,memory.total,driver_version,compute_cap --format=csv,noheader,nounits`
pub fn parse_nvidia_smi(text: &str) -> Option<Gpu> {
    let line = text.lines().find(|l| !l.trim().is_empty())?;
    let parts: Vec<&str> = line.split(',').map(str::trim).collect();
    if parts.len() < 3 {
        return None;
    }
    let vram_mb: f64 = parts[1].parse().ok()?;
    Some(Gpu {
        name: parts[0].to_string(),
        vendor: "NVIDIA".into(),
        vram_gb: vram_mb / 1024.0,
        driver: parts[2].to_string(),
        compute: parts.get(3).and_then(|v| v.parse().ok()),
    })
}

/// The free space from `df -Pk`, in GiB.
pub fn parse_df(text: &str) -> Option<f64> {
    let line = text.lines().nth(1)?;
    let kb: f64 = line.split_whitespace().nth(3)?.parse().ok()?;
    Some(kb / 1024.0 / 1024.0)
}

fn read(path: &str) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn command(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

fn command_with_timeout(program: &str, args: &[&str]) -> Option<String> {
    // `timeout` is coreutils and present on the supported platform. Without it, run the
    // command directly; a missing tool is handled by `command`.
    if let Ok(out) = Command::new("timeout").arg("3").arg(program).args(args).output() {
        return if out.status.success() { String::from_utf8(out.stdout).ok() } else { None };
    }
    command(program, args)
}

/// Look at the machine. All failures are silent and leave a field empty.
pub fn probe() -> Hardware {
    let mut hardware = Hardware::default();
    if let Some(text) = read("/proc/meminfo") {
        hardware.ram_gb = parse_meminfo(&text).unwrap_or_default();
        hardware.ram_available_gb = parse_mem_available(&text).unwrap_or_default();
    }
    if let Some(text) = read("/proc/cpuinfo") {
        let (cpu, cores, avx2) = parse_cpuinfo(&text);
        hardware.cpu = cpu;
        hardware.cores = cores;
        hardware.avx2 = avx2;
    }
    if let Some(text) =
        command_with_timeout("nvidia-smi", &["--query-gpu=name,memory.total,driver_version,compute_cap", "--format=csv,noheader,nounits"])
    {
        hardware.gpu = parse_nvidia_smi(&text);
    }
    if let Some(text) = command("df", &["-Pk", &buddy_core::paths::data_dir().display().to_string()]) {
        hardware.free_disk_gb = parse_df(&text).unwrap_or_default();
    }
    let system = SocketAddr::from(([127, 0, 0, 1], 11434));
    hardware.system_ollama = TcpStream::connect_timeout(&system, Duration::from_millis(300)).is_ok();
    hardware
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_is_read_in_gibibytes() {
        let text = "MemTotal:       32692636 kB\nMemAvailable:   22252128 kB\n";
        let gb = parse_meminfo(text).unwrap();
        assert!((gb - 31.18).abs() < 0.01, "{gb}");
        let available = parse_mem_available(text).unwrap();
        assert!((available - 21.22).abs() < 0.01, "{available}");
        assert_eq!(parse_meminfo("nothing here"), None);
        assert_eq!(parse_mem_available("nothing here"), None);
    }

    #[test]
    fn the_cpu_model_cores_and_avx2_are_read() {
        let text = "\
processor\t: 0
model name\t: Intel(R) Core(TM) i5-10300H CPU @ 2.50GHz
flags\t\t: fpu vme avx2 sse4_2

processor\t: 1
model name\t: Intel(R) Core(TM) i5-10300H CPU @ 2.50GHz
flags\t\t: fpu vme avx2 sse4_2
";
        let (cpu, cores, avx2) = parse_cpuinfo(text);
        assert_eq!(cpu, "Intel(R) Core(TM) i5-10300H CPU @ 2.50GHz");
        assert_eq!(cores, 2);
        assert!(avx2);
        assert_eq!(parse_cpuinfo("no cpu here"), (String::new(), 0, false));
    }

    #[test]
    fn an_nvidia_gpu_is_read_from_the_csv_query() {
        let text = "NVIDIA GeForce GTX 1650, 4096, 615.71.09, 7.5\n";
        let gpu = parse_nvidia_smi(text).unwrap();
        assert_eq!(gpu.name, "NVIDIA GeForce GTX 1650");
        assert_eq!(gpu.vendor, "NVIDIA");
        assert!((gpu.vram_gb - 4.0).abs() < 0.001);
        assert_eq!(gpu.driver, "615.71.09");
        assert_eq!(gpu.compute, Some(7.5));
        assert_eq!(parse_nvidia_smi(""), None);
    }

    #[test]
    fn free_disk_is_read_from_df() {
        let text = "Filesystem     1024-blocks      Used Available Capacity Mounted on\n/dev/nvme0n1p2   100000000  50000000  50000000      50% /\n";
        let gb = parse_df(text).unwrap();
        assert!((gb - 47.68).abs() < 0.01, "{gb}");
        assert_eq!(parse_df("only one line"), None);
    }
}
