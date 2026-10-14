//! Installing, removing and exporting SWORD modules, and reading the CrossWire repository's list.
//!
//! This is the part of the crate that writes, and it has no user interface and no network: a
//! *package* is a `.zip` (or a folder) laid out like a library, with `mods.d/<name>.conf` and the
//! module's data under `modules/…`, which is also what CrossWire serves as `rawzip/<Name>.zip`.
//! Nothing is ever overwritten without being asked for, and a path in an archive that would land
//! outside the folder it is installed into refuses the whole package.

use crate::{Conf, Kind, Module};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

/// The most a package may unpack to. Real modules are tens of megabytes; this is to stop a bomb.
const MAX_UNPACKED: u64 = 8 << 30;
/// The biggest `.conf` that is read.
const MAX_CONF: u64 = 1 << 20;

/// Why a package could not be installed, a module removed, or a list read.
#[derive(Debug)]
pub enum PackageError {
    Io(io::Error),
    /// Not a readable archive, or not a list of modules.
    Archive(String),
    /// A path in the package would leave the folder it is installed into.
    Unsafe(String),
    /// The package has no `mods.d/*.conf`.
    NoModules,
    /// A module's `DataPath` has no files in the package.
    MissingData(String),
    /// These modules (or files) are already there and were not to be replaced.
    Exists(Vec<String>),
    TooLarge,
}

impl std::fmt::Display for PackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageError::Io(e) => write!(f, "{e}"),
            PackageError::Archive(e) => write!(f, "not a usable module package: {e}"),
            PackageError::Unsafe(p) => write!(f, "refused: \"{p}\" would be written outside the modules folder"),
            PackageError::NoModules => write!(f, "no module in it: a package needs a mods.d folder with a .conf file"),
            PackageError::MissingData(m) => write!(f, "the data of {m} is not in the package"),
            PackageError::Exists(names) => write!(f, "already installed: {}", names.join(", ")),
            PackageError::TooLarge => write!(f, "the package unpacks to more than {} GB", MAX_UNPACKED >> 30),
        }
    }
}

impl std::error::Error for PackageError {}

impl From<io::Error> for PackageError {
    fn from(e: io::Error) -> Self {
        PackageError::Io(e)
    }
}

impl From<zip::result::ZipError> for PackageError {
    fn from(e: zip::result::ZipError) -> Self {
        match e {
            zip::result::ZipError::Io(e) => PackageError::Io(e),
            e => PackageError::Archive(e.to_string()),
        }
    }
}

/// A relative path from an archive's name for it, or `None` if it is absolute or climbs out with
/// `..`. Both kinds of slash separate, as archives made on Windows have backslashes.
fn safe_rel(name: &str) -> Option<PathBuf> {
    if name.contains('\0') || name.starts_with(['/', '\\']) {
        return None;
    }
    let mut out = PathBuf::new();
    for part in name.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return None,
            // A drive (`C:`) is no place inside a folder.
            p if p.contains(':') => return None,
            p => out.push(p),
        }
    }
    debug_assert!(out.components().all(|c| matches!(c, Component::Normal(_))));
    Some(out)
}

/// The module's conf files in a library, with their paths, in name order.
fn conf_files(root: &Path) -> Vec<(PathBuf, Conf)> {
    let Ok(dir) = fs::read_dir(root.join("mods.d")) else { return Vec::new() };
    let mut paths: Vec<PathBuf> = dir.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "conf")).collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|p| {
            let conf = Conf::parse(&String::from_utf8_lossy(&fs::read(&p).ok()?))?;
            Some((p, conf))
        })
        .collect()
}

/// Where a module's data lives: a folder, or (dictionaries) the files that start with a name.
enum Data {
    Dir(PathBuf),
    Stem { dir: PathBuf, prefix: String },
}

impl Data {
    /// The files, found by looking at the disk.
    fn files(&self) -> Vec<PathBuf> {
        match self {
            Data::Dir(dir) => {
                let mut files = Vec::new();
                walk(dir, &mut files);
                files
            }
            Data::Stem { dir, prefix } => {
                let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
                let mut files: Vec<PathBuf> = entries
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.is_file() && p.file_name().is_some_and(|n| owned_by_stem(&n.to_string_lossy(), prefix)))
                    .collect();
                files.sort();
                files
            }
        }
    }
}

fn owned_by_stem(file: &str, prefix: &str) -> bool {
    file.strip_prefix(prefix).is_some_and(|rest| rest.starts_with('.'))
}

/// Every file under `dir` (symbolic links are not followed), sorted.
fn walk(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut paths: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        let Ok(meta) = fs::symlink_metadata(&path) else { continue };
        if meta.is_dir() {
            walk(&path, into);
        } else if meta.is_file() {
            into.push(path);
        }
    }
}

