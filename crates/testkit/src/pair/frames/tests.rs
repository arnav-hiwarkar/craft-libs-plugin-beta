use std::num::NonZeroU32;
use std::time::Duration;

use super::super::{Arm, Count, Doubt, Gate, Verdict, Workload, parse_records, print_records};
use super::{
    Block, Cadence, DEFAULT_REFRESH_HZ, Frame, Heading, Identity, Opts, Output, Probe, Provenance, Run, Section, SeriesError, blocks, cadence, ident,
    millis, records, verdicts,
};

const FRAME_GATES: [Gate; 5] = [Gate::OverheadCpu, Gate::OverheadInterval, Gate::UiEqMissed, Gate::UiEqLongStalls, Gate::UiEqLatency];

fn hz(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).unwrap()
}

fn ms(n: f64) -> Duration {
    millis(n).unwrap()
}

fn frame<const N: usize>(start: f64, stages: [f64; N]) -> Frame<N> {
    Frame { start: ms(start), stages: stages.map(ms), latency: None, playback: Count::default(), seek: None }
}

fn identity() -> Identity {
    Identity {
        run: "r1".into(),
        app_pin: ident("359051359").unwrap(),
        fw_rev: ident("bafb4522").unwrap(),
        os: ident("macos-aarch64").unwrap(),
        runner_image: ident("macos15-20251110.1").unwrap(),
        night: ident("2026-11-21").unwrap(),
        refresh_hz: hz(60),
    }
}

fn workload() -> Workload {
    Workload {
        fixture_hash: ident("811531d151db2281").unwrap(),
        mode: ident("native").unwrap(),
        backend: ident("cpu").unwrap(),
        load: ident("idle").unwrap(),
        scale: ident("3000x2000@1920x1080").unwrap(),
    }
}

fn paint() -> Vec<Frame<6>> {
    let mut frames = vec![
        frame(0.0, [0.5, 0.25, 0.0, 0.0, 1.0, 0.25]),
        frame(16.667, [0.5, 0.25, 0.0, 0.0, 2.0, 0.25]),
        frame(33.333, [0.5, 0.25, 0.0, 0.0, 2.0, 0.25]),
        frame(75.0, [0.5, 2.0, 30.0, 1.0, 6.0, 0.5]),
    ];
    frames[1].latency = Some(ms(12.0));
    frames
}

fn paint_records(block: Block) -> Vec<super::RunRecord> {
    records(&identity(), "paint", &workload(), block, &Probe::of(&paint(), hz(60)), &FRAME_GATES).unwrap()
}

#[test]
fn missed_frames_and_long_stalls_follow_the_refresh_period() {
    let at = |rate: u32, interval: f64| cadence(ms(interval), hz(rate));
    assert_eq!(at(60, 16.667), Cadence::OnTime);
    assert_eq!(at(60, 24.999_999), Cadence::OnTime);
    assert_eq!(at(60, 25.0), Cadence::OnTime);
    assert_eq!(at(60, 25.000_001), Cadence::Missed);
    assert_eq!(at(60, 66.666_666), Cadence::Missed);
    assert_eq!(at(60, 66.666_667), Cadence::LongStall);
    assert_eq!(at(120, 12.5), Cadence::OnTime);
    assert_eq!(at(120, 12.500_001), Cadence::Missed);
    assert_eq!(at(120, 50.0), Cadence::Missed);
    assert_eq!(at(120, 50.000_001), Cadence::LongStall);
    assert_eq!(at(240, 49.0), Cadence::Missed);
    assert_eq!(at(30, 60.0), Cadence::Missed);
    assert_eq!(at(30, 133.333_333), Cadence::Missed);
    assert_eq!(at(30, 133.333_334), Cadence::LongStall);
}

#[test]
fn milliseconds_round_to_the_nanosecond_and_reject_what_no_duration_holds() {
    assert_eq!(millis(16.666_666_6), Some(Duration::from_nanos(16_666_667)));
    assert_eq!(millis(0.0), Some(Duration::ZERO));
    assert_eq!([millis(-0.001), millis(f64::NAN), millis(f64::INFINITY), millis(2e13)], [None; 4]);
}

