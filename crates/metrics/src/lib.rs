#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BatteryStatus {
    Charging,
    Discharging,
    Full,
    NotCharging,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuntimePmStatus {
    Active,
    Suspended,
    Suspending,
    Resuming,
    Unsupported,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BatterySample {
    pub present: bool,
    pub capacity_percent: Option<u8>,
    pub status: BatteryStatus,
    pub power_uw: Option<u64>,
    pub energy_uwh: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RaplSample {
    pub package_energy_uj: Option<u64>,
    pub max_energy_range_uj: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CpuSample {
    pub total_ticks: u64,
    pub idle_ticks: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CpuPressureSample {
    pub some_avg10: f32,
    pub some_avg60: f32,
    pub some_avg300: f32,
    pub some_total_us: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DiscreteGpuSample {
    pub present: bool,
    pub runtime_status: RuntimePmStatus,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemSnapshot {
    pub timestamp_ms: u64,
    pub battery: BatterySample,
    pub rapl: RaplSample,
    pub cpu: CpuSample,
    pub cpu_pressure: CpuPressureSample,
    pub discrete_gpu: DiscreteGpuSample,
}
