//! HexaGon: what of a Minecraft: Java Edition mod can Horizon run, and the
//! content mod that carries it.
//!
//! A Java mod is a jar (or a source archive of one) whose data and assets a
//! Horizon content mod (`.hmod`) can carry as they are: blocks, items,
//! recipes, loot, tags, models, textures, lang, sounds, worldgen. Its Java
//! code never runs, and is never read or decompiled here: the port is built
//! from the files a content mod may hold. What the code did is reported, not
//! guessed.
//!
//! `scan` says what each file is; `port` carries the portable part of each
//! into `out/<id>.hmod` and writes `hexagon-report.md`; `install` copies the
//! `.hmod` files into a game folder's `horizon-mods/`.
use horizon_port_pack::check::{self, Check};
use horizon_port_pack::jar;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub mod java;
pub mod rust_gen;

/// What a file is, from its own contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// A built mod jar: metadata and classes at its root.
    ModJar,
    /// A source archive (GitHub's "Download ZIP", a dev checkout): resources
    /// under `src/main/resources`, code as `.java`.
    SourceArchive,
    /// A resource pack: it applies over vanilla as it is.
    ResourcePack,
    /// Nothing Horizon can read (docs, an empty archive...).
    Other,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::ModJar => "mod jar",
            Kind::SourceArchive => "source archive",
            Kind::ResourcePack => "resource pack",
            Kind::Other => "other",
        }
    }
}

/// One file, as `scan` read it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inspection {
    pub file: String,
    pub path: PathBuf,
    pub kind: Kind,
    pub mod_id: String,
    pub title: String,
    pub version: String,
    pub license: String,
    /// `fabric`, `neoforge`, `forge`, `quilt`, or empty.
    pub loader: String,
    /// `assets/` and `data/` files found.
    pub resources: usize,
    /// Why it is [`Kind::Other`], or what the carrier has to know.
    pub note: String,
}

/// One file, after `port`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub file: String,
    pub mod_id: String,
    pub title: String,
    pub version: String,
    pub license: String,
    pub loader: String,
    pub kind: Kind,
    /// Works / Partly / Doesn't run / Resource pack / Skipped.
    pub verdict: String,
    pub summary: String,
    pub ports: Vec<String>,
    pub skipped: Vec<String>,
    /// A Horizon feature that already does this, when there is one.
    pub built_in: String,
    /// The `.hmod` written, relative to the output folder.
    pub hmod: String,
}

const MARKERS: [&str; 8] = [
    "/src/main/resources/",
    "/src/client/resources/",
    "/src/common/resources/",
    "/src/generated/resources/",
    "/src/main/generated/",
    "/common/src/main/resources/",
    "/fabric/src/main/resources/",
    "/neoforge/src/main/resources/",
];

const ROOT_MARKERS: [&str; 8] = [
    "assets/",
    "data/",
    "META-INF/",
    "src/",
    "fabric.mod.json",
    "quilt.mod.json",
    "pack.mcmeta",
    "gradle.properties",
];

/// The folder a GitHub source archive wraps everything in (`Repo-main/`), or
/// `""` when the files are at the root.
fn wrapper_prefix(names: &[String]) -> String {
    if names
        .iter()
        .any(|n| ROOT_MARKERS.iter().any(|m| n.starts_with(m)))
    {
        return String::new();
    }
    let Some(first) = names.first().and_then(|n| n.split('/').next()) else {
        return String::new();
    };
    let dir = format!("{first}/");
    if names.iter().all(|n| n.starts_with(&dir)) {
        dir
    } else {
        String::new()
    }
}

/// Every `assets/` and `data/` file, by its name inside the mod.
fn resources(names: &[String], prefix: &str) -> Vec<(String, String)> {
    let mut found: BTreeMap<String, String> = BTreeMap::new();
    for name in names {
        let rest = name.strip_prefix(prefix).unwrap_or(name);
        if rest.starts_with("assets/") || rest.starts_with("data/") {
            found.entry(rest.to_owned()).or_insert_with(|| name.clone());
            continue;
        }
        for marker in MARKERS {
            let Some(at) = name.find(marker) else {
                continue;
            };
            let inside = &name[at + marker.len()..];
            if inside.starts_with("assets/") || inside.starts_with("data/") {
                found
                    .entry(inside.to_owned())
                    .or_insert_with(|| name.clone());
            }
        }
    }
    found.into_iter().collect()
}