/// The data's place relative to the library, from the conf's `DataPath`: always under `modules`.
fn data_rel(conf: &Conf) -> Result<Option<PathBuf>, PackageError> {
    let Some(raw) = conf.get("DataPath").filter(|p| !p.trim().is_empty()) else { return Ok(None) };
    let rel = safe_rel(raw.trim()).ok_or_else(|| PackageError::Unsafe(raw.to_string()))?;
    if !rel.starts_with("modules") || rel.components().count() < 3 {
        return Err(PackageError::Unsafe(raw.to_string()));
    }
    Ok(Some(rel))
}

fn data_of(root: &Path, conf: &Conf) -> Result<Option<Data>, PackageError> {
    let Some(rel) = data_rel(conf)? else { return Ok(None) };
    let full = root.join(&rel);
    if full.is_dir() || conf.get("DataPath").is_some_and(|p| p.trim().ends_with('/')) {
        return Ok(Some(Data::Dir(full)));
    }
    let prefix = rel.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(Some(Data::Stem { dir: full.parent().map(Path::to_path_buf).unwrap_or_default(), prefix }))
}

/// A package's file `file` belongs to the data at `data` (a folder, or the start of file names).
fn owns(data: &Path, file: &Path) -> bool {
    file.starts_with(data)
        || (file.parent() == data.parent()
            && matches!((file.file_name(), data.file_name()), (Some(f), Some(d)) if owned_by_stem(&f.to_string_lossy(), &d.to_string_lossy())))
}

/// What the app can do with a module, as a reason when it cannot (the same checks that opening it
/// makes): locked, or a driver, compression, versification or markup that is not read yet.
pub fn readability(module: &Module) -> Result<(), String> {
    let opened = match module.kind() {
        Kind::Bible => module.bible().map(|_| ()),
        Kind::Commentary => module.commentary().map(|_| ()),
        Kind::Lexicon => module.lexicon().map(|_| ()),
        Kind::Devotional => return Err(format!("{} is a daily devotional, which this app does not read", module.name())),
        Kind::Other if module.is_locked() => return Err(format!("{} is locked and needs a key", module.name())),
        Kind::Other => {
            return Err(match module.conf.get("ModDrv") {
                Some(driver) => format!("{} uses the {driver} driver", module.name()),
                None => format!("{} has no driver", module.name()),
            });
        }
    };
    opened.map_err(|e| match e {
        crate::Error::Unsupported(what) => what,
        other => other.to_string(),
    })
}

/// The kind of module, as CrossWire groups them.
fn category_of(conf: &Conf, kind: Kind) -> String {
    match conf.get("Category").map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => c.to_string(),
        None => match kind {
            Kind::Bible => "Biblical Texts",
            Kind::Commentary => "Commentaries",
            Kind::Lexicon => "Lexicons / Dictionaries",
            Kind::Devotional => "Daily Devotional",
            Kind::Other => "Other",
        }
        .to_string(),
    }
}

/// Whether the license lets the module be downloaded and shared freely (public domain, GPL,
/// Creative Commons and the like). It is a marker for the list, not legal advice: the license
/// itself is shown too.
pub fn is_free_license(license: &str) -> bool {
    let l = license.to_lowercase();
    if l.trim().is_empty() {
        return false;
    }
    ["public domain", "gpl", "gnu", "creative commons", "cc by", "cc0", "cc-by", "freely distributable", "free non-commercial", "free for any", "free to", "open license"]
        .iter()
        .any(|word| l.contains(word))
}

fn number(conf: &Conf, key: &str) -> u64 {
    conf.get(key).and_then(|v| v.trim().parse().ok()).unwrap_or(0)
}

/// One module installed in a library.
#[derive(Clone, Debug)]
pub struct Installed {
    pub name: String,
    pub description: String,
    pub language: String,
    pub version: String,
    pub license: String,
    pub kind: Kind,
    pub category: String,
    /// Bytes on disk: the conf and the data.
    pub size: u64,
    /// The library it is in.
    pub root: PathBuf,
    /// Why the app cannot read it, if it cannot.
    pub unreadable: Option<String>,
}

impl Installed {
    pub fn is_readable(&self) -> bool {
        self.unreadable.is_none()
    }
}

