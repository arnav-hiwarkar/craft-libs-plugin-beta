use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use craft_plugin::{Permission, TaskState};

use crate::host::{Host, Origin};
use crate::plugin::{Instance, Io, PluginError, Scope};
use crate::registry::Bundle;

#[derive(Default)]
pub(crate) struct Staged;

pub(crate) enum Engine {}

impl Engine {
    pub fn open(_: &Bundle, _: &str, _: BTreeSet<Permission>, _: PathBuf) -> Result<Arc<Engine>, PluginError> {
        Err(PluginError::Load("plugins run in the desktop app only".into()))
    }

    pub fn journal(&self) -> &crate::journal::Journal {
        match *self {}
    }

    pub fn module_digest(&self) -> Option<&str> {
        match *self {}
    }

    pub fn started(&self) -> bool {
        match *self {}
    }

    pub fn failure(&self) -> Option<String> {
        match *self {}
    }

    pub fn described(&self) -> Option<Vec<u8>> {
        match *self {}
    }

    pub fn keep_described(&self, _: &[u8]) {
        match *self {}
    }

    pub fn cached(&self, _: &str) -> Option<Arc<[u8]>> {
        match *self {}
    }

    pub fn image(&self, _: &str) -> Option<Arc<crate::images::PluginImage>> {
        match *self {}
    }

    pub fn uncache(&self, _: &str) {
        match *self {}
    }

    pub fn cached_in(&self, _: &Staged, _: &str) -> Option<Arc<[u8]>> {
        match *self {}
    }

    pub fn commit(&self, _: Staged) {
        match *self {}
    }

    pub fn cache_generation(&self) -> u64 {
        match *self {}
    }

    pub fn copies_pixels(&self) -> bool {
        match *self {}
    }

    pub fn call(self: &Arc<Self>, _: &[u8], _: Io, _: &mut dyn Host, _: (Origin, Instance), _: Duration, _: Scope) -> Result<Vec<u8>, PluginError> {
        match **self {}
    }

    pub fn accept_drop(&self, _: &std::path::Path) -> Result<(), String> {
        match *self {}
    }

    pub fn know_tasks(&self, _: Vec<String>) {
        match *self {}
    }

    pub fn start(self: &Arc<Self>, _: &str, _: serde_json::Value, _: Option<Arc<std::sync::atomic::AtomicBool>>) -> Result<u64, String> {
        match **self {}
    }

    pub fn tasks(&self) -> Vec<TaskState> {
        match *self {}
    }

    pub fn running_tasks(&self) -> usize {
        match *self {}
    }

    pub fn cancel(&self, _: u64) -> bool {
        match *self {}
    }

    pub fn cancel_all(&self) {
        match *self {}
    }

    pub fn pump(self: &Arc<Self>, _: &mut dyn Host) -> usize {
        match **self {}
    }
}