/// The mod's metadata file, as (name in the carried jar, name in the archive).
fn metadata(names: &[String], prefix: &str) -> Option<(String, String)> {
    let roots = [
        "META-INF/neoforge.mods.toml",
        "META-INF/mods.toml",
        "fabric.mod.json",
        "quilt.mod.json",
    ];
    for root in roots {
        let at_root = format!("{prefix}{root}");
        if names.iter().any(|n| n == &at_root) {
            return Some((root.to_owned(), at_root));
        }
        if let Some(found) = names
            .iter()
            .find(|n| n.ends_with(&format!("/{root}")) && n.contains("/resources/"))
        {
            return Some((root.to_owned(), found.clone()));
        }
    }
    None
}

/// The archive's top-level `gradle.properties`, read as `key=value`.
fn gradle_properties(
    zip: &mut zip::ZipArchive<fs::File>,
    names: &[String],
    prefix: &str,
) -> BTreeMap<String, String> {
    let short = format!("{prefix}gradle.properties");
    let found = names.iter().find(|n| **n == short).or_else(|| {
        names
            .iter()
            .find(|n| n.ends_with("/gradle.properties") && n.matches('/').count() <= 2)
    });
    let Some(name) = found else {
        return BTreeMap::new();
    };
    let mut text = String::new();
    let read = match zip.by_name(name) {
        Ok(mut entry) => entry.read_to_string(&mut text).is_ok(),
        Err(_) => false,
    };
    if !read {
        return BTreeMap::new();
    }
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.starts_with('#') || l.starts_with('!') {
                return None;
            }
            let (k, v) = l.split_once('=')?;
            Some((k.trim().to_owned(), v.trim().to_owned()))
        })
        .collect()
}

fn root_project_name(
    zip: &mut zip::ZipArchive<fs::File>,
    names: &[String],
    prefix: &str,
) -> Option<String> {
    let name = names
        .iter()
        .find(|n| {
            **n == format!("{prefix}settings.gradle")
                || **n == format!("{prefix}settings.gradle.kts")
        })
        .or_else(|| {
            names
                .iter()
                .find(|n| n.ends_with("/settings.gradle") || n.ends_with("/settings.gradle.kts"))
        })?;
    let mut text = String::new();
    zip.by_name(name).ok()?.read_to_string(&mut text).ok()?;
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("rootProject.name") else {
            continue;
        };
        let rest = rest.trim_start().trim_start_matches('=').trim();
        let value = rest.trim_matches(['\'', '"']).trim();
        if !value.is_empty() {
            return Some(value.to_owned());
        }
    }
    None
}

/// `mod_id` (or `archives_base_name`...) when the archive has no metadata.
fn gradle_id(props: &BTreeMap<String, String>) -> Option<String> {
    [
        "mod_id",
        "modid",
        "modId",
        "mod_id_base",
        "archives_base_name",
    ]
    .iter()
    .filter_map(|k| props.get(*k))
    .map(|v| v.trim().to_owned())
    .find(|v| !v.is_empty() && !v.contains("${") && !v.eq_ignore_ascii_case("null"))
}

fn gradle_version(props: &BTreeMap<String, String>) -> String {
    ["mod_version", "version", "modVersion"]
        .iter()
        .find_map(|k| props.get(*k))
        .map(|v| v.trim_matches(['\'', '"', ' ']).to_owned())
        .filter(|v| !v.is_empty() && !v.starts_with("${"))
        .unwrap_or_default()
}

/// The id a content mod can have: lower case, letters, digits and `_`.
pub fn mod_id(raw: &str) -> String {
    let mut id: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    while id.contains("__") {
        id = id.replace("__", "_");
    }
    let id = id.trim_matches('_').to_owned();
    if id.is_empty() {
        return "unnamed_mod".to_owned();
    }
    if id.starts_with(|c: char| c.is_ascii_alphabetic()) {
        id
    } else {
        format!("mod_{id}")
    }
}

/// A file name without its version and Minecraft suffix, as a display name.
fn title_from_file(file: &str) -> String {
    let stem = file
        .trim_end_matches(".jar")
        .trim_end_matches(".zip")
        .trim_end_matches("-main")
        .trim_end_matches("-master")
        .trim_end_matches("-dev");
    let mut words: Vec<&str> = Vec::new();
    for word in stem.split(['-', '_', ' ']).filter(|w| !w.is_empty()) {
        let v = word.to_ascii_lowercase();
        let looks_like_version = v.chars().all(|c| c.is_ascii_digit() || c == '.')
            || v.starts_with("mc2")
            || v.starts_with("1.")
            || v == "multi"
            || v == "fabric"
            || v == "forge"
            || v == "neoforge";
        if looks_like_version {
            continue;
        }
        words.push(word);
    }
    let title = words.join(" ").trim().to_owned();
    if title.is_empty() {
        stem.to_owned()
    } else {
        title
    }
}

