use std::{
    collections::{HashMap, VecDeque},
    ffi::{CString, OsStr, c_char, c_void},
    fs::OpenOptions,
    io::{self, Write},
    mem,
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSample {
    pub timestamp_ms: u128,
    pub pid: u32,
    pub name: String,
    pub working_set_bytes: u64,
    pub cpu_time_100ns: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub is_foreground: bool,
}

impl ProcessSample {
    pub fn csv_header() -> &'static str {
        "timestamp_ms,pid,name,working_set_bytes,cpu_time_100ns,read_bytes,write_bytes,is_foreground"
    }

    pub fn to_csv_record(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{}",
            self.timestamp_ms,
            self.pid,
            escape_csv_field(&self.name),
            self.working_set_bytes,
            self.cpu_time_100ns,
            self.read_bytes,
            self.write_bytes,
            self.is_foreground
        )
    }
}

pub struct CsvProcessLogger;

impl CsvProcessLogger {
    pub fn append_samples(path: impl AsRef<Path>, samples: &[ProcessSample]) -> io::Result<()> {
        let path = path.as_ref();
        let needs_header = match std::fs::metadata(path) {
            Ok(metadata) => metadata.len() == 0,
            Err(error) if error.kind() == io::ErrorKind::NotFound => true,
            Err(error) => return Err(error),
        };

        let mut file = OpenOptions::new().create(true).append(true).open(path)?;

        if needs_header {
            file.write_all(ProcessSample::csv_header().as_bytes())?;
            file.write_all(b"\n")?;
        }

        for sample in samples {
            file.write_all(sample.to_csv_record().as_bytes())?;
            file.write_all(b"\n")?;
        }

        file.flush()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessCategory {
    Browser,
    Ide,
    Game,
    System,
    Background,
    Media,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AggregatedTelemetry {
    pub timestamp_ms: u128,
    pub total_cpu_delta_100ns: u64,
    pub foreground_cpu_delta_100ns: u64,
    pub total_disk_read_bytes: u64,
    pub total_disk_write_bytes: u64,
    pub active_process_count: u32,
    pub memory_pressure_bytes: u64,
    pub foreground_memory_bytes: u64,
    pub io_spike_score: f64,
    pub browser_process_count: u32,
    pub ide_process_count: u32,
    pub game_process_count: u32,
    pub system_process_count: u32,
    pub background_process_count: u32,
    pub media_process_count: u32,
    pub other_process_count: u32,
}

impl AggregatedTelemetry {
    pub fn csv_header() -> &'static str {
        "timestamp_ms,total_cpu_delta_100ns,foreground_cpu_delta_100ns,total_disk_read_bytes,total_disk_write_bytes,active_process_count,memory_pressure_bytes,foreground_memory_bytes,io_spike_score,browser_process_count,ide_process_count,game_process_count,system_process_count,background_process_count,media_process_count,other_process_count"
    }

    pub fn to_csv_record(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{},{:.6},{},{},{},{},{},{},{}",
            self.timestamp_ms,
            self.total_cpu_delta_100ns,
            self.foreground_cpu_delta_100ns,
            self.total_disk_read_bytes,
            self.total_disk_write_bytes,
            self.active_process_count,
            self.memory_pressure_bytes,
            self.foreground_memory_bytes,
            self.io_spike_score,
            self.browser_process_count,
            self.ide_process_count,
            self.game_process_count,
            self.system_process_count,
            self.background_process_count,
            self.media_process_count,
            self.other_process_count
        )
    }
}

pub struct CsvAggregatedTelemetryLogger;

impl CsvAggregatedTelemetryLogger {
    pub fn append_rows(path: impl AsRef<Path>, rows: &[AggregatedTelemetry]) -> io::Result<()> {
        let path = path.as_ref();
        let needs_header = match std::fs::metadata(path) {
            Ok(metadata) => metadata.len() == 0,
            Err(error) if error.kind() == io::ErrorKind::NotFound => true,
            Err(error) => return Err(error),
        };

        let mut file = OpenOptions::new().create(true).append(true).open(path)?;

        if needs_header {
            file.write_all(AggregatedTelemetry::csv_header().as_bytes())?;
            file.write_all(b"\n")?;
        }

        for row in rows {
            file.write_all(row.to_csv_record().as_bytes())?;
            file.write_all(b"\n")?;
        }

        file.flush()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeatureRow {
    pub values: Vec<f32>,
}

impl AggregatedTelemetry {
    pub fn to_feature_row(&self) -> FeatureRow {
        self.to_feature_row_with_names(Self::feature_names())
            .expect("built-in feature names are valid")
    }

    pub fn to_feature_row_with_names<S: AsRef<str>>(
        &self,
        feature_names: &[S],
    ) -> io::Result<FeatureRow> {
        let mut values = Vec::with_capacity(feature_names.len());

        for name in feature_names {
            values.push(self.feature_value(name.as_ref()).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown feature column: {}", name.as_ref()),
                )
            })?);
        }

        Ok(FeatureRow { values })
    }

