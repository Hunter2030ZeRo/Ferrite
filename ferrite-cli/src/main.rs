#[cfg(not(target_os = "linux"))]
compile_error!("ferrite-cli v0.1 currently supports Linux only");

#[cfg(target_os = "linux")]
fn main() -> std::io::Result<()> {
    use ferrite_analyzer::PowerAnalyzer;
    use ferrite_metrics::BatteryStatus;
    use ferrite_platform::linux::LinuxTelemetry;
    use ferrite_policy::PolicyEngine;
    use ferrite_predictor::{HeuristicPredictor, Predictor};
    use std::{thread, time::Duration};

    let command = std::env::args().nth(1).unwrap_or_else(|| "status".to_owned());
    if command != "status" {
        eprintln!("usage: ferrite-cli status");
        std::process::exit(2);
    }

    let telemetry = LinuxTelemetry::discover()?;
    let mut analyzer = PowerAnalyzer::new();
    let mut predictor = HeuristicPredictor;
    let mut policy = PolicyEngine;

    let first = telemetry.sample()?;
    analyzer.update(&first);
    thread::sleep(Duration::from_millis(250));
    let snapshot = telemetry.sample()?;
    let derived = analyzer.update(&snapshot);
    let prediction = predictor.predict(&snapshot, &derived);
    let desired = policy.decide(prediction, snapshot.battery.status);

    println!("Ferrite v0.1 — observe-only");
    println!();

    if snapshot.battery.present {
        let capacity = snapshot
            .battery
            .capacity_percent
            .map(|value| format!("{value}%"))
            .unwrap_or_else(|| "?".to_owned());
        let power = snapshot
            .battery
            .power_uw
            .map(|value| format!("{:.2} W", value as f64 / 1_000_000.0))
            .unwrap_or_else(|| "?".to_owned());
        println!("Battery   {capacity:>5}  {:<13} {power}", battery_status(snapshot.battery.status));
    } else {
        println!("Battery   not discovered");
    }

    println!(
        "CPU       util {:>7}  package {:>7}  PSI avg10 {:.2}%",
        optional_percent(derived.cpu_utilization),
        optional_watts(derived.package_power_w),
        snapshot.cpu_pressure.some_avg10,
    );

    println!(
        "System    power {:>7}  unattributed {:>7}",
        optional_watts(derived.system_power_w),
        optional_watts(derived.unattributed_power_w),
    );

    if snapshot.discrete_gpu.present {
        println!("dGPU      NVIDIA runtime PM: {}", runtime_status(snapshot.discrete_gpu.runtime_status));
    } else {
        println!("dGPU      NVIDIA display device not discovered");
    }

    println!();
    println!("Workload  {:?} ({:.0}% confidence)", prediction.class, prediction.confidence * 100.0);
    println!(
        "Proposed  EPP={:?}, max_perf={}%  [NOT APPLIED]",
        desired.epp, desired.max_performance_percent
    );

    if snapshot.battery.status == BatteryStatus::Discharging {
        if let (Some(energy), Some(power)) = (snapshot.battery.energy_uwh, snapshot.battery.power_uw) {
            if power > 0 {
                println!("Runtime   {:.1} h at current instantaneous draw", energy as f64 / power as f64);
            }
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn optional_percent(value: Option<f64>) -> String {
    value.map(|value| format!("{value:.1}%")).unwrap_or_else(|| "?".to_owned())
}

#[cfg(target_os = "linux")]
fn optional_watts(value: Option<f64>) -> String {
    value.map(|value| format!("{value:.2} W")).unwrap_or_else(|| "?".to_owned())
}

#[cfg(target_os = "linux")]
fn battery_status(status: ferrite_metrics::BatteryStatus) -> &'static str {
    use ferrite_metrics::BatteryStatus;
    match status {
        BatteryStatus::Charging => "charging",
        BatteryStatus::Discharging => "discharging",
        BatteryStatus::Full => "full",
        BatteryStatus::NotCharging => "not-charging",
        BatteryStatus::Unknown => "unknown",
    }
}

#[cfg(target_os = "linux")]
fn runtime_status(status: ferrite_metrics::RuntimePmStatus) -> &'static str {
    use ferrite_metrics::RuntimePmStatus;
    match status {
        RuntimePmStatus::Active => "active",
        RuntimePmStatus::Suspended => "suspended",
        RuntimePmStatus::Suspending => "suspending",
        RuntimePmStatus::Resuming => "resuming",
        RuntimePmStatus::Unsupported => "unsupported",
        RuntimePmStatus::Unknown => "unknown",
    }
}
