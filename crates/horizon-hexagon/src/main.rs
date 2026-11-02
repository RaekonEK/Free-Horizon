//! `hexagon`: port Minecraft: Java Edition mods to Horizon content mods.
//!
//! - `scan <dir>`: what each `.jar` / `.zip` is.
//! - `port <dir> [--work <dir>] [--out <dir>] [--only <id,...>] [--install <game folder>]`:
//!   carry what a content mod can hold into `<out>/<id>.hmod`, write
//!   `hexagon-report.md`, and optionally copy the `.hmod` files into the
//!   game folder's `horizon-mods/`.
//! - `install <out> --home <game folder>`: copy the `.hmod` files.
//! - `report <out>`: print the report.
use horizon_hexagon::{install, port_all, scan, translate_java, write_report};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage: hexagon scan <mods folder> | hexagon port <mods folder> [--work <dir>] [--out <dir>] [--only <id,...>] [--install <game folder>] | hexagon java <archive or sources folder> [--out <dir>] | hexagon install <out> --home <game folder> | hexagon report <out>";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut rest: Vec<String> = args.clone();
    let work = take_path(&mut rest, "--work")?;
    let out = take_path(&mut rest, "--out")?;
    let home = take_path(&mut rest, "--home")?;
    let only = take_string(&mut rest, "--only")?
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let command = rest.first().cloned().unwrap_or_default();
    let folder = rest.get(1).map(PathBuf::from);
    match command.as_str() {
        "scan" => {
            let folder = folder.ok_or_else(|| USAGE.to_owned())?;
            for i in scan(&folder)? {
                println!(
                    "{:<42} {:<10} {:<16} {}  {}",
                    trim(&i.file, 42),
                    i.mod_id,
                    i.kind.label(),
                    if i.license.is_empty() {
                        "?"
                    } else {
                        &i.license
                    },
                    i.note
                );
            }
            Ok(())
        }
        "port" => {
            let folder = folder.ok_or_else(|| USAGE.to_owned())?;
            let work = work.unwrap_or_else(|| default_work(&folder));
            let out = out.unwrap_or_else(|| work.join("hmods"));
            let entries = port_all(&folder, &work, &out, &only)?;
            write_report(&entries, &out)?;
            for e in &entries {
                println!(
                    "{:<9} {:<24} {}",
                    e.verdict,
                    e.mod_id,
                    if e.hmod.is_empty() {
                        &e.summary
                    } else {
                        &e.hmod
                    }
                );
            }
            println!("\n{}", out.join("hexagon-report.md").display());
            if let Some(home) = home {
                let copied = install(&out, &home)?;
                println!(
                    "installed {} mod(s) into {}",
                    copied.len(),
                    home.join("horizon-mods").display()
                );
            }
            Ok(())
        }
        "java" => {
            let source = folder.ok_or_else(|| USAGE.to_owned())?;
            let out = out.unwrap_or_else(|| {
                let stem = source
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "port".into());
                source
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join(".hexagon-work")
                    .join("rust")
                    .join(horizon_hexagon::rust_gen::snake(&stem))
            });
            let report = translate_java(&source, &out)?;
            println!(
                "{} files, {} classes, {} methods, {} statements; {} mixin(s), {} TODO(s)",
                report.files.len(),
                report.classes,
                report.methods,
                report.statements,
                report.mixins,
                report.issues
            );
            for f in &report.files {
                println!("{:<48} {}", trim(&f.path, 48), f.written);
            }
            println!(
                "\n{}\n{}",
                out.join("hexagon-java-report.md").display(),
                out.join("src").display()
            );
            Ok(())
        }
        "install" => {
            let out = folder.ok_or_else(|| USAGE.to_owned())?;
            let home = home.ok_or_else(|| USAGE.to_owned())?;
            let copied = install(&out, &home)?;
            println!(
                "installed {} mod(s) into {}",
                copied.len(),
                home.join("horizon-mods").display()
            );
            Ok(())
        }
        "report" => {
            let out = folder.ok_or_else(|| USAGE.to_owned())?;
            let text = std::fs::read_to_string(out.join("hexagon-report.md"))
                .map_err(|e| format!("{}: {e}", out.display()))?;
            print!("{text}");
            Ok(())
        }
        _ => Err(USAGE.to_owned()),
    }
}

fn default_work(mods: &Path) -> PathBuf {
    mods.parent()
        .unwrap_or(Path::new("."))
        .join(".hexagon-work")
}

fn take_path(args: &mut Vec<String>, name: &str) -> Result<Option<PathBuf>, String> {
    Ok(take_string(args, name)?.map(PathBuf::from))
}

fn take_string(args: &mut Vec<String>, name: &str) -> Result<Option<String>, String> {
    match args.iter().position(|a| a == name) {
        Some(i) if i + 1 < args.len() => {
            let value = args.remove(i + 1);
            args.remove(i);
            Ok(Some(value))
        }
        Some(_) => Err(USAGE.to_owned()),
        None => Ok(None),
    }
}

fn trim(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(width.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
