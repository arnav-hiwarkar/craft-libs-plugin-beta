use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use craft_plugin::{MANIFEST_FILE, Manifest, ManifestError, Permission, RunMode, native_target};

use crate::package::{self, PackageError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const STATE_FILE: &str = "state.json";
const RUNS_DIR: &str = ".runs";
const UNCLEAN_EXIT: &str = "the app closed unexpectedly while it ran unsandboxed";
const DATA_DIR: &str = ".data";
const UNPACKING_DIR: &str = ".unpacking";

pub(crate) fn install_root(data_folder: &Path) -> Option<&Path> {
    data_folder.file_name().is_some_and(|name| name == DATA_DIR).then(|| data_folder.parent()).flatten()
}

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("this plugin has no build for {0}")]
    NoBuildForThisMachine(String),
    #[error("plugin `{0}` is not installed")]
    NotInstalled(String),
    #[error(transparent)]
    Package(#[from] PackageError),
    #[error("plugin `{0}` has no build for this machine, so it cannot run unsandboxed")]
    NoNativeBuild(String),
    #[error("the plugin's entry file {0} is missing")]
    MissingEntry(PathBuf),
    #[error(
        "plugin `{0}` version {1} is already approved with different files; give this build a new version number or remove the installed one first"
    )]
    VersionReused(String, String),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("it has not been approved yet")]
    NotApproved,
    #[error("it is turned off")]
    Disabled,
    #[error("it was turned off because {0}")]
    Quarantined(String),
    #[error("it asks for more permissions than you approved")]
    Wider,
    #[error("its files changed since you approved them ({0})")]
    Changed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bundle {
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub mode: RunMode,
}

