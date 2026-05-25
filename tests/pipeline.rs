use ferrite::{
    AggregatedTelemetry, ProcessCategory, ProcessSample, TelemetryPipeline, categorize_process_name,
};

fn sample(
    timestamp_ms: u128,
    pid: u32,
    name: &str,
    cpu_time_100ns: u64,
    read_bytes: u64,
    write_bytes: u64,
    working_set_bytes: u64,
    is_foreground: bool,
) -> ProcessSample {
    ProcessSample {
        timestamp_ms,
        pid,
        name: name.to_string(),
        working_set_bytes,
        cpu_time_100ns,
        read_bytes,
        write_bytes,
        is_foreground,
    }
}

#[test]
fn pipeline_turns_cumulative_process_rows_into_aggregate_deltas() {
    let mut pipeline = TelemetryPipeline::default();

    let first = vec![
        sample(1_000, 10, "Code.exe", 100, 1_000, 2_000, 40, true),
        sample(1_000, 11, "chrome.exe", 50, 300, 400, 60, false),
    ];

    let first_row = pipeline.ingest(&first);
    assert_eq!(first_row.timestamp_ms, 1_000);
    assert_eq!(first_row.total_cpu_delta_100ns, 0);
    assert_eq!(first_row.total_disk_read_bytes, 0);
    assert_eq!(first_row.active_process_count, 2);
    assert_eq!(first_row.memory_pressure_bytes, 100);
    assert_eq!(first_row.foreground_memory_bytes, 40);

    let second = vec![
        sample(
            2_000,
            10,
            "Code.exe",
            160,
            1_000 + 1_048_576,
            2_000 + 1_048_576,
            44,
            true,
        ),
        sample(2_000, 11, "chrome.exe", 75, 500, 430, 70, false),
        sample(2_000, 12, "new.exe", 999, 999, 999, 5, false),
    ];

    let second_row = pipeline.ingest(&second);
    assert_eq!(second_row.timestamp_ms, 2_000);
    assert_eq!(second_row.total_cpu_delta_100ns, 85);
    assert_eq!(second_row.foreground_cpu_delta_100ns, 60);
    assert_eq!(second_row.total_disk_read_bytes, 1_048_776);
    assert_eq!(second_row.total_disk_write_bytes, 1_048_606);
    assert_eq!(second_row.active_process_count, 3);
    assert_eq!(second_row.memory_pressure_bytes, 119);
    assert_eq!(second_row.foreground_memory_bytes, 44);
    let expected_io_spike_score = (1_048_776 + 1_048_606) as f64 / 1_048_576.0;
    assert!((second_row.io_spike_score - expected_io_spike_score).abs() < 0.000001);
}

#[test]
fn process_names_are_bucketed_for_lightweight_models() {
    assert_eq!(
        categorize_process_name("chrome.exe"),
        ProcessCategory::Browser
    );
    assert_eq!(
        categorize_process_name("msedge.exe"),
        ProcessCategory::Browser
    );
    assert_eq!(categorize_process_name("Code.exe"), ProcessCategory::Ide);
    assert_eq!(
        categorize_process_name("rustrover64.exe"),
        ProcessCategory::Ide
    );
    assert_eq!(categorize_process_name("System"), ProcessCategory::System);
    assert_eq!(categorize_process_name("steam.exe"), ProcessCategory::Game);
    assert_eq!(categorize_process_name("vlc.exe"), ProcessCategory::Media);
    assert_eq!(
        categorize_process_name("onedrive.exe"),
        ProcessCategory::Background
    );
    assert_eq!(
        categorize_process_name("unknown.exe"),
        ProcessCategory::Other
    );
}

#[test]
fn aggregate_feature_csv_header_is_tcn_ready() {
    assert_eq!(
        AggregatedTelemetry::csv_header(),
        "timestamp_ms,total_cpu_delta_100ns,foreground_cpu_delta_100ns,total_disk_read_bytes,total_disk_write_bytes,active_process_count,memory_pressure_bytes,foreground_memory_bytes,io_spike_score,browser_process_count,ide_process_count,game_process_count,system_process_count,background_process_count,media_process_count,other_process_count"
    );
}
