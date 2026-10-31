//! Make the game executable on the player's own computer.
//!
//! The download holds no game code. It holds the Kirikae recompiler, the prebuilt runtime
//! library and a linker (Zig), in `recompiler/` next to setup. Here the recompiler translates the
//! player's own copy of the game's program files (game/exefs, written by setup) into machine
//! code, and Zig links that with the runtime into cruisn-blast(.exe). Everything happens in a
//! scratch folder that is deleted afterwards.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

#[cfg(windows)]
const EXE: &str = "cruisn-blast.exe";
#[cfg(not(windows))]
const EXE: &str = "cruisn-blast";
#[cfg(windows)]
const KIRIKAE: &str = "kirikae.exe";
#[cfg(not(windows))]
const KIRIKAE: &str = "kirikae";
#[cfg(windows)]
const ZIG: &str = "zig.exe";
#[cfg(not(windows))]
const ZIG: &str = "zig";

/// Changes whenever the recompiler, runtime or profile in a release change (CI sets it), so an
/// older executable is rebuilt after a new release is unpacked over an old one.
const BUILD_ID: &str = match option_env!("CRUISN_BUILD_ID") {
    Some(id) => id,
    None => "dev",
};

pub fn exe_name() -> &'static str {
    EXE
}

/// The `recompiler/` folder: next to setup, or (for a development checkout) given by
/// CRUISN_RECOMPILER.
fn tools_dir(setup_dir: &Path) -> Result<PathBuf, String> {
    let dir = std::env::var_os("CRUISN_RECOMPILER").map(PathBuf::from).unwrap_or_else(|| setup_dir.join("recompiler"));
    for f in [KIRIKAE, "cruisn-blast.profile", "libcruisn_blast_runtime.a", "runtime.libs"] {
        if !dir.join(f).is_file() {
            return Err(format!("{} is missing. Unpack the whole download again (setup needs the recompiler folder next to it).", dir.join(f).display()));
        }
    }
    if !dir.join("zig").join(ZIG).is_file() {
        return Err(format!("{} is missing. Unpack the whole download again.", dir.join("zig").join(ZIG).display()));
    }
    Ok(dir)
}

fn stamp_path(folder: &Path) -> PathBuf {
    folder.join("game").join("build-id")
}

/// The executable is there and was made by this release.
pub fn up_to_date(folder: &Path) -> bool {
    folder.join(EXE).is_file() && std::fs::read_to_string(stamp_path(folder)).is_ok_and(|s| s.trim() == BUILD_ID)
}

fn run(what: &str, cmd: &mut Command) -> Result<(), String> {
    let status = cmd.status().map_err(|e| format!("{what}: could not start: {e}"))?;
    if !status.success() {
        return Err(format!("{what} failed ({status}). The messages above say why."));
    }
    Ok(())
}

/// A path for the linker's response file and the assembler: forward slashes, quoted.
fn quoted(p: &Path) -> String {
    format!("\"{}\"", p.to_string_lossy().replace('\\', "/"))
}

pub fn build(setup_dir: &Path, folder: &Path) -> Result<(), String> {
    let tools = tools_dir(setup_dir)?;
    let game = folder.join("game");
    let work = folder.join("recompile-work");
    let _ = std::fs::remove_dir_all(&work);
    let result = build_in(&tools, &game, &work, folder);
    // The scratch folder holds the translated game code: never leave it lying around.
    let _ = std::fs::remove_dir_all(&work);
    result
}

fn build_in(tools: &Path, game: &Path, work: &Path, folder: &Path) -> Result<(), String> {
    let t0 = Instant::now();
    // Kirikae reads the program files; an empty stand-in replaces the 3.6 GB of game data,
    // which the finished executable reads from game/romfs.bin itself.
    let dump = work.join("dump");
    let exefs = dump.join("exefs");
    std::fs::create_dir_all(&exefs).map_err(|e| format!("{}: {e}", exefs.display()))?;
    for e in std::fs::read_dir(game.join("exefs")).map_err(|e| format!("game/exefs: {e}"))?.flatten() {
        std::fs::copy(e.path(), exefs.join(e.file_name())).map_err(|e| format!("copying the program files: {e}"))?;
    }
    std::fs::write(dump.join("romfs.bin"), b"x").map_err(|e| e.to_string())?;
    println!("  translating the game's code (several minutes; uses every CPU core)");
    let exe_tmp = make(tools, &dump, "cruisn-blast", Some(&tools.join("cruisn-blast.profile")), true, work)?;

    let exe = folder.join(EXE);
    let _ = std::fs::remove_file(&exe);
    std::fs::copy(&exe_tmp, &exe).map_err(|e| format!("{}: {e}", exe.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755));
    }
    std::fs::write(stamp_path(folder), BUILD_ID).map_err(|e| e.to_string())?;
    println!("  {EXE} made in {:.0} s", t0.elapsed().as_secs_f64());
    Ok(())
}

