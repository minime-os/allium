use anyhow::{Result, anyhow};
use std::path::PathBuf;

// Play is launched by Allium, so the CLI is a small contract between processes.
#[derive(Debug, PartialEq)]
pub struct Args {
    pub rom: PathBuf,
    pub core_path: PathBuf,
    pub core_id: String,
    pub dump_frame: Option<PathBuf>,
}

// The list of flags Play accepts.
enum Flag {
    Rom,
    Core,
    CoreId,
    DumpFrame,
}

impl Flag {
    fn parse(raw: &str) -> Result<Self> {
        match raw {
            "--rom" => Ok(Self::Rom),
            "--core" => Ok(Self::Core),
            "--core-id" => Ok(Self::CoreId),
            "--dump-frame" => Ok(Self::DumpFrame),
            _ => Err(anyhow!("Unknown argument: {}", raw)),
        }
    }
}

impl Args {
    // argv[0] is the binary name, "play", so skip it and parse the real arguments.
    pub fn from_env() -> Result<Self> {
        Self::parse_from(std::env::args().skip(1))
    }

    pub fn parse_from<I, T>(raw_args: I) -> Result<Self>
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        let mut raw_args = raw_args.into_iter();
        let mut rom = None;
        let mut core_path = None;
        let mut core_id = None;
        let mut dump_frame = None;

        // Raw args come in flag/value pairs, like "--rom game.nes".
        while let Some(raw_arg) = raw_args.next() {
            let raw_flag: String = raw_arg.into();
            let flag = Flag::parse(&raw_flag)?;
            let value = check_value(&mut raw_args, &raw_flag)?;

            match flag {
                Flag::Rom => {
                    rom = Some(PathBuf::from(value));
                }
                Flag::Core => {
                    core_path = Some(PathBuf::from(value));
                }
                Flag::CoreId => {
                    core_id = Some(value);
                }
                Flag::DumpFrame => {
                    dump_frame = Some(PathBuf::from(value));
                }
            }
        }

        // Return the final parsed args.
        Ok(Self {
            rom: rom.ok_or_else(|| anyhow!("Missing required argument: --rom"))?,
            core_path: core_path.ok_or_else(|| anyhow!("Missing required argument: --core"))?,
            core_id: core_id.ok_or_else(|| anyhow!("Missing required argument: --core-id"))?,
            dump_frame, // Optional.
        })
    }
}

// A flag like "--rom" must be followed by a value; otherwise the command is incomplete.
fn check_value<I, T>(iter: &mut I, flag: &str) -> Result<String>
where
    I: Iterator<Item = T>,
    T: Into<String>,
{
    iter.next()
        .map(Into::into)
        .ok_or_else(|| anyhow!("Missing value for {}", flag))
}

// These tests show which arguments Play accepts and rejects.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid() {
        let args = Args::parse_from(vec![
            "--rom",
            "test.nes",
            "--core",
            "nes_libretro.so",
            "--core-id",
            "nes",
        ])
        .unwrap();
        assert_eq!(args.rom, PathBuf::from("test.nes"));
        assert_eq!(args.core_path, PathBuf::from("nes_libretro.so"));
        assert_eq!(args.core_id, "nes");
    }

    #[test]
    fn test_parse_missing_rom() {
        let args = Args::parse_from(vec!["--core", "nes_libretro.so", "--core-id", "nes"]);
        assert!(args.is_err());
        assert_eq!(
            args.unwrap_err().to_string(),
            "Missing required argument: --rom"
        );
    }

    #[test]
    fn test_parse_missing_core() {
        let args = Args::parse_from(vec!["--rom", "test.nes", "--core-id", "nes"]);
        assert!(args.is_err());
        assert_eq!(
            args.unwrap_err().to_string(),
            "Missing required argument: --core"
        );
    }

    #[test]
    fn test_parse_missing_core_id() {
        let args = Args::parse_from(vec!["--rom", "test.nes", "--core", "nes_libretro.so"]);
        assert!(args.is_err());
        assert_eq!(
            args.unwrap_err().to_string(),
            "Missing required argument: --core-id"
        );
    }

    #[test]
    fn test_parse_unknown_arg() {
        let args = Args::parse_from(vec![
            "--rom",
            "test.nes",
            "--core",
            "nes_libretro.so",
            "--core-id",
            "nes",
            "--unknown",
            "foo",
        ]);
        assert!(args.is_err());
        assert!(args.unwrap_err().to_string().contains("Unknown argument"));
    }

    #[test]
    fn test_parse_dump_frame() {
        let args = Args::parse_from(vec![
            "--rom",
            "test.nes",
            "--core",
            "nes_libretro.so",
            "--core-id",
            "nes",
            "--dump-frame",
            "frame.ppm",
        ])
        .unwrap();

        assert_eq!(args.dump_frame, Some(PathBuf::from("frame.ppm")));
    }
}