/// Every module in every library, in each library's order, a module that two libraries have listed
/// under both (the one an earlier library has is the one that is read).
pub fn list_installed(roots: &[PathBuf]) -> Vec<Installed> {
    let mut found = Vec::new();
    for root in roots {
        for (path, conf) in conf_files(root) {
            let module = Module { conf, root: root.clone() };
            let conf = &module.conf;
            let mut unreadable = readability(&module).err();
            let mut size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            match data_of(root, conf) {
                Ok(Some(data)) => {
                    let files = data.files();
                    size += files.iter().filter_map(|f| fs::metadata(f).ok()).map(|m| m.len()).sum::<u64>();
                    if files.is_empty() && unreadable.is_none() {
                        unreadable = Some(format!("the data of {} is missing", module.name()));
                    }
                }
                Ok(None) => {}
                Err(e) => unreadable = unreadable.or(Some(e.to_string())),
            }
            found.push(Installed {
                name: module.name().to_string(),
                description: module.description().to_string(),
                language: conf.get("Lang").unwrap_or("").to_string(),
                version: conf.get("Version").unwrap_or("").to_string(),
                license: conf.get("DistributionLicense").unwrap_or("").to_string(),
                kind: module.kind(),
                category: category_of(conf, module.kind()),
                size,
                root: root.clone(),
                unreadable,
            });
        }
    }
    found
}

/// The conf file (path) and conf of the module called `name` in a library.
fn find_conf(root: &Path, name: &str) -> Option<(PathBuf, Conf)> {
    conf_files(root).into_iter().find(|(_, c)| c.name.eq_ignore_ascii_case(name))
}

/// Remove module `name` from the library at `root`: its conf and its data. Folders that this
/// leaves empty go too. Data that another module in the library uses as well is left. Returns the
/// number of bytes freed.
pub fn remove_module(root: &Path, name: &str) -> Result<u64, PackageError> {
    let Some((conf_path, conf)) = find_conf(root, name) else {
        return Err(PackageError::Io(io::Error::new(io::ErrorKind::NotFound, format!("{name} is not in {}", root.display()))));
    };
    let mut freed = fs::metadata(&conf_path).map(|m| m.len()).unwrap_or(0);
    if let (Some(data), Some(rel)) = (data_of(root, &conf)?, data_rel(&conf)?) {
        let shared = conf_files(root).into_iter().filter(|(p, _)| *p != conf_path).any(|(_, other)| {
            data_rel(&other).ok().flatten().is_some_and(|o| o.starts_with(&rel) || rel.starts_with(&o))
        });
        if !shared {
            for file in data.files() {
                freed += fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
                fs::remove_file(&file)?;
            }
            if let Data::Dir(dir) = &data {
                // Anything left that is not a plain file (an empty folder, a link) goes with it.
                if dir.exists() {
                    fs::remove_dir_all(dir)?;
                }
            }
            prune_empty(root, &root.join(&rel));
        }
    }
    fs::remove_file(&conf_path)?;
    Ok(freed)
}

/// Take away `from` and then each folder above it, up to (not including) `root/modules`, for as
/// long as it is empty.
fn prune_empty(root: &Path, from: &Path) {
    let stop = root.join("modules");
    let mut dir = from.to_path_buf();
    // A stem names no folder: its folder is the one to look at. So does a folder already gone.
    if !dir.is_dir() {
        dir.pop();
    }
    // Removing a folder that is not empty fails, which is where this ends.
    while dir.starts_with(&stop) && dir != stop && fs::remove_dir(&dir).is_ok() {
        dir.pop();
    }
}

/// Write module `name` of the library at `root` to a `.zip` another SWORD program can install.
pub fn export_module(root: &Path, name: &str, dest: &Path) -> Result<(), PackageError> {
    use zip::write::SimpleFileOptions;
    let Some((conf_path, conf)) = find_conf(root, name) else {
        return Err(PackageError::Io(io::Error::new(io::ErrorKind::NotFound, format!("{name} is not in {}", root.display()))));
    };
    let mut files = vec![conf_path];
    if let Some(data) = data_of(root, &conf)? {
        files.extend(data.files());
    }
    let part = dest.with_extension("zip.part");
    let written = (|| -> Result<(), PackageError> {
        let mut zip = zip::ZipWriter::new(File::create(&part)?);
        for file in &files {
            let rel = file.strip_prefix(root).map_err(|_| PackageError::Unsafe(file.display().to_string()))?;
            let entry = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            let size = fs::metadata(file)?.len();
            let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(size >= u32::MAX as u64);
            zip.start_file(entry, options)?;
            io::copy(&mut File::open(file)?, &mut zip)?;
        }
        zip.finish()?.flush()?;
        Ok(())
    })();
    match written {
        Ok(()) => fs::rename(&part, dest).map_err(Into::into),
        Err(e) => {
            let _ = fs::remove_file(&part);
            Err(e)
        }
    }
}