impl Bundle {
    pub fn open(dir: &Path) -> Result<Bundle, BundleError> {
        let file = dir.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&file).map_err(|e| BundleError::Io(file, e))?;
        let manifest = Manifest::parse(&text)?;
        let mode = manifest.mode(false);
        Ok(Bundle { dir: dir.to_path_buf(), manifest, mode })
    }

    pub fn manifest_of(source: &Path) -> Result<Manifest, BundleError> {
        if source.is_file() {
            return Ok(Manifest::parse(&package::manifest_text(source)?)?);
        }
        Bundle::open(source).map(|bundle| bundle.manifest)
    }

    pub fn with_mode(mut self, mode: RunMode) -> Bundle {
        self.mode = mode;
        self
    }

    pub fn digest(&self) -> Result<String, BundleError> {
        let mut files = Vec::new();
        list_files(&self.dir, "", &mut files).map_err(|e| BundleError::Io(self.dir.clone(), e))?;
        files.sort();
        let mut hasher = Sha256::new();
        for relative in files {
            let path = self.dir.join(&relative);
            let mut file = File::open(&path).map_err(|e| BundleError::Io(path.clone(), e))?;
            let length = file.metadata().map_err(|e| BundleError::Io(path.clone(), e))?.len();
            hasher.update(relative.as_bytes());
            hasher.update([0]);
            hasher.update(length.to_le_bytes());
            std::io::copy(&mut file, &mut hasher).map_err(|e| BundleError::Io(path, e))?;
        }
        Ok(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
    }

    pub fn entry_path(&self) -> Result<PathBuf, BundleError> {
        self.manifest.entry_file(self.mode).map(|file| self.dir.join(file)).ok_or_else(|| BundleError::NoBuildForThisMachine(native_target()))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Consent {
    pub enabled: bool,
    #[serde(default)]
    pub granted: BTreeSet<Permission>,
    #[serde(default)]
    pub unsandboxed: Option<String>,
    pub approved: Option<Approved>,
    pub quarantined: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Approved {
    pub version: String,
    pub digest: String,
}

pub struct Registry {
    root: PathBuf,
    consents: BTreeMap<String, Consent>,
    run: Option<(PathBuf, File)>,
}

impl Registry {
    pub fn open(root: &Path) -> Registry {
        let consents = std::fs::read_to_string(root.join(STATE_FILE)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
        Registry { root: root.to_path_buf(), consents, run: None }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn bundles(&self) -> Vec<Result<Bundle, BundleError>> {
        let Ok(entries) = std::fs::read_dir(&self.root) else { return Vec::new() };
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(|entry| Some(entry.ok()?.path()))
            .filter(|path| path.file_name().is_some_and(|name| !name.to_string_lossy().starts_with('.')))
            .filter(|path| path.join(MANIFEST_FILE).is_file())
            .collect();
        dirs.sort();
        dirs.iter().map(|dir| Bundle::open(dir)).collect()
    }

    pub fn consent(&self, id: &str) -> Option<&Consent> {
        self.consents.get(id)
    }

    pub fn mode(&self, manifest: &Manifest) -> RunMode {
        let chosen = self.consents.get(&manifest.id).is_some_and(|consent| consent.unsandboxed.as_ref() == Some(&manifest.version));
        manifest.mode(chosen)
    }

    pub fn grants(&self, bundle: &Bundle) -> Result<BTreeSet<Permission>, Refusal> {
        let manifest = &bundle.manifest;
        let consent = self.consents.get(&manifest.id).ok_or(Refusal::NotApproved)?;
        if let Some(reason) = &consent.quarantined {
            return Err(Refusal::Quarantined(reason.clone()));
        }
        if !consent.enabled {
            return Err(Refusal::Disabled);
        }
        if !manifest.permissions.is_subset(&consent.granted) {
            return Err(Refusal::Wider);
        }
        if self.mode(manifest) == RunMode::Unsandboxed && consent.unsandboxed.as_ref() != Some(&manifest.version) {
            return Err(Refusal::NotApproved);
        }
        let approved = consent.approved.as_ref().ok_or_else(|| Refusal::Changed("approved before files were recorded".into()))?;
        let found = bundle.digest().map_err(|e| Refusal::Changed(e.to_string()))?;
        match approved.digest == found {
            true => Ok(manifest.permissions.clone()),
            false => Err(Refusal::Changed(format!("approved {} {:.12}, found {} {found:.12}", approved.version, approved.digest, manifest.version))),
        }
    }

    pub fn running(&self, bundle: &Bundle) -> Bundle {
        bundle.clone().with_mode(self.mode(&bundle.manifest))
    }

    pub fn data_dir(&self, id: &str) -> PathBuf {
        self.root.join(DATA_DIR).join(id)
    }

    pub fn install(&mut self, source: &Path) -> Result<Bundle, BundleError> {
        let io = |path: &Path| {
            let path = path.to_path_buf();
            move |e| BundleError::Io(path, e)
        };
        std::fs::create_dir_all(&self.root).map_err(io(&self.root))?;
        let unpacking = self.root.join(UNPACKING_DIR);
        if unpacking.exists() {
            std::fs::remove_dir_all(&unpacking).map_err(io(&unpacking))?;
        }
        let from_file = source.is_file();
        let unpacked = if from_file {
            package::unpack(source, &unpacking).map_err(BundleError::from).and_then(|()| Bundle::open(&unpacking))
        } else {
            Bundle::open(source)
        };
        let incoming = match unpacked.and_then(|bundle| {
            let entry = bundle.entry_path()?;
            if !entry.is_file() {
                return Err(BundleError::MissingEntry(entry));
            }
            let approved = self.consents.get(&bundle.manifest.id).and_then(|consent| consent.approved.as_ref());
            match approved {
                Some(approved) if approved.version == bundle.manifest.version && approved.digest != bundle.digest()? => {
                    Err(BundleError::VersionReused(bundle.manifest.id.clone(), bundle.manifest.version.clone()))
                }
                _ => Ok(bundle),
            }
        }) {
            Ok(bundle) => bundle,
            Err(error) => {
                if from_file {
                    let _ = std::fs::remove_dir_all(&unpacking);
                }
                return Err(error);
            }
        };
        let target = self.root.join(&incoming.manifest.id);
        let staging = self.root.join(format!(".installing-{}", incoming.manifest.id));
        let retired = self.root.join(format!(".replaced-{}", incoming.manifest.id));
        for leftover in [&staging, &retired] {
            if leftover.exists() {
                std::fs::remove_dir_all(leftover).map_err(io(leftover))?;
            }
        }
        if from_file {
            std::fs::rename(&unpacking, &staging).map_err(io(&staging))?;
        } else {
            copy_dir(source, &staging).map_err(io(&staging))?;
        }
        let replaces = target.exists();
        if replaces {
            std::fs::rename(&target, &retired).map_err(io(&target))?;
        }
        if let Err(error) = std::fs::rename(&staging, &target) {
            if replaces {
                std::fs::rename(&retired, &target).map_err(io(&target))?;
            }
            return Err(BundleError::Io(target, error));
        }
        if replaces {
            std::fs::remove_dir_all(&retired).map_err(io(&retired))?;
        }
        Bundle::open(&target)
    }

    pub fn uninstall(&mut self, id: &str) -> Result<(), BundleError> {
        let target = self.installed_dir(id)?;
        let mut consents = self.consents.clone();
        consents.remove(id);
        self.commit(consents)?;
        std::fs::remove_dir_all(&target).map_err(|e| BundleError::Io(target, e))
    }

    pub fn approve(&mut self, bundle: &Bundle) -> Result<(), BundleError> {
        let manifest = &bundle.manifest;
        let chosen = self.consents.get(&manifest.id).and_then(|consent| consent.unsandboxed.clone()).filter(|version| version == &manifest.version);
        let unsandboxed = if manifest.has_wasm() { chosen } else { Some(manifest.version.clone()) };
        let approved = Some(Approved { version: manifest.version.clone(), digest: bundle.digest()? });
        let consent = Consent { enabled: true, granted: manifest.permissions.clone(), unsandboxed, approved, quarantined: None };
        let mut consents = self.consents.clone();
        consents.insert(manifest.id.clone(), consent);
        self.commit(consents)
    }

    pub fn set_unsandboxed(&mut self, manifest: &Manifest, unsandboxed: bool) -> Result<(), BundleError> {
        if unsandboxed && !manifest.can_choose_unsandboxed() {
            return Err(BundleError::NoNativeBuild(manifest.id.clone()));
        }
        if !manifest.has_wasm() {
            return Ok(());
        }
        let mut consents = self.consents.clone();
        let consent = consents.entry(manifest.id.clone()).or_default();
        consent.unsandboxed = unsandboxed.then(|| manifest.version.clone());
        self.commit(consents)
    }

    pub fn disable(&mut self, id: &str) -> Result<(), BundleError> {
        self.installed_dir(id)?;
        let mut consents = self.consents.clone();
        consents.entry(id.to_string()).or_default().enabled = false;
        self.commit(consents)
    }

    pub fn recover(&mut self) -> Result<Vec<String>, BundleError> {
        let Ok(entries) = std::fs::read_dir(self.root.join(RUNS_DIR)) else { return Ok(Vec::new()) };
        let mut consents = self.consents.clone();
        let mut quarantined = Vec::new();
        for path in entries.filter_map(|entry| Some(entry.ok()?.path())) {
            let Ok(file) = File::open(&path) else { continue };
            if file.try_lock().is_err() {
                continue;
            }
            let ids: Vec<String> = serde_json::from_reader(&file).unwrap_or_default();
            for id in ids {
                if let Some(consent) = consents.get_mut(&id) {
                    consent.quarantined = Some(UNCLEAN_EXIT.into());
                    quarantined.push(id);
                }
            }
            drop(file);
            std::fs::remove_file(&path).map_err(|e| BundleError::Io(path, e))?;
        }
        self.commit(consents)?;
        Ok(quarantined)
    }

    pub fn mark_running(&mut self, unsandboxed: &BTreeSet<String>) -> Result<(), BundleError> {
        static RUNS: AtomicU64 = AtomicU64::new(0);
        if self.run.is_none() {
            let dir = self.root.join(RUNS_DIR);
            let path = dir.join(format!("{}-{}.json", std::process::id(), RUNS.fetch_add(1, Ordering::Relaxed)));
            let file = std::fs::create_dir_all(&dir).and_then(|()| File::create(&path)).map_err(|e| BundleError::Io(path.clone(), e))?;
            file.try_lock().map_err(|e| BundleError::Io(path.clone(), e.into()))?;
            self.run = Some((path, file));
        }
        let Some((path, file)) = &mut self.run else { return Ok(()) };
        let text = serde_json::to_vec(unsandboxed).unwrap_or_default();
        file.set_len(0).and_then(|()| file.rewind()).and_then(|()| file.write_all(&text)).map_err(|e| BundleError::Io(path.clone(), e))
    }

    pub fn end_run(&mut self) -> Result<(), BundleError> {
        let Some((path, file)) = self.run.take() else { return Ok(()) };
        drop(file);
        std::fs::remove_file(&path).map_err(|e| BundleError::Io(path, e))
    }

    fn installed_dir(&self, id: &str) -> Result<PathBuf, BundleError> {
        self.bundles()
            .into_iter()
            .flatten()
            .find(|bundle| bundle.manifest.id == id)
            .map(|bundle| bundle.dir)
            .ok_or_else(|| BundleError::NotInstalled(id.to_string()))
    }

    fn commit(&mut self, consents: BTreeMap<String, Consent>) -> Result<(), BundleError> {
        let file = self.root.join(STATE_FILE);
        let staging = self.root.join(format!("{STATE_FILE}.new"));
        let text = serde_json::to_string_pretty(&consents).unwrap_or_default();
        std::fs::create_dir_all(&self.root)
            .and_then(|()| std::fs::write(&staging, text))
            .and_then(|()| std::fs::rename(&staging, &file))
            .map_err(|e| BundleError::Io(file, e))?;
        self.consents = consents;
        Ok(())
    }
}

fn list_files(dir: &Path, prefix: &str, files: &mut Vec<String>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let relative = format!("{prefix}{}", entry.file_name().to_string_lossy());
        if kind.is_dir() {
            list_files(&entry.path(), &format!("{relative}/"), files)?;
        } else if kind.is_file() {
            files.push(relative);
        }
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
