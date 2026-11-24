use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use craft_plugin::{ABI, Outcome, Pixel, Rect, Rendered};
use std::sync::Arc;

use wasmtime::{
    AsContextMut, Caller, Config, Engine, Extern, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc, UpdateDeadline,
};

use sha2::{Digest, Sha256};

use super::native;
use super::serve::{self, HostCx};
use super::tasks::Slot;
use crate::plugin::{Io, PluginError, drawn_in};

const TICK: Duration = Duration::from_millis(50);
const MEMORY_LIMIT: usize = 3 << 30;
const REPLY_LIMIT: usize = 64 << 20;
const TABLE_LIMIT: usize = 4;
const TABLE_ELEMENT_LIMIT: usize = 65_536;

type CallFn = TypedFunc<(u32, u32, u32, u32, u32, u32), ()>;
type AllocFn = TypedFunc<u32, u32>;
type FreeFn = TypedFunc<(u32, u32), ()>;

struct State {
    limits: StoreLimits,
    ctx: usize,
    alloc: Option<AllocFn>,
    deadline: Option<Instant>,
}

fn engine() -> Result<&'static Engine, PluginError> {
    static ENGINE: OnceLock<Result<Engine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut config = Config::new();
            config.epoch_interruption(true);
            config.relaxed_simd_deterministic(true);
            let engine = Engine::new(&config).map_err(|e| e.to_string())?;
            let clock = engine.weak();
            std::thread::Builder::new()
                .name("craft-plugin-clock".into())
                .spawn(move || {
                    while let Some(engine) = clock.upgrade() {
                        engine.increment_epoch();
                        std::thread::sleep(TICK);
                    }
                })
                .map_err(|e| e.to_string())?;
            Ok(engine)
        })
        .as_ref()
        .map_err(|e| PluginError::Load(e.clone()))
}

pub(crate) struct WasmModule {
    module: Module,
    linker: Linker<State>,
}

impl WasmModule {
    pub fn compile(bytes: &[u8], cache: &Path) -> Result<WasmModule, PluginError> {
        let engine = engine()?;
        let module = cached(engine, bytes, cache)?;
        let mut linker = Linker::new(engine);
        linker.func_wrap("craft", "host_call", host_call).map_err(load_error)?;
        Ok(WasmModule { module, linker })
    }

    pub fn instantiate(&self) -> Result<WasmGuest, PluginError> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(MEMORY_LIMIT)
            .memories(1)
            .instances(1)
            .tables(TABLE_LIMIT)
            .table_elements(TABLE_ELEMENT_LIMIT)
            .build();
        let mut store = Store::new(self.module.engine(), State { limits, ctx: 0, alloc: None, deadline: None });
        store.limiter(|state| &mut state.limits);
        store.set_epoch_deadline(ticks(Duration::from_secs(5)));
        let instance = self.linker.instantiate(&mut store, &self.module).map_err(load_error)?;
        let memory = instance.get_memory(&mut store, "memory").ok_or_else(|| missing("memory"))?;
        let abi: TypedFunc<(), u32> = instance.get_typed_func(&mut store, "craft_plugin_abi").map_err(load_error)?;
        let found = abi.call(&mut store, ()).map_err(load_error)?;
        if found != ABI {
            return Err(PluginError::Load(format!("plugin speaks ABI {found}, this app speaks ABI {ABI}")));
        }
        let call: CallFn = instance.get_typed_func(&mut store, "craft_plugin_call").map_err(load_error)?;
        let alloc: AllocFn = instance.get_typed_func(&mut store, "craft_plugin_alloc").map_err(load_error)?;
        let free: FreeFn = instance.get_typed_func(&mut store, "craft_plugin_free").map_err(load_error)?;
        store.data_mut().alloc = Some(alloc.clone());
        let out = alloc.call(&mut store, 8).map_err(load_error)?;
        Ok(WasmGuest { store, memory, call, alloc, free, out, pixels: (0, 0) })
    }
}

