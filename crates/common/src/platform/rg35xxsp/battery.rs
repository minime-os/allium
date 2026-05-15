use std::fs;

use anyhow::Result;

use crate::battery::Battery;

const BATTERY_DIR: &str = "/sys/class/power_supply/axp20x-battery";

pub struct Rg35xxSpBattery {
    charging: bool,
    percentage: i32,
}

impl Rg35xxSpBattery {
    pub fn new() -> Self {
        Self {
            charging: false,
            percentage: 100,
        }
    }
}

impl Battery for Rg35xxSpBattery {
    fn update(&mut self) -> Result<()> {
        self.percentage = read_capacity().unwrap_or(self.percentage);
        self.charging = read_status().is_some_and(|status| status != "Discharging");
        Ok(())
    }

    fn percentage(&self) -> i32 {
        self.percentage
    }

    fn charging(&self) -> bool {
        self.charging
    }
}

fn read_capacity() -> Option<i32> {
    fs::read_to_string(format!("{BATTERY_DIR}/capacity"))
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn read_status() -> Option<String> {
    Some(
        fs::read_to_string(format!("{BATTERY_DIR}/status"))
            .ok()?
            .trim()
            .to_string(),
    )
}
