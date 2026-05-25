# Ferrite

Ferrite is an autonomous PC optimizer desgined for Windows PCs with Intel processors, automatically analyzing your usage pattern and adjusting 
core scheduling, RAM usage, etc.

## Framework

Ferrite uses the following stack:

**- UI/Control Panel App:** WinUI 3

**- Core/Backend:** Rust

**- Pattern Analysis:** Intel® Core™ Ultra built-in NPU with OpenVINO™

## Prerequisites

Ferrite requires Intel® Core™ Ultra platform(Meteor Lake and after), specifically its NPU, for low-power, non-disruptive real-time inference of pattern analysis model. 
Intel® Core™ i series 10th Gen and after(Ice Lake and after) may use Gaussian & Neural Accelerator(GNA) for inference acceleration.
