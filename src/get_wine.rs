use std::collections::HashSet;
use std::env::home_dir;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum RunnerType {
    Proton,
    Wine,
}

#[derive(Debug, Clone)]
pub struct Runner {
    pub path: PathBuf,
    pub runner_type: RunnerType,
}

fn expand_home(path: &str) -> PathBuf {
    if path.starts_with("~/")
        && let Some(home) = home_dir()
    {
        return Path::new(&home).join(&path[2..]);
    }
    PathBuf::from(path)
}

fn unique_paths<I>(iter: I) -> Vec<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    let mut seen = HashSet::new();
    iter.into_iter()
        .filter_map(|p| {
            let key = p.canonicalize().unwrap_or_else(|_| p.clone());
            if seen.insert(key.clone()) {
                Some(key)
            } else {
                None
            }
        })
        .collect()
}

fn scan_candidates<F>(candidates: &[&str], mut keep: F) -> Vec<PathBuf>
where
    F: FnMut(&Path) -> bool,
{
    let mut found = Vec::new();
    for c in candidates {
        let base = expand_home(c);
        if let Ok(entries) = fs::read_dir(&base) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() && keep(&p) {
                    found.push(p);
                }
            }
        }
    }

    unique_paths(found)
}

fn is_executable(p: &Path) -> bool {
    p.exists()
        && p.metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

fn detect_runner_type(path: &Path) -> Option<RunnerType> {
    let name = path.file_name()?.to_str()?.to_lowercase();

    if name.contains("proton") {
        Some(RunnerType::Proton)
    } else if name.contains("wine") {
        Some(RunnerType::Wine)
    } else {
        None
    }
}

pub fn find_proton_dirs() -> Vec<Runner> {
    let candidates = [
        "~/.steam/steam/steamapps/common",
        "/usr/share/steam/steamapps/common",
        "~/.local/share/Steam/compatibilitytools.d",
        "/usr/share/steam/compatibilitytools.d",
        "~/.local/share/lutris/runners/proton",
        "~/.config/heroic/tools/proton",
    ];

    scan_candidates(&candidates, |p| {
        p.file_name()
            .and_then(|s| s.to_str())
            .map(|name| name.to_lowercase().contains("proton"))
            .unwrap_or(false)
    })
    .into_iter()
    .filter_map(|path| detect_runner_type(&path).map(|runner_type| Runner { path, runner_type }))
    .collect()
}

pub fn find_wine_binaries() -> Vec<Runner> {
    let compat_bases = [
        "~/.local/share/Steam/compatibilitytools.d",
        "~/.steam/steam/compatibilitytools.d",
        "/usr/share/steam/compatibilitytools.d",
        "~/.local/share/lutris/runners/wine",
        "~/.local/share/lutris/runners",
        "~/.config/heroic/tools/wine",
    ];

    let wine_names = ["wine", "wine64", "wine32"];

    let mut runners = Vec::new();
    for base in &compat_bases {
        let base = expand_home(base);
        if let Ok(entries) = fs::read_dir(&base) {
            for e in entries.flatten() {
                let entry = e.path();
                if !entry.is_dir() {
                    continue;
                }
                let bin = entry.join("bin");
                if bin.is_dir() {
                    for name in &wine_names {
                        let exe = bin.join(name);
                        if is_executable(&exe) {
                            if let Some(runner_type) = detect_runner_type(&entry) {
                                runners.push(Runner {
                                    path: entry.clone(),
                                    runner_type,
                                });
                            }
                            break;
                        }
                    }
                }
            }
        }
    }

    let unique: Vec<PathBuf> = unique_paths(runners.iter().map(|r| r.path.clone()));
    runners.retain(|r| unique.contains(&r.path));
    runners
}
