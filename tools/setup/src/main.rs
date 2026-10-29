//! Set up the game files for the native PC build of Cruis'n Blast from your own dump.
//!
//! Drag the base game NSP, the v2.0.0 update NSP and prod.keys onto setup.exe (or put them in
//! its folder and double-click it). It writes next to itself:
//!   game\exefs\...          the game's program files (checked against v2.0.0)
//!   game\romfs.bin          the game's data with the update applied (about 3.6 GB)
//!   cruisn-blast.shaders    the game's shader programs, compiled on the first launch
//! Nothing is downloaded and nothing leaves the PC. The NSPs are only read.

mod nsp;
mod unity;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use nsp::{Nca, Read2, Source};
use sha2::{Digest, Sha256};

const BASE_TITLE: u64 = 0x0100_B410_13C8_2000;
/// SHA-256 of the v2.0.0 program files the PC build was recompiled from.
const EXPECTED: &[(&str, &str)] = &[
    ("main", "e1d8295bb659f7b88bcbc33f9cbf52926db8d7112daa97ad6458ddb49acb9829"),
    ("main.npdm", "f56ae1f58a4bbc3976b6eba8608eb4fdca628e0ef0c7cc845fbd7121ff432e18"),
    ("rtld", "e1b018d97186c07e69d1203326183dddcd575af8fe192433b848c30ca650eb82"),
    ("sdk", "c54bf6080c82b1946574714e7bac2a1d2b15cc7e1198785bde0b95b393ead722"),
    ("subsdk0", "efc5c92a4b55ff050d040f1236f02663ccfdfab14a80e59f312c38e23449860f"),
];
const CHUNK: usize = 16 << 20;

fn here() -> PathBuf {
    std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)).unwrap_or_else(|| PathBuf::from("."))
}

/// Files ending in `ext` in the folder and one level of subfolders.
fn candidates(folder: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(folder) else { return out };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if p.is_file() && name.ends_with(ext) {
            out.push(p);
        } else if p.is_dir() && !["game", "build", "kirikae", "target"].contains(&name.as_str()) {
            if let Ok(sub) = std::fs::read_dir(&p) {
                let mut s: Vec<PathBuf> = sub.flatten().map(|e| e.path()).filter(|q| q.is_file() && q.to_string_lossy().to_lowercase().ends_with(ext)).collect();
                s.sort();
                out.extend(s);
            }
        }
    }
    out
}

fn search(places: &[PathBuf], ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in places {
        if p.is_dir() {
            out.extend(candidates(p, ext));
        } else if p.is_file() && p.to_string_lossy().to_lowercase().ends_with(ext) {
            out.push(p.clone());
        }
    }
    out
}

fn find_keys(places: &[PathBuf]) -> Result<PathBuf, String> {
    let mut found = search(places, "prod.keys");
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let p = Path::new(&home).join(".switch").join("prod.keys");
        if p.is_file() {
            found.push(p);
        }
    }
    found.into_iter().next().ok_or_else(|| "prod.keys not found. Put the prod.keys dumped from your Switch next to setup.exe, or drag it onto setup.exe.".into())
}

struct Game {
    src: Source,
    nca_offset: u64,
    path: PathBuf,
}

/// The program NCA of an NSP (offset), if it is Cruis'n Blast; and whether it is the update.
fn program_of(path: &Path, keys: &nsp::Keys) -> Result<Option<(Game, bool)>, String> {
    let src = Source::open(path)?;
    let ents = nsp::read_pfs0(&|o, n| src.read(o, n), 0)?;
    let tks = nsp::ticket_titlekeys(&src, &ents, keys)?;
    for (name, off, _) in &ents {
        if !name.ends_with(".nca") {
            continue;
        }
        let nca = Nca::open(&src, *off, keys, &tks)?;
        if nca.content_type != 0 {
            continue;
        }
        if nca.title_id & !0xFFF != BASE_TITLE {
            return Ok(None);
        }
        // The update's program carries the game's own title ID; its RomFS is a patch (BKTR).
        let update = nca.section(1).is_some_and(|s| s.enc_type() == 4);
        let off = *off;
        drop(nca);
        return Ok(Some((Game { src, nca_offset: off, path: path.to_path_buf() }, update)));
    }
    Ok(None)
}

