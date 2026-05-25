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

function ferriteArgs(mode, modelXml, noPredictionLog) {
  const args = ["--mode", mode, "--interval-ms", "1000"];

  if (mode === "infer") {
    args.push("--model-xml", modelXml);
    args.push("--ov-cache", "ov_cache");
    args.push("--require-openvino");
    if (noPredictionLog) {
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

function startDaemon(args, labels) {
  if (daemon) {
    return;
  }

  daemon = spawn(ferriteExe, args, { cwd: rootDir, windowsHide: true });
  setText(labels.state, `Running ${args.includes("infer") ? "infer" : "train"}`);
  setText(labels.command, `ferrite ${args.join(" ")}`);

  daemon.stdout.on("data", chunk => {
    const text = chunk.toString().trim();
    if (text) {
      setText(labels.status, text);
    }
  });

  daemon.stderr.on("data", chunk => {
    const text = chunk.toString().trim();
    if (text) {
      setText(labels.status, text);
    }
  });

  daemon.on("close", code => {
    setText(labels.state, `Stopped (${code})`);
    daemon = null;
  });
}

function stopDaemon(labels) {
  if (!daemon) {
    setText(labels.state, "Stopped");
    return;
  }

  daemon.kill();
  daemon = null;
  setText(labels.state, "Stopping...");
}

function makeLabel(text) {
  const label = new QLabel();
  label.setText(text);
  return label;
}

function main() {
  const win = new QMainWindow();
  win.setWindowTitle("Ferrite Control");
  win.resize(760, 460);

  const root = new QWidget();
  root.setObjectName("root");
  const layout = new FlexLayout();
  root.setLayout(layout);

  const title = makeLabel("Ferrite Control");
  title.setObjectName("title");

  const modelLabel = makeLabel("Model XML");
  const modelInput = new QLineEdit();
  modelInput.setText("C:/Users/ss_ch/Downloads/ferrite_tcn_fixed.xml");

  const state = makeLabel("Stopped");
  const status = makeLabel("Ready");
  const command = makeLabel("ferrite");

  const startTrain = new QPushButton();
  startTrain.setText("Start Training Log");
  startTrain.addEventListener("clicked", () => {
    startDaemon(ferriteArgs("train", modelInput.text(), false), { state, status, command });
  });

  const startInfer = new QPushButton();
  startInfer.setText("Start Inference");
  startInfer.addEventListener("clicked", () => {
    startDaemon(ferriteArgs("infer", modelInput.text(), true), { state, status, command });
  });

  const refresh = new QPushButton();
  refresh.setText("Status Once");
  refresh.addEventListener("clicked", () => {
    runOnce(["--mode", "infer", "--baseline-predictor", "--status-once", "--no-predictions"], result => {
      setText(status, result.stdout || result.stderr || `exit ${result.code}`);
    });
  });

  const stop = new QPushButton();
  stop.setText("Stop");
  stop.addEventListener("clicked", () => stopDaemon({ state, status, command }));

  [
    title,
    modelLabel,
    modelInput,
    startTrain,
    startInfer,
    refresh,
    stop,
    makeLabel("State"),
    state,
    makeLabel("Status"),
    status,
    makeLabel("Command"),
    command,
  ].forEach(widget => layout.addWidget(widget));

  root.setStyleSheet(`
    #root {
      padding: 18px;
      flex-direction: column;
      gap: 10px;
      background-color: #101214;
      color: #e8ecef;
    }
    #title {
      font-size: 24px;
      font-weight: bold;
      margin-bottom: 8px;
    }
    QLabel {
      color: #d7dde2;
      font-size: 13px;
    }
    QLineEdit {
      padding: 8px;
      border: 1px solid #394047;
      background-color: #171a1d;
      color: #f4f7f9;
    }
    QPushButton {
      padding: 10px;
      border: 1px solid #47515a;
      background-color: #23282d;
      color: #f4f7f9;
    }
    QPushButton:hover {
      background-color: #2e363d;
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