    pub fn feature_names() -> &'static [&'static str] {
        &[
            "total_cpu_delta_100ns",
            "foreground_cpu_delta_100ns",
            "total_disk_read_bytes",
            "total_disk_write_bytes",
            "active_process_count",
            "memory_pressure_bytes",
            "foreground_memory_bytes",
            "io_spike_score",
            "browser_process_count",
            "ide_process_count",
            "game_process_count",
            "system_process_count",
            "background_process_count",
            "media_process_count",
            "other_process_count",
        ]
    }

    fn feature_value(&self, name: &str) -> Option<f32> {
        match name {
            "total_cpu_delta_100ns" => Some(self.total_cpu_delta_100ns as f32),
            "foreground_cpu_delta_100ns" => Some(self.foreground_cpu_delta_100ns as f32),
            "total_disk_read_bytes" => Some(self.total_disk_read_bytes as f32),
            "total_disk_write_bytes" => Some(self.total_disk_write_bytes as f32),
            "active_process_count" => Some(self.active_process_count as f32),
            "memory_pressure_bytes" => Some(self.memory_pressure_bytes as f32),
            "foreground_memory_bytes" => Some(self.foreground_memory_bytes as f32),
            "io_spike_score" => Some(self.io_spike_score as f32),
            "browser_process_count" => Some(self.browser_process_count as f32),
            "ide_process_count" => Some(self.ide_process_count as f32),
            "game_process_count" => Some(self.game_process_count as f32),
            "system_process_count" => Some(self.system_process_count as f32),
            "background_process_count" => Some(self.background_process_count as f32),
            "media_process_count" => Some(self.media_process_count as f32),
            "other_process_count" => Some(self.other_process_count as f32),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RollingWindow {
    seq_len: usize,
    input_dim: usize,
    buffer: VecDeque<Vec<f32>>,
}

impl RollingWindow {
    pub fn new(seq_len: usize, input_dim: usize) -> Self {
        Self {
            seq_len,
            input_dim,
            buffer: VecDeque::with_capacity(seq_len),
        }
    }

    pub fn push(&mut self, mut row: Vec<f32>) {
        row.truncate(self.input_dim);
        while row.len() < self.input_dim {
            row.push(0.0);
        }

        if self.buffer.len() == self.seq_len {
            self.buffer.pop_front();
        }

        self.buffer.push_back(row);
    }

    pub fn is_ready(&self) -> bool {
        self.buffer.len() == self.seq_len
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn capacity(&self) -> usize {
        self.seq_len
    }

    pub fn as_flat_input(&self) -> Vec<f32> {
        let mut flat = Vec::with_capacity(self.seq_len * self.input_dim);

        for row in &self.buffer {
            flat.extend_from_slice(row);
        }

        flat
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Normalizer {
    pub mean: Vec<f32>,
    pub std: Vec<f32>,
}

impl Normalizer {
    pub fn load_npz(path: impl AsRef<Path>) -> io::Result<Self> {
        let entries = read_stored_zip_entries(path.as_ref())?;
        let mean = entries
            .iter()
            .find(|(name, _)| name == "mean.npy" || name == "arr_0.npy")
            .map(|(_, data)| parse_npy_vector(data))
            .transpose()?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "normalization stats missing mean.npy",
                )
            })?;
        let std = entries
            .iter()
            .find(|(name, _)| name == "std.npy" || name == "arr_1.npy")
            .map(|(_, data)| parse_npy_vector(data))
            .transpose()?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "normalization stats missing std.npy",
                )
            })?;

        Self::new(mean, std)
    }

    pub fn from_feature_csv(path: impl AsRef<Path>, feature_names: &[String]) -> io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let mut lines = text.lines();
        let header = lines
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "feature CSV is empty"))?;
        let columns = split_csv_line(header);
        let indices = feature_names
            .iter()
            .map(|feature| {
                columns
                    .iter()
                    .position(|column| column == feature)
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("feature CSV missing column: {feature}"),
                        )
                    })
            })
            .collect::<io::Result<Vec<_>>>()?;

        let mut count = 0_f64;
        let mut sums = vec![0.0_f64; indices.len()];
        let mut sum_squares = vec![0.0_f64; indices.len()];

        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            let values = split_csv_line(line);
            let mut parsed = Vec::with_capacity(indices.len());
            for &index in &indices {
                let value = values
                    .get(index)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "feature CSV row is truncated")
                    })?
                    .parse::<f64>()
                    .map_err(invalid_input)?;
                parsed.push(value);
            }

            count += 1.0;
            for (index, value) in parsed.into_iter().enumerate() {
                sums[index] += value;
                sum_squares[index] += value * value;
            }
        }

        if count == 0.0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "feature CSV has no data rows",
            ));
        }

        let mean = sums
            .iter()
            .map(|sum| (*sum / count) as f32)
            .collect::<Vec<_>>();
        let std = sums
            .iter()
            .zip(sum_squares.iter())
            .map(|(sum, sum_square)| {
                let variance = (sum_square / count) - (sum / count).powi(2);
                variance.max(0.0).sqrt() as f32
            })
            .collect::<Vec<_>>();

        Self::new(mean, std)
    }

    pub fn new(mean: Vec<f32>, std: Vec<f32>) -> io::Result<Self> {
        if mean.len() != std.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "normalizer mean/std length mismatch",
            ));
        }

        let std = std
            .into_iter()
            .map(|value| {
                if value.is_finite() && value.abs() >= 1.0e-6 {
                    value
                } else {
                    1.0
                }
            })
            .collect();

        Ok(Self { mean, std })
    }

    pub fn identity(input_dim: usize) -> Self {
        Self {
            mean: vec![0.0; input_dim],
            std: vec![1.0; input_dim],
        }
    }

    pub fn normalize_row(&self, row: &[f32]) -> Vec<f32> {
        row.iter()
            .enumerate()
            .map(|(index, value)| {
                let mean = self.mean.get(index).copied().unwrap_or(0.0);
                let std = self.std.get(index).copied().unwrap_or(1.0);
                sanitize_feature((*value - mean) / std)
            })
            .collect()
    }

    pub fn denormalize_row(&self, row: &[f32]) -> Vec<f32> {
        row.iter()
            .enumerate()
            .map(|(index, value)| {
                let mean = self.mean.get(index).copied().unwrap_or(0.0);
                let std = self.std.get(index).copied().unwrap_or(1.0);
                (*value * std) + mean
            })
            .collect()
    }

    pub fn normalize_window_flat(&self, flat: &[f32], input_dim: usize) -> Vec<f32> {
        flat.iter()
            .enumerate()
            .map(|(index, value)| {
                let feature_index = index % input_dim;
                let mean = self.mean.get(feature_index).copied().unwrap_or(0.0);
                let std = self.std.get(feature_index).copied().unwrap_or(1.0);
                sanitize_feature((*value - mean) / std)
            })
            .collect()
    }
}

pub fn load_feature_columns_json(path: impl AsRef<Path>) -> io::Result<Vec<String>> {
    let text = std::fs::read_to_string(path)?;
    parse_json_string_array(&text)
}

#[derive(Debug, Clone, PartialEq)]
pub struct PredictionResult {
    pub pred_norm: Vec<f32>,
    pub pred_real: Vec<f32>,
    pub actual_real: Option<Vec<f32>>,
    pub anomaly_score: Option<f32>,
    pub timestamp_ms: u128,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PredictionLogRow {
    pub timestamp_ms: u128,
    pub anomaly_score: f32,
    pub device: String,
    pub feature_name: String,
    pub pred_value: f32,
    pub actual_value: f32,
}

impl PredictionLogRow {
    pub fn csv_header() -> &'static str {
        "timestamp_ms,anomaly_score,device,feature_name,pred_value,actual_value,abs_error"
    }

    pub fn abs_error(&self) -> f32 {
        (self.pred_value - self.actual_value).abs()
    }

    pub fn to_csv_record(&self) -> String {
        format!(
            "{},{:.6},{},{},{:.6},{:.6},{:.6}",
            self.timestamp_ms,
            self.anomaly_score,
            escape_csv_field(&self.device),
            escape_csv_field(&self.feature_name),
            self.pred_value,
            self.actual_value,
            self.abs_error()
        )
    }
}