#[test]
fn a_probe_counts_missed_frames_and_long_stalls_over_its_intervals() {
    let frames: Vec<Frame<1>> = [0.0, 16.0, 32.0, 60.0, 76.0, 176.0].into_iter().map(|t| frame(t, [1.0])).collect();
    let probe = Probe::of(&frames, hz(60));
    assert_eq!((probe.intervals, probe.missed, probe.stalls), (5, 2, 1));
    assert_eq!((probe.interval.count(), probe.cpu.count(), probe.latency.count()), (5, 6, 0));
}

#[test]
fn a_frame_that_starts_before_the_previous_one_gives_no_interval() {
    let frames: Vec<Frame<1>> = [0.0, 16.0, 10.0, 26.0].into_iter().map(|t| frame(t, [1.0])).collect();
    let probe = Probe::of(&frames, hz(60));
    assert_eq!((probe.intervals, probe.interval.count(), probe.missed), (2, 2, 0));
}

#[test]
fn the_frame_total_is_the_sum_of_its_stages() {
    let probe = Probe::of(&[frame(0.0, [3.0, 4.5, 0.5])], hz(60));
    assert_eq!(probe.cpu, Probe::of(&[frame(0.0, [8.0])], hz(60)).cpu);
    assert_eq!(probe.stages.iter().map(|stage| stage.count()).collect::<Vec<_>>(), [1, 1, 1]);
}

#[test]
fn a_frame_series_emits_exact_record_lines() {
    let text = print_records(&paint_records(Block { unit: 2, position: 1, arm: Arm::AA }));
    let head = "craft-pair/1 test_id=CONF-calibrate run_id=r1.paint.2.1.";
    let identity = "app_pin=359051359 fw_rev=bafb4522 os=macos-aarch64 runner_image=macos15-20251110.1 night=2026-11-21 \
                    fixture_hash=811531d151db2281 mode=native backend=cpu load=idle scale=3000x2000@1920x1080 block=2 arm=AA";
    let line = |gate: &str, samples: &str| format!("{head}{gate} {identity} gate={gate} refresh_hz=60 samples={samples}\n");
    let expected = [
        line("OverheadCpu", "hist:763:1,796:2,1037:1"),
        line("OverheadInterval", "hist:960:2,1040:1"),
        line("UiEqMissed", "count:1/3"),
        line("UiEqLongStalls", "count:0/3"),
        line("UiEqLatency", "hist:924:1"),
    ]
    .concat();
    assert_eq!(text, expected);
}

#[test]
fn playback_frames_give_late_counts_and_seek_latencies() {
    let mut frames: Vec<Frame<1>> = [0.0, 16.0, 33.0].into_iter().map(|t| frame(t, [1.0])).collect();
    frames[0].playback = Count::new(1, 2).unwrap();
    frames[1].playback = Count::new(0, 1).unwrap();
    frames[1].seek = Some(ms(40.0));
    frames[2].seek = Some(ms(20.0));
    let probe = Probe::of(&frames, hz(60));
    let block = Block { unit: 1, position: 0, arm: Arm::A };
    let samples: Vec<String> = records(&identity(), "scrub", &workload(), block, &probe, &[Gate::PlaybackLate, Gate::PlaybackSeek])
        .unwrap()
        .iter()
        .map(|record| format!("{:?} {}", record.gate, record.samples))
        .collect();
    assert_eq!(samples, ["PlaybackLate count:1/3", "PlaybackSeek hist:973:1,1037:1"]);
}

#[test]
fn a_gate_no_frame_series_measures_is_an_error() {
    let probe = Probe::of(&paint(), hz(60));
    let block = Block { unit: 1, position: 0, arm: Arm::A };
    assert_eq!(records(&identity(), "paint", &workload(), block, &probe, &[Gate::UiEqCpu]), Err(SeriesError::Unmeasured(Gate::UiEqCpu)));
    let long = "x".repeat(130);
    assert_eq!(
        records(&identity(), &long, &workload(), block, &probe, &FRAME_GATES).map(|records| records.len()),
        Err(SeriesError::Ident(format!("r1.{long}.1.0.OverheadCpu")))
    );
}

