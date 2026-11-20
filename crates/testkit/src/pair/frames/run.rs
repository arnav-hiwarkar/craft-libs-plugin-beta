use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::path::Path;

use super::{Block, Gate, Heading, Identity, Probe, RunRecord, Section, SeriesError, Workload, ident, records, verdicts};

pub const DEFAULT_REFRESH_HZ: NonZeroU32 = match NonZeroU32::new(60) {
    Some(hz) => hz,
    None => NonZeroU32::MIN,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opts {
    pub workloads: Vec<String>,
    pub units: u32,
    pub frames: u32,
    pub refresh_hz: NonZeroU32,
    pub night: Option<String>,
    pub runner_image: Option<String>,
    pub skip_build: bool,
}

fn positive(value: Option<&&str>, flag: &str) -> Result<u32, String> {
    value.and_then(|v| v.parse::<u32>().ok()).filter(|v| *v > 0).ok_or_else(|| format!("{flag} needs a whole number above 0"))
}

impl Opts {
    pub fn parse(args: &[&str], workloads: &[&str]) -> Result<Self, String> {
        let mut o = Opts {
            workloads: Vec::new(),
            units: 3,
            frames: 3600,
            refresh_hz: DEFAULT_REFRESH_HZ,
            night: None,
            runner_image: None,
            skip_build: false,
        };
        let mut it = args.iter();
        while let Some(a) = it.next() {
            match *a {
                "--workload" => {
                    let name =
                        it.next().filter(|n| workloads.contains(n)).ok_or_else(|| format!("--workload needs one of {}", workloads.join(", ")))?;
                    o.workloads.push((*name).to_owned());
                }
                "--units" => o.units = positive(it.next(), "--units")?,
                "--frames" => o.frames = positive(it.next(), "--frames")?,
                "--refresh-hz" => o.refresh_hz = NonZeroU32::new(positive(it.next(), "--refresh-hz")?).unwrap_or(DEFAULT_REFRESH_HZ),
                "--night" => o.night = Some(it.next().ok_or("--night needs an id")?.to_string()),
                "--runner-image" => o.runner_image = Some(it.next().ok_or("--runner-image needs an id")?.to_string()),
                "--skip-build" => o.skip_build = true,
                other => return Err(format!("unknown option `{other}`")),
            }
        }
        if o.frames < 2 {
            return Err("--frames needs at least 2: an interval is between two frames".into());
        }
        if o.workloads.is_empty() {
            o.workloads = workloads.iter().map(|w| (*w).to_owned()).collect();
        }
        Ok(o)
    }
}

impl Identity {
    pub fn of(o: &Opts, run: String, commit: Option<&str>, dirty: bool, fw_rev: Option<&str>, date: &str) -> Result<Self, SeriesError> {
        let commit = commit.filter(|c| !c.is_empty()).unwrap_or("unknown");
        let image = match (std::env::var("ImageOS"), std::env::var("ImageVersion")) {
            (Ok(os), Ok(version)) => format!("{os}-{version}"),
            _ => "local".into(),
        };
        Ok(Identity {
            run,
            app_pin: ident(&if dirty { format!("{commit}+dirty") } else { commit.to_owned() })?,
            fw_rev: ident(fw_rev.unwrap_or("unknown"))?,
            os: ident(&format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH))?,
            runner_image: ident(o.runner_image.as_deref().unwrap_or(&image))?,
            night: ident(o.night.as_deref().unwrap_or(date.get(..10).unwrap_or(date)))?,
            refresh_hz: o.refresh_hz,
        })
    }
}

pub fn load_label(contended: bool) -> &'static str {
    if contended { "contended" } else { "idle" }
}

pub fn clear(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path).or_else(|_| std::fs::remove_dir_all(path)) {
        Err(e) if path.exists() => Err(format!("remove {}: {e}", path.display())),
        _ => Ok(()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Measured<const N: usize> {
    workload: Workload,
    probe: Probe<N>,
    records: Vec<RunRecord>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run<const N: usize> {
    measured: BTreeMap<String, Measured<N>>,
}

impl<const N: usize> Run<N> {
    pub fn add(&mut self, id: &Identity, name: &str, workload: Workload, block: Block, probe: &Probe<N>, gates: &[Gate]) -> Result<(), SeriesError> {
        let block_records = records(id, name, &workload, block, probe, gates)?;
        let m = self.measured.entry(name.to_owned()).or_insert_with(|| Measured {
            workload: workload.clone(),
            probe: Probe::default(),
            records: Vec::new(),
        });
        if m.workload != workload {
            return Err(SeriesError::WorkloadChanged(name.to_owned()));
        }
        m.probe.merge(probe);
        m.records.extend(block_records);
        Ok(())
    }

    pub fn records(&self) -> Vec<RunRecord> {
        self.measured.values().flat_map(|m| m.records.iter().cloned()).collect()
    }

    pub fn summary(&self, heading: &Heading<'_>, stages: [&str; N]) -> Result<String, SeriesError> {
        let mut summary = heading.to_string();
        for (name, m) in &self.measured {
            let verdicts = verdicts(&m.records)?;
            let section = Section { name, workload: &m.workload, stages, probe: &m.probe, verdicts: &verdicts };
            summary.push_str(&format!("{section}\n"));
        }
        Ok(summary)
    }
}
