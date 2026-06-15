// This module is responsible for opening compressed ZIP archives.
// It searches the archive for a game file with a valid extension (e.g. .nes).
// It then extracts the game file to a temporary folder and returns the paths so we can load and later clean it up.

use anyhow::{Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct ExtractedRom {
    pub file_path: PathBuf,
    pub dir_path: PathBuf,
}

// Holds paths for a resolved ROM. If extracted from a ZIP,
// it manages the lifetime of the temporary directory via Drop.
pub struct ResolvedRom {
    pub active_path: PathBuf,
    pub extracted_dir: Option<PathBuf>,
}

impl Drop for ResolvedRom {
    fn drop(&mut self) {
        if let Some(dir) = &self.extracted_dir {
            log::info!("Cleaning up extracted ROM directory: {:?}", dir);
            if let Err(err) = fs::remove_dir_all(dir) {
                log::warn!("Failed to remove extracted ROM dir {:?}: {}", dir, err);
            }
        }
    }
}

// Decouples ROM resolution logic by handling both zip files and uncompressed files
// returning a ResolvedRom struct that manages the temporary folder's lifetime.
pub fn resolve_rom_path(
    rom_path: &Path,
    valid_extensions: &str,
    block_extract: bool,
) -> Result<ResolvedRom> {
    if !is_zip_path(rom_path) || block_extract {
        return Ok(ResolvedRom {
            active_path: rom_path.to_path_buf(),
            extracted_dir: None,
        });
    }

    let extracted = extract_zip_rom(rom_path, valid_extensions)?;
    Ok(ResolvedRom {
        active_path: extracted.file_path,
        extracted_dir: Some(extracted.dir_path),
    })
}

// Checks if a file path points to a ZIP file.
pub fn is_zip_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("zip"))
}

// Extracts a supported ROM from a ZIP file to a temporary directory.
pub fn extract_zip_rom(zip_path: &Path, valid_extensions: &str) -> Result<ExtractedRom> {
    let file = fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let index = find_zip_rom_index(&mut archive, valid_extensions)?;
    let mut entry = archive.by_index(index)?;
    let file_name = Path::new(entry.name())
        .file_name()
        .ok_or_else(|| anyhow!("ZIP ROM entry has no file name"))?
        .to_owned();

    let dir = create_temp_dir()?;
    let file_path = dir.join(file_name);
    let mut out = fs::File::create(&file_path)?;
    std::io::copy(&mut entry, &mut out)?;

    Ok(ExtractedRom {
        file_path,
        dir_path: dir,
    })
}

// Creates a unique temporary directory for extraction.
fn create_temp_dir() -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!(
        "allium-play-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

// Helper checking if a file entry name ends in a supported extension.
fn has_valid_extension(entry_name: &str, valid_extensions: &[String]) -> bool {
    let Some(extension) = Path::new(entry_name)
        .extension()
        .and_then(|val| val.to_str())
        .map(|val| val.to_ascii_lowercase())
    else {
        return false;
    };
    valid_extensions.is_empty() || valid_extensions.iter().any(|ext| ext == &extension)
}

// Finds the index of the first supported ROM entry within the ZIP archive.
fn find_zip_rom_index(
    archive: &mut zip::ZipArchive<fs::File>,
    valid_extensions: &str,
) -> Result<usize> {
    let valid_exts: Vec<String> = valid_extensions
        .split('|')
        .filter(|ext| !ext.is_empty())
        .map(|ext| ext.to_ascii_lowercase())
        .collect();

    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if !entry.is_dir() && has_valid_extension(entry.name(), &valid_exts) {
            return Ok(index);
        }
    }
    Err(anyhow!("ZIP ROM contains no supported ROM file"))
}
