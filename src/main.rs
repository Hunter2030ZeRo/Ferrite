use std::{env, error::Error, path::PathBuf, thread, time::Duration};

use ferrite::{
    AggregatedTelemetry, CsvAggregatedTelemetryLogger, CsvPredictionLogger, CsvProcessLogger,
    LastValuePredictor, Normalizer, OpenVinoPredictor, OptimizationEngine, Predictor,
    RollingRuntime, TelemetryPipeline, collect_process_samples, load_feature_columns_json,
    window_progress_capacity,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunMode {
    Train,
    Infer,
}

impl RunMode {
    fn as_str(self) -> &'static str {
        match self {
            RunMode::Train => "train",
            RunMode::Infer => "infer",
        }
    }
}

#[derive(Debug, Clone)]
struct RunConfig {
    mode: RunMode,
    raw_output: PathBuf,
    features_output: Option<PathBuf>,
    prediction_output: Option<PathBuf>,
    model_xml: PathBuf,
    ov_cache_dir: PathBuf,
    norm_stats: PathBuf,
    feature_cols: PathBuf,
    stats_csv: PathBuf,
    require_openvino: bool,
    baseline_predictor: bool,
    status_once: bool,
    interval: Duration,
    iterations: Option<u64>,
    runtime_window: usize,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            mode: RunMode::Train,
            raw_output: PathBuf::from("ferrite_log.csv"),
            features_output: Some(PathBuf::from("ferrite_tcn_features.csv")),
            prediction_output: Some(PathBuf::from("ferrite_prediction_log.csv")),
            model_xml: PathBuf::from("models/ferrite_tcn.xml"),
            ov_cache_dir: PathBuf::from("ov_cache"),
            norm_stats: PathBuf::from("models/ferrite_norm_stats.npz"),
            feature_cols: PathBuf::from("models/ferrite_feature_cols.json"),
            stats_csv: PathBuf::from("ferrite_tcn_features.csv"),
            require_openvino: false,
            baseline_predictor: false,
            status_once: false,
            interval: Duration::from_secs(1),
            iterations: None,
            runtime_window: 60,
        }
    }
}

