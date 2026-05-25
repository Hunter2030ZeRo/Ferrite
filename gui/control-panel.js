const { spawn } = require("child_process");
const path = require("path");
const {
  QMainWindow,
  QWidget,
  QLabel,
  QPushButton,
  QLineEdit,
  FlexLayout,
} = require("@nodegui/nodegui");

const rootDir = path.resolve(__dirname, "..");
const ferriteExe = path.join(rootDir, "target", "debug", "ferrite.exe");

let daemon = null;

function setText(label, value) {
  label.setText(String(value));
}

function timestamp() {
  return new Date().toLocaleTimeString("en-SG", { hour12: false });
}

function appendLog(logLabel, message) {
  const current = logLabel.text();
  const next = `[${timestamp()}] ${message}`;
  const lines = `${current}\n${next}`.trim().split("\n").slice(-14);
  logLabel.setText(lines.join("\n"));
}

function ferriteArgs(mode, config) {
  const args = ["--mode", mode, "--interval-ms", config.interval.text()];

  if (mode === "train") {
    args.push("--output", config.rawOutput.text());
    args.push("--features-output", config.featureOutput.text());
    args.push("--no-predictions");
  } else {
    args.push("--model-xml", config.modelInput.text());
    args.push("--ov-cache", "ov_cache");
    args.push("--require-openvino");
    args.push("--runtime-window", config.windowInput.text());
    if (config.noPredictionLog) {
      args.push("--no-predictions");
    }
  }

  return args;
}

function runOnce(args, onDone) {
  const child = spawn(ferriteExe, args, { cwd: rootDir, windowsHide: true });
  let stdout = "";
  let stderr = "";

  child.stdout.on("data", chunk => {
    stdout += chunk.toString();
  });
  child.stderr.on("data", chunk => {
    stderr += chunk.toString();
  });
  child.on("close", code => {
    onDone({ code, stdout: stdout.trim(), stderr: stderr.trim() });
  });
}

function parseStatusLine(text) {
  const firstJson = text
    .split(/\r?\n/)
    .map(line => line.trim())
    .find(line => line.startsWith("{") && line.endsWith("}"));
  if (!firstJson) {
    return null;
  }

  try {
    return JSON.parse(firstJson);
  } catch {
    return null;
  }
}

function updateStatusCards(status, cards) {
  if (!status) {
    return;
  }

  setText(cards.mode, status.mode || "-");
  setText(cards.device, status.device || "-");
  setText(cards.window, `${status.window_len}/${status.window_capacity}`);
  setText(cards.predictions, String(status.prediction_rows ?? 0));
  setText(cards.optimization, status.optimization || "observe");
  setText(cards.persist, status.raw_persisted ? "TRAINING LOG" : "MEMORY ONLY");
}

function startDaemon(mode, config, cards, logLabel) {
  if (daemon) {
    appendLog(logLabel, "Ferrite is already running");
    return;
  }

  const args = ferriteArgs(mode, config);
  daemon = spawn(ferriteExe, args, { cwd: rootDir, windowsHide: true });
  setText(cards.state, "ONLINE");
  setText(cards.command, `ferrite ${args.join(" ")}`);
  appendLog(logLabel, `started ${mode} mode`);

  daemon.stdout.on("data", chunk => {
    const text = chunk.toString().trim();
    if (text) {
      const parsed = parseStatusLine(text);
      updateStatusCards(parsed, cards);
      appendLog(logLabel, text);
    }
  });

  daemon.stderr.on("data", chunk => {
    const text = chunk.toString().trim();
    if (text) {
      appendLog(logLabel, text);
    }
  });

  daemon.on("close", code => {
    setText(cards.state, `OFFLINE (${code})`);
    appendLog(logLabel, `daemon stopped with code ${code}`);
    daemon = null;
  });
}

function stopDaemon(cards, logLabel) {
  if (!daemon) {
    setText(cards.state, "OFFLINE");
    appendLog(logLabel, "daemon is not running");
    return;
  }

  daemon.kill();
  daemon = null;
  setText(cards.state, "STOPPING");
  appendLog(logLabel, "stop requested");
}

function makeLabel(text, objectName) {
  const label = new QLabel();
  label.setText(text);
  if (objectName) {
    label.setObjectName(objectName);
  }
  return label;
}

function makeInput(value) {
  const input = new QLineEdit();
  input.setText(value);
  return input;
}

function makeButton(text) {
  const button = new QPushButton();
  button.setText(text);
  return button;
}

function makePanel(title) {
  const panel = new QWidget();
  panel.setObjectName("panel");
  const layout = new FlexLayout();
  panel.setLayout(layout);
  layout.addWidget(makeLabel(title, "panelTitle"));
  return { panel, layout };
}

function addField(layout, label, input) {
  layout.addWidget(makeLabel(label, "fieldLabel"));
  layout.addWidget(input);
}

function addMetric(layout, label, valueLabel) {
  const row = new QWidget();
  row.setObjectName("metricRow");
  const rowLayout = new FlexLayout();
  row.setLayout(rowLayout);
  rowLayout.addWidget(makeLabel(label, "metricName"));
  rowLayout.addWidget(valueLabel);
  layout.addWidget(row);
}