/// Reads `path` and says what it is, without touching any Java file.
pub fn inspect(path: &Path) -> Result<Inspection, String> {
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("{}: {e}", path.display()))?;
    let names: Vec<String> = zip.file_names().map(str::to_owned).collect();
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let prefix = wrapper_prefix(&names);
    let res = resources(&names, &prefix);
    let meta = metadata(&names, &prefix);
    let loader = meta
        .as_ref()
        .map(|(target, _)| match target.as_str() {
            "fabric.mod.json" => "fabric",
            "quilt.mod.json" => "quilt",
            t if t.contains("neoforge") => "neoforge",
            _ => "forge",
        })
        .unwrap_or("");
    let mut id = String::new();
    let mut title = String::new();
    let mut version = String::new();
    let mut license = String::new();
    let mut depends = Vec::new();
    if let Some((target, source)) = &meta {
        let mut text = String::new();
        let read = match zip.by_name(source) {
            Ok(mut entry) => entry.read_to_string(&mut text).is_ok(),
            Err(_) => false,
        };
        if read {
            let info = if target == "fabric.mod.json" || target == "quilt.mod.json" {
                jar::fabric(&text)
            } else {
                jar::mods_toml(&text)
            };
            if let Ok(info) = info {
                id = info.mod_id;
                title = info.name;
                version = info.version;
                license = info.license;
                depends = info.depends;
            }
        }
    }
    let props = gradle_properties(&mut zip, &names, &prefix);
    // `${mod_id}` and friends are filled by the build; they say nothing here.
    let placeholder = |s: &str| s.is_empty() || s.contains("${") || s.eq_ignore_ascii_case("null");
    if placeholder(&id) {
        id.clear();
    }
    if placeholder(&license) {
        license.clear();
    }
    if placeholder(&version) {
        version.clear();
    }
    if placeholder(&title) {
        title.clear();
    }
    if id.is_empty() {
        id = gradle_id(&props)
            .or_else(|| root_project_name(&mut zip, &names, &prefix))
            .unwrap_or_else(|| title_from_file(&file_name));
    }
    if version.is_empty() {
        version = gradle_version(&props);
    }
    if title.is_empty() {
        title = title_from_file(&file_name);
    }
    if version.is_empty() {
        version = "0.0.0".to_owned();
    }
    let has_classes = names.iter().any(|n| n.ends_with(".class"));
    let has_java = names.iter().any(|n| n.ends_with(".java"));
    let has_pack_meta = names
        .iter()
        .any(|n| n == "pack.mcmeta" || n.ends_with("/resources/pack.mcmeta"));
    let kind = if meta.is_some() && has_classes {
        Kind::ModJar
    } else if has_pack_meta && meta.is_none() && !has_java {
        Kind::ResourcePack
    } else if has_java || !res.is_empty() {
        Kind::SourceArchive
    } else {
        Kind::Other
    };
    let note = match kind {
        Kind::ModJar => String::new(),
        Kind::SourceArchive if res.is_empty() => {
            "No assets/ or data/: everything it does is Java code.".to_owned()
        }
        Kind::SourceArchive => {
            "Source archive: resources carried over; code reported, never read.".to_owned()
        }
        Kind::ResourcePack => {
            "Applies over vanilla as a resource pack; no .hmod needed.".to_owned()
        }
        Kind::Other => "Nothing Horizon can carry (no assets, data or pack metadata).".to_owned(),
    };
    let _ = depends;
    Ok(Inspection {
        file: file_name,
        path: path.to_path_buf(),
        kind,
        mod_id: mod_id(&id),
        title,
        version,
        license,
        loader: loader.to_owned(),
        resources: res.len(),
        note,
    })
}

/// Every archive in `dir`, `.zip` and `.jar` only.
pub fn scan(dir: &Path) -> Result<Vec<Inspection>, String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && matches!(
                    p.extension()
                        .and_then(|e| e.to_str())
                        .map(str::to_ascii_lowercase)
                        .as_deref(),
                    Some("zip" | "jar")
                )
        })
        .collect();
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        match inspect(&path) {
            Ok(i) => out.push(i),
            Err(e) => out.push(Inspection {
                file: path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                path: path.clone(),
                kind: Kind::Other,
                mod_id: mod_id(&title_from_file(
                    &path.file_name().unwrap_or_default().to_string_lossy(),
                )),
                title: title_from_file(&path.file_name().unwrap_or_default().to_string_lossy()),
                version: "0.0.0".into(),
                license: String::new(),
                loader: String::new(),
                resources: 0,
                note: e,
            }),
        }
    }
    Ok(out)
}

