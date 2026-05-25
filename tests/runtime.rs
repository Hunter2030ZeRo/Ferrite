use ferrite::{
    AggregatedTelemetry, LastValuePredictor, Normalizer, OpenVinoPredictor, PredictionLogRow,
    RollingRuntime, RollingWindow, anomaly_score_mse, window_progress_capacity,
};

#[test]
fn rolling_window_keeps_latest_rows_and_flattens_time_major() {
    let mut window = RollingWindow::new(3, 2);

    window.push(vec![1.0, 2.0]);
    window.push(vec![3.0, 4.0]);
    assert!(!window.is_ready());

    window.push(vec![5.0, 6.0]);
    assert!(window.is_ready());
    assert_eq!(window.len(), 3);
    assert_eq!(window.as_flat_input(), vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);

    window.push(vec![7.0, 8.0]);
    assert_eq!(window.len(), 3);
    assert_eq!(window.as_flat_input(), vec![3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
}

#[test]
fn window_progress_capacity_grows_in_chunks() {
    assert_eq!(window_progress_capacity(0, 60), 60);
    assert_eq!(window_progress_capacity(1, 60), 60);
    assert_eq!(window_progress_capacity(60, 60), 60);
    assert_eq!(window_progress_capacity(61, 60), 120);
    assert_eq!(window_progress_capacity(120, 60), 120);
    assert_eq!(window_progress_capacity(121, 60), 180);
}

#[test]
fn normalizer_handles_small_std_and_repeats_across_flat_window() {
    let normalizer = Normalizer::new(vec![10.0, 100.0], vec![2.0, 0.0]).unwrap();

    assert_eq!(normalizer.normalize_row(&[14.0, 105.0]), vec![2.0, 5.0]);
    assert_eq!(normalizer.denormalize_row(&[2.0, 5.0]), vec![14.0, 105.0]);
    assert_eq!(
        normalizer.normalize_window_flat(&[10.0, 100.0, 12.0, 102.0], 2),
        vec![0.0, 0.0, 1.0, 2.0]
    );
}

#[test]
fn normalizer_loads_numpy_npz_stats() {
    let path = std::env::temp_dir().join(format!(
        "ferrite-norm-stats-{}-{}.npz",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    write_test_npz(&path, &[10.0, 100.0], &[2.0, 0.0]);

    let normalizer = Normalizer::load_npz(&path).unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(normalizer.mean, vec![10.0, 100.0]);
    assert_eq!(normalizer.std, vec![2.0, 1.0]);
}

#[test]
fn normalizer_can_be_estimated_from_feature_csv() {
    let path = std::env::temp_dir().join(format!(
        "ferrite-features-{}-{}.csv",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    std::fs::write(
        &path,
        "timestamp_ms,total_cpu_delta_100ns,foreground_cpu_delta_100ns\n1,10,100\n2,14,104\n3,16,106\n",
    )
    .unwrap();

    let normalizer = Normalizer::from_feature_csv(
        &path,
        &[
            "total_cpu_delta_100ns".to_string(),
            "foreground_cpu_delta_100ns".to_string(),
        ],
    )
    .unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(normalizer.mean, vec![40.0 / 3.0, 310.0 / 3.0]);
    assert!((normalizer.std[0] - 2.4944382).abs() < 0.000001);
    assert!((normalizer.std[1] - 2.4944382).abs() < 0.000001);
}

#[test]
fn denormalization_keeps_nan_predictions_visible() {
    let normalizer = Normalizer::identity(1);
    assert!(normalizer.denormalize_row(&[f32::NAN])[0].is_nan());
}

#[test]
fn aggregate_feature_rows_follow_training_column_order() {
    let row = AggregatedTelemetry {
        timestamp_ms: 1,
        total_cpu_delta_100ns: 2,
        foreground_cpu_delta_100ns: 3,
        total_disk_read_bytes: 4,
        total_disk_write_bytes: 5,
        active_process_count: 6,
        memory_pressure_bytes: 7,
        foreground_memory_bytes: 8,
        io_spike_score: 9.0,
        browser_process_count: 10,
        ide_process_count: 11,
        game_process_count: 12,
        system_process_count: 13,
        background_process_count: 14,
        media_process_count: 15,
        other_process_count: 16,
    };

    let values = row
        .to_feature_row_with_names(&[
            "other_process_count".to_string(),
            "total_cpu_delta_100ns".to_string(),
            "io_spike_score".to_string(),
        ])
        .unwrap()
        .values;

    assert_eq!(values, vec![16.0, 2.0, 9.0]);
}

#[test]
fn anomaly_score_is_normalized_space_mse() {
    let score = anomaly_score_mse(&[1.0, 2.0, 3.0], &[1.0, 4.0, 5.0]);
    assert!((score - 8.0 / 3.0).abs() < f32::EPSILON);
    assert_eq!(anomaly_score_mse(&[], &[1.0]), 0.0);
}

#[test]
fn prediction_log_row_uses_long_debug_format() {
    let row = PredictionLogRow {
        timestamp_ms: 123,
        anomaly_score: 0.25,
        device: "CPU".to_string(),
        feature_name: "total_cpu_delta_100ns".to_string(),
        pred_value: 10.0,
        actual_value: 12.5,
    };

    assert_eq!(
        PredictionLogRow::csv_header(),
        "timestamp_ms,anomaly_score,device,feature_name,pred_value,actual_value,abs_error"
    );
    assert_eq!(
        row.to_csv_record(),
        "123,0.250000,CPU,total_cpu_delta_100ns,10.000000,12.500000,2.500000"
    );
}

#[test]
fn rolling_runtime_scores_previous_prediction_when_next_row_arrives() {
    let normalizer = Normalizer::identity(2);
    let predictor = LastValuePredictor::new(2);
    let mut runtime = RollingRuntime::new(2, 2, normalizer, predictor);

    assert!(runtime.step(100, vec![1.0, 10.0]).unwrap().is_none());
    assert!(runtime.step(200, vec![2.0, 20.0]).unwrap().is_none());

    let result = runtime.step(300, vec![4.0, 10.0]).unwrap().unwrap();
    assert_eq!(result.timestamp_ms, 300);
    assert_eq!(result.pred_real, vec![2.0, 20.0]);
    assert_eq!(result.actual_real, Some(vec![4.0, 10.0]));
    assert_eq!(result.anomaly_score, Some(52.0));

    let rows = result.to_log_rows("BASELINE", &["cpu", "io"]);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].feature_name, "cpu");
    assert_eq!(rows[0].pred_value, 2.0);
    assert_eq!(rows[0].actual_value, 4.0);
    assert_eq!(rows[1].feature_name, "io");
    assert_eq!(rows[1].pred_value, 20.0);
    assert_eq!(rows[1].actual_value, 10.0);
}

#[test]
fn openvino_device_order_is_npu_first_with_auto_and_cpu_fallback() {
    assert_eq!(
        OpenVinoPredictor::npu_first_device_order(),
        ["NPU", "AUTO", "CPU"]
    );
}

#[test]
fn openvino_constructor_reports_missing_model_artifacts_before_loading_runtime() {
    let path = std::env::temp_dir().join(format!(
        "missing-ferrite-tcn-{}-{}.xml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let error = OpenVinoPredictor::new_npu_first(&path, std::env::temp_dir(), 60, 15)
        .expect_err("missing model should be rejected before OpenVINO loads");

    assert!(error.to_string().contains("model XML not found"));
}

fn write_test_npz(path: &std::path::Path, mean: &[f32], std: &[f32]) {
    fn npy(name: &str, values: &[f32]) -> (String, Vec<u8>) {
        let mut header = format!(
            "{{'descr': '<f4', 'fortran_order': False, 'shape': ({},), }}",
            values.len()
        )
        .into_bytes();
        let base_len = 10 + header.len() + 1;
        let padding = (16 - (base_len % 16)) % 16;
        header.extend(std::iter::repeat_n(b' ', padding));
        header.push(b'\n');

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\x93NUMPY");
        bytes.extend_from_slice(&[1, 0]);
        bytes.extend_from_slice(&(header.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&header);
        for value in values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }

        (format!("{name}.npy"), bytes)
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xffff_ffff_u32;
        for &byte in bytes {
            crc ^= byte as u32;
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        !crc
    }

    let files = [npy("mean", mean), npy("std", std)];
    let mut archive = Vec::new();
    let mut central = Vec::new();

    for (name, data) in files {
        let offset = archive.len() as u32;
        let crc = crc32(&data);
        let name_bytes = name.as_bytes();
        archive.extend_from_slice(&0x0403_4b50_u32.to_le_bytes());
        archive.extend_from_slice(&20_u16.to_le_bytes());
        archive.extend_from_slice(&0_u16.to_le_bytes());
        archive.extend_from_slice(&0_u16.to_le_bytes());
        archive.extend_from_slice(&0_u16.to_le_bytes());
        archive.extend_from_slice(&0_u16.to_le_bytes());
        archive.extend_from_slice(&crc.to_le_bytes());
        archive.extend_from_slice(&(data.len() as u32).to_le_bytes());
        archive.extend_from_slice(&(data.len() as u32).to_le_bytes());
        archive.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        archive.extend_from_slice(&0_u16.to_le_bytes());
        archive.extend_from_slice(name_bytes);
        archive.extend_from_slice(&data);

        central.extend_from_slice(&0x0201_4b50_u32.to_le_bytes());
        central.extend_from_slice(&20_u16.to_le_bytes());
        central.extend_from_slice(&20_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name_bytes);
    }

    let central_offset = archive.len() as u32;
    let central_size = central.len() as u32;
    archive.extend_from_slice(&central);
    archive.extend_from_slice(&0x0605_4b50_u32.to_le_bytes());
    archive.extend_from_slice(&0_u16.to_le_bytes());
    archive.extend_from_slice(&0_u16.to_le_bytes());
    archive.extend_from_slice(&2_u16.to_le_bytes());
    archive.extend_from_slice(&2_u16.to_le_bytes());
    archive.extend_from_slice(&central_size.to_le_bytes());
    archive.extend_from_slice(&central_offset.to_le_bytes());
    archive.extend_from_slice(&0_u16.to_le_bytes());

    std::fs::write(path, archive).unwrap();
}