function main() {
  const win = new QMainWindow();
  win.setWindowTitle("Ferrite Control");
  win.resize(940, 680);

  const root = new QWidget();
  root.setObjectName("root");
  const layout = new FlexLayout();
  root.setLayout(layout);

  const eyebrow = makeLabel("NPU WORKLOAD INTELLIGENCE", "eyebrow");
  const title = makeLabel("Ferrite Control", "title");
  const subtitle = makeLabel("Telemetry capture, TCN inference, and conservative optimization control.", "subtitle");

  const cards = {
    state: makeLabel("OFFLINE", "stateValue"),
    mode: makeLabel("-", "metricValue"),
    device: makeLabel("-", "metricValue"),
    window: makeLabel("0/60", "metricValue"),
    predictions: makeLabel("0", "metricValue"),
    optimization: makeLabel("observe", "metricValue"),
    persist: makeLabel("MEMORY ONLY", "metricValue"),
    command: makeLabel("ferrite", "commandText"),
  };

  const config = {
    modelInput: makeInput("C:/Users/ss_ch/Downloads/ferrite_tcn_fixed.xml"),
    rawOutput: makeInput("ferrite_log.csv"),
    featureOutput: makeInput("ferrite_tcn_features.csv"),
    interval: makeInput("1000"),
    windowInput: makeInput("60"),
    noPredictionLog: true,
  };

  const statusPanel = makePanel("Runtime Status");
  addMetric(statusPanel.layout, "State", cards.state);
  addMetric(statusPanel.layout, "Mode", cards.mode);
  addMetric(statusPanel.layout, "Device", cards.device);
  addMetric(statusPanel.layout, "Window", cards.window);
  addMetric(statusPanel.layout, "Predictions", cards.predictions);
  addMetric(statusPanel.layout, "Optimization", cards.optimization);
  addMetric(statusPanel.layout, "Persistence", cards.persist);

  const configPanel = makePanel("Model & Timing");
  addField(configPanel.layout, "Model XML", config.modelInput);
  addField(configPanel.layout, "Interval ms", config.interval);
  addField(configPanel.layout, "TCN window", config.windowInput);
  addField(configPanel.layout, "Raw log path", config.rawOutput);
  addField(configPanel.layout, "Feature log path", config.featureOutput);

  const actionPanel = makePanel("Controls");
  const startTrain = makeButton("Start Training Capture");
  const startInfer = makeButton("Start NPU Inference");
  const refresh = makeButton("Probe Status");
  const stop = makeButton("Stop Engine");
  const logLabel = makeLabel("ready", "logText");

  startTrain.addEventListener("clicked", () => {
    startDaemon("train", config, cards, logLabel);
  });
  startInfer.addEventListener("clicked", () => {
    startDaemon("infer", config, cards, logLabel);
  });
  refresh.addEventListener("clicked", () => {
    runOnce(["--mode", "infer", "--baseline-predictor", "--status-once", "--no-predictions"], result => {
      const parsed = parseStatusLine(result.stdout);
      updateStatusCards(parsed, cards);
      appendLog(logLabel, result.stdout || result.stderr || `exit ${result.code}`);
    });
  });
  stop.addEventListener("clicked", () => stopDaemon(cards, logLabel));

  [startTrain, startInfer, refresh, stop].forEach(button => actionPanel.layout.addWidget(button));

  const commandPanel = makePanel("Command");
  commandPanel.layout.addWidget(cards.command);

  const logPanel = makePanel("Event Stream");
  logPanel.layout.addWidget(logLabel);

  [
    eyebrow,
    title,
    subtitle,
    statusPanel.panel,
    configPanel.panel,
    actionPanel.panel,
    commandPanel.panel,
    logPanel.panel,
  ].forEach(widget => layout.addWidget(widget));

  root.setStyleSheet(`
    #root {
      padding: 18px;
      flex-direction: column;
      gap: 10px;
      background-color: #071014;
      color: #eaf8ff;
      font-family: Consolas;
    }
    #eyebrow {
      color: #46f0c2;
      font-size: 11px;
      letter-spacing: 0px;
    }
    #title {
      color: #f1fbff;
      font-size: 28px;
      font-weight: bold;
    }
    #subtitle {
      color: #8fa8b3;
      font-size: 13px;
      margin-bottom: 6px;
    }
    #panel {
      padding: 12px;
      border: 1px solid #1d3b45;
      background-color: #0b171d;
      flex-direction: column;
      gap: 6px;
    }
    #panelTitle {
      color: #54d6ff;
      font-size: 14px;
      font-weight: bold;
      margin-bottom: 4px;
    }
    #metricRow {
      flex-direction: row;
      gap: 12px;
      min-height: 24px;
    }
    #metricName {
      color: #78919b;
      min-width: 140px;
    }
    #metricValue, #stateValue {
      color: #e9fbff;
      font-weight: bold;
    }
    #stateValue {
      color: #5cffb1;
    }
    #fieldLabel {
      color: #78919b;
      margin-top: 4px;
    }
    #commandText, #logText {
      color: #b9f6ff;
      background-color: #050b0e;
      padding: 8px;
      border: 1px solid #16333d;
      font-size: 12px;
    }
    QLabel {
      color: #d7edf4;
      font-size: 13px;
    }
    QLineEdit {
      padding: 8px;
      border: 1px solid #244650;
      background-color: #081115;
      color: #f4fdff;
      min-height: 22px;
    }
    QPushButton {
      padding: 10px;
      border: 1px solid #2e6371;
      background-color: #10242b;
      color: #eefcff;
      min-height: 28px;
    }
    QPushButton:hover {
      background-color: #17333d;
      border: 1px solid #48d7ff;
    }
  `);

  win.setCentralWidget(root);
  win.show();
  global.ferriteControlWindow = win;
}

try {
  main();
} catch (error) {
  console.error(error && error.stack ? error.stack : error);
  process.exitCode = 1;
}
