use ferrite_metrics::BatteryStatus;
use ferrite_predictor::{Prediction, WorkloadClass};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EppPreference {
    Power,
    BalancePower,
    #[default]
    BalancePerformance,
    Performance,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DesiredPolicy {
    pub epp: EppPreference,
    pub max_performance_percent: u8,
}

#[derive(Debug, Default)]
pub struct PolicyEngine;

impl PolicyEngine {
    pub fn decide(&mut self, prediction: Prediction, battery_status: BatteryStatus) -> DesiredPolicy {
        if battery_status != BatteryStatus::Discharging {
            return DesiredPolicy { epp: EppPreference::BalancePerformance, max_performance_percent: 100 };
        }

        match prediction.class {
            WorkloadClass::Idle => DesiredPolicy { epp: EppPreference::Power, max_performance_percent: 45 },
            WorkloadClass::InteractiveLight => DesiredPolicy { epp: EppPreference::BalancePower, max_performance_percent: 70 },
            WorkloadClass::CpuBurst => DesiredPolicy { epp: EppPreference::BalancePerformance, max_performance_percent: 100 },
            WorkloadClass::CpuSustained => DesiredPolicy { epp: EppPreference::BalancePerformance, max_performance_percent: 100 },
            WorkloadClass::Unknown => DesiredPolicy { epp: EppPreference::BalancePower, max_performance_percent: 80 },
        }
    }
}
