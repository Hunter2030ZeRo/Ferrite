use ferrite::{CsvProcessLogger, ProcessSample};

#[test]
fn csv_header_matches_v0_1_telemetry_schema() {
    assert_eq!(
        ProcessSample::csv_header(),
        "timestamp_ms,pid,name,working_set_bytes,cpu_time_100ns,read_bytes,write_bytes,is_foreground"
    );
}

#[test]
fn csv_record_serializes_values_and_escapes_names() {
    let sample = ProcessSample {
        timestamp_ms: 42,
        pid: 1234,
        name: "editor,\"fast\"".to_string(),
        working_set_bytes: 987_654,
        cpu_time_100ns: 111,
        read_bytes: 222,
        write_bytes: 333,
        is_foreground: true,
    };

    assert_eq!(
        sample.to_csv_record(),
        "42,1234,\"editor,\"\"fast\"\"\",987654,111,222,333,true"
    );
}

#[test]
fn csv_logger_writes_header_once_then_appends_samples() {
    let path = std::env::temp_dir().join(format!(
        "ferrite-logger-test-{}-{}.csv",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let sample = ProcessSample {
        timestamp_ms: 1,
        pid: 7,
        name: "demo.exe".to_string(),
        working_set_bytes: 8,
        cpu_time_100ns: 9,
        read_bytes: 10,
        write_bytes: 11,
        is_foreground: false,
    };

    CsvProcessLogger::append_samples(&path, &[sample.clone()]).unwrap();
    CsvProcessLogger::append_samples(&path, &[sample]).unwrap();

    let csv = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(&path);

    let lines: Vec<_> = csv.lines().collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], ProcessSample::csv_header());
    assert_eq!(lines[1], "1,7,demo.exe,8,9,10,11,false");
    assert_eq!(lines[2], "1,7,demo.exe,8,9,10,11,false");
}
