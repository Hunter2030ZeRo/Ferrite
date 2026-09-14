#[cfg(not(target_os = "linux"))]
compile_error!("ferrited v0.1 currently supports Linux only");

#[cfg(target_os = "linux")]
fn main() -> std::io::Result<()> {
    use ferrite_actuator::{Actuator, NoopActuator};
    use ferrite_analyzer::PowerAnalyzer;
    use ferrite_platform::linux::LinuxTelemetry;
    use ferrite_policy::PolicyEngine;
    use ferrite_predictor::{HeuristicPredictor, Predictor};
    use std::{thread, time::Duration};

    let telemetry = LinuxTelemetry::discover()?;
    let mut analyzer = PowerAnalyzer::new();
    let mut predictor = HeuristicPredictor;
    let mut policy = PolicyEngine;
    let mut actuator = NoopActuator;
    let interval = std::env::var("FERRITE_INTERVAL_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(2_000)
        .max(100);

    loop {
        match telemetry.sample() {
            Ok(snapshot) => {
                let derived = analyzer.update(&snapshot);
                let prediction = predictor.predict(&snapshot, &derived);
                let desired = policy.decide(prediction, snapshot.battery.status);
                actuator.apply(&desired)?;
            }
            Err(error) => eprintln!("ferrited: telemetry sample failed: {error}"),
        }
        thread::sleep(Duration::from_millis(interval));
    }
}
