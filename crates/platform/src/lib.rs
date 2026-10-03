#[cfg(target_os = "linux")]
pub mod linux {
    use ferrite_metrics::{
        BatterySample, BatteryStatus, CpuPressureSample, CpuSample, DiscreteGpuSample, RaplSample,
        RuntimePmStatus, SystemSnapshot,
    };
    use std::{
        fs::{self, File},
        io::{self, Read},
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[derive(Debug)]
    struct BatteryPaths {
        capacity: Option<PathBuf>,
        status: Option<PathBuf>,
        power_now: Option<PathBuf>,
        energy_now: Option<PathBuf>,
        current_now: Option<PathBuf>,
        voltage_now: Option<PathBuf>,
    }

    impl BatteryPaths {
        fn new(root: PathBuf) -> Self {
            Self {
                capacity: existing(root.join("capacity")),
                status: existing(root.join("status")),
                power_now: existing(root.join("power_now")),
                energy_now: existing(root.join("energy_now")),
                current_now: existing(root.join("current_now")),
                voltage_now: existing(root.join("voltage_now")),
            }
        }
    }

    #[derive(Debug)]
    struct RaplPaths {
        energy_uj: PathBuf,
        max_energy_range_uj: Option<PathBuf>,
    }

    #[derive(Debug)]
    pub struct LinuxTelemetry {
        battery: Option<BatteryPaths>,
        rapl: Option<RaplPaths>,
        nvidia_runtime_status: Option<PathBuf>,
    }

    impl LinuxTelemetry {
        pub fn discover() -> io::Result<Self> {
            Ok(Self {
                battery: discover_battery().map(BatteryPaths::new),
                rapl: discover_rapl(),
                nvidia_runtime_status: discover_nvidia_runtime_status(),
            })
        }

        pub fn sample(&self) -> io::Result<SystemSnapshot> {
            Ok(SystemSnapshot {
                timestamp_ms: unix_time_ms(),
                battery: self.sample_battery(),
                rapl: self.sample_rapl(),
                cpu: sample_cpu()?,
                cpu_pressure: sample_cpu_pressure().unwrap_or_default(),
                discrete_gpu: self.sample_discrete_gpu(),
            })
        }

        fn sample_battery(&self) -> BatterySample {
            let Some(paths) = &self.battery else {
                return BatterySample::default();
            };

            let capacity_percent = paths
                .capacity
                .as_deref()
                .and_then(read_u64)
                .and_then(|value| u8::try_from(value.min(100)).ok());
            let status = paths
                .status
                .as_deref()
                .map(read_battery_status)
                .unwrap_or_default();
            let energy_uwh = paths.energy_now.as_deref().and_then(read_u64);

            let power_uw = paths.power_now.as_deref().and_then(read_u64).or_else(|| {
                let current_ua = paths.current_now.as_deref().and_then(read_u64)?;
                let voltage_uv = paths.voltage_now.as_deref().and_then(read_u64)?;
                let microwatts = (u128::from(current_ua) * u128::from(voltage_uv)) / 1_000_000;
                u64::try_from(microwatts).ok()
            });

            BatterySample {
                present: true,
                capacity_percent,
                status,
                power_uw,
                energy_uwh,
            }
        }

        fn sample_rapl(&self) -> RaplSample {
            let Some(paths) = &self.rapl else {
                return RaplSample::default();
            };

            RaplSample {
                package_energy_uj: read_u64(&paths.energy_uj),
                max_energy_range_uj: paths.max_energy_range_uj.as_deref().and_then(read_u64),
            }
        }

        fn sample_discrete_gpu(&self) -> DiscreteGpuSample {
            let Some(path) = &self.nvidia_runtime_status else {
                return DiscreteGpuSample::default();
            };

            DiscreteGpuSample {
                present: true,
                runtime_status: read_runtime_pm_status(path),
            }
        }
    }

    fn existing(path: PathBuf) -> Option<PathBuf> {
        path.exists().then_some(path)
    }

    fn discover_battery() -> Option<PathBuf> {
        let entries = fs::read_dir("/sys/class/power_supply").ok()?;
        for entry in entries.flatten() {
            let root = entry.path();
            let kind = fs::read_to_string(root.join("type")).ok()?;
            if kind.trim() == "Battery" {
                return Some(root);
            }
        }
        None
    }

    fn discover_rapl() -> Option<RaplPaths> {
        for root in [
            Path::new("/sys/class/powercap/intel-rapl:0"),
            Path::new("/sys/devices/virtual/powercap/intel-rapl:0"),
        ] {
            let energy_uj = root.join("energy_uj");
            if energy_uj.exists() {
                return Some(RaplPaths {
                    energy_uj,
                    max_energy_range_uj: existing(root.join("max_energy_range_uj")),
                });
            }
        }
        None
    }

    fn discover_nvidia_runtime_status() -> Option<PathBuf> {
        let entries = fs::read_dir("/sys/bus/pci/devices").ok()?;
        for entry in entries.flatten() {
            let root = entry.path();
            if read_hex_u32(&root.join("vendor")) != Some(0x10de) {
                continue;
            }
            let class = read_hex_u32(&root.join("class"))?;
            if (class >> 16) != 0x03 {
                continue;
            }
            let runtime = root.join("power/runtime_status");
            if runtime.exists() {
                return Some(runtime);
            }
        }
        None
    }

    fn sample_cpu() -> io::Result<CpuSample> {
        let mut buffer = [0_u8; 512];
        let text = read_small(Path::new("/proc/stat"), &mut buffer)?;
        let line = text.lines().next().ok_or_else(|| io::Error::other("missing /proc/stat cpu line"))?;
        let mut fields = line.split_whitespace();
        if fields.next() != Some("cpu") {
            return Err(io::Error::other("invalid /proc/stat cpu line"));
        }

        let mut values = [0_u64; 8];
        for value in &mut values {
            *value = fields.next().and_then(|field| field.parse().ok()).unwrap_or(0);
        }

        let idle_ticks = values[3].saturating_add(values[4]);
        let total_ticks = values.iter().copied().sum();
        Ok(CpuSample { total_ticks, idle_ticks })
    }

    fn sample_cpu_pressure() -> io::Result<CpuPressureSample> {
        let mut buffer = [0_u8; 512];
        let text = read_small(Path::new("/proc/pressure/cpu"), &mut buffer)?;
        let line = text
            .lines()
            .find(|line| line.starts_with("some "))
            .ok_or_else(|| io::Error::other("missing CPU PSI some line"))?;

        let mut sample = CpuPressureSample::default();
        for field in line.split_whitespace().skip(1) {
            let Some((key, value)) = field.split_once('=') else {
                continue;
            };
            match key {
                "avg10" => sample.some_avg10 = value.parse().unwrap_or(0.0),
                "avg60" => sample.some_avg60 = value.parse().unwrap_or(0.0),
                "avg300" => sample.some_avg300 = value.parse().unwrap_or(0.0),
                "total" => sample.some_total_us = value.parse().unwrap_or(0),
                _ => {}
            }
        }
        Ok(sample)
    }

    fn read_u64(path: &Path) -> Option<u64> {
        let mut buffer = [0_u8; 64];
        read_small(path, &mut buffer).ok()?.trim().parse().ok()
    }

    fn read_hex_u32(path: &Path) -> Option<u32> {
        let mut buffer = [0_u8; 32];
        let text = read_small(path, &mut buffer).ok()?.trim();
        let text = text.strip_prefix("0x").unwrap_or(text);
        u32::from_str_radix(text, 16).ok()
    }

    fn read_battery_status(path: &Path) -> BatteryStatus {
        let mut buffer = [0_u8; 32];
        let Ok(value) = read_small(path, &mut buffer) else {
            return BatteryStatus::Unknown;
        };
        match value.trim() {
            "Charging" => BatteryStatus::Charging,
            "Discharging" => BatteryStatus::Discharging,
            "Full" => BatteryStatus::Full,
            "Not charging" => BatteryStatus::NotCharging,
            _ => BatteryStatus::Unknown,
        }
    }

    fn read_runtime_pm_status(path: &Path) -> RuntimePmStatus {
        let mut buffer = [0_u8; 32];
        let Ok(value) = read_small(path, &mut buffer) else {
            return RuntimePmStatus::Unsupported;
        };
        match value.trim() {
            "active" => RuntimePmStatus::Active,
            "suspended" => RuntimePmStatus::Suspended,
            "suspending" => RuntimePmStatus::Suspending,
            "resuming" => RuntimePmStatus::Resuming,
            _ => RuntimePmStatus::Unknown,
        }
    }

    fn read_small<'a>(path: &Path, buffer: &'a mut [u8]) -> io::Result<&'a str> {
        let mut file = File::open(path)?;
        let read = file.read(buffer)?;
        std::str::from_utf8(&buffer[..read]).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    fn unix_time_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|duration| u64::try_from(duration.as_millis()).ok())
            .unwrap_or(0)
    }
}
