use ferrite_policy::DesiredPolicy;
use std::io;

#[derive(Debug, Clone, Copy, Default)]
pub struct ActuationReport {
    pub changed: bool,
}

pub trait Actuator {
    fn apply(&mut self, desired: &DesiredPolicy) -> io::Result<ActuationReport>;
}

#[derive(Debug, Default)]
pub struct NoopActuator;

impl Actuator for NoopActuator {
    fn apply(&mut self, _desired: &DesiredPolicy) -> io::Result<ActuationReport> {
        Ok(ActuationReport { changed: false })
    }
}