/// The synthetic jar a source archive is checked as: its resources plus a
/// metadata file (the archive's own when it has one that parses, else one
/// written from `gradle.properties`).
fn source_jar(inspection: &Inspection, work: &Path) -> Result<PathBuf, String> {
    let file = fs::File::open(&inspection.path)
        .map_err(|e| format!("{}: {e}", inspection.path.display()))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| format!("{}: {e}", inspection.path.display()))?;
    let names: Vec<String> = zip.file_names().map(str::to_owned).collect();
    let prefix = wrapper_prefix(&names);
    let res = resources(&names, &prefix);
    let dir = work.join("jars");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("{}.jar", inspection.mod_id));
    let out = fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut writer = zip::ZipWriter::new(out);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (target, source) in &res {
        let mut bytes = Vec::new();
        let Ok(mut entry) = zip.by_name(source) else {
            continue;
        };
        if entry.size() > 1 << 26 || entry.read_to_end(&mut bytes).is_err() {
            continue;
        }
        writer
            .start_file(target.as_str(), options)
            .map_err(|e| e.to_string())?;
        writer.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    let manifest = format!(
        "modLoader = \"javafml\"\nloaderVersion = \"[1,)\"\nlicense = {}\n[[mods]]\nmodId = {}\nversion = {}\ndisplayName = {}\n",
        toml::Value::String(if inspection.license.is_empty() {
            "not stated".into()
        } else {
            inspection.license.clone()
        }),
        toml::Value::String(inspection.mod_id.clone()),
        toml::Value::String(inspection.version.clone()),
        toml::Value::String(inspection.title.clone()),
    );
    writer
        .start_file("META-INF/neoforge.mods.toml", options)
        .map_err(|e| e.to_string())?;
    writer
        .write_all(manifest.as_bytes())
        .map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(path)
}