fn find_games(places: &[PathBuf], keys: &nsp::Keys) -> Result<(Game, Game), String> {
    let (mut base, mut update) = (None, None);
    for path in search(places, ".nsp") {
        match program_of(&path, keys) {
            Ok(Some((g, true))) if update.is_none() => update = Some(g),
            Ok(Some((g, false))) if base.is_none() => base = Some(g),
            Ok(_) => {}
            Err(e) => println!("  skipping {}: {e}", path.file_name().unwrap_or_default().to_string_lossy()),
        }
    }
    let base = base.ok_or("The Cruis'n Blast base game NSP (0100B41013C82000) was not found.")?;
    let update = update.ok_or(
        "The Cruis'n Blast v2.0.0 update NSP (0100B41013C82800) was not found.\nThe PC build is made from version 2.0.0, so the update is required.",
    )?;
    Ok((base, update))
}

fn nca<'a>(g: &'a Game, keys: &nsp::Keys) -> Result<Nca<'a>, String> {
    let ents = nsp::read_pfs0(&|o, n| g.src.read(o, n), 0)?;
    let tks = nsp::ticket_titlekeys(&g.src, &ents, keys)?;
    Nca::open(&g.src, g.nca_offset, keys, &tks)
}

fn sha256_file(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect())
}

