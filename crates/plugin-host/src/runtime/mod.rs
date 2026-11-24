mod gate;
mod native;
mod serve;
mod tasks;
mod wasm;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use craft_plugin::{Call, Outcome, Permission, RunMode, TaskState};
use serde_json::Value;

use self::native::NativeGuest;
use self::serve::{HostCx, Identity};
use self::tasks::{PROJECT_CLOSED, Project, Slot, Tasks};
use self::wasm::{WasmGuest, WasmModule};
use crate::cache::{BUDGET, Cache};
use crate::host::{Host, Origin};
use crate::images::{IMAGE_BUDGET, Images, PluginImage};
use crate::plugin::{Io, PluginError};
use crate::registry::Bundle;

const COMPILED_DIR: &str = ".compiled";

fn app_folders(data_dir: &Path) -> Vec<PathBuf> {
    let shared = data_dir.parent().unwrap_or(data_dir);
    std::iter::once(shared).chain(crate::registry::install_root(shared)).map(Path::to_path_buf).collect()
}

struct LazyModule {
    bundle: Bundle,
    digest: String,
    name: String,
    cache: PathBuf,
    module: OnceLock<Result<WasmModule, PluginError>>,
}

impl LazyModule {
    fn get(&self) -> Result<&WasmModule, PluginError> {
        let read = || {
            let entry = self.bundle.entry_path().map_err(|e| PluginError::Load(e.to_string()))?;
            match self.bundle.digest_keeping(Some(&entry)).map_err(|e| PluginError::Load(e.to_string()))? {
                (digest, Some(bytes)) if digest == self.digest => Ok(bytes),
                (_, Some(_)) => Err(PluginError::Load("the plugin's files changed since it was started; turn it off and on again".into())),
                (_, None) => Err(PluginError::Load(format!("{}: the entry file is missing", entry.display()))),
            }
        };
        self.module.get_or_init(|| WasmModule::load(&self.name, &self.cache, read)).as_ref().map_err(Clone::clone)
    }

    fn instantiate(&self) -> Result<WasmGuest, PluginError> {
        self.get()?.instantiate()
    }
}

enum Kind {
    Wasm { module: Box<LazyModule>, main: Mutex<Option<Box<WasmGuest>>>, pool: Mutex<Vec<WasmGuest>> },
    Native(NativeGuest),
}

pub(crate) struct Engine {
    identity: Identity,
    kind: Kind,
}

impl Engine {
    pub fn open(bundle: &Bundle, digest: &str, grants: BTreeSet<Permission>, data_dir: PathBuf) -> Result<Arc<Engine>, PluginError> {
        let entry = bundle.entry_path().map_err(|e| PluginError::Load(e.to_string()))?;
        let kind = if bundle.mode == RunMode::Sandboxed {
            let module = LazyModule {
                bundle: bundle.clone(),
                digest: digest.to_string(),
                name: wasm::module_name(digest)?,
                cache: data_dir.parent().unwrap_or(&data_dir).join(COMPILED_DIR),
                module: OnceLock::new(),
            };
            Kind::Wasm { module: Box::new(module), main: Mutex::new(None), pool: Mutex::new(Vec::new()) }
        } else {
            Kind::Native(NativeGuest::load(&entry)?)
        };
        Ok(Engine::with(kind, &bundle.manifest.id, bundle.dir.clone(), grants, data_dir))
    }

    pub fn in_process(plugin: craft_plugin::Plugin, id: &str, dir: PathBuf, grants: BTreeSet<Permission>, data_dir: PathBuf) -> Arc<Engine> {
        Engine::with(Kind::Native(NativeGuest::in_process(plugin)), id, dir, grants, data_dir)
    }

    fn with(kind: Kind, id: &str, dir: PathBuf, grants: BTreeSet<Permission>, data_dir: PathBuf) -> Arc<Engine> {
        let identity = Identity {
            id: id.into(),
            dir,
            app_folders: app_folders(&data_dir),
            data_dir,
            grants,
            cache: Arc::new(Cache::new(BUDGET)),
            tasks: Tasks::default(),
            picked: Mutex::new(Vec::new()),
            images: Images::new(IMAGE_BUDGET),
            journal: Default::default(),
        };
        Arc::new(Engine { identity, kind })
    }

    pub fn journal(&self) -> &crate::journal::Journal {
        &self.identity.journal
    }

    pub fn module_digest(&self) -> Option<&str> {
        match &self.kind {
            Kind::Wasm { module, .. } => Some(&module.name),
            Kind::Native(_) => None,
        }
    }

    pub fn started(&self) -> bool {
        match &self.kind {
            Kind::Wasm { module, .. } => module.module.get().is_some(),
            Kind::Native(_) => true,
        }
    }

    pub fn described(&self) -> Option<Vec<u8>> {
        match &self.kind {
            Kind::Wasm { module, .. } => wasm::described(&module.cache, &module.name),
            Kind::Native(_) => None,
        }
    }

    pub fn keep_described(&self, reply: &[u8]) {
        if let Kind::Wasm { module, .. } = &self.kind {
            wasm::keep_described(&module.cache, &module.name, reply);
        }
    }

    pub fn cached(&self, key: &str) -> Option<Arc<[u8]>> {
        self.identity.cache.get(key)
    }

    pub fn image(&self, key: &str) -> Option<Arc<PluginImage>> {
        self.identity.images.get(key)
    }