#[derive(Clone, Debug)]
enum Source {
    Zip(PathBuf),
    Dir(PathBuf),
}

/// A module that a package holds.
#[derive(Clone, Debug)]
pub struct PackageModule {
    pub name: String,
    pub description: String,
    pub version: String,
    /// `mods.d/<file>.conf`
    conf_file: PathBuf,
}

#[derive(Clone, Debug)]
struct PackFile {
    rel: PathBuf,
    size: u64,
    /// The place in a zip's list of entries.
    index: usize,
}

/// A `.zip` or a folder of modules, looked through and found sound, and ready to be installed.
#[derive(Clone, Debug)]
pub struct Package {
    source: Source,
    pub modules: Vec<PackageModule>,
    files: Vec<PackFile>,
}

impl Package {
    /// Look through a zip. Nothing is written.
    pub fn from_zip(path: &Path) -> Result<Package, PackageError> {
        let mut zip = zip::ZipArchive::new(File::open(path)?)?;
        let mut files = Vec::new();
        let mut confs = Vec::new();
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index)?;
            let rel = safe_rel(entry.name()).ok_or_else(|| PackageError::Unsafe(entry.name().to_string()))?;
            if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
                return Err(PackageError::Unsafe(format!("{} (a link)", entry.name())));
            }
            if entry.is_dir() || !wanted(&rel) {
                continue;
            }
            if is_conf(&rel) {
                let mut bytes = Vec::new();
                entry.by_ref().take(MAX_CONF).read_to_end(&mut bytes)?;
                confs.push((rel.clone(), bytes));
            }
            files.push(PackFile { rel, size: entry.size(), index });
        }
        Package::build(Source::Zip(path.to_path_buf()), files, confs)
    }

    /// Look through a folder laid out like a library (it has a `mods.d`). Nothing is written.
    pub fn from_dir(path: &Path) -> Result<Package, PackageError> {
        if !path.join("mods.d").is_dir() {
            return Err(PackageError::NoModules);
        }
        let mut all = Vec::new();
        walk(&path.join("mods.d"), &mut all);
        walk(&path.join("modules"), &mut all);
        let mut files = Vec::new();
        let mut confs = Vec::new();
        for (index, file) in all.into_iter().enumerate() {
            let Ok(rel) = file.strip_prefix(path) else { continue };
            let rel = rel.to_path_buf();
            if !wanted(&rel) {
                continue;
            }
            let size = fs::metadata(&file)?.len();
            if is_conf(&rel) {
                let mut bytes = Vec::new();
                File::open(&file)?.take(MAX_CONF).read_to_end(&mut bytes)?;
                confs.push((rel.clone(), bytes));
            }
            files.push(PackFile { rel, size, index });
        }
        Package::build(Source::Dir(path.to_path_buf()), files, confs)
    }

    fn build(source: Source, files: Vec<PackFile>, confs: Vec<(PathBuf, Vec<u8>)>) -> Result<Package, PackageError> {
        let mut modules = Vec::new();
        for (rel, bytes) in confs {
            let Some(conf) = Conf::parse(&String::from_utf8_lossy(&bytes)) else { continue };
            if let Some(data) = data_rel(&conf)?
                && !files.iter().any(|f| owns(&data, &f.rel))
            {
                return Err(PackageError::MissingData(conf.name));
            }
            modules.push(PackageModule {
                description: conf.get("Description").unwrap_or(&conf.name).to_string(),
                version: conf.get("Version").unwrap_or("").to_string(),
                name: conf.name,
                conf_file: rel,
            });
        }
        if modules.is_empty() {
            return Err(PackageError::NoModules);
        }
        let total: u64 = files.iter().map(|f| f.size).sum();
        if total > MAX_UNPACKED {
            return Err(PackageError::TooLarge);
        }
        Ok(Package { source, modules, files })
    }

    /// The modules of the package that a library already has (by name, or by conf file), and so
    /// would be replaced.
    pub fn conflicts(&self, target: &Path) -> Vec<String> {
        let present = conf_files(target);
        self.modules
            .iter()
            .filter(|m| {
                target.join(&m.conf_file).exists() || present.iter().any(|(_, c)| c.name.eq_ignore_ascii_case(&m.name))
            })
            .map(|m| m.name.clone())
            .collect()
    }

    /// Copy the package into the library at `target`. A module that is already there is only
    /// replaced if `replace` says so (the old one goes first); a file is never written over. If
    /// anything fails, what this wrote is taken away again. Returns the names installed.
    pub fn install(&self, target: &Path, replace: bool) -> Result<Vec<String>, PackageError> {
        let conflicts = self.conflicts(target);
        if !conflicts.is_empty() && !replace {
            return Err(PackageError::Exists(conflicts));
        }
        fs::create_dir_all(target)?;
        for name in &conflicts {
            if find_conf(target, name).is_some() {
                remove_module(target, name)?;
            } else if let Some(m) = self.modules.iter().find(|m| &m.name == name) {
                // Same conf file but another name inside: the file is the one to make way.
                fs::remove_file(target.join(&m.conf_file))?;
            }
        }
        let mut written: Vec<PathBuf> = Vec::new();
        match self.write_all(target, &mut written) {
            Ok(()) => Ok(self.modules.iter().map(|m| m.name.clone()).collect()),
            Err(e) => {
                let mut parents = BTreeSet::new();
                for file in &written {
                    let _ = fs::remove_file(file);
                    parents.extend(file.parent().map(Path::to_path_buf));
                }
                for dir in parents {
                    prune_empty(target, &dir);
                }
                Err(e)
            }
        }
    }

    fn write_all(&self, target: &Path, written: &mut Vec<PathBuf>) -> Result<(), PackageError> {
        let mut zip = match &self.source {
            Source::Zip(path) => Some(zip::ZipArchive::new(File::open(path)?)?),
            Source::Dir(_) => None,
        };
        // The confs go last: a module is not listed until its data is all there.
        let mut order: Vec<&PackFile> = self.files.iter().collect();
        order.sort_by_key(|f| is_conf(&f.rel));
        for file in order {
            let dest = target.join(&file.rel);
            if let Some(dir) = dest.parent() {
                fs::create_dir_all(dir)?;
            }
            let mut out = match OpenOptions::new().write(true).create_new(true).open(&dest) {
                Ok(out) => out,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    return Err(PackageError::Exists(vec![file.rel.display().to_string()]));
                }
                Err(e) => return Err(e.into()),
            };
            written.push(dest);
            match (&mut zip, &self.source) {
                (Some(zip), _) => {
                    let mut entry = zip.by_index(file.index)?;
                    io::copy(&mut entry.by_ref().take(file.size), &mut out)?;
                }
                (None, Source::Dir(dir)) => {
                    io::copy(&mut File::open(dir.join(&file.rel))?, &mut out)?;
                }
                (None, Source::Zip(_)) => unreachable!("a zip source has its archive open"),
            }
        }
        Ok(())
    }
}

