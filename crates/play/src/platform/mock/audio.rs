// Silent headless mock audio backend for automated testing environments.
#![allow(dead_code)]

pub struct MockAudio;

impl MockAudio {
    pub fn new() -> Self {
        Self
    }
}
