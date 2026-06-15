pub mod minime;

pub fn init_logging() -> anyhow::Result<()> {
    minime::init_logging()
}

pub type DefaultPlatform = minime::MinimePlatform;