/// Only a library's own parts are taken from a package; whatever else it holds is left.
fn wanted(rel: &Path) -> bool {
    rel.starts_with("mods.d") || rel.starts_with("modules")
}

fn is_conf(rel: &Path) -> bool {
    rel.components().count() == 2 && rel.starts_with("mods.d") && rel.extension().is_some_and(|e| e == "conf")
}

/// One module in the CrossWire repository's list.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteModule {
    pub name: String,
    pub description: String,
    pub language: String,
    pub version: String,
    /// Bytes it takes once installed (`InstallSize`), 0 when the conf does not say.
    pub size: u64,
    pub license: String,
    pub category: String,
    pub locked: bool,
    /// Free to download and share (see [`is_free_license`]).
    pub free: bool,
    /// Why the app could not read it once installed, if it could not.
    pub unreadable: Option<String>,
}

impl RemoteModule {
    pub fn from_conf(conf: &Conf) -> RemoteModule {
        let module = Module { conf: conf.clone(), root: PathBuf::new() };
        let license = conf.get("DistributionLicense").unwrap_or("").trim().to_string();
        RemoteModule {
            name: conf.name.clone(),
            description: module.description().to_string(),
            language: conf.get("Lang").unwrap_or("").to_string(),
            version: conf.get("Version").unwrap_or("").to_string(),
            size: number(conf, "InstallSize"),
            category: category_of(conf, module.kind()),
            locked: module.is_locked(),
            free: is_free_license(&license),
            unreadable: readability(&module).err(),
            license,
        }
    }
}

