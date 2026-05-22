// Silent headless mock audio backend for automated testing environments.

use crate::platform::AudioBackend;

pub struct MockAudio;

impl AudioBackend for MockAudio {}

impl MockAudio {
    // Initializes the mock audio backend.
    pub fn new() -> Self {
        Self
    }
}