/// Packs a folder into `dest` (`.hmod`), keeping the folder's paths.
fn pack(folder: &Path, dest: &Path) -> Result<u64, String> {
    fs::create_dir_all(dest.parent().unwrap_or(dest)).map_err(|e| e.to_string())?;
    let file = fs::File::create(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    fn walk(
        dir: &Path,
        root: &Path,
        writer: &mut zip::ZipWriter<fs::File>,
        options: zip::write::SimpleFileOptions,
    ) -> Result<(), String> {
        let mut entries: Vec<PathBuf> = fs::read_dir(dir)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, root, writer, options)?;
            } else {
                let name = path
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                writer
                    .start_file(name, options)
                    .map_err(|e| e.to_string())?;
                writer.write_all(&bytes).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    walk(folder, folder, &mut writer, options)?;
    writer.finish().map_err(|e| e.to_string())?;
    fs::metadata(dest)
        .map(|m| m.len())
        .map_err(|e| e.to_string())
}

/// A Horizon feature that already does what this mod does, when one is known.
fn built_in(id: &str, title: &str) -> String {
    let haystack = format!("{} {}", id, title).to_ascii_lowercase();
    let table = [
        (
            "dynamiclight",
            "Dynamic lights are built in: Extras → Dynamic Lights.",
        ),
        ("gammautils", "Fullbright is built in: Extras → Fullbright."),
        (
            "gamma_utils",
            "Fullbright is built in: Extras → Fullbright.",
        ),
        (
            "mousetweaks",
            "Mouse Tweaks controls are built in: Extras → inventory.",
        ),
        (
            "betterblockentities",
            "Horizon draws block entities in its own renderer; nothing to port.",
        ),
        (
            "keystrokes",
            "An official native mod already does this (Mods page).",
        ),
        (
            "fpscounter",
            "An official native mod already does this (Mods page).",
        ),
        (
            "minimap",
            "An official native mod already does this (Mods page).",
        ),
        (
            "zoom",
            "An official native mod already does this (Mods page).",
        ),
    ];
    for (needle, text) in table {
        if haystack.contains(needle) {
            return text.to_owned();
        }
    }
    String::new()
}

/// Carries one file into `<out>/<id>.hmod`, as far as a content mod goes.
pub fn port_entry(inspection: &Inspection, work: &Path, out: &Path) -> Entry {
    let mut entry = Entry {
        file: inspection.file.clone(),
        mod_id: inspection.mod_id.clone(),
        title: inspection.title.clone(),
        version: inspection.version.clone(),
        license: inspection.license.clone(),
        loader: inspection.loader.clone(),
        kind: inspection.kind,
        verdict: String::new(),
        summary: inspection.note.clone(),
        ports: Vec::new(),
        skipped: Vec::new(),
        built_in: built_in(&inspection.mod_id, &inspection.title),
        hmod: String::new(),
    };
    if inspection.kind == Kind::ResourcePack {
        entry.verdict = "Resource pack".to_owned();
        entry.summary = "Use it in the game's Resource Packs screen.".to_owned();
        return entry;
    }
    if inspection.kind == Kind::Other {
        entry.verdict = "Skipped".to_owned();
        return entry;
    }
    let jar_path = match inspection.kind {
        Kind::ModJar => inspection.path.clone(),
        _ => match source_jar(inspection, work) {
            Ok(p) => p,
            Err(e) => {
                entry.verdict = "Skipped".to_owned();
                entry.summary = e;
                return entry;
            }
        },
    };
    let destination = work.join("mods").join(&inspection.mod_id);
    match check::check(&jar_path, &destination) {
        Ok(c) => {
            fill(&mut entry, &c);
            if entry.verdict == "Doesn't run" && entry.ports.is_empty() && entry.built_in.is_empty()
            {
                entry.skipped.push(
                    "Everything it does is Java code: needs a native mod (docs/native-mods.md)"
                        .to_owned(),
                );
            }
            let carry = destination.join("carry");
            note_unread_worldgen(&mut entry, &carry);
            let dest = out.join(format!("{}.hmod", inspection.mod_id));
            let portable = matches!(entry.verdict.as_str(), "Works" | "Partly");
            if portable && carry.join("horizon-mod.toml").is_file() {
                match pack(&carry, &dest) {
                    Ok(_) => {
                        entry.hmod = dest
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    }
                    Err(e) => entry.summary = format!("{} (packaging failed: {e})", entry.summary),
                }
            } else if dest.is_file() {
                // A previous run carried it: it no longer runs, so it must not stay installed.
                let _ = fs::remove_file(&dest);
            }
        }
        Err(e) => {
            entry.verdict = "Skipped".to_owned();
            entry.summary = e;
        }
    }
    entry
}

/// Worldgen registries a content mod may add to are structures, structure
/// sets, template pools, processor lists and configured/placed features
/// (`horizon_client::mod_content`); biomes, noise and density functions are
/// not read from mods, so a carried `worldgen/biome/` file does nothing.
const UNREAD_WORLDGEN: [&str; 4] = ["biome", "noise", "density_function", "noise_settings"];

/// Says how much of a mod's worldgen a content mod cannot use, so the report
/// does not claim a mod's biomes ported.
fn note_unread_worldgen(entry: &mut Entry, carry: &Path) {
    let data = carry.join("data");
    let Ok(namespaces) = fs::read_dir(&data) else {
        return;
    };
    let mut count = 0usize;
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for namespace in namespaces.flatten() {
        let worldgen = namespace.path().join("worldgen");
        let Ok(dirs) = fs::read_dir(&worldgen) else {
            continue;
        };
        for dir in dirs.flatten() {
            let kind = dir.file_name().to_string_lossy().into_owned();
            let Some(kind) = UNREAD_WORLDGEN.iter().find(|k| **k == kind).copied() else {
                continue;
            };
            let files = fs::read_dir(dir.path())
                .map(|d| d.flatten().count())
                .unwrap_or(0);
            count += files;
            *kinds.entry(kind).or_default() += files;
        }
    }
    if count == 0 {
        return;
    }
    let what: Vec<String> = kinds.iter().map(|(k, n)| format!("{n} {k}")).collect();
    entry.skipped.push(format!(
        "{} ({}) carried but not read: Horizon does not take terrain, biomes or noise from mods",
        count,
        what.join(", ")
    ));
}

fn fill(entry: &mut Entry, check: &Check) {
    entry.verdict = check.verdict.label().to_owned();
    entry.summary = check.summary.clone();
    entry.ports = check.ports.clone();
    entry.skipped = check.skipped.clone();
    if !check.name.is_empty() {
        entry.title = check.name.clone();
    }
    if !check.version.is_empty() && !check.version.starts_with("${") {
        entry.version = check.version.clone();
    }
    if !check.license.is_empty() {
        entry.license = check.license.clone();
    }
}

/// Scans `dir` and ports every archive into `<out>`; returns the report.
pub fn port_all(
    dir: &Path,
    work: &Path,
    out: &Path,
    only: &[String],
) -> Result<Vec<Entry>, String> {
    let inspections = scan(dir)?;
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut entries = Vec::new();
    for inspection in &inspections {
        if !only.is_empty() && !only.iter().any(|id| id == &inspection.mod_id) {
            continue;
        }
        entries.push(port_entry(inspection, work, out));
    }
    Ok(entries)
}

/// The report as Markdown.
pub fn markdown(entries: &[Entry]) -> String {
    let mut text = String::from("# HexaGon report\n\n");
    let count = |label: &str| entries.iter().filter(|e| e.verdict == label).count();
    text.push_str(&format!(
        "{} files: {} work, {} partly, {} don't run, {} resource packs, {} skipped.\n\n",
        entries.len(),
        count("Works"),
        count("Partly"),
        count("Doesn't run"),
        count("Resource pack"),
        count("Skipped"),
    ));
    text.push_str("| file | id | kind | verdict | ports | not ported | hmod |\n|---|---|---|---|---|---|---|\n");
    for e in entries {
        let ports = if e.ports.is_empty() {
            "—".to_owned()
        } else {
            e.ports.join("; ")
        };
        let skipped = if e.skipped.is_empty() {
            if e.built_in.is_empty() {
                "—".to_owned()
            } else {
                e.built_in.clone()
            }
        } else {
            e.skipped.join("; ")
        };
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            e.file,
            e.mod_id,
            e.kind.label(),
            e.verdict,
            ports.replace('|', "/"),
            skipped.replace('|', "/"),
            if e.hmod.is_empty() { "—" } else { &e.hmod },
        ));
    }
    text
}

