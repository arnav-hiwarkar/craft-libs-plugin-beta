use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use super::super::print_records;
use super::{Block, Gate, Heading, Identity, Probe, RunRecord, Section, SeriesError, Workload, blocks, ident, records, verdicts};

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
    pub runner_image: String,
    pub skip_build: bool,
}

fn positive(value: Option<&&str>, flag: &str) -> Result<u32, String> {
    value.and_then(|v| v.parse::<u32>().ok()).filter(|v| *v > 0).ok_or_else(|| format!("{flag} needs a whole number above 0"))
}

impl Opts {
    pub fn parse(args: &[&str], workloads: &[&str], hosted_image: Option<&str>) -> Result<Self, String> {
        let mut o = Opts {
            workloads: Vec::new(),
            units: 3,
            frames: 3600,
            refresh_hz: DEFAULT_REFRESH_HZ,
            night: None,
            runner_image: hosted_image.unwrap_or("local").to_owned(),
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
                "--runner-image" => o.runner_image = it.next().ok_or("--runner-image needs an id")?.to_string(),
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provenance<'a> {
    pub run: String,
    pub commit: Option<&'a str>,
    pub dirty: bool,
    pub framework: Option<&'a str>,
    pub date: &'a str,
}

impl Identity {
    pub fn of(o: &Opts, p: Provenance<'_>) -> Result<Self, SeriesError> {
        let commit = p.commit.filter(|c| !c.is_empty()).unwrap_or("unknown");
        Ok(Identity {
            run: p.run,
            app_pin: ident(&if p.dirty { format!("{commit}+dirty") } else { commit.to_owned() })?,
            fw_rev: ident(p.framework.unwrap_or("unknown"))?,
            os: ident(&format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH))?,
            runner_image: ident(&o.runner_image)?,
            night: ident(o.night.as_deref().unwrap_or(p.date.get(..10).unwrap_or(p.date)))?,
            refresh_hz: o.refresh_hz,
        })
    }
}

pub fn load_label(contended: bool) -> &'static str {
    if contended { "contended" } else { "idle" }
}

fn clear(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path).or_else(|_| std::fs::remove_dir_all(path)) {
        Err(e) if path.exists() => Err(format!("remove {}: {e}", path.display())),
        _ => Ok(()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    dir: PathBuf,
}

impl Output {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn blocks(&self) -> PathBuf {
        self.dir.join("pair")
    }

    pub fn records(&self) -> PathBuf {
        self.dir.join("pair.records")
    }

    pub fn summary(&self) -> PathBuf {
        self.dir.join("pair-summary.md")
    }

    pub fn prepare(&self) -> Result<(), String> {
        for stale in [self.records(), self.summary(), self.blocks()] {
            clear(&stale)?;
        }
        std::fs::create_dir_all(self.blocks()).map_err(|e| format!("create {}: {e}", self.blocks().display()))
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

    pub fn drive(
        o: &Opts,
        id: &Identity,
        gates: impl Fn(&str) -> Vec<Gate>,
        mut measure: impl FnMut(&str, Block) -> Result<(Workload, Probe<N>), String>,
    ) -> Result<Self, String> {
        let mut run = Run::default();
        for block in blocks(o.units) {
            for name in &o.workloads {
                let (workload, probe) = measure(name, block)?;
                run.add(id, name, workload, block, &probe, &gates(name)).map_err(|e| e.to_string())?;
            }
        }
        Ok(run)
    }

    pub fn write(&self, out: &Output, id: &Identity, load: &str, o: &Opts, stages: [&str; N]) -> Result<String, String> {
        let all = self.records();
        let records = out.records();
        std::fs::write(&records, print_records(&all)).map_err(|e| format!("write {}: {e}", records.display()))?;
        let shown = records.display().to_string();
        let summary = self.summary(&Heading { id, load, units: o.units, frames: o.frames, records: &shown }, stages).map_err(|e| e.to_string())?;
        std::fs::write(out.summary(), &summary).map_err(|e| format!("write {}: {e}", out.summary().display()))?;
        println!("\n{summary}");
        println!("wrote {shown} ({} records)", all.len());
        Ok(summary)
    }
}