/// The release's own check (CI): recompile and link a small test program the same way.
pub fn selftest(setup_dir: &Path, input: &Path, out: &Path) -> Result<(), String> {
    let tools = tools_dir(setup_dir)?;
    let work = out.with_extension("work");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let exe = make(&tools, input, "selftest", None, false, &work)?;
    std::fs::copy(&exe, out).map_err(|e| format!("{}: {e}", out.display()))?;
    let _ = std::fs::remove_dir_all(&work);
    Ok(())
}

/// Recompile `input` with Kirikae and link it with the runtime: returns the executable in `work`.
/// `external`: the executable reads the game's files from game/ instead of carrying them.
fn make(tools: &Path, input: &Path, name: &str, profile: Option<&Path>, external: bool, work: &Path) -> Result<PathBuf, String> {
    let kirikae = tools.join(KIRIKAE);
    let zig = tools.join("zig").join(ZIG);
    let out = work.join("out");
    let mut cmd = Command::new(&kirikae);
    cmd.arg("build").arg(input).arg("-o").arg(&out).args(["--name", name, "--fast-regs", "--objects-only"]);
    if let Some(p) = profile {
        cmd.arg("--profile").arg(p);
    }
    run("The recompiler", cmd.env("RUST_MIN_STACK", "268435456"))?;
    let project = out.join(format!("{name}-build"));
    let built = project.join("src").join("package.bin");
    let package = work.join("package.bin");
    if external {
        run("The recompiler (package)", Command::new(&kirikae).args(["dev", "external-package"]).arg(&built).arg(&package))?;
    } else {
        std::fs::copy(&built, &package).map_err(|e| e.to_string())?;
    }

    // The package table as an object of its own.
    #[cfg(windows)]
    let section = ".section .rdata,\"dr\"";
    #[cfg(not(windows))]
    let section = ".section .rodata";
    let asm = format!(
        "    {section}\n    .balign 16\n    .globl kirikae_package\nkirikae_package:\n    .incbin {}\n    .balign 8\n    .globl kirikae_package_len\nkirikae_package_len:\n    .quad . - kirikae_package\n",
        quoted(&package)
    );
    let asm_path = work.join("package.S");
    std::fs::write(&asm_path, asm).map_err(|e| e.to_string())?;

    let objects = std::fs::read_to_string(project.join("objects.txt")).map_err(|e| format!("objects.txt: {e}"))?;
    let mut rsp: Vec<String> = objects.lines().map(str::trim).filter(|l| !l.is_empty()).map(|l| quoted(Path::new(l))).collect();
    rsp.push(quoted(&tools.join("libcruisn_blast_runtime.a")));
    let libs = std::fs::read_to_string(tools.join("runtime.libs")).map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    {
        // ALSA and udev come from the player's system at run time; these stand-ins only name them.
        rsp.push(quoted(&tools.join("stubs").join("libasound.so.2")));
        rsp.push(quoted(&tools.join("stubs").join("libudev.so.1")));
        rsp.extend(libs.split_whitespace().filter(|l| *l != "-lasound" && *l != "-ludev").map(String::from));
    }
    #[cfg(windows)]
    {
        // Import libraries of the `windows` crates (lib/, from the runtime's build).
        rsp.push(format!("-L{}", quoted(&tools.join("lib"))));
        rsp.extend(libs.split_whitespace().map(String::from));
    }
    let rsp_path = work.join("link.rsp");
    std::fs::write(&rsp_path, rsp.join("\n")).map_err(|e| e.to_string())?;

    println!("  linking");
    let exe_tmp = work.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    let mut link = Command::new(&zig);
    link.arg("cc");
    #[cfg(windows)]
    link.args(["-target", "x86_64-windows-gnu", "-Wl,--subsystem,windows"]);
    #[cfg(not(windows))]
    link.args(["-target", "x86_64-linux-gnu.2.31"]);
    link.arg("-o").arg(&exe_tmp).arg(&asm_path).arg(format!("@{}", rsp_path.display()));
    link.current_dir(work).env("ZIG_GLOBAL_CACHE_DIR", work.join("zig-cache")).env("ZIG_LOCAL_CACHE_DIR", work.join("zig-cache"));
    run("The linker", &mut link)?;
    Ok(exe_tmp)
}