impl PredictionResult {
    pub fn to_log_rows(&self, device: &str, feature_names: &[&str]) -> Vec<PredictionLogRow> {
        let Some(actual_real) = &self.actual_real else {
            return Vec::new();
        };
        let anomaly_score = self.anomaly_score.unwrap_or(0.0);

        self.pred_real
            .iter()
            .zip(actual_real.iter())
            .enumerate()
            .map(|(index, (pred_value, actual_value))| PredictionLogRow {
                timestamp_ms: self.timestamp_ms,
                anomaly_score,
                device: device.to_string(),
                feature_name: feature_names
                    .get(index)
                    .copied()
                    .unwrap_or("unknown")
                    .to_string(),
                pred_value: *pred_value,
                actual_value: *actual_value,
            })
            .collect()
    }
}

pub struct CsvPredictionLogger;

impl CsvPredictionLogger {
    pub fn append_rows(path: impl AsRef<Path>, rows: &[PredictionLogRow]) -> io::Result<()> {
        let path = path.as_ref();
        let needs_header = match std::fs::metadata(path) {
            Ok(metadata) => metadata.len() == 0,
            Err(error) if error.kind() == io::ErrorKind::NotFound => true,
            Err(error) => return Err(error),
        };

        let mut file = OpenOptions::new().create(true).append(true).open(path)?;

        if needs_header {
            file.write_all(PredictionLogRow::csv_header().as_bytes())?;
            file.write_all(b"\n")?;
        }

        for row in rows {
            file.write_all(row.to_csv_record().as_bytes())?;
            file.write_all(b"\n")?;
        }

        file.flush()
    }
}

pub fn anomaly_score_mse(pred_norm: &[f32], actual_norm: &[f32]) -> f32 {
    let compared = pred_norm.iter().zip(actual_norm.iter());
    let mut count = 0_usize;
    let mut total = 0.0_f32;

    for (pred, actual) in compared {
        let delta = pred - actual;
        total += delta * delta;
        count += 1;
    }

    if count == 0 {
        0.0
    } else {
        total / count as f32
    }
}

pub fn window_progress_capacity(current: u64, chunk_size: u64) -> u64 {
    let chunk_size = chunk_size.max(1);
    let current = current.max(1);
    current.div_ceil(chunk_size) * chunk_size
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationMode {
    Observe,
    Recommend,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptimizationAction {
    ProtectForeground { pid: u32, name: String },
    LowerBackgroundPriority { pid: u32, name: String },
    PreferEcoQos { pid: u32, name: String },
}

impl OptimizationAction {
    pub fn pid(&self) -> Option<u32> {
        match self {
            Self::ProtectForeground { pid, .. }
            | Self::LowerBackgroundPriority { pid, .. }
            | Self::PreferEcoQos { pid, .. } => Some(*pid),
        }
    }

    fn summary(&self) -> String {
        match self {
            Self::ProtectForeground { pid, name } => {
                format!("protect foreground {name}({pid})")
            }
            Self::LowerBackgroundPriority { pid, name } => {
                format!("lower background priority {name}({pid})")
            }
            Self::PreferEcoQos { pid, name } => format!("prefer EcoQoS {name}({pid})"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OptimizationDecision {
    pub mode: OptimizationMode,
    pub anomaly_score: Option<f32>,
    pub actions: Vec<OptimizationAction>,
}

impl OptimizationDecision {
    pub fn summary(&self) -> String {
        if self.actions.is_empty() {
            return "observe".to_string();
        }

        self.actions
            .iter()
            .map(OptimizationAction::summary)
            .collect::<Vec<_>>()
            .join("; ")
    }
}

#[derive(Debug, Clone)]
pub struct OptimizationEngine {
    anomaly_threshold: f32,
    max_background_targets: usize,
}

impl Default for OptimizationEngine {
    fn default() -> Self {
        Self {
            anomaly_threshold: 1.0,
            max_background_targets: 2,
        }
    }
}

impl OptimizationEngine {
    pub fn new(anomaly_threshold: f32, max_background_targets: usize) -> Self {
        Self {
            anomaly_threshold,
            max_background_targets: max_background_targets.max(1),
        }
    }

    pub fn decide(
        &self,
        prediction: PredictionResult,
        samples: &[ProcessSample],
    ) -> OptimizationDecision {
        let score = prediction.anomaly_score;
        if !score.is_some_and(|value| value.is_finite() && value >= self.anomaly_threshold) {
            return OptimizationDecision {
                mode: OptimizationMode::Observe,
                anomaly_score: score,
                actions: Vec::new(),
            };
        }

        let mut actions = Vec::new();
        for sample in samples.iter().filter(|sample| sample.is_foreground) {
            actions.push(OptimizationAction::ProtectForeground {
                pid: sample.pid,
                name: sample.name.clone(),
            });
        }

        let mut candidates = samples
            .iter()
            .filter(|sample| !sample.is_foreground)
            .filter(|sample| {
                !matches!(
                    categorize_process_name(&sample.name),
                    ProcessCategory::System
                )
            })
            .collect::<Vec<_>>();

        candidates.sort_by_key(|sample| std::cmp::Reverse(sample.working_set_bytes));

        for sample in candidates.into_iter().take(self.max_background_targets) {
            actions.push(OptimizationAction::LowerBackgroundPriority {
                pid: sample.pid,
                name: sample.name.clone(),
            });
            actions.push(OptimizationAction::PreferEcoQos {
                pid: sample.pid,
                name: sample.name.clone(),
            });
        }

        OptimizationDecision {
            mode: if actions.is_empty() {
                OptimizationMode::Observe
            } else {
                OptimizationMode::Recommend
            },
            anomaly_score: score,
            actions,
        }
    }
}

pub trait Predictor {
    fn device(&self) -> &str;
    fn infer(&self, input_flat_norm: &[f32]) -> io::Result<Vec<f32>>;
}

impl<T: Predictor + ?Sized> Predictor for Box<T> {
    fn device(&self) -> &str {
        (**self).device()
    }

    fn infer(&self, input_flat_norm: &[f32]) -> io::Result<Vec<f32>> {
        (**self).infer(input_flat_norm)
    }
}

#[derive(Debug, Clone)]
pub struct LastValuePredictor {
    input_dim: usize,
}

impl LastValuePredictor {
    pub fn new(input_dim: usize) -> Self {
        Self { input_dim }
    }
}

impl Predictor for LastValuePredictor {
    fn device(&self) -> &str {
        "BASELINE"
    }

    fn infer(&self, input_flat_norm: &[f32]) -> io::Result<Vec<f32>> {
        if input_flat_norm.len() < self.input_dim {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "input window is shorter than input_dim",
            ));
        }

        Ok(input_flat_norm[input_flat_norm.len() - self.input_dim..].to_vec())
    }
}

pub struct OpenVinoPredictor {
    api: Arc<OpenVinoApi>,
    core: *mut OvCore,
    compiled_model: *mut OvCompiledModel,
    infer_request: *mut OvInferRequest,
    input_dim: usize,
    seq_len: usize,
    device: String,
}

impl std::fmt::Debug for OpenVinoPredictor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenVinoPredictor")
            .field("input_dim", &self.input_dim)
            .field("seq_len", &self.seq_len)
            .field("device", &self.device)
            .finish_non_exhaustive()
    }
}

impl OpenVinoPredictor {
    pub fn npu_first_device_order() -> [&'static str; 3] {
        ["NPU", "AUTO", "CPU"]
    }

    pub fn new_npu_first(
        model_xml_path: impl AsRef<Path>,
        cache_dir: impl AsRef<Path>,
        seq_len: usize,
        input_dim: usize,
    ) -> io::Result<Self> {
        Self::new_with_device_order(
            model_xml_path,
            cache_dir,
            seq_len,
            input_dim,
            &Self::npu_first_device_order(),
        )
    }

    pub fn new_with_device_order(
        model_xml_path: impl AsRef<Path>,
        cache_dir: impl AsRef<Path>,
        seq_len: usize,
        input_dim: usize,
        devices: &[&str],
    ) -> io::Result<Self> {
        let model_xml_path = model_xml_path.as_ref();
        validate_openvino_artifacts(model_xml_path)?;
        std::fs::create_dir_all(cache_dir)?;

        if seq_len == 0 || input_dim == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seq_len and input_dim must be greater than zero",
            ));
        }

        let api = Arc::new(OpenVinoApi::load()?);
        let mut core = ptr::null_mut();
        ov_check(unsafe { (api.ov_core_create)(&mut core) }, "ov_core_create")?;

        let model_path = path_to_cstring(model_xml_path)?;
        let mut failures = Vec::new();

        for device in devices {
            let device_name = CString::new(*device).map_err(invalid_input)?;
            let mut compiled_model = ptr::null_mut();
            let status = unsafe {
                (api.ov_core_compile_model_from_file)(
                    core,
                    model_path.as_ptr(),
                    device_name.as_ptr(),
                    0,
                    &mut compiled_model,
                )
            };

            if status == OV_STATUS_OK && !compiled_model.is_null() {
                let mut infer_request = ptr::null_mut();
                ov_check(
                    unsafe {
                        (api.ov_compiled_model_create_infer_request)(
                            compiled_model,
                            &mut infer_request,
                        )
                    },
                    "ov_compiled_model_create_infer_request",
                )?;

                return Ok(Self {
                    api,
                    core,
                    compiled_model,
                    infer_request,
                    input_dim,
                    seq_len,
                    device: (*device).to_string(),
                });
            }

            failures.push(format!("{device}: status {status}"));
        }

        unsafe {
            if !core.is_null() {
                (api.ov_core_free)(core);
            }
        }

        Err(io::Error::other(format!(
            "failed to compile OpenVINO model on NPU/AUTO/CPU ({})",
            failures.join(", ")
        )))
    }
}