#[test]
fn records_round_trip_through_the_pair_reader() {
    let records: Vec<_> = blocks(2).flat_map(paint_records).collect();
    assert_eq!(records.len(), 40);
    assert_eq!(parse_records(&print_records(&records)), Ok(records));
}

#[test]
fn identical_arms_pass_the_latency_gates_and_three_frames_leave_rates_inconclusive() {
    let records: Vec<_> = blocks(3).flat_map(paint_records).collect();
    let noisy = Verdict::Inconclusive(Doubt::Noisy);
    assert_eq!(
        verdicts(&records),
        Ok(vec![
            (Gate::UiEqLatency, Verdict::Pass),
            (Gate::UiEqMissed, noisy),
            (Gate::UiEqLongStalls, noisy),
            (Gate::OverheadCpu, Verdict::Pass),
            (Gate::OverheadInterval, Verdict::Pass),
        ])
    );
}

#[test]
fn abba_blocks_label_the_same_binary_a_and_aa() {
    let labels: Vec<(u32, usize, Arm)> = blocks(2).map(|b| (b.unit, b.position, b.arm)).collect();
    assert_eq!(
        labels,
        [(1, 0, Arm::A), (1, 1, Arm::AA), (1, 2, Arm::AA), (1, 3, Arm::A), (2, 0, Arm::A), (2, 1, Arm::AA), (2, 2, Arm::AA), (2, 3, Arm::A)]
    );
}

#[test]
fn the_summary_shows_each_stage_and_only_the_measured_metrics() {
    let mut frames: Vec<Frame<2>> = [0.0, 16.0, 60.0].into_iter().map(|t| frame(t, [0.5, 12.0])).collect();
    frames[1].playback = Count::new(1, 4).unwrap();
    frames[2].seek = Some(ms(12.0));
    let probe = Probe::of(&frames, hz(60));
    let verdicts =
        [(Gate::OverheadCpu, Verdict::Pass), (Gate::PlaybackLate, Verdict::Fail), (Gate::PlaybackSeek, Verdict::Inconclusive(Doubt::Noisy))];
    let section = Section { name: "scrub", workload: &workload(), stages: ["input", "ui_build"], probe: &probe, verdicts: &verdicts }.to_string();
    assert_eq!(
        section,
        "### scrub (cpu, 3000x2000@1920x1080)\n\n\
         | Metric | n | p50 ms | p95 ms | p99 ms |\n|---|---:|---:|---:|---:|\n\
         | C input | 3 | 0.50 | 0.50 | 0.50 |\n\
         | C ui_build | 3 | 12.0 | 12.0 | 12.0 |\n\
         | C frame | 3 | 12.5 | 12.5 | 12.5 |\n\
         | I | 2 | 16.1 | 43.8 | 43.8 |\n\
         | P seek | 1 | 12.0 | 12.0 | 12.0 |\n\
         \nMissed frames: 1 of 2. Long stalls: 0 of 2. Late playback frames: 1 of 4.\n\n\
         | Gate | A/AA verdict |\n|---|---|\n\
         | OverheadCpu | Pass |\n| PlaybackLate | **Fail** |\n| PlaybackSeek | Inconclusive (Noisy) |\n"
    );
    let heading = Heading { id: &identity(), load: "idle", units: 3, frames: 600, records: "target/perf/pair.records" }.to_string();
    assert_eq!(
        heading,
        "## Paired frame probe: A/AA calibration (`CONF-calibrate`)\n\n\
         - Night `2026-11-21`, runner image `macos15-20251110.1`, os `macos-aarch64`, app `359051359`, framework `bafb4522`, load `idle`\n\
         - 3 ABBA units of 600 frames per block at 60 Hz; records in `target/perf/pair.records`\n\n"
    );
}

#[test]
fn starts_less_than_a_nanosecond_apart_give_a_one_nanosecond_interval_below_the_histogram() {
    let frames: Vec<Frame<1>> = [0.000_000_4, 0.000_000_8].into_iter().map(|t| frame(t, [1.0])).collect();
    assert_eq!(frames.iter().map(|f| f.start).collect::<Vec<_>>(), [Duration::ZERO, Duration::from_nanos(1)]);
    let probe = Probe::of(&frames, hz(60));
    assert_eq!((probe.intervals, probe.missed, probe.samples(Gate::OverheadInterval).map(|s| s.to_string())), (1, 0, Some("hist:0:1".to_owned())));
}