impl RunConfig {
    fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut config = Self::default();
        let mut args = args.into_iter();
        let _program = args.next();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-h" | "--help" => return Err(usage()),
                "--mode" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--mode requires train or infer".to_string())?;
                    config.mode = match value.as_str() {
                        "train" => RunMode::Train,
                        "infer" => RunMode::Infer,
                        _ => return Err("--mode must be either train or infer".to_string()),
                    };
                }
                "--train" => {
                    config.mode = RunMode::Train;
                }
                "--infer" | "--optimize" => {
                    config.mode = RunMode::Infer;
                }
                "-o" | "--output" | "--raw-output" => {
                    let value = args
                        .next()
                        .ok_or_else(|| format!("{arg} requires a path"))?;
                    config.raw_output = PathBuf::from(value);
                }
                "--features-output" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--features-output requires a path".to_string())?;
                    config.features_output = Some(PathBuf::from(value));
                }
                "--no-features" => {
                    config.features_output = None;
                }
                "--prediction-output" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--prediction-output requires a path".to_string())?;
                    config.prediction_output = Some(PathBuf::from(value));
                }
                "--no-predictions" => {
                    config.prediction_output = None;
                }
                "--model-xml" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--model-xml requires a path".to_string())?;
                    config.model_xml = PathBuf::from(value);
                }
                "--ov-cache" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--ov-cache requires a path".to_string())?;
                    config.ov_cache_dir = PathBuf::from(value);
                }
                "--norm-stats" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--norm-stats requires a path".to_string())?;
                    config.norm_stats = PathBuf::from(value);
                }
                "--feature-cols" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--feature-cols requires a path".to_string())?;
                    config.feature_cols = PathBuf::from(value);
                }
                "--stats-csv" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--stats-csv requires a path".to_string())?;
                    config.stats_csv = PathBuf::from(value);
                }
                "--require-openvino" => {
                    config.require_openvino = true;
                }
                "--baseline-predictor" => {
                    config.baseline_predictor = true;
                }
                "--status-once" => {
                    config.status_once = true;
                    config.iterations = Some(1);
                }
                "--runtime-window" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--runtime-window requires a value".to_string())?;
                    let runtime_window = value
                        .parse::<usize>()
                        .map_err(|_| "--runtime-window must be an integer".to_string())?;
                    if runtime_window == 0 {
                        return Err("--runtime-window must be greater than zero".to_string());
                    }
                    config.runtime_window = runtime_window;
                }
                "--interval-ms" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--interval-ms requires a value".to_string())?;
                    let millis = value
                        .parse::<u64>()
                        .map_err(|_| "--interval-ms must be an integer".to_string())?;
                    if millis == 0 {
                        return Err("--interval-ms must be greater than zero".to_string());
                    }
                    config.interval = Duration::from_millis(millis);
                }
                "--iterations" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--iterations requires a value".to_string())?;
                    let iterations = value
                        .parse::<u64>()
                        .map_err(|_| "--iterations must be an integer".to_string())?;
                    if iterations == 0 {
                        return Err("--iterations must be greater than zero".to_string());
                    }
                    config.iterations = Some(iterations);
                }
                unknown => return Err(format!("unknown argument: {unknown}\n\n{}", usage())),
            }
        }

        config.apply_mode_defaults();
        Ok(config)
    }

    fn apply_mode_defaults(&mut self) {
        if self.mode == RunMode::Infer {
            self.features_output = None;
            if self.prediction_output.is_none() {
                self.prediction_output = Some(PathBuf::from("ferrite_prediction_log.csv"));
            }
        }
    }

    fn inference_enabled(&self) -> bool {
        self.mode == RunMode::Infer || self.prediction_output.is_some()
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let config = match RunConfig::from_args(env::args()) {
        Ok(config) => config,
        Err(message) if message.starts_with("Usage:") => {
            println!("{message}");
            return Ok(());
        }
        Err(message) => return Err(message.into()),
    };

    let mut completed_iterations = 0_u64;
    let mut pipeline = TelemetryPipeline::default();
    let runtime_parts = build_runtime_parts(&config)?;
    let optimizer = OptimizationEngine::default();
    let mut last_optimization = "observe".to_string();
    if config.mode == RunMode::Infer && config.runtime_window != 60 {
        eprintln!(
            "warning: --runtime-window {} must match the exported TCN sequence length; use 60 unless the model was exported with another sequence length",
            config.runtime_window
        );
    }
    let mut runtime = RollingRuntime::new(
        config.runtime_window,
        runtime_parts.feature_names.len(),
        runtime_parts.normalizer,
        runtime_parts.predictor,
    );

    loop {
        let samples = collect_process_samples()?;
        if config.mode == RunMode::Train {
            CsvProcessLogger::append_samples(&config.raw_output, &samples)?;
        }
        let features = pipeline.ingest(&samples);

        if config.mode == RunMode::Train
            && let Some(features_output) = &config.features_output
        {
            CsvAggregatedTelemetryLogger::append_rows(
                features_output,
                std::slice::from_ref(&features),
            )?;
        }

        let mut wrote_prediction_rows = 0_usize;
        if config.inference_enabled() {
            let feature_row = features.to_feature_row_with_names(&runtime_parts.feature_names)?;
            if let Some(result) = runtime.step(features.timestamp_ms, feature_row.values)? {
                let decision = optimizer.decide(result.clone(), &samples);
                last_optimization = decision.summary();
                if let Some(prediction_output) = &config.prediction_output {
                    let feature_name_refs = runtime_parts
                        .feature_names
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>();
                    let rows = result.to_log_rows(runtime.device(), &feature_name_refs);
                    wrote_prediction_rows = rows.len();
                    CsvPredictionLogger::append_rows(prediction_output, &rows)?;
                }
            }
        }

        completed_iterations += 1;
        let progress_capacity =
            window_progress_capacity(completed_iterations, config.runtime_window as u64);

        if config.status_once {
            println!(
                "{{\"mode\":\"{}\",\"device\":\"{}\",\"raw_samples\":{},\"window_len\":{},\"window_capacity\":{},\"prediction_rows\":{},\"optimization\":\"{}\",\"raw_persisted\":{},\"features_persisted\":{}}}",
                config.mode.as_str(),
                runtime.device(),
                samples.len(),
                completed_iterations,
                progress_capacity,
                wrote_prediction_rows,
                json_escape(&last_optimization),
                config.mode == RunMode::Train,
                config.mode == RunMode::Train && config.features_output.is_some()
            );
        }

        match config.mode {
            RunMode::Train => eprintln!(
                "train: wrote {} raw samples to {}, 1 feature row{}, {} prediction rows",
                samples.len(),
                config.raw_output.display(),
                config
                    .features_output
                    .as_ref()
                    .map(|path| format!(" to {}", path.display()))
                    .unwrap_or_else(|| " in memory".to_string()),
                wrote_prediction_rows
            ),
            RunMode::Infer => eprintln!(
                "infer: collected {} raw samples in memory, feature row in memory, window {}/{}, {} prediction rows, optimization: {}",
                samples.len(),
                completed_iterations,
                progress_capacity,
                wrote_prediction_rows,
                last_optimization
            ),
        }

        if config.iterations == Some(completed_iterations) {
            break;
        }

        thread::sleep(config.interval);
    }

    Ok(())
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn usage() -> String {
    "Usage: ferrite [--mode train|infer] [--output PATH] [--features-output PATH] [--no-features] [--prediction-output PATH] [--no-predictions] [--model-xml PATH] [--ov-cache PATH] [--norm-stats PATH] [--feature-cols PATH] [--stats-csv PATH] [--require-openvino] [--baseline-predictor] [--status-once] [--runtime-window N] [--interval-ms N] [--iterations N]\n\
     \n\
     Modes: --mode train stores raw and feature CSVs for dataset collection. --mode infer keeps raw/features in memory and runs TCN inference for optimization use.\n\
     Defaults: --mode train --output ferrite_log.csv --features-output ferrite_tcn_features.csv --prediction-output ferrite_prediction_log.csv --model-xml models/ferrite_tcn.xml --ov-cache ov_cache --norm-stats models/ferrite_norm_stats.npz --feature-cols models/ferrite_feature_cols.json --stats-csv ferrite_tcn_features.csv --runtime-window 60 --interval-ms 1000\n\
     \n\
     Prediction mode targets OpenVINO NPU first, then AUTO, then CPU. Use --require-openvino to fail instead of falling back to the baseline predictor."
        .to_string()
}

struct RuntimeParts {
    predictor: Box<dyn Predictor>,
    normalizer: Normalizer,
    feature_names: Vec<String>,
}

fn build_runtime_parts(config: &RunConfig) -> Result<RuntimeParts, Box<dyn Error>> {
    let default_feature_names = AggregatedTelemetry::feature_names()
        .iter()
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();

    if config.baseline_predictor || !config.inference_enabled() {
        let input_dim = default_feature_names.len();
        return Ok(RuntimeParts {
            predictor: Box::new(LastValuePredictor::new(input_dim)),
            normalizer: Normalizer::identity(input_dim),
            feature_names: default_feature_names,
        });
    }

    let openvino_parts = (|| -> Result<RuntimeParts, Box<dyn Error>> {
        let feature_names = match load_feature_columns_json(&config.feature_cols) {
            Ok(feature_names) => feature_names,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!(
                    "feature columns file unavailable ({}: {error}); using built-in Ferrite feature order",
                    config.feature_cols.display()
                );
                default_feature_names.clone()
            }
            Err(error) => {
                return Err(format!(
                    "failed to load feature columns from {}: {error}",
                    config.feature_cols.display()
                )
                .into());
            }
        };
        let input_dim = feature_names.len();
        let normalizer = match Normalizer::load_npz(&config.norm_stats) {
            Ok(normalizer) => normalizer,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let features_output = config.features_output.as_ref().unwrap_or(&config.stats_csv);
                eprintln!(
                    "normalization stats unavailable ({}: {error}); estimating stats from {}",
                    config.norm_stats.display(),
                    features_output.display()
                );
                Normalizer::from_feature_csv(features_output, &feature_names).map_err(
                    |csv_error| {
                        format!(
                            "failed to estimate normalization stats from {}: {csv_error}",
                            features_output.display()
                        )
                    },
                )?
            }
            Err(error) => {
                return Err(format!(
                    "failed to load normalization stats from {}: {error}",
                    config.norm_stats.display()
                )
                .into());
            }
        };
        if normalizer.mean.len() != input_dim {
            return Err(format!(
                "normalizer dimension mismatch: feature_cols has {input_dim}, norm_stats has {}",
                normalizer.mean.len()
            )
            .into());
        }

        let predictor = OpenVinoPredictor::new_npu_first(
            &config.model_xml,
            &config.ov_cache_dir,
            config.runtime_window,
            input_dim,
        )?;

        eprintln!(
            "OpenVINO predictor loaded on {} (NPU-first target order: NPU -> AUTO -> CPU)",
            predictor.device()
        );

        Ok(RuntimeParts {
            predictor: Box::new(predictor),
            normalizer,
            feature_names,
        })
    })();

    match openvino_parts {
        Ok(parts) => Ok(parts),
        Err(error) if config.require_openvino => Err(error),
        Err(error) => {
            let input_dim = default_feature_names.len();
            eprintln!(
                "OpenVINO predictor unavailable ({error}); falling back to BASELINE. Use --require-openvino to fail instead."
            );
            Ok(RuntimeParts {
                predictor: Box::new(LastValuePredictor::new(input_dim)),
                normalizer: Normalizer::identity(input_dim),
                feature_names: default_feature_names,
            })
        }
    }
}
