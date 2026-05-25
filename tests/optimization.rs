use ferrite::{
    OptimizationAction, OptimizationEngine, OptimizationMode, PredictionResult, ProcessSample,
};

fn sample(pid: u32, name: &str, working_set_bytes: u64, is_foreground: bool) -> ProcessSample {
    ProcessSample {
        timestamp_ms: 1,
        pid,
        name: name.to_string(),
        working_set_bytes,
        cpu_time_100ns: 0,
        read_bytes: 0,
        write_bytes: 0,
        is_foreground,
    }
}

fn result(score: Option<f32>) -> PredictionResult {
    PredictionResult {
        pred_norm: vec![0.0],
        pred_real: vec![0.0],
        actual_real: Some(vec![0.0]),
        anomaly_score: score,
        timestamp_ms: 99,
    }
}

#[test]
fn low_or_invalid_scores_only_observe() {
    let engine = OptimizationEngine::default();
    let samples = [sample(10, "chrome.exe", 100, false)];

    assert_eq!(
        engine.decide(result(Some(0.05)), &samples).mode,
        OptimizationMode::Observe
    );
    assert_eq!(
        engine.decide(result(Some(f32::NAN)), &samples).mode,
        OptimizationMode::Observe
    );
}

#[test]
fn high_anomaly_recommends_background_optimization_candidates() {
    let engine = OptimizationEngine::default();
    let decision = engine.decide(
        result(Some(3.0)),
        &[
            sample(10, "Code.exe", 200, true),
            sample(11, "System", 900, false),
            sample(12, "chrome.exe", 300, false),
            sample(13, "onedrive.exe", 500, false),
        ],
    );

    assert_eq!(decision.mode, OptimizationMode::Recommend);
    assert!(
        decision
            .actions
            .contains(&OptimizationAction::ProtectForeground {
                pid: 10,
                name: "Code.exe".to_string(),
            })
    );
    assert!(
        decision
            .actions
            .contains(&OptimizationAction::LowerBackgroundPriority {
                pid: 13,
                name: "onedrive.exe".to_string(),
            })
    );
    assert!(
        decision
            .actions
            .contains(&OptimizationAction::PreferEcoQos {
                pid: 13,
                name: "onedrive.exe".to_string(),
            })
    );
    assert!(
        !decision
            .actions
            .iter()
            .any(|action| action.pid() == Some(11))
    );
}
