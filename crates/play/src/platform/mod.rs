// Platform type aliases and bootstrap.
// Each platform (Miyoo, Simulator, Mock) is a concrete struct with inherent methods.

pub mod mock;

#[cfg(feature = "miyoo")]
pub mod miyoo;

#[cfg(feature = "simulator")]
pub mod simulator;

#[cfg(feature = "rg35xxsp")]
pub mod rg35xxsp;

pub fn init_logging() -> anyhow::Result<()> {
    #[cfg(feature = "miyoo")]
    return miyoo::init_logging();
    #[cfg(feature = "simulator")]
    return simulator::init_logging();
    #[cfg(feature = "rg35xxsp")]
    return rg35xxsp::init_logging();
    #[cfg(not(any(feature = "miyoo", feature = "simulator", feature = "rg35xxsp")))]
    return mock::init_logging();
}

#[cfg(feature = "miyoo")]
pub type DefaultPlatform = miyoo::MiyooPlatform;

#[cfg(feature = "simulator")]
pub type DefaultPlatform = simulator::SimulatorPlatform;

#[cfg(feature = "rg35xxsp")]
pub type DefaultPlatform = rg35xxsp::Rg35xxspPlatform;

#[cfg(not(any(feature = "miyoo", feature = "simulator", feature = "rg35xxsp")))]
pub type DefaultPlatform = mock::MockPlatform;
