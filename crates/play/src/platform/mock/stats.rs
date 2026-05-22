use crate::platform::HostStats;

pub struct MockStats;

impl MockStats {
    pub fn new() -> Self {
        Self
    }
}

impl HostStats for MockStats {
    fn cpu_usage(&mut self) -> Option<f64> {
        None
    }
}