    pub fn uncache(&self, key: &str) {
        self.identity.cache.remove(key);
        self.identity.cache.changed_outside_a_render();
    }

    pub fn cache_generation(&self) -> u64 {
        self.identity.cache.generation()
    }

    pub fn copies_pixels(&self) -> bool {
        matches!(self.kind, Kind::Wasm { .. })
    }

    pub fn call(
        self: &Arc<Self>,
        request: &[u8],
        io: Io,
        host: &mut dyn Host,
        origin: Origin,
        budget: Duration,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<Vec<u8>, PluginError> {
        let stateful = origin == Origin::Interactive;
        let mut cx = HostCx { host, plugin: &self.identity, engine: self, origin, task: None, on_task_thread: false, paused: Duration::ZERO };
        match &self.kind {
            Kind::Native(guest) => match io {
                Io::None => Ok(guest.call(request, None, &mut cx)),
                Io::InPlace(pixels) => Ok(guest.call(request, Some(pixels), &mut cx)),
                Io::Region { input, output, skip } => Ok(guest.render_band(request, input, output, skip, &mut cx)),
                Io::Render { .. } | Io::Draw { .. } | Io::Pass { .. } => {
                    Err(PluginError::Protocol("native plugins read their images through their addresses".into()))
                }
            },
            Kind::Wasm { module, main, .. } if stateful => {
                let mut slot = main.lock().map_err(|_| PluginError::Trap("an earlier call panicked".into()))?;
                let mut guest = match slot.take() {
                    Some(guest) => guest,
                    None => Box::new(module.instantiate()?),
                };
                let reply = guest.call(request, io, &mut cx, budget, cancel)?;
                *slot = Some(guest);
                Ok(reply)
            }
            Kind::Wasm { module, pool, .. } => {
                let pooled = pool.lock().ok().and_then(|mut pool| pool.pop());
                let mut guest = match pooled {
                    Some(guest) => guest,
                    None => module.instantiate()?,
                };
                let reply = guest.call(request, io, &mut cx, budget, cancel)?;
                if let Ok(mut pool) = pool.lock()
                    && pool.len() < rayon::current_num_threads()
                {
                    pool.push(guest);
                }
                Ok(reply)
            }
        }
    }

    pub fn accept_drop(&self, path: &Path) -> Result<(), String> {
        self.identity.remember_pick(path.to_path_buf(), false)
    }

    pub fn know_tasks(&self, names: Vec<String>) {
        self.identity.tasks.know(names);
    }

    pub fn start(self: &Arc<Self>, task: &str, args: Value, project: Project) -> Result<u64, String> {
        let request = serde_json::to_vec(&Call::Task { task: task.into(), args }).map_err(|e| e.to_string())?;
        let (id, slot) = self.identity.tasks.add(task, project)?;
        let (engine, running) = (self.clone(), slot.clone());
        let name = task.to_string();
        let spawned = std::thread::Builder::new().name(format!("craft-plugin-task-{id}")).spawn(move || {
            let outcome = engine.run_task(&request, &running);
            if let Err(error) = &outcome {
                engine.identity.journal.push(craft_plugin::LogLevel::Error, format!("task `{name}` failed: {error}"));
            }
            running.finish(outcome);
        });
        if let Err(error) = spawned {
            slot.finish(Err(error.to_string()));
        }
        Ok(id)
    }

    fn run_task(self: &Arc<Self>, request: &[u8], slot: &Arc<Slot>) -> Result<Value, String> {
        let mut detached = Detached;
        let mut cx = HostCx {
            host: &mut detached,
            plugin: &self.identity,
            engine: self,
            origin: Origin::Task,
            task: Some(slot.clone()),
            on_task_thread: true,
            paused: Duration::ZERO,
        };
        let reply = match &self.kind {
            Kind::Native(guest) => guest.call(request, None, &mut cx),
            Kind::Wasm { module, .. } => {
                module.instantiate().and_then(|mut guest| guest.run_task(request, &mut cx, slot.clone())).map_err(|e| e.to_string())?
            }
        };
        let outcome: Outcome<Value> = serde_json::from_slice(&reply).map_err(|e| e.to_string())?;
        outcome.into()
    }

    pub fn tasks(&self) -> Vec<TaskState> {
        self.identity.tasks.list()
    }

    pub fn running_tasks(&self) -> usize {
        self.identity.tasks.running()
    }

    pub fn cancel(&self, id: u64) -> bool {
        self.identity.tasks.cancel(id)
    }

    pub fn cancel_all(&self) {
        self.identity.tasks.cancel_all();
    }

    pub fn pump(self: &Arc<Self>, host: &mut dyn Host) -> usize {
        let queue = self.identity.tasks.take_queue();
        let served = queue.len();
        for forwarded in queue {
            if forwarded.task.project_closed() {
                let _ =
                    forwarded.reply.send((serde_json::to_vec(&Outcome::<Value>::Err(PROJECT_CLOSED.into())).unwrap_or_default(), serve::Bulk::None));
                continue;
            }
            let mut cx = HostCx {
                host: &mut *host,
                plugin: &self.identity,
                engine: self,
                origin: Origin::Task,
                task: Some(forwarded.task.clone()),
                on_task_thread: false,
                paused: Duration::ZERO,
            };
            let answer = native::guarded(&mut cx, &forwarded.request, &forwarded.bulk);
            let _ = forwarded.reply.send(answer);
        }
        self.identity.tasks.cancel_orphans();
        served
    }
}

struct Detached;
impl Host for Detached {}
