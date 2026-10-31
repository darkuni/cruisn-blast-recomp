//! `main` for a Cruis'n Blast executable linked on the player's machine.
//!
//! The release ships this library, Kirikae and a linker, but no game code: setup.exe
//! recompiles the game from the player's own dump into object files (they define
//! `kirikae_entries` and `kirikae_entry_count`, the recompiled functions) and links them with
//! this library and a small object holding `kirikae_package`: the description of the game
//! files the executable reads from `game/` next to itself (names, sizes and hashes).

use kirikae_rt::Entry;

unsafe extern "C" {
    static kirikae_entries: Entry;
    static kirikae_entry_count: u64;
    static kirikae_package: u8;
    static kirikae_package_len: u64;
}

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const std::ffi::c_char) -> i32 {
    // SAFETY: the table, the package and their lengths come from objects generated together.
    let (entries, package) = unsafe {
        (
            std::slice::from_raw_parts(&raw const kirikae_entries, kirikae_entry_count as usize),
            std::slice::from_raw_parts(&raw const kirikae_package, kirikae_package_len as usize),
        )
    };
    kirikae_rt::program_main(package, entries)
}
