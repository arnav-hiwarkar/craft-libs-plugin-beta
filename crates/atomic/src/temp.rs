use std::sync::atomic::{AtomicU64, Ordering};

const SUFFIX: &str = ".craft_tmp";
const EARLIER_SUFFIXES: [&str; 2] = [SUFFIX, ".pcsave.tmp"];
const STEM_BYTES: usize = 64;

pub fn temp_name(file: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}.{}-{n}{SUFFIX}", stem(file), pid())
}

fn stem(file: &str) -> &str {
    let mut end = file.len().min(STEM_BYTES);
    while !file.is_char_boundary(end) {
        end -= 1;
    }
    file.get(..end).unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
fn pid() -> u32 {
    std::process::id()
}

#[cfg(target_arch = "wasm32")]
fn pid() -> u32 {
    0
}

pub fn temp_owner(name: &str) -> Option<u32> {
    let rest = EARLIER_SUFFIXES.iter().find_map(|suffix| name.strip_suffix(suffix))?;
    let (stem, ids) = rest.rsplit_once('.')?;
    let (pid, n) = ids.split_once('-')?;
    let stem = stem.strip_prefix('.').unwrap_or(stem);
    let decimal = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && (s == "0" || !s.starts_with('0'));
    if stem.is_empty() || stem.chars().count() > STEM_BYTES || !decimal(pid) || !decimal(n) || n.parse::<u64>().is_err() {
        return None;
    }
    pid.parse().ok()
}