pub(crate) struct WasmGuest {
    store: Store<State>,
    memory: Memory,
    call: CallFn,
    alloc: AllocFn,
    free: FreeFn,
    out: u32,
    pixels: (u32, u32),
}

impl WasmGuest {
    pub fn call(&mut self, request: &[u8], io: Io, cx: &mut HostCx, budget: Duration) -> Result<Vec<u8>, PluginError> {
        self.store.set_epoch_deadline(ticks(budget));
        self.run(request, io, cx, Instant::now().checked_add(budget))
    }

    pub fn run_task(&mut self, request: &[u8], cx: &mut HostCx, slot: Arc<Slot>) -> Result<Vec<u8>, PluginError> {
        self.store.epoch_deadline_callback(move |_| {
            if slot.overdue() {
                return Err(wasmtime::format_err!("the task was cancelled"));
            }
            Ok(UpdateDeadline::Continue(1))
        });
        self.store.set_epoch_deadline(1);
        self.run(request, Io::None, cx, None)
    }

    fn run(&mut self, request: &[u8], io: Io, cx: &mut HostCx, deadline: Option<Instant>) -> Result<Vec<u8>, PluginError> {
        self.store.data_mut().ctx = native::erase(cx);
        self.store.data_mut().deadline = deadline;
        let reply = self.exchange(request, io);
        self.store.data_mut().ctx = 0;
        self.store.data_mut().deadline = None;
        reply.map_err(|e| {
            log::warn!(target: "plugin", "{e:#}");
            PluginError::Trap(e.root_cause().to_string())
        })
    }

    fn exchange(&mut self, request: &[u8], io: Io) -> wasmtime::Result<Vec<u8>> {
        let (inputs, out_bytes): ([&[Pixel]; 2], usize) = match &io {
            Io::None => ([&[], &[]], 0),
            Io::InPlace(pixels) => ([pixels, &[]], 0),
            Io::Region { input, .. } => ([input, &[]], 0),
            Io::Render { input, out, .. } => ([input, out], 0),
            Io::Draw { out, .. } => ([&[], &[]], size_of_val::<[Pixel]>(&**out)),
            Io::Pass { src, aux, out } => ([src, aux], size_of_val::<[Pixel]>(&**out)),
        };
        let [first, second] = inputs.map(bytemuck::cast_slice::<Pixel, u8>);
        let pixel_bytes = first.len() + second.len() + out_bytes;
        let pixels = self.pixel_buffer(pixel_bytes)?;
        let place = self.memory.data_mut(&mut self.store).get_mut(pixels as usize..).and_then(|tail| tail.get_mut(..pixel_bytes));
        let place = place.ok_or_else(|| wasmtime::format_err!("the frame does not fit in plugin memory"))?;
        let (first_place, rest) = place.split_at_mut(first.len());
        first_place.copy_from_slice(first);
        let (second_place, out_place) = rest.split_at_mut(second.len());
        second_place.copy_from_slice(second);
        if matches!(io, Io::Pass { .. }) {
            out_place.fill(0);
        }

        let request_at = self.alloc.call(&mut self.store, len(request)?)?;
        if request_at == 0 {
            wasmtime::bail!("the plugin could not allocate the request");
        }
        self.memory.write(&mut self.store, request_at as usize, request)?;
        self.memory.write(&mut self.store, self.out as usize, &[0; 8])?;
        let args = (0, request_at, len(request)?, pixels, u32::try_from(pixel_bytes)?, self.out);
        self.call.call(&mut self.store, args)?;
        self.free.call(&mut self.store, (request_at, len(request)?))?;

        let mut out = [0u32; 2];
        self.memory.read(&self.store, self.out as usize, bytemuck::cast_slice_mut(&mut out))?;
        let reply = reply_bytes(self.memory.data(&self.store), out)?.to_vec();
        self.free.call(&mut self.store, (out[0], out[1]))?;
        if !succeeded(&reply) {
            return Ok(reply);
        }

        let (output, skip, at): (&mut [Pixel], usize, Option<Rect>) = match io {
            Io::None => return Ok(reply),
            Io::InPlace(output) => (output, 0, None),
            Io::Region { output, skip, .. } => (output, skip, None),
            Io::Render { input, out, at } => (out, input.len(), Some(at)),
            Io::Draw { out, at } => (out, 0, Some(at)),
            Io::Pass { src, aux, out } => (out, src.len() + aux.len(), None),
        };
        let rows = at.map_or(0..output.len(), |at| drawn_rows(&reply, at));
        let output: &mut [u8] = bytemuck::cast_slice_mut(&mut output[rows.clone()]);
        let from = pixels as usize + (skip + rows.start) * size_of::<Pixel>();
        let rendered = self.memory.data(&self.store).get(from..).and_then(|tail| tail.get(..output.len()));
        output.copy_from_slice(rendered.ok_or_else(|| wasmtime::format_err!("the frame does not fit in plugin memory"))?);
        Ok(reply)
    }

