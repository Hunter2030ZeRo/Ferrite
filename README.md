# Ferrite

Ferrite is a low-overhead adaptive power and workload manager. The Linux reboot starts deliberately in **observe-only** mode: measure first, prove where energy is going, then add narrowly scoped control loops.

The original Ferrite prototype targeted Windows/Intel systems and used process telemetry plus a TCN predictor. The new design keeps prediction optional and makes the core useful without an NPU or ML runtime.

## Design rules

- **Sleep first.** The daemon should spend almost all idle time asleep.
- **Measure before acting.** v0.1 does not change EPP, runtime PM, cgroups, or TLP-owned settings.
- **No heavyweight runtime in the daemon.** No async runtime, GUI, database, or ML framework is required by the core.
- **TLP is complementary.** TLP owns static device defaults; Ferrite will eventually own dynamic, measured decisions.
- **Prediction is gated.** Simple states should be handled heuristically; a predictor should run only when it can improve a decision.

## Initial resource budget

| Metric | Target |
| --- | ---: |
| Idle CPU | < 0.1% |
| Steady-state wakeups | <= 2/s |
| RSS | < 10 MiB target, < 20 MiB hard ceiling |
| Idle threads | 1 |
| Steady-state heap allocation | 0 target |
| Continuous disk/network I/O | none |

## Workspace

- `crates/metrics`: shared telemetry data model
- `crates/platform`: OS-specific collection; Linux sysfs/procfs implementation first
- `crates/analyzer`: derives CPU utilization and power deltas
- `crates/predictor`: lightweight workload classification/prediction interface
- `crates/policy`: converts workload state into a desired policy
- `crates/actuator`: policy application boundary; currently no-op only
- `ferrited`: tiny resident observer daemon
- `ferrite-cli`: direct status probe for hardware bring-up

## v0.1 bring-up

```bash
cargo run -p ferrite-cli -- status
cargo run --release -p ferrited
```

The CLI takes two samples so CPU utilization and RAPL package power can be calculated. Ferrite currently discovers:

- battery capacity/status/power from `/sys/class/power_supply`
- Intel RAPL package energy from `/sys/class/powercap` or `/sys/devices/virtual/powercap`
- aggregate CPU counters from `/proc/stat`
- CPU pressure from `/proc/pressure/cpu`
- the first NVIDIA display/3D PCI function and its runtime-PM state

No system setting is changed yet.

## Next milestones

1. Validate telemetry on the Galaxy Book and quantify Ferrite's own wakeup/RSS/power overhead.
2. Replace the fixed observer timer with an event-driven Linux loop and adaptive sampling states (QUIET/NORMAL/BURST).
3. Add Unix-socket IPC so the CLI/UI never lives inside the daemon.
4. Add power-regression/culprit detection.
5. Add opt-in EPP/uclamp actuation while leaving TLP-owned static knobs alone.
6. Benchmark heuristic prediction against a tiny native TCN; NPU/OpenVINO remains an optional backend.