impl Predictor for OpenVinoPredictor {
    fn device(&self) -> &str {
        &self.device
    }

    fn infer(&self, input_flat_norm: &[f32]) -> io::Result<Vec<f32>> {
        let expected_len = self.seq_len * self.input_dim;
        if input_flat_norm.len() != expected_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "OpenVINO input length mismatch: expected {expected_len}, got {}",
                    input_flat_norm.len()
                ),
            ));
        }

        let mut input_data = input_flat_norm.to_vec();
        let mut dims = [1_usize, self.seq_len, self.input_dim];
        let input_shape = OvShape {
            rank: dims.len(),
            dims: dims.as_mut_ptr(),
        };
        let mut input_tensor = ptr::null_mut();

        ov_check(
            unsafe {
                (self.api.ov_tensor_create_from_host_ptr)(
                    OV_ELEMENT_F32,
                    input_shape,
                    input_data.as_mut_ptr().cast::<c_void>(),
                    &mut input_tensor,
                )
            },
            "ov_tensor_create_from_host_ptr",
        )?;

        let result = self.infer_with_input_tensor(input_tensor);

        unsafe {
            if !input_tensor.is_null() {
                (self.api.ov_tensor_free)(input_tensor);
            }
        }

        result
    }
}

impl OpenVinoPredictor {
    fn infer_with_input_tensor(&self, input_tensor: *mut OvTensor) -> io::Result<Vec<f32>> {
        ov_check(
            unsafe {
                (self.api.ov_infer_request_set_input_tensor_by_index)(
                    self.infer_request,
                    0,
                    input_tensor,
                )
            },
            "ov_infer_request_set_input_tensor_by_index",
        )?;
        ov_check(
            unsafe { (self.api.ov_infer_request_infer)(self.infer_request) },
            "ov_infer_request_infer",
        )?;

        let mut output_tensor = ptr::null_mut();
        ov_check(
            unsafe {
                (self.api.ov_infer_request_get_output_tensor_by_index)(
                    self.infer_request,
                    0,
                    &mut output_tensor,
                )
            },
            "ov_infer_request_get_output_tensor_by_index",
        )?;

        let mut byte_size = 0_usize;
        let mut output_data = ptr::null_mut();
        let output_result = (|| {
            ov_check(
                unsafe { (self.api.ov_tensor_get_byte_size)(output_tensor, &mut byte_size) },
                "ov_tensor_get_byte_size",
            )?;
            ov_check(
                unsafe { (self.api.ov_tensor_data)(output_tensor, &mut output_data) },
                "ov_tensor_data",
            )?;

            let available = byte_size / mem::size_of::<f32>();
            if available < self.input_dim {
                return Err(io::Error::other(format!(
                    "OpenVINO output too small: expected at least {} f32 values, got {available}",
                    self.input_dim
                )));
            }

            let values = unsafe {
                std::slice::from_raw_parts(output_data.cast::<f32>(), self.input_dim).to_vec()
            };
            Ok(values)
        })();

        unsafe {
            if !output_tensor.is_null() {
                (self.api.ov_tensor_free)(output_tensor);
            }
        }

        output_result
    }
}