    fn pixel_buffer(&mut self, bytes: usize) -> wasmtime::Result<u32> {
        let wanted = u32::try_from(bytes)?;
        if self.pixels.1 < wanted {
            if self.pixels.1 > 0 {
                self.free.call(&mut self.store, self.pixels)?;
            }
            self.pixels = (0, 0);
            let at = self.alloc.call(&mut self.store, wanted)?;
            if at == 0 {
                wasmtime::bail!("the plugin could not allocate a {bytes} byte frame");
            }
            self.pixels = (at, wanted);
        }
        Ok(self.pixels.0)
    }
}

fn host_call(mut caller: Caller<'_, State>, _ctx: u32, request: u32, request_len: u32, bulk: u32, bulk_len: u32, out: u32) -> wasmtime::Result<()> {
    let memory = caller.get_export("memory").and_then(Extern::into_memory).ok_or_else(|| wasmtime::format_err!("no memory"))?;
    let slice = |at: u32, len: u32| -> wasmtime::Result<Vec<u8>> {
        let (start, end) = (at as usize, at as usize + len as usize);
        memory.data(&caller).get(start..end).map(<[u8]>::to_vec).ok_or_else(|| wasmtime::format_err!("host call reads outside memory"))
    };
    let request = slice(request, request_len)?;
    let (reply, bulk_out, paused) = match serve::refuse_oversized(&request, bulk_len as usize) {
        Some((reply, bulk_out)) => (reply, bulk_out, Duration::ZERO),
        None => native::serve_erased(caller.data().ctx, &request, &slice(bulk, bulk_len)?),
    };
    if !paused.is_zero()
        && let Some(deadline) = caller.data().deadline.and_then(|deadline| deadline.checked_add(paused))
    {
        caller.data_mut().deadline = Some(deadline);
        caller.as_context_mut().set_epoch_deadline(ticks(deadline.saturating_duration_since(Instant::now())));
    }
    if caller.data().deadline.is_some_and(|deadline| Instant::now() > deadline) {
        wasmtime::bail!("the plugin passed its time limit while the host served a call");
    }
    let alloc = caller.data().alloc.clone().ok_or_else(|| wasmtime::format_err!("host call before start"))?;
    let mut place = |bytes: &[u8]| -> wasmtime::Result<[u32; 2]> {
        if bytes.is_empty() {
            return Ok([0, 0]);
        }
        let at = alloc.call(&mut caller, len(bytes)?)?;
        if at == 0 {
            wasmtime::bail!("the plugin could not allocate {} bytes", bytes.len());
        }
        memory.write(&mut caller, at as usize, bytes)?;
        Ok([at, len(bytes)?])
    };
    let [reply_at, reply_len] = place(&reply)?;
    let [bulk_at, bulk_len] = place(bulk_out.bytes())?;
    memory.write(&mut caller, out as usize, bytemuck::cast_slice(&[reply_at, reply_len, bulk_at, bulk_len]))?;
    Ok(())
}

