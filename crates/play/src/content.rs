// Content loading: resolves ROM paths, handles ZIP extraction, and constructs
// the libretro game info descriptor passed to the core.

use crate::core::CoreInfo;
use crate::paths::PlayPaths;
use crate::unzip;
use anyhow::{Result, anyhow};
use libretro::retro_game_info;
use std::ffi::CString;
use std::fs;
use std::os::raw::c_void;
use std::ptr;

pub fn resolve_and_prepare_rom(
    paths: &PlayPaths,
    sys_info: &CoreInfo,
) -> Result<(
    retro_game_info,
    Option<unzip::ResolvedRom>,
    Option<Vec<u8>>,
    Option<CString>,
)> {
    let resolved = unzip::resolve_rom_path(
        &paths.rom,
        &sys_info.valid_extensions,
        sys_info.block_extract,
    )?;
    let path_str = resolved
        .active_path
        .to_str()
        .ok_or_else(|| anyhow!("Invalid ROM path"))?;
    let rom_path_cstring = CString::new(path_str)?;

    let mut game_info = retro_game_info {
        path: rom_path_cstring.as_ptr(),
        data: ptr::null(),
        size: 0,
        meta: ptr::null(),
    };

    let rom_data = if !sys_info.need_fullpath {
        let data = fs::read(&resolved.active_path)?;
        game_info.data = data.as_ptr() as *const c_void;
        game_info.size = data.len();
        Some(data)
    } else {
        None
    };

    Ok((game_info, Some(resolved), rom_data, Some(rom_path_cstring)))
}
