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
const STATE_LOCK: &str = "state.lock";
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
    #[error("the plugin lists the LUT file {0}, but it is missing")]
    MissingLut(PathBuf),
    #[error("{0}")]
    Rejected(String),
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
        read_files(&self.dir, None).map(|(digest, _)| digest)
    }

    pub fn read(dir: &Path) -> Result<(Bundle, String), BundleError> {
        let (digest, text) = read_files(dir, Some(MANIFEST_FILE))?;
        let text = text.ok_or_else(|| BundleError::Io(dir.join(MANIFEST_FILE), std::io::ErrorKind::NotFound.into()))?;
        let text =
            String::from_utf8(text).map_err(|e| BundleError::Io(dir.join(MANIFEST_FILE), std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
        let manifest = Manifest::parse(&text)?;
        let mode = manifest.mode(false);
        Ok((Bundle { dir: dir.to_path_buf(), manifest, mode }, digest))
    }

    pub(crate) fn entry_if_unchanged(&self, digest: &str) -> Result<Vec<u8>, String> {
        let entry = self.manifest.entry_file(self.mode).ok_or_else(|| BundleError::NoBuildForThisMachine(native_target()).to_string())?;
        match read_files(&self.dir, Some(entry)).map_err(|e| e.to_string())? {
            (found, Some(bytes)) if found == digest => Ok(bytes),
            (_, Some(_)) => Err("the plugin's files changed since it was started; turn it off and on again".into()),
            (_, None) => Err(format!("{}: the entry file is missing", self.dir.join(entry).display())),
        }
    }

    fn has_file(&self, relative: &str) -> bool {
        let mut files = Vec::new();
        list_files(&self.dir, "", &mut files).is_ok() && files.iter().any(|file| file == relative)
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

pub struct ApprovedBundle {
    pub grants: BTreeSet<Permission>,
    pub digest: String,
    pub bundle: Bundle,
}

pub struct Registry {
    root: PathBuf,
    consents: BTreeMap<String, Consent>,
    run: Option<(PathBuf, File)>,
}

impl Registry {
    pub fn open(root: &Path) -> Registry {
        Registry { root: root.to_path_buf(), consents: read_state(root), run: None }
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
        self.approved(bundle).map(|approved| approved.grants)
    }

    pub fn approved(&self, bundle: &Bundle) -> Result<ApprovedBundle, Refusal> {
        let consent = self.consents.get(&bundle.manifest.id).ok_or(Refusal::NotApproved)?;
        if let Some(reason) = &consent.quarantined {
            return Err(Refusal::Quarantined(reason.clone()));
        }
        if !consent.enabled {
            return Err(Refusal::Disabled);
        }
        let (bundle, found) = Bundle::read(&bundle.dir).map_err(|e| Refusal::Changed(e.to_string()))?;
        let manifest = &bundle.manifest;
        if !manifest.permissions.is_subset(&consent.granted) {
            return Err(Refusal::Wider);
        }
        if self.mode(manifest) == RunMode::Unsandboxed && consent.unsandboxed.as_ref() != Some(&manifest.version) {
            return Err(Refusal::NotApproved);
        }
        let approved = consent.approved.as_ref().ok_or_else(|| Refusal::Changed("approved before files were recorded".into()))?;
        if approved.digest != found {
            return Err(Refusal::Changed(format!("approved {} {:.12}, found {} {found:.12}", approved.version, approved.digest, manifest.version)));
        }
        Ok(ApprovedBundle { grants: manifest.permissions.clone(), digest: found, bundle: self.running(&bundle) })
    }

    pub fn running(&self, bundle: &Bundle) -> Bundle {
        bundle.clone().with_mode(self.mode(&bundle.manifest))
    }

    pub fn data_dir(&self, id: &str) -> PathBuf {
        self.root.join(DATA_DIR).join(id)
    }

    pub fn install(&mut self, source: &Path) -> Result<Bundle, BundleError> {
        self.install_checked(source, |_| Ok(()))
    }

    pub fn install_checked(&mut self, source: &Path, check: impl FnOnce(&Bundle) -> Result<(), String>) -> Result<Bundle, BundleError> {
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
            if bundle.manifest.is_data_only() {
                if let Some(lut) = bundle.manifest.contributes.luts.iter().map(|lut| bundle.dir.join(lut)).find(|lut| !lut.is_file()) {
                    return Err(BundleError::MissingLut(lut));
                }
            } else {
                let entry = bundle.entry_path()?;
                if !bundle.manifest.entry_file(bundle.mode).is_some_and(|file| bundle.has_file(file)) {
                    return Err(BundleError::MissingEntry(entry));
                }
            }
            check(&bundle).map_err(BundleError::Rejected)?;
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
        self.update(|consents| Ok(consents.remove(id)))?;
        std::fs::remove_dir_all(&target).map_err(|e| BundleError::Io(target, e))
    }

    pub fn approve(&mut self, bundle: &Bundle) -> Result<(), BundleError> {
        let manifest = &bundle.manifest;
        let approved = Some(Approved { version: manifest.version.clone(), digest: bundle.digest()? });
        self.update(|consents| {
            let chosen = consents.get(&manifest.id).and_then(|consent| consent.unsandboxed.clone()).filter(|version| version == &manifest.version);
            let unsandboxed = if manifest.has_wasm() || manifest.is_data_only() { chosen } else { Some(manifest.version.clone()) };
            let consent = Consent { enabled: true, granted: manifest.permissions.clone(), unsandboxed, approved, quarantined: None };
            consents.insert(manifest.id.clone(), consent);
            Ok(())
        })
    }

    pub fn set_unsandboxed(&mut self, manifest: &Manifest, unsandboxed: bool) -> Result<(), BundleError> {
        if unsandboxed && !manifest.can_choose_unsandboxed() {
            return Err(BundleError::NoNativeBuild(manifest.id.clone()));
        }
        if !manifest.has_wasm() {
            return Ok(());
        }
        self.update(|consents| {
            consents.entry(manifest.id.clone()).or_default().unsandboxed = unsandboxed.then(|| manifest.version.clone());
            Ok(())
        })
    }

    pub fn disable(&mut self, id: &str) -> Result<(), BundleError> {
        self.installed_dir(id)?;
        self.update(|consents| {
            consents.entry(id.to_string()).or_default().enabled = false;
            Ok(())
        })
    }

    pub fn recover(&mut self) -> Result<Vec<String>, BundleError> {
        let Ok(entries) = std::fs::read_dir(self.root.join(RUNS_DIR)) else { return Ok(Vec::new()) };
        self.update(|consents| {
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
            Ok(quarantined)
        })
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

    fn update<T>(&mut self, change: impl FnOnce(&mut BTreeMap<String, Consent>) -> Result<T, BundleError>) -> Result<T, BundleError> {
        let file = self.root.join(STATE_FILE);
        let lock = self.root.join(STATE_LOCK);
        let held = std::fs::create_dir_all(&self.root)
            .and_then(|()| File::create(&lock))
            .and_then(|held| held.lock().map(|()| held))
            .map_err(|e| BundleError::Io(lock, e))?;
        let mut consents = read_state(&self.root);
        let changed = change(&mut consents)?;
        let staging = self.root.join(format!("{STATE_FILE}.new"));
        let text = serde_json::to_string_pretty(&consents).unwrap_or_default();
        std::fs::write(&staging, text).and_then(|()| std::fs::rename(&staging, &file)).map_err(|e| BundleError::Io(file, e))?;
        drop(held);
        self.consents = consents;
        Ok(changed)
    }
}

fn read_state(root: &Path) -> BTreeMap<String, Consent> {
    std::fs::read_to_string(root.join(STATE_FILE)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn read_files(dir: &Path, kept: Option<&str>) -> Result<(String, Option<Vec<u8>>), BundleError> {
    let mut files = Vec::new();
    list_files(dir, "", &mut files).map_err(|e| BundleError::Io(dir.to_path_buf(), e))?;
    files.sort();
    let (mut hasher, mut found) = (Sha256::new(), None);
    for relative in files {
        let path = dir.join(&relative);
        let bytes = std::fs::read(&path).map_err(|e| BundleError::Io(path, e))?;
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
        if kept == Some(relative.as_str()) {
            found = Some(bytes);
        }
    }
    Ok((hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect(), found))
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