const KEY_FILE: &str = "key";
const MAC_LEN: usize = 32;
const MAC_EXTENSION: &str = "mac";

fn cached(engine: &Engine, bytes: &[u8], cache: &Path) -> Result<Module, PluginError> {
    let name = cache_name(engine, bytes);
    let path = cache.join(format!("{name}.cwasm"));
    let key = match install_key(cache) {
        Ok(key) => key,
        Err(error) => {
            log::warn!("{}: the compiled plugin cache is off: {error}", cache.display());
            return engine.precompile_module(bytes).and_then(|compiled| native::fresh_module(engine, &compiled)).map_err(load_error);
        }
    };
    if let Some(module) = signed_entry(&key, &name, &path).and_then(|body| native::compiled_module(engine, &body)) {
        return Ok(module);
    }
    let compiled = engine.precompile_module(bytes).map_err(load_error)?;
    let module = native::fresh_module(engine, &compiled).map_err(load_error)?;
    if let Err(error) = store(&key, &name, &path, &compiled) {
        log::warn!("{}: the compiled plugin was not cached: {error}", path.display());
    }
    Ok(module)
}

fn signed_entry(key: &[u8; 32], name: &str, path: &Path) -> Option<Vec<u8>> {
    let mac = std::fs::read(path.with_extension(MAC_EXTENSION)).ok()?;
    let mut file = std::fs::File::open(path).ok()?;
    let mut body = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut body).ok()?;
    let meta = file.metadata().ok()?;
    let expected = hmac(key, &[name.as_bytes(), &identity(&meta)]);
    let equal = mac.len() == MAC_LEN && mac.iter().zip(expected).fold(0, |differ, (a, b)| differ | (a ^ b)) == 0;
    (equal && body.len() as u64 == meta.len()).then_some(body)
}

fn store(key: &[u8; 32], name: &str, path: &Path, compiled: &[u8]) -> std::io::Result<()> {
    let part = |extension: &str| path.with_extension(format!("{extension}.{}.part", std::process::id()));
    let (staged, staged_mac) = (part("cwasm"), part(MAC_EXTENSION));
    let stored = std::fs::File::create(&staged).and_then(|mut file| {
        std::io::Write::write_all(&mut file, compiled)?;
        std::fs::rename(&staged, path)?;
        let mac = hmac(key, &[name.as_bytes(), &identity(&file.metadata()?)]);
        std::fs::write(&staged_mac, mac)?;
        std::fs::rename(&staged_mac, path.with_extension(MAC_EXTENSION))
    });
    if stored.is_err() {
        let _ = std::fs::remove_file(&staged);
        let _ = std::fs::remove_file(&staged_mac);
    }
    stored
}

#[cfg(unix)]
fn identity(meta: &std::fs::Metadata) -> Vec<u8> {
    use std::os::unix::fs::MetadataExt;
    [meta.dev(), meta.ino(), meta.size(), meta.mtime() as u64, meta.mtime_nsec() as u64, meta.ctime() as u64, meta.ctime_nsec() as u64]
        .iter()
        .flat_map(|field| field.to_le_bytes())
        .collect()
}

#[cfg(not(unix))]
fn identity(meta: &std::fs::Metadata) -> Vec<u8> {
    let nanos = |time: std::io::Result<std::time::SystemTime>| {
        time.ok().and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |since| since.as_nanos() as u64)
    };
    [meta.len(), nanos(meta.modified()), nanos(meta.created())].iter().flat_map(|field| field.to_le_bytes()).collect()
}

