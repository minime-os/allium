
pub struct MockStats;

impl MockStats {
    pub fn new() -> Self {
        Self
    }

    pub fn cpu_usage(&mut self) -> Option<f64> {
        None
    }
}
