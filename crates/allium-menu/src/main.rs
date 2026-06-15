#![deny(clippy::all)]
#![warn(rust_2018_idioms)]

use std::time::Duration;

use anyhow::Result;

use allium_menu::{AlliumMenu, RetroArchInfo};
use common::{
    platform::{DefaultPlatform, Platform},
    retroarch::RetroArchCommand,
};
use simple_logger::SimpleLogger;

#[tokio::main]
async fn main() -> Result<()> {
    SimpleLogger::new().env().init().unwrap();

    let info = RetroArchCommand::GetInfo.send_recv().await?.map(|ret| {
        let mut rets = ret.split_ascii_whitespace().skip(1);

        let max_disk_slots = rets.next().map_or(0, |s| s.parse().unwrap_or(0));
        let disk_slot = rets.next().map_or(0, |s| s.parse().unwrap_or(0));
        let state_slot = rets.next().map(|s| s.parse().unwrap_or(0));

        RetroArchInfo {
            max_disk_slots,
            disk_slot,
            state_slot,
        }
    });

    if info.is_some() {
        RetroArchCommand::Pause.send().await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let platform = DefaultPlatform::new()?;
    let mut app = AlliumMenu::new(platform).await?;
    app.prepare(info).await?;
    app.run_event_loop().await?;
    app.save()?;
    Ok(())
}
