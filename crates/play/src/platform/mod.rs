// Platform type aliases and bootstrap.
// Each platform (Miyoo, Simulator, Mock) is a concrete struct with inherent methods.

pub mod mock;

#[cfg(feature = "miyoo")]
pub mod miyoo;

#[cfg(feature = "simulator")]
pub mod simulator;

pub fn init_logging() -> anyhow::Result<()> {
    #[cfg(feature = "miyoo")]
    return miyoo::init_logging();
    #[cfg(feature = "simulator")]
    return simulator::init_logging();
    #[cfg(not(any(feature = "miyoo", feature = "simulator")))]
    return mock::init_logging();
}

#[cfg(feature = "miyoo")]
pub type DefaultPlatform = miyoo::MiyooPlatform;

#[cfg(feature = "simulator")]
pub type DefaultPlatform = simulator::SimulatorPlatform;

#[cfg(not(any(feature = "miyoo", feature = "simulator")))]
pub type DefaultPlatform = mock::MockPlatform;