#[test]
fn options_have_defaults_and_reject_what_cannot_run() {
    let workloads = ["hover", "paint"];
    let o = Opts::parse(&[], &workloads, None).unwrap();
    assert_eq!(
        o,
        Opts {
            workloads: vec!["hover".into(), "paint".into()],
            units: 3,
            frames: 3600,
            refresh_hz: DEFAULT_REFRESH_HZ,
            night: None,
            runner_image: "local".into(),
            skip_build: false
        }
    );
    assert_eq!(Opts::parse(&[], &workloads, Some("macos15-20251110.1")).unwrap().runner_image, "macos15-20251110.1");
    let args =
        ["--workload", "paint", "--units", "5", "--frames", "600", "--refresh-hz", "120", "--night", "n1", "--runner-image", "img", "--skip-build"];
    let o = Opts::parse(&args, &workloads, Some("hosted")).unwrap();
    assert_eq!((o.workloads, o.units, o.frames, o.refresh_hz.get()), (vec!["paint".to_owned()], 5, 600, 120));
    assert_eq!((o.night.as_deref(), o.runner_image.as_str(), o.skip_build), (Some("n1"), "img", true));
    for bad in [&["--units", "0"][..], &["--frames", "x"], &["--frames", "1"], &["--refresh-hz", "0"], &["--workload", "scroll"], &["--quick"]] {
        assert!(Opts::parse(bad, &workloads, None).is_err(), "{bad:?}");
    }
    assert_eq!(Opts::parse(&["--quick"], &workloads, None), Err("unknown option `--quick`".into()));
}

