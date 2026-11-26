use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::wire::Value;

pub const MANIFEST_FILE: &str = "plugin.json";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub entry: Entry,
    #[serde(default, skip_serializing_if = "Contributes::is_empty")]
    pub contributes: Contributes,
    #[serde(default)]
    pub permissions: BTreeSet<Permission>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Contributes {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presets: Vec<ContributedPreset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub luts: Vec<String>,
}

impl Contributes {
    pub fn is_empty(&self) -> bool {
        self.presets.is_empty() && self.luts.is_empty()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ContributedPreset {
    pub effect: String,
    pub name: String,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wasm: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub native: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunMode {
    Sandboxed,
    Unsandboxed,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Permission {
    #[serde(rename = "document.read")]
    DocumentRead,
    #[serde(rename = "document.write")]
    DocumentWrite,
    #[serde(rename = "files.read")]
    FilesRead,
    #[serde(rename = "files.write")]
    FilesWrite,
    #[serde(rename = "clipboard")]
    Clipboard,
}

impl Permission {
    pub const ALL: [Permission; 5] =
        [Permission::DocumentRead, Permission::DocumentWrite, Permission::FilesRead, Permission::FilesWrite, Permission::Clipboard];

    pub fn name(self) -> &'static str {
        match self {
            Permission::DocumentRead => "document.read",
            Permission::DocumentWrite => "document.write",
            Permission::FilesRead => "files.read",
            Permission::FilesWrite => "files.write",
            Permission::Clipboard => "clipboard",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            Permission::DocumentRead => "Read the open project: compositions, layers, properties and selection",
            Permission::DocumentWrite => "Edit the open project: create and change layers, effects and keyframes",
            Permission::FilesRead => "Read files outside the plugin's own folders",
            Permission::FilesWrite => "Write files outside the plugin's own folders",
            Permission::Clipboard => "Read and replace the text on the system clipboard",
        }
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ManifestError {
    Json(String),
    Id(String),
    Field(&'static str),
    EntryPath(String),
    DataOnlyPermissions,
    Preset(String),
    Lut(String),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::Json(e) => write!(f, "plugin.json is not valid: {e}"),
            ManifestError::Id(id) => {
                write!(f, "plugin id `{id}` must be reverse-domain, lower case, such as com.example.glow")
            }
            ManifestError::Field(name) => write!(f, "plugin.json field `{name}` is empty"),
            ManifestError::EntryPath(path) => {
                write!(f, "entry `{path}` must be a file name inside the plugin folder")
            }
            ManifestError::DataOnlyPermissions => {
                write!(
                    f,
                    "a plugin with no `entry` only ships presets and LUTs, so it cannot ask for permissions; remove `permissions` or add an `entry`"
                )
            }
            ManifestError::Preset(problem) => write!(f, "plugin.json preset {problem}"),
            ManifestError::Lut(path) => write!(f, "plugin.json lut `{path}` must be a .cube file name inside the plugin folder, listed once"),
        }
    }
}

impl std::error::Error for ManifestError {}

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
        let manifest: Manifest = serde_json::from_str(text).map_err(|e| ManifestError::Json(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if !valid_id(&self.id) {
            return Err(ManifestError::Id(self.id.clone()));
        }
        if self.name.trim().is_empty() {
            return Err(ManifestError::Field("name"));
        }
        if self.version.trim().is_empty() {
            return Err(ManifestError::Field("version"));
        }
        let paths: Vec<&String> = self.entry.wasm.iter().chain(self.entry.native.values()).collect();
        self.contributes.validate()?;
        if paths.is_empty() {
            if self.contributes.is_empty() {
                return Err(ManifestError::Field("entry"));
            }
            if !self.permissions.is_empty() {
                return Err(ManifestError::DataOnlyPermissions);
            }
        }
        match paths.into_iter().find(|path| !valid_entry_path(path)) {
            Some(path) => Err(ManifestError::EntryPath(path.clone())),
            None => Ok(()),
        }
    }

    pub fn is_data_only(&self) -> bool {
        self.entry.wasm.is_none() && self.entry.native.is_empty()
    }

    pub fn has_wasm(&self) -> bool {
        self.entry.wasm.is_some()
    }

    pub fn native_file(&self) -> Option<&str> {
        self.entry.native.get(&native_target()).map(String::as_str)
    }

    pub fn can_choose_unsandboxed(&self) -> bool {
        self.has_wasm() && self.native_file().is_some()
    }

    pub fn mode(&self, unsandboxed_chosen: bool) -> RunMode {
        if !self.is_data_only() && (!self.has_wasm() || (unsandboxed_chosen && self.native_file().is_some())) {
            RunMode::Unsandboxed
        } else {
            RunMode::Sandboxed
        }
    }

    pub fn entry_file(&self, mode: RunMode) -> Option<&str> {
        match mode {
            RunMode::Sandboxed => self.entry.wasm.as_deref(),
            RunMode::Unsandboxed => self.native_file(),
        }
    }
}

pub fn native_target() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn valid_id(id: &str) -> bool {
    let segment_ok = |s: &str| !s.is_empty() && !s.starts_with('-') && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    id.len() <= 128 && id.split('.').count() >= 2 && id.split('.').all(segment_ok)
}

impl Contributes {
    fn validate(&self) -> Result<(), ManifestError> {
        let mut named = BTreeSet::new();
        for preset in &self.presets {
            if preset.effect.trim().is_empty() || preset.name.trim().is_empty() {
                return Err(ManifestError::Preset("needs a non-empty `effect` and `name`".into()));
            }
            if !named.insert((&preset.effect, &preset.name)) {
                return Err(ManifestError::Preset(format!("`{}` is listed twice for effect `{}`", preset.name, preset.effect)));
            }
        }
        let mut files = BTreeSet::new();
        match self.luts.iter().find(|lut| !valid_entry_path(lut) || !lut.to_ascii_lowercase().ends_with(".cube") || !files.insert(*lut)) {
            Some(lut) => Err(ManifestError::Lut(lut.clone())),
            None => Ok(()),
        }
    }
}

fn valid_entry_path(path: &str) -> bool {
    !path.is_empty() && !path.starts_with('.') && path.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(json: &str) -> Result<Manifest, ManifestError> {
        Manifest::parse(json)
    }

    #[test]
    fn a_wasm_plugin_is_sandboxed_and_keeps_its_permissions() {
        let m = manifest(
            r#"{"id":"com.example.glow","name":"Glow","version":"1.0.0","entry":{"wasm":"plugin.wasm"},
                "permissions":["document.read","files.write"]}"#,
        )
        .unwrap();
        assert_eq!(m.mode(true), RunMode::Sandboxed);
        assert_eq!(m.entry_file(RunMode::Sandboxed), Some("plugin.wasm"));
        assert_eq!(m.entry_file(RunMode::Unsandboxed), None);
        assert_eq!(m.permissions.into_iter().collect::<Vec<_>>(), [Permission::DocumentRead, Permission::FilesWrite]);
    }

    #[test]
    fn a_native_plugin_resolves_the_library_for_this_machine_only() {
        let json = format!(
            r#"{{"id":"com.example.native","name":"N","version":"1","entry":{{"native":{{"{}":"libn.dylib","plan9-mips":"n.so"}}}}}}"#,
            native_target()
        );
        let m = manifest(&json).unwrap();
        assert_eq!(m.mode(false), RunMode::Unsandboxed);
        assert_eq!(m.entry_file(RunMode::Unsandboxed), Some("libn.dylib"));
        assert_eq!(m.entry_file(RunMode::Sandboxed), None);
    }

    #[test]
    fn a_plugin_with_both_entries_is_sandboxed_until_unsandboxed_is_chosen() {
        let json = format!(
            r#"{{"id":"com.example.both","name":"B","version":"1","entry":{{"wasm":"b.wasm","native":{{"{}":"libb.so","plan9-mips":"b.so"}}}}}}"#,
            native_target()
        );
        let m = manifest(&json).unwrap();
        assert!(m.can_choose_unsandboxed());
        assert_eq!(m.mode(false), RunMode::Sandboxed);
        assert_eq!(m.mode(true), RunMode::Unsandboxed);
        assert_eq!(m.entry_file(RunMode::Sandboxed), Some("b.wasm"));
        assert_eq!(m.entry_file(RunMode::Unsandboxed), Some("libb.so"));
    }

    #[test]
    fn a_plugin_with_both_entries_but_no_native_build_for_this_machine_cannot_be_unsandboxed() {
        let m = manifest(r#"{"id":"com.example.both","name":"B","version":"1","entry":{"wasm":"b.wasm","native":{"plan9-mips":"b.so"}}}"#).unwrap();
        assert!(!m.can_choose_unsandboxed());
        assert_eq!(m.mode(true), RunMode::Sandboxed);
    }

    #[test]
    fn an_entry_with_nothing_in_it_is_refused() {
        assert_eq!(manifest(r#"{"id":"com.example.none","name":"N","version":"1","entry":{}}"#), Err(ManifestError::Field("entry")));
    }

    #[test]
    fn ids_and_entry_paths_that_could_escape_the_folder_are_refused() {
        let with = |id: &str, entry: &str| manifest(&format!(r#"{{"id":"{id}","name":"N","version":"1","entry":{{"wasm":"{entry}"}}}}"#));
        assert!(matches!(with("glow", "p.wasm"), Err(ManifestError::Id(_))));
        assert!(matches!(with("Com.Example.Glow", "p.wasm"), Err(ManifestError::Id(_))));
        assert!(matches!(with("com.example/../x", "p.wasm"), Err(ManifestError::Id(_))));
        assert!(matches!(with("com.example.glow", "../p.wasm"), Err(ManifestError::EntryPath(_))));
        assert!(matches!(with("com.example.glow", "/abs/p.wasm"), Err(ManifestError::EntryPath(_))));
        assert!(matches!(with("com.example.glow", "sub/p.wasm"), Err(ManifestError::EntryPath(_))));
    }

    #[test]
    fn an_unknown_permission_is_an_error_not_a_silent_grant() {
        let r = manifest(r#"{"id":"com.example.glow","name":"N","version":"1","entry":{"wasm":"p.wasm"},"permissions":["network"]}"#);
        assert!(matches!(r, Err(ManifestError::Json(_))));
    }

    #[test]
    fn a_manifest_with_presets_or_luts_and_no_entry_is_data_only() {
        let m = manifest(
            r#"{"id":"com.example.looks","name":"Looks","version":"1","contributes":{"luts":["warm.cube"],
                "presets":[{"effect":"ec.color.tint","name":"Warm","values":{"amount":40}}]}}"#,
        )
        .unwrap();
        assert!(m.is_data_only());
        assert_eq!(m.mode(true), RunMode::Sandboxed);
        assert_eq!(m.contributes.presets[0].values["amount"], Value::Number(40.0));
        assert!(
            !manifest(r#"{"id":"com.example.p","name":"P","version":"1","entry":{"wasm":"p.wasm"},"contributes":{"luts":["a.cube"]}}"#)
                .unwrap()
                .is_data_only()
        );
    }

    #[test]
    fn malformed_contributions_are_refused_with_the_reason() {
        let with = |contributes: &str| manifest(&format!(r#"{{"id":"com.example.l","name":"L","version":"1","contributes":{contributes}}}"#));
        assert_eq!(with("{}"), Err(ManifestError::Field("entry")));
        assert!(matches!(with(r#"{"luts":["../a.cube"]}"#), Err(ManifestError::Lut(_))));
        assert!(matches!(with(r#"{"luts":["a.png"]}"#), Err(ManifestError::Lut(_))));
        assert!(matches!(with(r#"{"luts":["a.cube","a.cube"]}"#), Err(ManifestError::Lut(_))));
        assert!(matches!(with(r#"{"presets":[{"effect":"ec.color.tint","name":" "}]}"#), Err(ManifestError::Preset(_))));
        let twice = with(r#"{"presets":[{"effect":"ec.color.tint","name":"A"},{"effect":"ec.color.tint","name":"A"}]}"#).unwrap_err();
        assert_eq!(twice.to_string(), "plugin.json preset `A` is listed twice for effect `ec.color.tint`");
        assert!(matches!(with(r#"{"luts":["a.cube"],"extra":1}"#), Err(ManifestError::Json(_))));
        let asks = manifest(r#"{"id":"com.example.l","name":"L","version":"1","contributes":{"luts":["a.cube"]},"permissions":["files.read"]}"#);
        assert_eq!(asks, Err(ManifestError::DataOnlyPermissions));
    }
}