/// Writes `hexagon-report.json` and `.md` into `out`.
pub fn write_report(entries: &[Entry], out: &Path) -> Result<(), String> {
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let json = serde_json::to_vec_pretty(entries).map_err(|e| e.to_string())?;
    fs::write(out.join("hexagon-report.json"), json).map_err(|e| e.to_string())?;
    fs::write(out.join("hexagon-report.md"), markdown(entries)).map_err(|e| e.to_string())?;
    Ok(())
}

/// Copies every `.hmod` in `out` into `<home>/horizon-mods/`, and turns each
/// on in `horizon-mods.txt` beside `options.txt`: a mod the file does not
/// list stays off, so a copied `.hmod` alone does nothing.
pub fn install(out: &Path, home: &Path) -> Result<Vec<String>, String> {
    let mods = home.join("horizon-mods");
    fs::create_dir_all(&mods).map_err(|e| format!("{}: {e}", mods.display()))?;
    let mut copied = Vec::new();
    let mut ids = Vec::new();
    for entry in fs::read_dir(out)
        .map_err(|e| format!("{}: {e}", out.display()))?
        .flatten()
    {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("hmod") {
            continue;
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        fs::copy(&path, mods.join(&name)).map_err(|e| format!("{name}: {e}"))?;
        ids.push(name.trim_end_matches(".hmod").to_owned());
        copied.push(name);
    }
    enable(home, &ids)?;
    copied.sort();
    Ok(copied)
}

/// Adds `mod.<id>:true` to `<home>/horizon-mods.txt`, keeping the lines the
/// player already has. The game reads that file (`mod_list::FILE`).
fn enable(home: &Path, ids: &[String]) -> Result<(), String> {
    let file = home.join("horizon-mods.txt");
    let text = fs::read_to_string(&file).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut changed = false;
    for id in ids {
        let key = format!("mod.{id}");
        let at = lines
            .iter()
            .position(|l| l.trim_start().starts_with(&format!("{key}:")));
        match at {
            Some(i) if lines[i].trim() == format!("{key}:true") => {}
            Some(i) => {
                lines[i] = format!("{key}:true");
                changed = true;
            }
            None => {
                lines.push(format!("{key}:true"));
                changed = true;
            }
        }
    }
    if changed {
        let mut text = lines.join("\n");
        text.push('\n');
        fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    Ok(())
}

/// One Java file, translated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaFile {
    pub path: String,
    pub written: String,
    pub classes: Vec<String>,
    pub stats: rust_gen::Stats,
    pub issues: Vec<String>,
    /// The Java mixins it declares (`@Inject` and friends), for the report.
    pub mixins: Vec<String>,
}

/// What `java` did to one archive or folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaReport {
    pub source: String,
    pub files: Vec<JavaFile>,
    pub classes: usize,
    pub methods: usize,
    pub statements: usize,
    pub issues: usize,
    pub mixins: usize,
}

