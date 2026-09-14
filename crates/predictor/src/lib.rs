use ferrite_analyzer::DerivedMetrics;
use ferrite_metrics::SystemSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkloadClass {
    Idle,
    InteractiveLight,
    CpuBurst,
    CpuSustained,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Prediction {
    pub class: WorkloadClass,
    pub confidence: f32,
}

pub trait Predictor {
    fn predict(&mut self, snapshot: &SystemSnapshot, derived: &DerivedMetrics) -> Prediction;
}

#[derive(Debug, Default)]
pub struct HeuristicPredictor;

impl Predictor for HeuristicPredictor {
    fn predict(&mut self, snapshot: &SystemSnapshot, derived: &DerivedMetrics) -> Prediction {
        let Some(utilization) = derived.cpu_utilization else {
            return Prediction::default();
        };
        let psi = snapshot.cpu_pressure.some_avg10 as f64;

        if utilization < 5.0 && psi < 0.2 {
            Prediction { class: WorkloadClass::Idle, confidence: 0.95 }
        } else if utilization < 35.0 && psi < 2.0 {
            Prediction { class: WorkloadClass::InteractiveLight, confidence: 0.80 }
        } else if utilization < 75.0 && psi < 8.0 {
            Prediction { class: WorkloadClass::CpuBurst, confidence: 0.70 }
        } else {
            Prediction { class: WorkloadClass::CpuSustained, confidence: 0.80 }
        }
    }
}
