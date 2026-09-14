use ferrite_metrics::{BatteryStatus, SystemSnapshot};

#[derive(Debug, Clone, Copy, Default)]
pub struct DerivedMetrics {
    pub cpu_utilization: Option<f64>,
    pub system_power_w: Option<f64>,
    pub package_power_w: Option<f64>,
    pub unattributed_power_w: Option<f64>,
}

#[derive(Debug, Default)]
pub struct PowerAnalyzer {
    previous: Option<SystemSnapshot>,
}

impl PowerAnalyzer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, current: &SystemSnapshot) -> DerivedMetrics {
        let system_power_w = (current.battery.status == BatteryStatus::Discharging)
            .then_some(current.battery.power_uw)
            .flatten()
            .map(|power| power as f64 / 1_000_000.0);

        let (cpu_utilization, package_power_w) = self
            .previous
            .as_ref()
            .map(|previous| {
                (
                    cpu_utilization(previous, current),
                    package_power(previous, current),
                )
            })
            .unwrap_or((None, None));

        let unattributed_power_w = match (system_power_w, package_power_w) {
            (Some(system), Some(package)) => Some((system - package).max(0.0)),
            _ => None,
        };

        self.previous = Some(*current);
        DerivedMetrics {
            cpu_utilization,
            system_power_w,
            package_power_w,
            unattributed_power_w,
        }
    }
}

fn cpu_utilization(previous: &SystemSnapshot, current: &SystemSnapshot) -> Option<f64> {
    let total_delta = current.cpu.total_ticks.checked_sub(previous.cpu.total_ticks)?;
    let idle_delta = current.cpu.idle_ticks.checked_sub(previous.cpu.idle_ticks)?;
    if total_delta == 0 {
        return None;
    }
    let busy = total_delta.saturating_sub(idle_delta);
    Some((busy as f64 / total_delta as f64) * 100.0)
}

fn package_power(previous: &SystemSnapshot, current: &SystemSnapshot) -> Option<f64> {
    let previous_energy = previous.rapl.package_energy_uj?;
    let current_energy = current.rapl.package_energy_uj?;
    let elapsed_ms = current.timestamp_ms.checked_sub(previous.timestamp_ms)?;
    if elapsed_ms == 0 {
        return None;
    }

    let delta_uj = if current_energy >= previous_energy {
        current_energy - previous_energy
    } else {
        let max = current.rapl.max_energy_range_uj?;
        max.checked_sub(previous_energy)?.saturating_add(current_energy)
    };

    Some(delta_uj as f64 / 1_000.0 / elapsed_ms as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_metrics::{CpuSample, RaplSample};

    #[test]
    fn derives_cpu_and_rapl_power() {
        let mut analyzer = PowerAnalyzer::new();
        let first = SystemSnapshot {
            timestamp_ms: 1_000,
            cpu: CpuSample { total_ticks: 1_000, idle_ticks: 700 },
            rapl: RaplSample { package_energy_uj: Some(1_000_000), max_energy_range_uj: Some(10_000_000) },
            ..SystemSnapshot::default()
        };
        let second = SystemSnapshot {
            timestamp_ms: 2_000,
            cpu: CpuSample { total_ticks: 1_100, idle_ticks: 750 },
            rapl: RaplSample { package_energy_uj: Some(3_000_000), max_energy_range_uj: Some(10_000_000) },
            ..SystemSnapshot::default()
        };
        analyzer.update(&first);
        let derived = analyzer.update(&second);
        assert_eq!(derived.cpu_utilization, Some(50.0));
        assert_eq!(derived.package_power_w, Some(2.0));
    }
}