/// Every `.java` file of an archive (a jar's sources, or a source archive's
/// `src/main/java`), as (path inside it, text).
pub fn java_sources(path: &Path) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    if path.is_dir() {
        fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, String)>) {
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            paths.sort();
            for p in paths {
                if p.is_dir() {
                    walk(&p, root, out);
                } else if p.extension().and_then(|e| e.to_str()) == Some("java")
                    && let Ok(text) = fs::read_to_string(&p)
                {
                    let name = p
                        .strip_prefix(root)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.push((name, text));
                }
            }
        }
        walk(path, path, &mut out);
        return Ok(out);
    }
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("{}: {e}", path.display()))?;
    for i in 0..zip.len() {
        let Ok(mut entry) = zip.by_index(i) else {
            continue;
        };
        let name = entry.name().to_owned();
        if !name.ends_with(".java") {
            continue;
        }
        let mut text = String::new();
        if entry.read_to_string(&mut text).is_err() {
            continue;
        }
        out.push((name, text));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// The mixin annotations a method carries, as `@Name(args)`.
fn mixin_annotations(m: &java::Method) -> Vec<String> {
    m.annotations
        .iter()
        .filter(|a| {
            let short = a.name.rsplit('.').next().unwrap_or(&a.name);
            matches!(
                short,
                "Inject"
                    | "Redirect"
                    | "ModifyVariable"
                    | "ModifyArg"
                    | "ModifyArgs"
                    | "Overwrite"
                    | "Accessor"
                    | "Invoker"
                    | "Shadow"
            )
        })
        .map(|a| format!("@{}({})", a.name, a.args))
        .collect()
}

/// Reads every Java file under `path` and writes the Rust it can, as
/// `src/<package or file>.rs` in `out`, beside `hexagon-java-report.md`.
pub fn translate_java(path: &Path, out: &Path) -> Result<JavaReport, String> {
    let sources = java_sources(path)?;
    if sources.is_empty() {
        return Err(format!("{}: no .java files", path.display()));
    }
    // Parse everything first: a class may use another one from any file.
    let mut units = Vec::new();
    let mut known = std::collections::BTreeSet::new();
    for (name, text) in &sources {
        let toks = java::lex(text)?;
        let mut parser = java::Parser::new(toks);
        let unit = parser.unit();
        fn add_known(ty: &java::TypeDecl, known: &mut std::collections::BTreeSet<String>) {
            // Interfaces have no Rust body here, so they are not types yet.
            if ty.kind != "interface" && ty.kind != "@interface" {
                known.insert(ty.name.clone());
            }
            for nested in &ty.nested {
                add_known(nested, known);
            }
        }
        for ty in &unit.types {
            add_known(ty, &mut known);
        }
        units.push((name.clone(), text.clone(), unit, parser.problems));
    }
    let src = out.join("src");
    fs::create_dir_all(&src).map_err(|e| format!("{}: {e}", src.display()))?;
    let mut report = JavaReport {
        source: path.to_string_lossy().into_owned(),
        files: Vec::new(),
        classes: 0,
        methods: 0,
        statements: 0,
        issues: 0,
        mixins: 0,
    };
    let mut modules = Vec::new();
    let returns = rust_gen::collect_returns(units.iter().map(|u| &u.2));
    let arities = rust_gen::collect_arities(units.iter().map(|u| &u.2));
    for (name, _text, unit, problems) in units {
        let translated = rust_gen::translate(&unit, &name, &known, &returns, &arities);
        let mut issues = translated.issues;
        issues.extend(problems);
        let mut mixins = Vec::new();
        for ty in &unit.types {
            for m in &ty.methods {
                for a in mixin_annotations(m) {
                    mixins.push(format!("{}::{} {a}", ty.name, m.name));
                }
            }
        }
        let module = module_name(&name, &unit);
        let file = src.join(format!("{module}.rs"));
        fs::write(&file, &translated.text).map_err(|e| format!("{}: {e}", file.display()))?;
        report.classes += translated.stats.classes;
        report.methods += translated.stats.methods;
        report.statements += translated.stats.statements;
        report.issues += issues.len();
        report.mixins += mixins.len();
        modules.push(module.clone());
        report.files.push(JavaFile {
            path: name,
            written: format!("src/{module}.rs"),
            classes: unit.types.iter().map(|t| t.name.clone()).collect(),
            stats: translated.stats,
            issues,
            mixins,
        });
    }
    // `lib.rs` holds the modules and the mixin scaffold.
    let mut lib = String::from(
        "//! Translated from Java by HexaGon.\n\
         //! Java never runs here: this crate is what the translator could carry.\n\
         //! Read `hexagon-java-report.md` for what it could not.\n\n",
    );
    for module in &modules {
        lib.push_str(&format!("pub mod {module};\n"));
    }
    lib.push('\n');
    for module in &modules {
        lib.push_str(&format!("pub use {module}::*;\n"));
    }
    lib.push('\n');
    let mixin_count = report.mixins;
    if mixin_count > 0 {
        lib.push_str(&format!(
            "// {mixin_count} Java mixin(s) became TODOs: each needs an [[injection]] in\n\
             // horizon-mixin.toml naming a Horizon function (docs/native-mods.md).\n"
        ));
    }
    fs::write(src.join("lib.rs"), lib).map_err(|e| e.to_string())?;
    let crate_name = rust_gen::snake(
        &out.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "hexagon_port".into()),
    );
    fs::write(
        out.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{crate_name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n\n[dependencies]\nserde_json = \"1\"\n"
        ),
    )
    .map_err(|e| e.to_string())?;
    fs::write(out.join("hexagon-java-report.md"), java_markdown(&report))
        .map_err(|e| e.to_string())?;
    // A mixin manifest needs one target per injection; the report names each.
    if mixin_count > 0 {
        fs::write(
            out.join("horizon-mixin.toml"),
            format!(
                "# Written by HexaGon. Fill each target with the Horizon function it hooks\n\
                 # (docs/native-mods.md), then move the Rust beside it.\n\
                 [mod]\nid = \"{}\"\nversion = \"0.1.0\"\npriority = 100\n\n\
                 [compatibility]\n# horizon_revision = \"<the Nightly's commit>\"\n\n\
                 # {} injection(s): see hexagon-java-report.md\n",
                out.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "hexagon_port".into()),
                mixin_count
            ),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(report)
}

fn module_name(path: &str, unit: &java::CompilationUnit) -> String {
    if let Some(ty) = unit.types.first()
        && !ty.name.is_empty()
    {
        return rust_gen::snake(&ty.name);
    }
    let stem = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".java");
    rust_gen::snake(stem)
}

