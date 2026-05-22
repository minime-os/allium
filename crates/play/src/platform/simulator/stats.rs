use crate::platform::HostStats;

pub struct SimulatorStats;

impl SimulatorStats {
    pub fn new() -> Self {
        Self
    }
}

impl HostStats for SimulatorStats {
    fn cpu_usage(&mut self) -> Option<f64> {
        None
    }
}
