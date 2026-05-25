# Ferrite

Ferrite is an autonomous PC optimizer desgined for Windows PCs with Intel processors, automatically analyzing your usage pattern and adjusting 
core scheduling, RAM usage, etc.

## How does it work?

Ferrite daemon logs the data of the processes on your system, which is stored locally and not collected by any means. This log is used for the internal Temporal Convolutional Network(TCN) to infer the system usage in the next timestep, allowing Ferrite engine to respond to sudden spike or increment in system load and optimize the environment accordingly.

## Framework

Ferrite uses the following stack:

**- UI/Control Panel App:** NodeGUI

**- Core/Backend:** Rust

**- Pattern Analysis:** Intel® Core™ Ultra built-in NPU with OpenVINO™

## Prerequisites

Ferrite requires Intel® Core™ Ultra platform(Meteor Lake and after), specifically its NPU, for low-power, non-disruptive real-time inference of pattern analysis model. 
Intel® Core™ i series 10th Gen and after(Ice Lake and after) may use Gaussian & Neural Accelerator(GNA) for inference acceleration(Support coming soon).