#[test]
fn an_identity_names_a_dirty_or_unknown_commit_and_defaults_the_night_to_the_date() {
    let o = Opts::parse(&["--runner-image", "img"], &["a"], None).unwrap();
    let provenance = Provenance { run: "r1".into(), commit: Some("abc"), dirty: true, framework: Some("bafb4522"), date: "2026-11-21 10:00 UTC" };
    let id = Identity::of(&o, provenance.clone()).unwrap();
    let os = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    assert_eq!(
        (id.app_pin.as_str(), id.fw_rev.as_str(), id.os.as_str(), id.runner_image.as_str(), id.night.as_str()),
        ("abc+dirty", "bafb4522", os.as_str(), "img", "2026-11-21")
    );
    let unknown = Provenance { commit: Some(""), dirty: false, framework: None, ..provenance.clone() };
    let id = Identity::of(&Opts { night: Some("n2".into()), ..o.clone() }, unknown).unwrap();
    assert_eq!((id.app_pin.as_str(), id.fw_rev.as_str(), id.night.as_str()), ("unknown", "unknown", "n2"));
    assert_eq!(Identity::of(&o, Provenance { commit: Some("a b"), ..provenance }), Err(SeriesError::Ident("a b+dirty".into())));
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("craft-testkit-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn preparing_the_output_removes_stale_records_summary_and_block_reports() {
    let dir = scratch("prepare");
    let out = Output::new(dir.clone());
    std::fs::create_dir_all(out.blocks()).unwrap();
    std::fs::write(out.records(), "craft-pair/1 stale\n").unwrap();
    std::fs::write(out.summary(), "stale").unwrap();
    std::fs::write(out.blocks().join("paint-1-0.json"), "{}").unwrap();
    out.prepare().unwrap();
    let left: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!((left, std::fs::read_dir(out.blocks()).unwrap().count()), (vec!["pair".to_owned()], 0));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_run_measures_each_workload_in_each_abba_block_and_writes_its_records_and_summary() {
    let o = Opts::parse(&["--units", "2", "--frames", "4"], &["hover", "paint"], None).unwrap();
    let probe = Probe::of(&paint(), hz(60));
    let mut calls = Vec::new();
    let run = Run::drive(
        &o,
        &identity(),
        |_| FRAME_GATES.to_vec(),
        |name, block| {
            calls.push(format!("{name}.{}.{:?}", block.unit, block.arm));
            Ok((workload(), probe.clone()))
        },
    )
    .unwrap();
    assert_eq!(
        calls,
        [
            "hover.1.A",
            "paint.1.A",
            "hover.1.AA",
            "paint.1.AA",
            "hover.1.AA",
            "paint.1.AA",
            "hover.1.A",
            "paint.1.A",
            "hover.2.A",
            "paint.2.A",
            "hover.2.AA",
            "paint.2.AA",
            "hover.2.AA",
            "paint.2.AA",
            "hover.2.A",
            "paint.2.A"
        ]
    );
    assert_eq!(run.records().len(), 80);
    let dir = scratch("write");
    let out = Output::new(dir.clone());
    out.prepare().unwrap();
    let summary = run.write(&out, &identity(), "idle", &o, ["input", "ui_build", "command", "damage", "composite", "upload"]).unwrap();
    assert_eq!(parse_records(&std::fs::read_to_string(out.records()).unwrap()), Ok(run.records()));
    assert_eq!(std::fs::read_to_string(out.summary()).unwrap(), summary);
    assert!(summary.contains(&format!("- 2 ABBA units of 4 frames per block at 60 Hz; records in `{}`\n", out.records().display())), "{summary}");
    assert_eq!(summary.matches("\n### ").count(), 2);
    std::fs::remove_dir_all(&dir).unwrap();
    let failing = Run::<6>::drive(&o, &identity(), |_| FRAME_GATES.to_vec(), |name, _| Err(format!("{name} failed")));
    assert_eq!(failing, Err("hover failed".into()));
}

#[test]
fn a_run_pools_each_workload_over_its_blocks_and_refuses_a_changed_workload() {
    let probe = Probe::of(&paint(), hz(60));
    let mut run = Run::default();
    for block in blocks(3) {
        run.add(&identity(), "paint", workload(), block, &probe, &FRAME_GATES).unwrap();
    }
    assert_eq!(run.records().len(), 60);
    let changed = Workload { load: ident("contended").unwrap(), ..workload() };
    let block = Block { unit: 4, position: 0, arm: Arm::A };
    assert_eq!(run.add(&identity(), "paint", changed, block, &probe, &FRAME_GATES), Err(SeriesError::WorkloadChanged("paint".into())));
    assert_eq!(run.records().len(), 60);
    let heading = Heading { id: &identity(), load: "idle", units: 3, frames: 4, records: "pair.records" };
    let stages = ["input", "ui_build", "command", "damage", "composite", "upload"];
    assert_eq!(
        run.summary(&heading, stages).unwrap(),
        "## Paired frame probe: A/AA calibration (`CONF-calibrate`)\n\n\
         - Night `2026-11-21`, runner image `macos15-20251110.1`, os `macos-aarch64`, app `359051359`, framework `bafb4522`, load `idle`\n\
         - 3 ABBA units of 4 frames per block at 60 Hz; records in `pair.records`\n\n\
         ### paint (cpu, 3000x2000@1920x1080)\n\n\
         | Metric | n | p50 ms | p95 ms | p99 ms |\n|---|---:|---:|---:|---:|\n\
         | C input | 48 | 0.50 | 0.50 | 0.50 |\n\
         | C ui_build | 48 | 0.25 | 2.01 | 2.01 |\n\
         | C command | 48 | < 0.001 | 30.0 | 30.0 |\n\
         | C damage | 48 | < 0.001 | 1.00 | 1.00 |\n\
         | C composite | 48 | 2.01 | 6.00 | 6.00 |\n\
         | C upload | 48 | 0.25 | 0.50 | 0.50 |\n\
         | C frame | 48 | 3.00 | 40.1 | 40.1 |\n\
         | I | 36 | 16.7 | 41.7 | 41.7 |\n\
         | L | 12 | 12.0 | 12.0 | 12.0 |\n\
         \nMissed frames: 12 of 36. Long stalls: 0 of 36.\n\n\
         | Gate | A/AA verdict |\n|---|---|\n\
         | UiEqLatency | Pass |\n| UiEqMissed | Inconclusive (Noisy) |\n| UiEqLongStalls | Inconclusive (Noisy) |\n\
         | OverheadCpu | Pass |\n| OverheadInterval | Pass |\n\n"
    );
}