impl Drop for OpenVinoPredictor {
    fn drop(&mut self) {
        unsafe {
            if !self.infer_request.is_null() {
                (self.api.ov_infer_request_free)(self.infer_request);
            }
            if !self.compiled_model.is_null() {
                (self.api.ov_compiled_model_free)(self.compiled_model);
            }
            if !self.core.is_null() {
                (self.api.ov_core_free)(self.core);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct RollingRuntime<P> {
    window: RollingWindow,
    normalizer: Normalizer,
    predictor: P,
    pending_prediction_norm: Option<Vec<f32>>,
}

impl<P: Predictor> RollingRuntime<P> {
    pub fn new(seq_len: usize, input_dim: usize, normalizer: Normalizer, predictor: P) -> Self {
        Self {
            window: RollingWindow::new(seq_len, input_dim),
            normalizer,
            predictor,
            pending_prediction_norm: None,
        }
    }

    pub fn device(&self) -> &str {
        self.predictor.device()
    }

    pub fn window_len(&self) -> usize {
        self.window.len()
    }

    pub fn window_capacity(&self) -> usize {
        self.window.capacity()
    }

    pub fn step(
        &mut self,
        timestamp_ms: u128,
        row_real: Vec<f32>,
    ) -> io::Result<Option<PredictionResult>> {
        let row_norm = self.normalizer.normalize_row(&row_real);

        let completed_prediction = self.pending_prediction_norm.take().map(|pred_norm| {
            let anomaly_score = anomaly_score_mse(&pred_norm, &row_norm);
            let pred_real = self.normalizer.denormalize_row(&pred_norm);

            PredictionResult {
                pred_norm,
                pred_real,
                actual_real: Some(row_real.clone()),
                anomaly_score: Some(anomaly_score),
                timestamp_ms,
            }
        });

        self.window.push(row_real);

        if self.window.is_ready() {
            let flat_real = self.window.as_flat_input();
            let flat_norm = self
                .normalizer
                .normalize_window_flat(&flat_real, self.window.input_dim);
            self.pending_prediction_norm = Some(self.predictor.infer(&flat_norm)?);
        }

        Ok(completed_prediction)
    }
}

fn sanitize_feature(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn read_stored_zip_entries(path: &Path) -> io::Result<Vec<(String, Vec<u8>)>> {
    let bytes = std::fs::read(path)?;
    let mut offset = 0_usize;
    let mut entries = Vec::new();

    while offset + 4 <= bytes.len() {
        let signature = le_u32(&bytes, offset)?;
        if signature == 0x0201_4b50 || signature == 0x0605_4b50 {
            break;
        }
        if signature != 0x0403_4b50 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid ZIP local file header",
            ));
        }

        let flags = le_u16(&bytes, offset + 6)?;
        let compression = le_u16(&bytes, offset + 8)?;
        if flags & 0x08 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ZIP data descriptors are not supported for normalization stats",
            ));
        }
        if compression != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "compressed NPZ entries are not supported; save stats with numpy.savez",
            ));
        }

        let compressed_size = le_u32(&bytes, offset + 18)? as usize;
        let name_len = le_u16(&bytes, offset + 26)? as usize;
        let extra_len = le_u16(&bytes, offset + 28)? as usize;
        let name_start = offset + 30;
        let name_end = name_start + name_len;
        let data_start = name_end + extra_len;
        let data_end = data_start + compressed_size;

        if data_end > bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "truncated ZIP entry in normalization stats",
            ));
        }

        let name =
            String::from_utf8(bytes[name_start..name_end].to_vec()).map_err(invalid_input)?;
        entries.push((name, bytes[data_start..data_end].to_vec()));
        offset = data_end;
    }

    Ok(entries)
}

fn parse_npy_vector(bytes: &[u8]) -> io::Result<Vec<f32>> {
    if bytes.len() < 10 || &bytes[..6] != b"\x93NUMPY" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid NPY header",
        ));
    }

    let major = bytes[6];
    let header_start;
    let header_len;
    match major {
        1 => {
            header_len = le_u16(bytes, 8)? as usize;
            header_start = 10;
        }
        2 | 3 => {
            header_len = le_u32(bytes, 8)? as usize;
            header_start = 12;
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported NPY version: {major}"),
            ));
        }
    }

    let header_end = header_start + header_len;
    if header_end > bytes.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "truncated NPY header",
        ));
    }

    let header = std::str::from_utf8(&bytes[header_start..header_end]).map_err(invalid_input)?;
    if !header.contains("'fortran_order': False") && !header.contains("\"fortran_order\": False") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Fortran-order normalization arrays are not supported",
        ));
    }

    let dtype = if header.contains("'descr': '<f4'")
        || header.contains("'descr': '|f4'")
        || header.contains("\"descr\": \"<f4\"")
    {
        4
    } else if header.contains("'descr': '<f8'") || header.contains("\"descr\": \"<f8\"") {
        8
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported normalization dtype in NPY header: {header}"),
        ));
    };

    let values = &bytes[header_end..];
    if values.len() % dtype != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "NPY payload length is not aligned to dtype",
        ));
    }

    let mut parsed = Vec::with_capacity(values.len() / dtype);
    for chunk in values.chunks_exact(dtype) {
        parsed.push(if dtype == 4 {
            f32::from_le_bytes(chunk.try_into().expect("chunk length checked"))
        } else {
            f64::from_le_bytes(chunk.try_into().expect("chunk length checked")) as f32
        });
    }

    Ok(parsed)
}

fn parse_json_string_array(text: &str) -> io::Result<Vec<String>> {
    let mut chars = text.trim().chars().peekable();
    if chars.next() != Some('[') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "feature column JSON must be an array",
        ));
    }

    let mut values = Vec::new();
    loop {
        while matches!(chars.peek(), Some(ch) if ch.is_whitespace() || *ch == ',') {
            chars.next();
        }
        match chars.peek() {
            Some(']') => {
                chars.next();
                break;
            }
            Some('"') => {
                chars.next();
                let mut value = String::new();
                while let Some(ch) = chars.next() {
                    match ch {
                        '"' => break,
                        '\\' => {
                            let escaped = chars.next().ok_or_else(|| {
                                io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    "unterminated JSON escape",
                                )
                            })?;
                            value.push(match escaped {
                                '"' => '"',
                                '\\' => '\\',
                                '/' => '/',
                                'b' => '\u{0008}',
                                'f' => '\u{000c}',
                                'n' => '\n',
                                'r' => '\r',
                                't' => '\t',
                                other => {
                                    return Err(io::Error::new(
                                        io::ErrorKind::InvalidData,
                                        format!("unsupported JSON escape: \\{other}"),
                                    ));
                                }
                            });
                        }
                        other => value.push(other),
                    }
                }
                values.push(value);
            }
            Some(other) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unexpected JSON token in feature columns: {other}"),
                ));
            }
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unterminated feature column JSON array",
                ));
            }
        }
    }

    Ok(values)
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut quoted = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                current.push('"');
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                values.push(current.trim().to_string());
                current.clear();
            }
            other => current.push(other),
        }
    }

    values.push(current.trim().to_string());
    values
}