fn write_exefs(update: &Nca, game: &Path) -> Result<(), String> {
    let sec = update.section(0).ok_or("The update has no program files.")?;
    let (po, _) = sec.data_region()?;
    let files = nsp::read_pfs0(&|o, n| sec.read(o, n), po)?;
    let exefs = game.join("exefs");
    std::fs::create_dir_all(&exefs).map_err(|e| format!("{}: {e}", exefs.display()))?;
    for (name, off, size) in &files {
        let bytes = sec.read(*off, *size as usize)?;
        let p = exefs.join(name);
        std::fs::write(&p, bytes).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    for (name, want) in EXPECTED {
        match sha256_file(&exefs.join(name)) {
            None => return Err(format!("The update has no {name}: this is not the v2.0.0 update.")),
            Some(h) if h != *want => return Err(format!("{name} does not match v2.0.0. Use the v2.0.0 update (version 131072).")),
            _ => {}
        }
    }
    println!("  program files OK ({} files, version 2.0.0)", files.len());
    Ok(())
}

fn romfs_ok(path: &Path) -> bool {
    let Ok(img) = nsp::Image::open(path) else { return false };
    match nsp::romfs_files(&img, 0) {
        Ok(files) => files.len() > 100 && files.iter().any(|f| f.0 == "/Data/Managed/Metadata/global-metadata.dat"),
        Err(_) => false,
    }
}

fn write_romfs(update: &Nca, base: &Nca, game: &Path) -> Result<PathBuf, String> {
    let patch = update.section(1).ok_or("The update has no game data.")?;
    let base_sec = base.section(1).ok_or("The base game has no game data.")?;
    let sec = nsp::Patched::new(patch, base_sec)?;
    let (ro, size) = sec.data_region()?;
    let out = game.join("romfs.bin");
    if std::fs::metadata(&out).is_ok_and(|m| m.len() == size) && romfs_ok(&out) {
        println!("  game data already there");
        return Ok(out);
    }
    let tmp = game.join("romfs.bin.part");
    // An interrupted setup carries on where it stopped.
    let mut done = std::fs::metadata(&tmp).map_or(0, |m| m.len() / CHUNK as u64 * CHUNK as u64).min(size);
    if done > 0 {
        println!("  carrying on from {:.2} GB", done as f64 / 1e9);
    }
    let mut f = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    f.set_len(done).map_err(|e| e.to_string())?;
    use std::io::Seek;
    f.seek(std::io::SeekFrom::Start(done)).map_err(|e| e.to_string())?;
    let (start, first) = (Instant::now(), done);
    let mut last = Instant::now() - std::time::Duration::from_secs(2);
    while done < size {
        let n = CHUNK.min((size - done) as usize);
        f.write_all(&sec.read(ro + done, n)?).map_err(|e| format!("{}: {e}", tmp.display()))?;
        done += n as u64;
        if last.elapsed().as_secs_f64() >= 1.0 || done == size {
            last = Instant::now();
            let rate = (done - first) as f64 / start.elapsed().as_secs_f64().max(0.001) / 1e6;
            print!("\r  game data: {:5.2} of {:.2} GB ({rate:.0} MB/s)   ", done as f64 / 1e9, size as f64 / 1e9);
            let _ = std::io::stdout().flush();
        }
    }
    println!();
    drop(f);
    std::fs::rename(&tmp, &out).map_err(|e| format!("{}: {e}", out.display()))?;
    if !romfs_ok(&out) {
        return Err("The game data came out damaged. Check the NSPs (re-dump them if needed).".into());
    }
    Ok(out)
}

fn write_shaders(romfs: &Path, out: &Path) -> Result<(), String> {
    let img = nsp::Image::open(romfs)?;
    let picked: Vec<(String, u64, u64)> = nsp::romfs_files(&img, 0)?
        .into_iter()
        .filter_map(|(p, o, s)| p.strip_prefix("/Data/").map(|r| (r.to_string(), o, s)))
        .filter(|(r, _, _)| unity::is_unity_file(r))
        .collect();
    let mut shaders = unity::Shaders::default();
    for (i, (rel, off, size)) in picked.iter().enumerate() {
        print!("\r  shaders: file {} of {}   ", i + 1, picked.len());
        let _ = std::io::stdout().flush();
        let bytes = img.read(*off, *size as usize)?;
        shaders.add_file(&bytes).map_err(|e| format!("{rel}: {e}"))?;
    }
    println!();
    if shaders.programs.is_empty() {
        return Err("No shaders found in the game data.".into());
    }
    shaders.write(out)?;
    println!("  {} shader programs from {} shaders", shaders.programs.len(), shaders.shaders);
    Ok(())
}

fn run(folder: &Path, places: &[PathBuf]) -> Result<(), String> {
    println!("Cruis'n Blast PC - game file setup");
    println!("Setting up {}", folder.display());
    let keys_path = find_keys(places)?;
    println!("  keys: {}", keys_path.display());
    let keys = nsp::load_keys(&keys_path)?;
    let (base, update) = find_games(places, &keys)?;
    println!("  base game: {}", base.path.file_name().unwrap_or_default().to_string_lossy());
    println!("  update:    {}", update.path.file_name().unwrap_or_default().to_string_lossy());
    let (base_nca, update_nca) = (nca(&base, &keys)?, nca(&update, &keys)?);
    let game = folder.join("game");
    std::fs::create_dir_all(&game).map_err(|e| format!("{}: {e}", game.display()))?;
    let t0 = Instant::now();
    println!("Program files");
    write_exefs(&update_nca, &game)?;
    println!("Game data (update applied)");
    let romfs = write_romfs(&update_nca, &base_nca, &game)?;
    println!("Shader programs");
    write_shaders(&romfs, &folder.join("cruisn-blast.shaders"))?;
    println!("\nDone in {:.0} s. Start cruisn-blast.exe.", t0.elapsed().as_secs_f64());
    println!("The game no longer needs the NSPs or prod.keys; keep them somewhere safe.");
    Ok(())
}

fn main() {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    // `setup --out <folder> [inputs...]` writes somewhere other than the exe's folder.
    let (folder, inputs) = match args.first().map(|a| a.as_os_str() == "--out") {
        Some(true) if args.len() >= 2 => (std::path::absolute(&args[1]).unwrap_or(args[1].clone()), args[2..].to_vec()),
        _ => (here(), args),
    };
    let mut places: Vec<PathBuf> = inputs.iter().map(|a| std::path::absolute(a).unwrap_or(a.clone())).collect();
    places.push(folder.clone());
    let code = match run(&folder, &places) {
        Ok(()) => 0,
        Err(e) => {
            println!("\nSTOP: {e}");
            1
        }
    };
    if std::env::var_os("SETUP_NO_PAUSE").is_none() {
        print!("\nPress Enter to close.");
        let _ = std::io::stdout().flush();
        let _ = std::io::stdin().read_line(&mut String::new());
    }
    std::process::exit(code);
}
