// Silent headless mock audio backend for automated testing environments.

pub struct MockAudio;

impl MockAudio {
    pub fn new() -> Self {
        Self
    }
}