fn hmac(key: &[u8], message: &[&[u8]]) -> [u8; 32] {
    let mut padded = [0u8; 64];
    padded[..key.len()].copy_from_slice(key);
    let pad = |byte: u8| padded.map(|k| k ^ byte);
    let inner = message.iter().fold(Sha256::new().chain_update(pad(0x36)), |digest, part| digest.chain_update(part)).finalize();
    Sha256::new().chain_update(pad(0x5c)).chain_update(inner).finalize().into()
}

fn install_key(cache: &Path) -> std::io::Result<[u8; 32]> {
    std::fs::create_dir_all(cache)?;
    let path = cache.join(KEY_FILE);
    match read_key(&path) {
        Ok(key) => return Ok(key),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            log::warn!("{}: replacing an unusable cache key: {error}", path.display());
            if let Err(error) = std::fs::remove_file(&path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                return Err(error);
            }
        }
    }
    publish_key(&path)?;
    read_key(&path)
}

fn publish_key(path: &Path) -> std::io::Result<()> {
    let (mut key, mut tag) = ([0u8; 32], [0u8; 8]);
    getrandom::fill(&mut key).and_then(|()| getrandom::fill(&mut tag)).map_err(|e| std::io::Error::other(e.to_string()))?;
    let staged = path.with_extension(format!("{}.{:x}.part", std::process::id(), u64::from_le_bytes(tag)));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let written = options.open(&staged).and_then(|mut file| std::io::Write::write_all(&mut file, &key));
    let published = written.and_then(|()| match std::fs::hard_link(&staged, path) {
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        linked => linked,
    });
    let _ = std::fs::remove_file(&staged);
    published
}

fn read_key(path: &Path) -> std::io::Result<[u8; 32]> {
    #[cfg(unix)]
    let file = {
        use rustix::fs::{Mode, OFlags};
        std::fs::File::from(rustix::fs::open(path, OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC, Mode::empty())?)
    };
    #[cfg(not(unix))]
    let file = std::fs::File::open(path)?;
    let meta = file.metadata()?;
    #[cfg(unix)]
    let private = {
        use std::os::unix::fs::MetadataExt;
        meta.uid() == rustix::process::getuid().as_raw() && meta.mode() & 0o077 == 0
    };
    #[cfg(not(unix))]
    let private = true;
    if !meta.is_file() || !private || meta.len() != 32 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "the key must be a 32-byte file only its owner can read"));
    }
    let mut key = [0u8; 32];
    std::io::Read::read_exact(&mut &file, &mut key)?;
    Ok(key)
}

struct DigestHasher(Sha256);

impl Hasher for DigestHasher {
    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    fn finish(&self) -> u64 {
        0
    }
}