fn java_markdown(r: &JavaReport) -> String {
    let mut text = format!(
        "# HexaGon: Java to Rust\n\n{}\n\n{} files, {} classes, {} methods, {} statements; {} mixins, {} TODOs.\n\n",
        r.source,
        r.files.len(),
        r.classes,
        r.methods,
        r.statements,
        r.mixins,
        r.issues
    );
    text.push_str("| file | Rust | classes | methods | TODOs |\n|---|---|---|---|---|\n");
    for f in &r.files {
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            f.path,
            f.written,
            f.classes.join(", "),
            f.stats.methods,
            f.issues.len()
        ));
    }
    text.push_str("\n## What needs a decision\n\n");
    for f in &r.files {
        if f.issues.is_empty() && f.mixins.is_empty() {
            continue;
        }
        text.push_str(&format!("### {}\n\n", f.path));
        for m in &f.mixins {
            text.push_str(&format!("- mixin `{m}`\n"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for issue in &f.issues {
            if seen.insert(issue.clone()) {
                text.push_str(&format!("- {issue}\n"));
            }
        }
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn archive(dir: &Path, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join(name);
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        for (n, bytes) in files {
            zip.start_file(*n, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hexagon-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_source_archive_is_read_through_its_wrapper() {
        let dir = temp("source");
        let path = archive(
            &dir,
            "Ruby-main.zip",
            &[
                ("Ruby-main/gradle.properties", b"mod_id=ruby\nmod_version=1.2.3\n"),
                ("Ruby-main/src/main/resources/assets/ruby/blockstates/ruby_block.json", br#"{"variants": {"": {"model": "ruby:block/ruby_block"}}}"#),
                ("Ruby-main/src/main/resources/data/ruby/recipe/ruby.json", br#"{"type": "minecraft:crafting_shapeless", "ingredients": ["minecraft:red_dye"], "result": {"id": "ruby:ruby"}}"#),
                ("Ruby-main/src/main/java/ruby/Ruby.java", b"class Ruby {}"),
            ],
        );
        let i = inspect(&path).unwrap();
        assert_eq!(i.kind, Kind::SourceArchive);
        assert_eq!((i.mod_id.as_str(), i.version.as_str()), ("ruby", "1.2.3"));
        assert_eq!(i.resources, 2);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ids_are_what_a_content_mod_can_use() {
        assert_eq!(mod_id("The Sift"), "the_sift");
        assert_eq!(mod_id("3d-Skin-Layers"), "mod_3d_skin_layers");
        assert_eq!(mod_id("fabric-api"), "fabric_api");
    }

    #[test]
    fn a_resource_pack_is_not_a_mod() {
        let dir = temp("pack");
        let path = archive(
            &dir,
            "Pack-main.zip",
            &[
                ("Pack-main/src/main/resources/pack.mcmeta", b"{}"),
                (
                    "Pack-main/src/main/resources/assets/minecraft/lang/en_us.json",
                    b"{}",
                ),
            ],
        );
        let i = inspect(&path).unwrap();
        assert_eq!(i.kind, Kind::ResourcePack);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn installing_turns_the_mods_on_and_keeps_other_lines() {
        let home = temp("install");
        let out = home.join("out");
        fs::create_dir_all(&out).unwrap();
        archive(
            &out,
            "ruby.hmod",
            &[("horizon-mod.toml", b"id = \"ruby\"\n")],
        );
        fs::write(
            home.join("horizon-mods.txt"),
            "mod.other:false\nkey.ruby.zoom:key.keyboard.c\n",
        )
        .unwrap();
        let copied = install(&out, &home).unwrap();
        assert_eq!(copied, ["ruby.hmod"]);
        let text = fs::read_to_string(home.join("horizon-mods.txt")).unwrap();
        assert!(text.contains("mod.other:false"));
        assert!(text.contains("key.ruby.zoom:key.keyboard.c"));
        assert!(text.contains("mod.ruby:true"));
        // A second install changes nothing.
        install(&out, &home).unwrap();
        assert_eq!(
            text,
            fs::read_to_string(home.join("horizon-mods.txt")).unwrap()
        );
        fs::remove_dir_all(&home).unwrap();
    }
}