fn le_u16(bytes: &[u8], offset: usize) -> io::Result<u16> {
    let end = offset + 2;
    let slice = bytes.get(offset..end).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "unexpected end of binary data")
    })?;
    Ok(u16::from_le_bytes(
        slice.try_into().expect("slice length checked"),
    ))
}

fn le_u32(bytes: &[u8], offset: usize) -> io::Result<u32> {
    let end = offset + 4;
    let slice = bytes.get(offset..end).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "unexpected end of binary data")
    })?;
    Ok(u32::from_le_bytes(
        slice.try_into().expect("slice length checked"),
    ))
}

const OV_STATUS_OK: i32 = 0;
const OV_ELEMENT_F32: u32 = 4;

#[repr(C)]
struct OvCore {
    _private: [u8; 0],
}

#[repr(C)]
struct OvCompiledModel {
    _private: [u8; 0],
}

#[repr(C)]
struct OvInferRequest {
    _private: [u8; 0],
}

#[repr(C)]
struct OvTensor {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct OvShape {
    rank: usize,
    dims: *mut usize,
}

struct OpenVinoApi {
    _library: OpenVinoLibrary,
    ov_core_create: unsafe extern "C" fn(*mut *mut OvCore) -> i32,
    ov_core_free: unsafe extern "C" fn(*mut OvCore),
    ov_core_compile_model_from_file: unsafe extern "C" fn(
        *const OvCore,
        *const c_char,
        *const c_char,
        usize,
        *mut *mut OvCompiledModel,
    ) -> i32,
    ov_compiled_model_create_infer_request:
        unsafe extern "C" fn(*const OvCompiledModel, *mut *mut OvInferRequest) -> i32,
    ov_compiled_model_free: unsafe extern "C" fn(*mut OvCompiledModel),
    ov_infer_request_set_input_tensor_by_index:
        unsafe extern "C" fn(*mut OvInferRequest, usize, *mut OvTensor) -> i32,
    ov_infer_request_infer: unsafe extern "C" fn(*mut OvInferRequest) -> i32,
    ov_infer_request_get_output_tensor_by_index:
        unsafe extern "C" fn(*mut OvInferRequest, usize, *mut *mut OvTensor) -> i32,
    ov_infer_request_free: unsafe extern "C" fn(*mut OvInferRequest),
    ov_tensor_create_from_host_ptr:
        unsafe extern "C" fn(u32, OvShape, *mut c_void, *mut *mut OvTensor) -> i32,
    ov_tensor_get_byte_size: unsafe extern "C" fn(*const OvTensor, *mut usize) -> i32,
    ov_tensor_data: unsafe extern "C" fn(*const OvTensor, *mut *mut c_void) -> i32,
    ov_tensor_free: unsafe extern "C" fn(*mut OvTensor),
}

impl OpenVinoApi {
    fn load() -> io::Result<Self> {
        let library = OpenVinoLibrary::load()?;

        let api = unsafe {
            Self {
                ov_core_create: library.symbol(b"ov_core_create\0")?,
                ov_core_free: library.symbol(b"ov_core_free\0")?,
                ov_core_compile_model_from_file: library
                    .symbol(b"ov_core_compile_model_from_file\0")?,
                ov_compiled_model_create_infer_request: library
                    .symbol(b"ov_compiled_model_create_infer_request\0")?,
                ov_compiled_model_free: library.symbol(b"ov_compiled_model_free\0")?,
                ov_infer_request_set_input_tensor_by_index: library
                    .symbol(b"ov_infer_request_set_input_tensor_by_index\0")?,
                ov_infer_request_infer: library.symbol(b"ov_infer_request_infer\0")?,
                ov_infer_request_get_output_tensor_by_index: library
                    .symbol(b"ov_infer_request_get_output_tensor_by_index\0")?,
                ov_infer_request_free: library.symbol(b"ov_infer_request_free\0")?,
                ov_tensor_create_from_host_ptr: library
                    .symbol(b"ov_tensor_create_from_host_ptr\0")?,
                ov_tensor_get_byte_size: library.symbol(b"ov_tensor_get_byte_size\0")?,
                ov_tensor_data: library.symbol(b"ov_tensor_data\0")?,
                ov_tensor_free: library.symbol(b"ov_tensor_free\0")?,
                _library: library,
            }
        };

        Ok(api)
    }
}

struct OpenVinoLibrary {
    handle: *mut c_void,
}

impl OpenVinoLibrary {
    fn load() -> io::Result<Self> {
        for name in ["openvino_c.dll", "openvino.dll"] {
            let wide = wide_null(name);
            let handle = unsafe { LoadLibraryW(wide.as_ptr()) };
            if !handle.is_null() {
                return Ok(Self { handle });
            }
        }

        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "OpenVINO runtime DLL not found (expected openvino_c.dll); install OpenVINO Runtime or add it to PATH",
        ))
    }

    unsafe fn symbol<T: Copy>(&self, name: &[u8]) -> io::Result<T> {
        let ptr = unsafe { GetProcAddress(self.handle, name.as_ptr().cast::<c_char>()) };
        if ptr.is_null() {
            let symbol = String::from_utf8_lossy(name.strip_suffix(&[0]).unwrap_or(name));
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("OpenVINO symbol not found: {symbol}"),
            ));
        }

        Ok(unsafe { mem::transmute_copy(&ptr) })
    }
}

impl Drop for OpenVinoLibrary {
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_null() {
                FreeLibrary(self.handle);
            }
        }
    }
}

unsafe impl Send for OpenVinoLibrary {}
unsafe impl Sync for OpenVinoLibrary {}

unsafe extern "system" {
    fn LoadLibraryW(lp_lib_file_name: *const u16) -> *mut c_void;
    fn GetProcAddress(h_module: *mut c_void, lp_proc_name: *const c_char) -> *mut c_void;
    fn FreeLibrary(h_lib_module: *mut c_void) -> i32;
}

fn validate_openvino_artifacts(model_xml_path: &Path) -> io::Result<()> {
    if !model_xml_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("OpenVINO model XML not found: {}", model_xml_path.display()),
        ));
    }

    let mut bin_path = model_xml_path.to_path_buf();
    bin_path.set_extension("bin");
    if !bin_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("OpenVINO model BIN not found: {}", bin_path.display()),
        ));
    }

    Ok(())
}