fn cache_name(engine: &Engine, bytes: &[u8]) -> String {
    let mut hasher = DigestHasher(Sha256::new());
    hasher.write(bytes);
    hasher.write(std::env::consts::ARCH.as_bytes());
    hasher.write(std::env::consts::OS.as_bytes());
    engine.precompile_compatibility_hash().hash(&mut hasher);
    hasher.0.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

fn drawn_rows(reply: &[u8], at: Rect) -> std::ops::Range<usize> {
    let rendered = match serde_json::from_slice::<Outcome<Option<Rendered>>>(reply) {
        Ok(Outcome::Ok(rendered)) => rendered,
        _ => None,
    };
    let drawn = drawn_in(rendered, at);
    let (width, top) = (at.width() as usize, (drawn.top - at.top) as usize);
    match drawn.is_empty() {
        true => 0..0,
        false => top * width..(top + drawn.height() as usize) * width,
    }
}

fn reply_bytes(memory: &[u8], [at, len]: [u32; 2]) -> wasmtime::Result<&[u8]> {
    if len as usize > REPLY_LIMIT {
        wasmtime::bail!(
            "the plugin's reply is {len} bytes; replies over {REPLY_LIMIT} bytes are refused, so return less (a smaller overlay or fewer items)"
        );
    }
    let end = (at as usize).checked_add(len as usize);
    end.and_then(|end| memory.get(at as usize..end)).filter(|_| at != 0).ok_or_else(|| wasmtime::format_err!("the plugin returned no reply"))
}

fn succeeded(reply: &[u8]) -> bool {
    matches!(serde_json::from_slice::<Outcome<serde::de::IgnoredAny>>(reply), Ok(Outcome::Ok(_)))
}

fn len(bytes: &[u8]) -> wasmtime::Result<u32> {
    Ok(u32::try_from(bytes.len())?)
}

fn ticks(budget: Duration) -> u64 {
    (budget.as_millis() / TICK.as_millis()).max(1) as u64
}

fn load_error(error: wasmtime::Error) -> PluginError {
    PluginError::Load(format!("{error:#}"))
}

fn missing(export: &str) -> PluginError {
    PluginError::Load(format!("the module does not export `{export}`"))
}

#[cfg(test)]
mod tests {
    use super::{KEY_FILE, WasmModule, cache_name, cached, engine, hmac, install_key, reply_bytes, signed_entry};

    const EMPTY: &[u8] = &[0, 0x61, 0x73, 0x6d, 1, 0, 0, 0];
    const ONE_IMPORT: &[u8] = &[0, 0x61, 0x73, 0x6d, 1, 0, 0, 0, 1, 4, 1, 0x60, 0, 0, 2, 7, 1, 1, b'a', 1, b'b', 0, 0];

    fn tables_module(tables: &[u32]) -> Vec<u8> {
        let mut body = vec![tables.len() as u8];
        for &min in tables {
            body.extend([0x70, 0x00]);
            let mut rest = min;
            loop {
                let byte = (rest & 0x7f) as u8;
                rest >>= 7;
                if rest == 0 {
                    body.push(byte);
                    break;
                }
                body.push(byte | 0x80);
            }
        }
        let mut module = vec![0, 0x61, 0x73, 0x6d, 1, 0, 0, 0, 4, body.len() as u8];
        module.extend(body);
        module
    }

    fn instantiate_error(module: &[u8]) -> String {
        let dir = tempfile::tempdir().unwrap();
        match WasmModule::compile(module, dir.path()).unwrap().instantiate() {
            Ok(_) => panic!("the module has no exports and cannot instantiate"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn a_table_of_100000_elements_is_refused_and_four_small_tables_pass_the_limit() {
        let refused = instantiate_error(&tables_module(&[100_000]));
        assert!(refused.contains("table") && refused.contains("limit"), "{refused}");
        let passed = instantiate_error(&tables_module(&[10, 10, 10, 10]));
        assert!(passed.contains("does not export `memory`"), "{passed}");
        let too_many = instantiate_error(&tables_module(&[1, 1, 1, 1, 1]));
        assert!(too_many.contains("table") && too_many.contains("limit"), "{too_many}");
    }

    #[test]
    fn hmac_sha256_matches_rfc_4231_test_case_2() {
        let mac = hmac(b"Jefe", &[b"what do ya ", b"want for nothing?"]);
        let hex: String = mac.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(hex, "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    }

    #[test]
    fn a_planted_compiled_module_is_recompiled_and_a_signed_one_is_reused() {
        let engine = engine().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join(format!("{}.cwasm", cache_name(engine, EMPTY)));
        std::fs::write(&entry, engine.precompile_module(ONE_IMPORT).unwrap()).unwrap();
        assert_eq!(cached(engine, EMPTY, dir.path()).unwrap().imports().len(), 0);
        #[cfg(unix)]
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(dir.path().join(KEY_FILE)).unwrap().permissions()) & 0o777, 0o600);
        let signed = std::fs::read(&entry).unwrap();
        assert_eq!(cached(engine, EMPTY, dir.path()).unwrap().imports().len(), 0);
        assert_eq!(std::fs::read(&entry).unwrap(), signed);
    }

    #[test]
    fn a_signed_entry_copied_over_another_plugins_entry_is_recompiled_not_loaded() {
        let engine = engine().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let entry = |bytes: &[u8]| dir.path().join(format!("{}.cwasm", cache_name(engine, bytes)));
        assert_eq!(cached(engine, ONE_IMPORT, dir.path()).unwrap().imports().len(), 1);
        assert_eq!(cached(engine, EMPTY, dir.path()).unwrap().imports().len(), 0);
        std::fs::copy(entry(ONE_IMPORT), entry(EMPTY)).unwrap();
        assert_eq!(cached(engine, EMPTY, dir.path()).unwrap().imports().len(), 0);
    }

    #[test]
    fn an_untouched_entry_is_read_without_a_rewrite_and_one_changed_in_place_with_its_time_restored_is_refused() {
        let engine = engine().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let name = cache_name(engine, EMPTY);
        let entry = dir.path().join(format!("{name}.cwasm"));
        cached(engine, EMPTY, dir.path()).unwrap();
        let written = std::fs::metadata(&entry).unwrap().modified().unwrap();
        cached(engine, EMPTY, dir.path()).unwrap();
        assert_eq!(std::fs::metadata(&entry).unwrap().modified().unwrap(), written);
        let key = install_key(dir.path()).unwrap();
        let mut body = signed_entry(&key, &name, &entry).unwrap();
        assert_eq!(body, std::fs::read(&entry).unwrap());
        body[0] ^= 1;
        let mut file = std::fs::OpenOptions::new().write(true).open(&entry).unwrap();
        std::io::Write::write_all(&mut file, &body).unwrap();
        file.set_modified(written).unwrap();
        drop(file);
        assert_eq!(std::fs::metadata(&entry).unwrap().modified().unwrap(), written);
        assert!(signed_entry(&key, &name, &entry).is_none(), "an entry changed in place was accepted");
    }

    #[test]
    fn processes_that_make_the_key_at_the_same_time_all_end_up_with_the_one_key_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let start = std::sync::Barrier::new(16);
        let keys: Vec<_> = std::thread::scope(|scope| {
            let racers: Vec<_> = (0..16)
                .map(|_| {
                    scope.spawn(|| {
                        start.wait();
                        install_key(dir.path()).map_err(|e| e.to_string())
                    })
                })
                .collect();
            racers.into_iter().map(|racer| racer.join().unwrap()).collect()
        });
        let on_disk: [u8; 32] = std::fs::read(dir.path().join(KEY_FILE)).unwrap().try_into().unwrap();
        assert_eq!(keys, vec![Ok(on_disk); 16]);
    }

    #[cfg(unix)]
    #[test]
    fn a_key_others_can_read_or_a_link_in_its_place_is_never_used() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(KEY_FILE);
        let planted = dir.path().join("planted");
        std::fs::write(&planted, [7u8; 32]).unwrap();
        std::fs::set_permissions(&planted, std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
        std::os::unix::fs::symlink(&planted, &path).unwrap();
        assert_ne!(install_key(dir.path()).unwrap(), [7u8; 32]);
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, [9u8; 32]).unwrap();
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o644)).unwrap();
        assert_ne!(install_key(dir.path()).unwrap(), [9u8; 32]);
        let key = install_key(dir.path()).unwrap();
        assert_eq!(install_key(dir.path()).unwrap(), key);
    }

    #[test]
    fn a_reply_over_the_byte_cap_is_refused_before_it_is_copied() {
        let memory = [7u8; 16];
        assert_eq!(reply_bytes(&memory, [4, 3]).unwrap(), &[7, 7, 7]);
        let refused = reply_bytes(&memory, [4, u32::MAX]).unwrap_err().to_string();
        assert_eq!(
            refused,
            "the plugin's reply is 4294967295 bytes; replies over 67108864 bytes are refused, so return less (a smaller overlay or fewer items)"
        );
        assert_eq!(reply_bytes(&memory, [0, 3]).unwrap_err().to_string(), "the plugin returned no reply");
    }
}