/// Read the repository's list: a gzipped tarball of every module's `.conf`. A conf that is not
/// one is skipped.
pub fn parse_listing(tar_gz: &[u8]) -> Result<Vec<RemoteModule>, PackageError> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(tar_gz));
    let mut modules = Vec::new();
    for entry in archive.entries().map_err(|e| PackageError::Archive(e.to_string()))? {
        let mut entry = entry.map_err(|e| PackageError::Archive(e.to_string()))?;
        let is_conf = entry.header().entry_type().is_file() && entry.path().is_ok_and(|p| p.extension().is_some_and(|e| e == "conf"));
        if !is_conf {
            continue;
        }
        let mut bytes = Vec::new();
        entry.by_ref().take(MAX_CONF).read_to_end(&mut bytes).map_err(|e| PackageError::Archive(e.to_string()))?;
        if let Some(conf) = Conf::parse(&String::from_utf8_lossy(&bytes)) {
            modules.push(RemoteModule::from_conf(&conf));
        }
    }
    if modules.is_empty() {
        return Err(PackageError::Archive("no modules in the list".into()));
    }
    modules.sort_by_key(|m| m.name.to_lowercase());
    Ok(modules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn put(root: &Path, rel: &str, bytes: &[u8]) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    const ASV: &str = "[ASV]\nDescription=American Standard Version\nModDrv=zText\nSourceType=OSIS\nCompressType=ZIP\nBlockType=BOOK\nDataPath=./modules/texts/ztext/asv/\nLang=en\nVersion=1.3\nDistributionLicense=Public Domain\n";

    const STRONGS: &str = "[StrongsGreek]\nDescription=Strong's Greek\nModDrv=zLD\nCompressType=ZIP\nDataPath=./modules/lexdict/zld/strongsgreek/strongsgreek\nLang=grc\n";

    /// A library with a readable Bible, a locked one, one in an unknown versification, a dictionary
    /// kept as a stem, and a driver that is not read.
    fn library(root: &Path) {
        put(root, "mods.d/asv.conf", ASV.as_bytes());
        put(root, "modules/texts/ztext/asv/nt.bzv", b"vvvv");
        put(root, "modules/texts/ztext/asv/nt.bzs", b"ssss");
        put(root, "modules/texts/ztext/asv/nt.bzz", b"zzzzzzzz");
        put(root, "mods.d/locked.conf", b"[Locked]\nModDrv=zText\nSourceType=OSIS\nCompressType=ZIP\nCipherKey=\nDataPath=./modules/texts/ztext/locked/\n");
        put(root, "modules/texts/ztext/locked/nt.bzv", b"1");
        put(root, "mods.d/synodal.conf", b"[Synodal]\nModDrv=zText\nSourceType=OSIS\nCompressType=ZIP\nVersification=Synodal\nDataPath=./modules/texts/ztext/synodal/\n");
        put(root, "modules/texts/ztext/synodal/nt.bzv", b"1");
        put(root, "mods.d/strongsgreek.conf", STRONGS.as_bytes());
        put(root, "modules/lexdict/zld/strongsgreek/strongsgreek.idx", b"ii");
        put(root, "modules/lexdict/zld/strongsgreek/strongsgreek.zdt", b"dddd");
        put(root, "mods.d/book.conf", b"[Creed]\nModDrv=RawGenBook\nDataPath=./modules/genbook/rawgenbook/creed/creed\n");
        put(root, "modules/genbook/rawgenbook/creed/creed.bdt", b"b");
    }

    fn zip_of(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, bytes) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn the_repository_list_is_read_from_its_tarball_of_confs() {
        let mut tar = tar::Builder::new(Vec::new());
        let mut add = |name: &str, text: &str| {
            let mut header = tar::Header::new_gnu();
            header.set_size(text.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, name, text.as_bytes()).unwrap();
        };
        add("mods.d/kjv.conf", "[KJV]\nDescription=King James\nModDrv=zText\nSourceType=OSIS\nCompressType=ZIP\nLang=en\nVersion=2.3\nInstallSize=4321\nDistributionLicense=Public Domain\nDataPath=./modules/texts/ztext/kjv/\n");
        add("mods.d/esv.conf", "[ESV]\nDescription=English Standard\nModDrv=zText\nCipherKey=\nCategory=Biblical Texts\nDistributionLicense=Copyrighted; Permission to distribute granted to CrossWire\nLang=en\n");
        add("mods.d/notes.txt", "not a conf");
        add("mods.d/broken.conf", "no header here");
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&tar.into_inner().unwrap()).unwrap();
        let list = parse_listing(&gz.finish().unwrap()).unwrap();
        assert_eq!(list.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["ESV", "KJV"]);
        let kjv = &list[1];
        assert_eq!((kjv.description.as_str(), kjv.language.as_str(), kjv.version.as_str(), kjv.size), ("King James", "en", "2.3", 4321));
        assert_eq!((kjv.free, kjv.locked, kjv.category.as_str()), (true, false, "Biblical Texts"));
        assert_eq!(kjv.unreadable, None);
        let esv = &list[0];
        assert!(esv.locked && !esv.free, "locked, and copyrighted");
        assert!(esv.unreadable.as_deref().unwrap().contains("locked"));
        assert!(parse_listing(b"not gzip").is_err());
    }

    #[test]
    fn licenses_that_allow_sharing_are_marked_free() {
        assert!(is_free_license("Public Domain"));
        assert!(is_free_license("GNU General Public License"));
        assert!(is_free_license("Creative Commons: By-NC-SA 4.0"));
        assert!(!is_free_license("Copyrighted; Permission to distribute granted to CrossWire"));
        assert!(!is_free_license(""));
    }

    #[test]
    fn modules_are_listed_with_what_the_app_can_and_cannot_read() {
        let dir = tempfile::tempdir().unwrap();
        library(dir.path());
        let list = list_installed(&[dir.path().to_path_buf()]);
        let find = |n: &str| list.iter().find(|m| m.name == n).unwrap();
        let asv = find("ASV");
        assert_eq!((asv.kind, asv.language.as_str(), asv.version.as_str(), asv.license.as_str()), (Kind::Bible, "en", "1.3", "Public Domain"));
        assert_eq!(asv.size, ASV.len() as u64 + 4 + 4 + 8, "the conf and its three files");
        assert!(asv.is_readable());
        assert!(find("StrongsGreek").is_readable());
        assert_eq!(find("StrongsGreek").size, STRONGS.len() as u64 + 2 + 4, "a stem owns its own files");
        assert!(find("Locked").unreadable.as_deref().unwrap().contains("locked"));
        assert!(find("Synodal").unreadable.as_deref().unwrap().contains("Synodal"));
        assert!(find("Creed").unreadable.as_deref().unwrap().contains("RawGenBook"));
    }

    #[test]
    fn a_module_with_no_data_files_is_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "mods.d/asv.conf", ASV.as_bytes());
        let list = list_installed(&[dir.path().to_path_buf()]);
        assert!(list[0].unreadable.as_deref().unwrap().contains("missing"));
    }

    #[test]
    fn every_searched_folder_is_listed() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        library(a.path());
        put(b.path(), "mods.d/asv.conf", ASV.as_bytes());
        put(b.path(), "modules/texts/ztext/asv/nt.bzv", b"v");
        let list = list_installed(&[a.path().into(), b.path().into()]);
        assert_eq!(list.iter().filter(|m| m.name == "ASV").map(|m| m.root.clone()).collect::<Vec<_>>(), [a.path(), b.path()]);
    }

    #[test]
    fn removing_a_module_takes_its_conf_and_data_and_leaves_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        library(dir.path());
        let freed = remove_module(dir.path(), "asv").unwrap();
        assert!(freed > 0);
        assert!(!dir.path().join("mods.d/asv.conf").exists());
        assert!(!dir.path().join("modules/texts/ztext/asv").exists());
        assert!(dir.path().join("modules/texts/ztext/locked/nt.bzv").exists(), "a neighbour is left");
        remove_module(dir.path(), "StrongsGreek").unwrap();
        assert!(!dir.path().join("modules/lexdict").exists(), "folders left empty go too");
        assert!(dir.path().join("modules/texts/ztext/synodal").exists());
        assert!(remove_module(dir.path(), "Nope").is_err());
    }

    #[test]
    fn data_another_module_shares_is_not_removed() {
        let dir = tempfile::tempdir().unwrap();
        library(dir.path());
        put(dir.path(), "mods.d/asv2.conf", ASV.replace("[ASV]", "[ASV2]").as_bytes());
        remove_module(dir.path(), "ASV").unwrap();
        assert!(dir.path().join("modules/texts/ztext/asv/nt.bzv").exists());
        assert!(dir.path().join("mods.d/asv2.conf").exists());
    }

    #[test]
    fn a_data_path_that_leaves_the_library_is_refused_and_nothing_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "mods.d/evil.conf", b"[Evil]\nModDrv=zText\nDataPath=../elsewhere/\n");
        put(dir.path(), "../elsewhere/keep.txt", b"x");
        let err = remove_module(dir.path(), "Evil").unwrap_err();
        assert!(matches!(err, PackageError::Unsafe(_)), "{err}");
        assert!(dir.path().join("mods.d/evil.conf").exists());
        assert!(dir.path().join("../elsewhere/keep.txt").exists());
        let _ = fs::remove_dir_all(dir.path().join("../elsewhere"));
    }

    #[test]
    fn an_exported_module_installs_into_another_library_unchanged() {
        let (from, to, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        library(from.path());
        for name in ["ASV", "StrongsGreek"] {
            let zip = out.path().join(format!("{name}.zip"));
            export_module(from.path(), name, &zip).unwrap();
            let package = Package::from_zip(&zip).unwrap();
            assert_eq!(package.modules.len(), 1);
            assert_eq!(package.install(to.path(), false).unwrap(), [name]);
        }
        assert_eq!(fs::read(to.path().join("modules/texts/ztext/asv/nt.bzz")).unwrap(), b"zzzzzzzz");
        assert_eq!(fs::read(to.path().join("mods.d/asv.conf")).unwrap(), ASV.as_bytes());
        assert_eq!(fs::read(to.path().join("modules/lexdict/zld/strongsgreek/strongsgreek.zdt")).unwrap(), b"dddd");
        assert!(!to.path().join("modules/texts/ztext/locked").exists(), "only the module that was exported");
        assert!(list_installed(&[to.path().into()]).iter().all(|m| m.is_readable()));
        assert!(!out.path().join("ASV.zip.part").exists());
    }

    #[test]
    fn a_folder_laid_out_like_a_library_installs_like_a_zip() {
        let (from, to) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        library(from.path());
        put(from.path(), "readme.txt", b"left out");
        let package = Package::from_dir(from.path()).unwrap();
        assert_eq!(package.modules.len(), 5);
        package.install(to.path(), false).unwrap();
        assert_eq!(fs::read(to.path().join("modules/texts/ztext/asv/nt.bzv")).unwrap(), b"vvvv");
        assert!(!to.path().join("readme.txt").exists());
        assert!(matches!(Package::from_dir(to.path().join("modules").as_path()), Err(PackageError::NoModules)));
    }

    #[test]
    fn a_path_that_climbs_out_of_the_target_refuses_the_whole_package() {
        let dir = tempfile::tempdir().unwrap();
        let (lib, zip) = (dir.path().join("lib"), dir.path().join("evil.zip"));
        for bad in ["../evil.conf", "mods.d/../../evil.conf", "/etc/evil.conf", "modules\\..\\..\\evil", "C:/evil"] {
            zip_of(&zip, &[("mods.d/a.conf", b"[A]\nModDrv=zText\n"), (bad, b"x")]);
            let err = Package::from_zip(&zip).unwrap_err();
            assert!(matches!(err, PackageError::Unsafe(_)), "{bad}: {err}");
        }
        assert!(!lib.exists() && !dir.path().join("evil.conf").exists());
    }

    #[test]
    fn a_package_without_a_conf_or_without_its_data_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("p.zip");
        zip_of(&zip, &[("modules/texts/ztext/x/nt.bzv", b"v")]);
        assert!(matches!(Package::from_zip(&zip), Err(PackageError::NoModules)));
        zip_of(&zip, &[("mods.d/x.conf", b"[X]\nModDrv=zText\nDataPath=./modules/texts/ztext/x/\n")]);
        assert!(matches!(Package::from_zip(&zip), Err(PackageError::MissingData(n)) if n == "X"));
        fs::write(&zip, b"not a zip").unwrap();
        assert!(matches!(Package::from_zip(&zip), Err(PackageError::Archive(_))));
    }

    #[test]
    fn an_installed_module_is_only_replaced_when_asked() {
        let (from, to) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        library(from.path());
        let zip = to.path().join("asv.zip");
        export_module(from.path(), "ASV", &zip).unwrap();
        let target = to.path().join("lib");
        let package = Package::from_zip(&zip).unwrap();
        package.install(&target, false).unwrap();
        fs::write(target.join("modules/texts/ztext/asv/old.txt"), b"mine").unwrap();
        assert_eq!(package.conflicts(&target), ["ASV"]);
        match package.install(&target, false) {
            Err(PackageError::Exists(names)) => assert_eq!(names, ["ASV"]),
            other => panic!("{other:?}"),
        }
        assert!(target.join("modules/texts/ztext/asv/old.txt").exists(), "nothing was touched");
        package.install(&target, true).unwrap();
        assert!(!target.join("modules/texts/ztext/asv/old.txt").exists(), "the old module went first");
        assert!(target.join("modules/texts/ztext/asv/nt.bzv").exists());
    }

    #[test]
    fn a_file_in_the_way_stops_the_install_and_what_was_written_is_taken_back() {
        let (from, to) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        library(from.path());
        let zip = to.path().join("asv.zip");
        export_module(from.path(), "ASV", &zip).unwrap();
        let target = to.path().join("lib");
        // A stray file where one of the module's files goes, with no module owning it.
        put(&target, "modules/texts/ztext/asv/nt.bzz", b"stray");
        let err = Package::from_zip(&zip).unwrap().install(&target, false).unwrap_err();
        assert!(matches!(err, PackageError::Exists(_)), "{err}");
        assert_eq!(fs::read(target.join("modules/texts/ztext/asv/nt.bzz")).unwrap(), b"stray");
        assert!(!target.join("mods.d/asv.conf").exists(), "the conf is written last, and not at all");
        assert!(!target.join("modules/texts/ztext/asv/nt.bzs").exists());
    }
}