fn ov_check(status: i32, operation: &str) -> io::Result<()> {
    if status == OV_STATUS_OK {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{operation} failed with OpenVINO status {status}"
        )))
    }
}

fn path_to_cstring(path: &Path) -> io::Result<CString> {
    CString::new(path.to_string_lossy().as_bytes()).map_err(invalid_input)
}

fn invalid_input(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, error.to_string())
}

fn wide_null(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}

#[derive(Debug, Default)]
pub struct TelemetryPipeline {
    previous: HashMap<u32, PreviousProcessCounters>,
}

#[derive(Debug, Clone)]
struct PreviousProcessCounters {
    name: String,
    cpu_time_100ns: u64,
    read_bytes: u64,
    write_bytes: u64,
}

impl TelemetryPipeline {
    pub fn ingest(&mut self, samples: &[ProcessSample]) -> AggregatedTelemetry {
        let timestamp_ms = samples
            .iter()
            .map(|sample| sample.timestamp_ms)
            .max()
            .unwrap_or(0);
        let mut total_cpu_delta_100ns = 0_u64;
        let mut foreground_cpu_delta_100ns = 0_u64;
        let mut total_disk_read_bytes = 0_u64;
        let mut total_disk_write_bytes = 0_u64;
        let mut memory_pressure_bytes = 0_u64;
        let mut foreground_memory_bytes = 0_u64;
        let mut category_counts = CategoryCounts::default();
        let mut current = HashMap::with_capacity(samples.len());

        for sample in samples {
            let category = categorize_process_name(&sample.name);
            category_counts.add(category);
            memory_pressure_bytes = memory_pressure_bytes.saturating_add(sample.working_set_bytes);

            let deltas = self
                .previous
                .get(&sample.pid)
                .filter(|previous| previous.name.eq_ignore_ascii_case(&sample.name))
                .map(|previous| ProcessDeltas {
                    cpu_time_100ns: sample
                        .cpu_time_100ns
                        .saturating_sub(previous.cpu_time_100ns),
                    read_bytes: sample.read_bytes.saturating_sub(previous.read_bytes),
                    write_bytes: sample.write_bytes.saturating_sub(previous.write_bytes),
                })
                .unwrap_or_default();

            total_cpu_delta_100ns = total_cpu_delta_100ns.saturating_add(deltas.cpu_time_100ns);
            total_disk_read_bytes = total_disk_read_bytes.saturating_add(deltas.read_bytes);
            total_disk_write_bytes = total_disk_write_bytes.saturating_add(deltas.write_bytes);

            if sample.is_foreground {
                foreground_cpu_delta_100ns =
                    foreground_cpu_delta_100ns.saturating_add(deltas.cpu_time_100ns);
                foreground_memory_bytes =
                    foreground_memory_bytes.saturating_add(sample.working_set_bytes);
            }

            current.insert(
                sample.pid,
                PreviousProcessCounters {
                    name: sample.name.clone(),
                    cpu_time_100ns: sample.cpu_time_100ns,
                    read_bytes: sample.read_bytes,
                    write_bytes: sample.write_bytes,
                },
            );
        }

        self.previous = current;

        AggregatedTelemetry {
            timestamp_ms,
            total_cpu_delta_100ns,
            foreground_cpu_delta_100ns,
            total_disk_read_bytes,
            total_disk_write_bytes,
            active_process_count: samples.len() as u32,
            memory_pressure_bytes,
            foreground_memory_bytes,
            io_spike_score: io_spike_score(total_disk_read_bytes, total_disk_write_bytes),
            browser_process_count: category_counts.browser,
            ide_process_count: category_counts.ide,
            game_process_count: category_counts.game,
            system_process_count: category_counts.system,
            background_process_count: category_counts.background,
            media_process_count: category_counts.media,
            other_process_count: category_counts.other,
        }
    }
}

#[derive(Debug, Default)]
struct ProcessDeltas {
    cpu_time_100ns: u64,
    read_bytes: u64,
    write_bytes: u64,
}

#[derive(Debug, Default)]
struct CategoryCounts {
    browser: u32,
    ide: u32,
    game: u32,
    system: u32,
    background: u32,
    media: u32,
    other: u32,
}

impl CategoryCounts {
    fn add(&mut self, category: ProcessCategory) {
        match category {
            ProcessCategory::Browser => self.browser += 1,
            ProcessCategory::Ide => self.ide += 1,
            ProcessCategory::Game => self.game += 1,
            ProcessCategory::System => self.system += 1,
            ProcessCategory::Background => self.background += 1,
            ProcessCategory::Media => self.media += 1,
            ProcessCategory::Other => self.other += 1,
        }
    }
}

pub fn categorize_process_name(name: &str) -> ProcessCategory {
    let normalized = name.trim().to_ascii_lowercase();
    let stem = normalized.strip_suffix(".exe").unwrap_or(&normalized);

    match stem {
        "chrome" | "msedge" | "firefox" | "brave" | "opera" | "vivaldi" => ProcessCategory::Browser,
        "code" | "devenv" | "idea64" | "pycharm64" | "clion64" | "rustrover64" | "cursor"
        | "zed" => ProcessCategory::Ide,
        "steam" | "epicgameslauncher" | "battle.net" | "riotclientservices" | "valorant"
        | "leagueclient" => ProcessCategory::Game,
        "system" | "registry" | "smss" | "csrss" | "wininit" | "services" | "lsass" | "svchost"
        | "dwm" | "explorer" => ProcessCategory::System,
        "onedrive"
        | "dropbox"
        | "googledrivesync"
        | "searchindexer"
        | "securityhealthservice"
        | "widgets" => ProcessCategory::Background,
        "vlc" | "spotify" | "obs64" | "wmplayer" | "audiodg" => ProcessCategory::Media,
        _ => ProcessCategory::Other,
    }
}

fn io_spike_score(read_bytes: u64, write_bytes: u64) -> f64 {
    const MIB: f64 = 1_048_576.0;
    (read_bytes.saturating_add(write_bytes) as f64) / MIB
}

pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn escape_csv_field(value: &str) -> String {
    if !value.contains([',', '"', '\n', '\r']) {
        return value.to_string();
    }

    let escaped = value.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

#[cfg(windows)]
pub use platform::collect_process_samples;

#[cfg(not(windows))]
pub fn collect_process_samples() -> io::Result<Vec<ProcessSample>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Ferrite telemetry collection is implemented for Windows only",
    ))
}

#[cfg(windows)]
mod platform {
    use super::{ProcessSample, now_ms};
    use std::{ffi::c_void, io, mem};

    type Bool = i32;
    type Dword = u32;
    type Handle = *mut c_void;
    type Hwnd = *mut c_void;

    const MAX_PATH: usize = 260;
    const TH32CS_SNAPPROCESS: Dword = 0x0000_0002;
    const PROCESS_QUERY_LIMITED_INFORMATION: Dword = 0x0000_1000;
    const PROCESS_VM_READ: Dword = 0x0000_0010;
    const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct FileTime {
        dw_low_date_time: Dword,
        dw_high_date_time: Dword,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct ProcessEntry32W {
        dw_size: Dword,
        cnt_usage: Dword,
        th32_process_id: Dword,
        th32_default_heap_id: usize,
        th32_module_id: Dword,
        cnt_threads: Dword,
        th32_parent_process_id: Dword,
        pc_pri_class_base: i32,
        dw_flags: Dword,
        sz_exe_file: [u16; MAX_PATH],
    }

    impl Default for ProcessEntry32W {
        fn default() -> Self {
            Self {
                dw_size: mem::size_of::<Self>() as Dword,
                cnt_usage: 0,
                th32_process_id: 0,
                th32_default_heap_id: 0,
                th32_module_id: 0,
                cnt_threads: 0,
                th32_parent_process_id: 0,
                pc_pri_class_base: 0,
                dw_flags: 0,
                sz_exe_file: [0; MAX_PATH],
            }
        }
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct ProcessMemoryCounters {
        cb: Dword,
        page_fault_count: Dword,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn CreateToolhelp32Snapshot(dw_flags: Dword, th32_process_id: Dword) -> Handle;
        fn Process32FirstW(h_snapshot: Handle, lppe: *mut ProcessEntry32W) -> Bool;
        fn Process32NextW(h_snapshot: Handle, lppe: *mut ProcessEntry32W) -> Bool;
        fn OpenProcess(
            dw_desired_access: Dword,
            b_inherit_handle: Bool,
            dw_process_id: Dword,
        ) -> Handle;
        fn GetProcessTimes(
            h_process: Handle,
            lp_creation_time: *mut FileTime,
            lp_exit_time: *mut FileTime,
            lp_kernel_time: *mut FileTime,
            lp_user_time: *mut FileTime,
        ) -> Bool;
        fn GetProcessIoCounters(h_process: Handle, lp_io_counters: *mut IoCounters) -> Bool;
        fn CloseHandle(h_object: Handle) -> Bool;
    }

    #[link(name = "Psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(
            process: Handle,
            ppsmem_counters: *mut ProcessMemoryCounters,
            cb: Dword,
        ) -> Bool;
    }

    #[link(name = "User32")]
    unsafe extern "system" {
        fn GetForegroundWindow() -> Hwnd;
        fn GetWindowThreadProcessId(hwnd: Hwnd, lpdw_process_id: *mut Dword) -> Dword;
    }

    pub fn collect_process_samples() -> io::Result<Vec<ProcessSample>> {
        let timestamp_ms = now_ms();
        let foreground_pid = foreground_process_id();
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };

        if snapshot == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }

        let result = collect_from_snapshot(snapshot, timestamp_ms, foreground_pid);
        unsafe {
            CloseHandle(snapshot);
        }
        result
    }

    fn collect_from_snapshot(
        snapshot: Handle,
        timestamp_ms: u128,
        foreground_pid: Option<u32>,
    ) -> io::Result<Vec<ProcessSample>> {
        let mut entry = ProcessEntry32W::default();

        if unsafe { Process32FirstW(snapshot, &mut entry) } == 0 {
            return Err(io::Error::last_os_error());
        }

        let mut samples = Vec::new();

        loop {
            if let Some(sample) = sample_for_entry(&entry, timestamp_ms, foreground_pid) {
                samples.push(sample);
            }

            if unsafe { Process32NextW(snapshot, &mut entry) } == 0 {
                break;
            }
        }

        Ok(samples)
    }

    fn sample_for_entry(
        entry: &ProcessEntry32W,
        timestamp_ms: u128,
        foreground_pid: Option<u32>,
    ) -> Option<ProcessSample> {
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
                0,
                entry.th32_process_id,
            )
        };

        if handle.is_null() {
            return None;
        }

        let name = widestring_to_string(&entry.sz_exe_file);
        let working_set_bytes = process_working_set_bytes(handle).unwrap_or(0);
        let cpu_time_100ns = process_cpu_time_100ns(handle).unwrap_or(0);
        let (read_bytes, write_bytes) = process_io_bytes(handle).unwrap_or((0, 0));

        unsafe {
            CloseHandle(handle);
        }

        Some(ProcessSample {
            timestamp_ms,
            pid: entry.th32_process_id,
            name,
            working_set_bytes,
            cpu_time_100ns,
            read_bytes,
            write_bytes,
            is_foreground: foreground_pid == Some(entry.th32_process_id),
        })
    }

    fn process_working_set_bytes(handle: Handle) -> Option<u64> {
        let mut counters = ProcessMemoryCounters {
            cb: mem::size_of::<ProcessMemoryCounters>() as Dword,
            ..ProcessMemoryCounters::default()
        };

        if unsafe {
            GetProcessMemoryInfo(
                handle,
                &mut counters,
                mem::size_of::<ProcessMemoryCounters>() as Dword,
            )
        } == 0
        {
            return None;
        }

        Some(counters.working_set_size as u64)
    }

    fn process_cpu_time_100ns(handle: Handle) -> Option<u64> {
        let mut creation_time = FileTime::default();
        let mut exit_time = FileTime::default();
        let mut kernel_time = FileTime::default();
        let mut user_time = FileTime::default();

        if unsafe {
            GetProcessTimes(
                handle,
                &mut creation_time,
                &mut exit_time,
                &mut kernel_time,
                &mut user_time,
            )
        } == 0
        {
            return None;
        }

        Some(filetime_to_u64(kernel_time) + filetime_to_u64(user_time))
    }

    fn process_io_bytes(handle: Handle) -> Option<(u64, u64)> {
        let mut counters = IoCounters::default();

        if unsafe { GetProcessIoCounters(handle, &mut counters) } == 0 {
            return None;
        }

        Some((counters.read_transfer_count, counters.write_transfer_count))
    }

    fn foreground_process_id() -> Option<u32> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() {
            return None;
        }

        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }

        (pid != 0).then_some(pid)
    }

    fn filetime_to_u64(value: FileTime) -> u64 {
        ((value.dw_high_date_time as u64) << 32) | value.dw_low_date_time as u64
    }

    fn widestring_to_string(value: &[u16]) -> String {
        let len = value.iter().position(|&ch| ch == 0).unwrap_or(value.len());
        String::from_utf16_lossy(&value[..len])
    }
}
