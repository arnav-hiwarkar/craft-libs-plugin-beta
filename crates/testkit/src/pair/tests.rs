use std::num::NonZeroU32;
use std::time::Duration;

use super::bootstrap::SplitMix64;
use super::gate::GATES;
use super::{
    AA_VERDICTS, Arm, Calibration, Controls, Count, Decision, Doubt, Eligibility, Gate, Hist, Ident, Kind, LOOKS, LineError, MAX_POOL_BYTES,
    MAX_RECORDS, Measurement, PairError, Problem, Quantile, Reading, RunRecord, Samples, Unit, Verdict, Workload, calibration_verdict, decide,
    parse_records, print_records,
};

const IMAGE: &str = "macos-15-arm64";

fn id(text: &str) -> Ident {
    Ident::new(text).unwrap()
}

fn hz(rate: u32) -> NonZeroU32 {
    NonZeroU32::new(rate).unwrap()
}

fn uniform(rng: &mut SplitMix64) -> f64 {
    ((rng.next() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
}

fn gaussian(rng: &mut SplitMix64) -> f64 {
    let (u, v) = (uniform(rng), uniform(rng));
    (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
}

fn workload() -> Workload {
    Workload { fixture_hash: id("sha256:9f2c"), mode: id("wasm"), backend: id("metal"), load: id("idle"), scale: id("6mp") }
}

fn record(night: &str, block: u32, position: usize, arm: Arm, gate: Gate, samples: Samples) -> RunRecord {
    RunRecord {
        test_id: id("CONF-aa-sequential"),
        run_id: id(&format!("{night}-{block}-{position}")),
        app_pin: id("pc-359051359"),
        fw_rev: id("cl-9b33fe822"),
        os: id("macos-15"),
        runner_image: id(IMAGE),
        night: id(night),
        workload: workload(),
        block,
        arm,
        gate,
        refresh_hz: hz(120),
        samples,
    }
}

fn constant(values: &[(u64, usize)]) -> Samples {
    let mut hist = Hist::new();
    for (nanos, times) in values {
        for _ in 0..*times {
            hist.record(Duration::from_nanos(*nanos));
        }
    }
    Samples::Latency(hist)
}

fn quartets(night: &str, units: u32, gate: Gate, sides: impl Fn(u32, Arm) -> Samples) -> Vec<RunRecord> {
    let mut records = Vec::new();
    for block in 0..units {
        for (position, arm) in [Arm::A, Arm::B].into_iter().enumerate() {
            records.push(record(night, block, position, arm, gate, sides(block, arm)));
        }
    }
    records
}

struct Load {
    median_ns: f64,
    spread: f64,
    unit_drift: f64,
    block_noise: f64,
    frames: usize,
}

const STEADY: Load = Load { median_ns: 2_440_000.0, spread: 0.3, unit_drift: 0.05, block_noise: 0.02, frames: 500 };

fn night(rng: &mut SplitMix64, load: &Load, name: &str, units: u32, shift: Duration) -> Vec<RunRecord> {
    let mut records = Vec::new();
    for block in 0..units {
        let drift = (load.unit_drift * gaussian(rng)).exp();
        for (position, arm) in [Arm::A, Arm::B, Arm::B, Arm::A].into_iter().enumerate() {
            let noise = (load.block_noise * gaussian(rng)).exp();
            let added = if arm == Arm::B { shift.as_nanos() as f64 } else { 0.0 };
            let mut samples = Hist::new();
            for _ in 0..load.frames {
                let nanos = load.median_ns * drift * noise * (load.spread * gaussian(rng)).exp() + added;
                samples.record(Duration::from_nanos(nanos as u64));
            }
            records.push(record(name, block, position, arm, Gate::UiEqCpu, Samples::Latency(samples)));
        }
    }
    records
}

fn calibration(gate: Gate, aa: Vec<Verdict>) -> Calibration {
    Calibration {
        gate,
        runner_image: id(IMAGE),
        aa,
        controls: Controls { injected_delay: Verdict::Fail, gpu_heavy: Verdict::Fail, null: Verdict::Pass },
    }
}

fn eligible() -> Vec<Calibration> {
    GATES.into_iter().map(|gate| calibration(gate, vec![Verdict::Pass; AA_VERDICTS])).collect()
}

fn measured(records: &[RunRecord]) -> Measurement {
    match decide(records, &eligible()) {
        Ok(Decision::Measured(measurement)) => measurement,
        other => panic!("not measured: {other:?}"),
    }
}

#[test]
fn hist_quantiles_stay_within_one_percent_from_one_microsecond_to_sixty_seconds() {
    let mut rng = SplitMix64(7);
    let (low, high) = (1_000f64.ln(), 60_000_000_000f64.ln());
    let mut worst = 0f64;
    for _ in 0..40 {
        let samples: Vec<u64> = (0..1_000).map(|_| (low + uniform(&mut rng) * (high - low)).exp() as u64).collect();
        let (mut even, mut odd) = (Hist::new(), Hist::new());
        for (index, nanos) in samples.iter().enumerate() {
            let half = if index % 2 == 0 { &mut even } else { &mut odd };
            half.record(Duration::from_nanos(*nanos));
        }
        even.merge(&odd);
        let mut sorted = samples.clone();
        sorted.sort_unstable();
        for permyriad in [1, 10, 100, 2_500, 5_000, 9_000, 9_500, 9_900, 9_999, 10_000, 1 + (rng.next() % 10_000) as u16] {
            let rank = (sorted.len() * usize::from(permyriad)).div_ceil(10_000);
            let exact = sorted[rank - 1] as f64;
            let Some(Some(Reading::At(estimate))) = Quantile::from_permyriad(permyriad).map(|quantile| even.quantile(quantile)) else {
                panic!("quantile {permyriad} of in-range samples is not a value");
            };
            worst = worst.max((estimate.as_nanos() as f64 - exact).abs() / exact);
        }
    }
    assert!(worst <= 0.01, "worst relative quantile error {worst}");
    assert!(worst > 0.001, "the property never exercised bucket error: {worst}");
}

#[test]
fn hist_counts_samples_outside_its_range_and_keeps_its_edges() {
    let mut hist = Hist::new();
    for nanos in [0, 999, 1_000, 60_000_000_000, 60_000_000_001, u64::MAX] {
        hist.record(Duration::from_nanos(nanos));
    }
    hist.record(Duration::MAX);
    assert_eq!((hist.count(), hist.underflow(), hist.overflow()), (7, 2, 3));
    assert_eq!(hist.quantile(Quantile::P50), Some(Reading::At(Duration::from_nanos(59_861_106_688))));
    assert_eq!(hist.quantile(Quantile::from_permyriad(2_857).unwrap()), Some(Reading::Below));
    assert_eq!(hist.quantile(Quantile::from_permyriad(4_285).unwrap()), Some(Reading::At(Duration::from_nanos(1_004))));
    assert_eq!(hist.quantile(Quantile::P95), Some(Reading::Above));
    assert_eq!(Hist::new().quantile(Quantile::P50), None);
    assert_eq!((Quantile::from_permyriad(0), Quantile::from_permyriad(10_001)), (None, None));
}

#[test]
fn allow_follows_the_gate_table_floor_fraction_and_ceiling() {
    let cases: [(Gate, u64, u32, u64); 31] = [
        (Gate::UiEqCpu, 4_000_000, 120, 1_000_000),
        (Gate::UiEqCpu, 30_000_000, 120, 3_000_000),
        (Gate::UiEqGpu, 4_000_000, 120, 1_000_000),
        (Gate::UiEqGpu, 30_000_000, 120, 3_000_000),
        (Gate::UiEqLatency, 20_000_000, 60, 16_666_666),
        (Gate::UiEqLatency, 200_000_000, 60, 20_000_000),
        (Gate::UiEqExtractionLatency, 4_000_000, 120, 2_000_000),
        (Gate::UiEqExtractionLatency, 30_000_000, 120, 3_000_000),
        (Gate::UiLoadLatency, 20_000_000, 120, 8_333_333),
        (Gate::UiLoadLatency, 100_000_000, 120, 10_000_000),
        (Gate::UiLoadStrokeLatency, 4_000_000, 120, 2_000_000),
        (Gate::UiLoadStrokeLatency, 30_000_000, 120, 3_000_000),
        (Gate::OverheadCpu, 0, 120, 0),
        (Gate::OverheadCpu, 10_000_000, 120, 1_500_000),
        (Gate::OverheadInterval, 8_000_000, 120, 1_200_000),
        (Gate::PlaybackSeek, 100_000_000, 120, 50_000_000),
        (Gate::PlaybackSeek, 1_000_000_000, 120, 100_000_000),
        (Gate::CommandWallMedian, 40_000_000, 120, 50_000_000),
        (Gate::CommandWallMedian, 1_000_000_000, 120, 100_000_000),
        (Gate::CommandWallTail, 80_000_000, 120, 20_000_000),
        (Gate::CommandWallTail, 100_000_000, 120, 0),
        (Gate::CommandWallTail, 100_000_001, 120, 50_000_000),
        (Gate::CommandWallTail, 1_000_000_000, 120, 100_000_000),
        (Gate::ExportWall, 0, 120, 0),
        (Gate::ExportWall, 1_000_000_000, 120, 100_000_000),
        (Gate::ExportGpu, 2_000_000, 120, 200_000),
        (Gate::UiEqMissed, 90_000_000, 120, 5_000_000),
        (Gate::UiEqLongStalls, 90_000_000, 120, 100_000),
        (Gate::UiLoadMissed, 90_000_000, 120, 10_000_000),
        (Gate::PlaybackLate, 90_000_000, 120, 10_000_000),
        (Gate::PlaybackLate, 0, 120, 10_000_000),
    ];
    for (gate, base, rate, expected) in cases {
        assert_eq!(gate.kind().allow(base, hz(rate)), expected, "{gate:?} at {base}");
    }
    let units: Vec<Unit> = [Gate::UiEqCpu, Gate::UiEqMissed].map(|gate| gate.kind().unit()).to_vec();
    assert_eq!(units, [Unit::Nanoseconds, Unit::PartsPerBillion]);
}

#[test]
fn each_latency_gate_compares_its_own_quantile() {
    let tail = |arm: Arm| constant(&[(10_000_000, 900), (if arm == Arm::B { 40_000_000 } else { 20_000_000 }, 100)]);
    for gate in GATES.into_iter().filter(|gate| matches!(gate.kind(), Kind::Latency { .. })) {
        let expected = match gate.kind() {
            Kind::Latency { quantile, .. } if quantile == Quantile::P95 => 20_054_016,
            _ => 0,
        };
        let records = quartets("2026-11-21", 2, gate, |_, arm| tail(arm));
        assert_eq!(measured(&records).delta, expected, "{gate:?}");
    }
}

#[test]
fn the_same_records_in_any_order_give_the_same_decision() {
    let records = night(&mut SplitMix64(11), &STEADY, "2026-11-21", 6, Duration::from_micros(300));
    let mut reversed = records.clone();
    reversed.reverse();
    assert_eq!(decide(&records, &eligible()), decide(&reversed, &eligible()));
    assert_eq!(
        measured(&records),
        Measurement { unit: Unit::Nanoseconds, allow: 1_000_000, delta: 344_064, low: 62_465, high: 625_663, excluded_strata: 0 }
    );
}

#[test]
fn conf_power_twice_the_allowance_fails_and_a_null_shift_passes() {
    let shifted = night(&mut SplitMix64(21), &STEADY, "2026-11-21", 4, Duration::from_millis(2));
    let null = night(&mut SplitMix64(22), &STEADY, "2026-11-21", 4, Duration::ZERO);
    assert_eq!(calibration_verdict(&shifted), Ok(Verdict::Fail));
    assert_eq!(calibration_verdict(&null), Ok(Verdict::Pass));
}

#[test]
fn rate_gates_compare_event_rates_in_parts_per_billion() {
    let rates = |extra: u64| {
        quartets("2026-11-21", 3, Gate::UiEqMissed, move |block, arm| {
            let events = 100 + u64::from(block) * 5 + if arm == Arm::B { extra } else { 0 };
            Samples::Count(Count::new(events, 10_000).unwrap())
        })
    };
    let regressed = measured(&rates(200));
    assert_eq!((regressed.unit, regressed.allow, regressed.delta), (Unit::PartsPerBillion, 5_000_000, 20_000_000));
    assert_eq!(regressed.verdict(), Verdict::Fail);
    let close = measured(&rates(5));
    assert_eq!((close.delta, close.verdict()), (500_000, Verdict::Pass));
}

#[test]
fn renaming_runs_leaves_the_decision_bit_identical() {
    let records = night(&mut SplitMix64(12), &STEADY, "2026-11-21", 4, Duration::from_micros(300));
    let renamed: Vec<RunRecord> =
        records.iter().map(|record| RunRecord { run_id: id(&format!("renamed-{}", record.run_id)), ..record.clone() }).collect();
    assert_eq!(decide(&renamed, &eligible()), decide(&records, &eligible()));
    let mut repeated = records.clone();
    repeated[3].run_id = records[0].run_id.clone();
    assert_eq!(decide(&repeated, &eligible()), Err(PairError::DuplicateRun { run_id: records[0].run_id.clone() }));
}

#[test]
fn rate_intervals_carry_the_binomial_noise_of_few_events() {
    let stalls = |frames: u64, plugin_events: u64, units: u32| {
        quartets("2026-11-21", units, Gate::UiEqLongStalls, move |_, arm| {
            Samples::Count(Count::new(if arm == Arm::B { plugin_events } else { 0 }, frames).unwrap())
        })
    };
    let two_events = measured(&stalls(10_000, 1, 2));
    assert_eq!((two_events.allow, two_events.delta), (100_000, 100_000));
    assert_eq!(two_events.verdict(), Verdict::Inconclusive(Doubt::Noisy));
    let regressed = measured(&stalls(100_000, 50, 4));
    assert_eq!(regressed.delta, 500_000);
    assert_eq!(regressed.verdict(), Verdict::Fail);
}

#[test]
fn a_shift_equal_to_the_allowance_is_noisy_and_noise_never_widens_the_allowance() {
    let noisy = Load { block_noise: 0.2, ..STEADY };
    let records = night(&mut SplitMix64(31), &noisy, "2026-11-21", 4, Duration::from_millis(1));
    let measurement = measured(&records);
    assert_eq!((measurement.verdict(), measurement.allow), (Verdict::Inconclusive(Doubt::Noisy), 1_000_000));
    assert!(measurement.low <= 1_000_000 && measurement.high > 1_000_000, "{measurement:?}");
}

#[test]
fn constant_units_within_one_bucket_of_the_allowance_are_noisy() {
    let records = quartets("2026-11-21", 2, Gate::UiEqCpu, |_, arm| constant(&[(if arm == Arm::B { 2_990_000 } else { 2_000_000 }, 50)]));
    let measurement = measured(&records);
    assert_eq!(
        measurement,
        Measurement { unit: Unit::Nanoseconds, allow: 1_000_000, delta: 991_232, low: 966_656, high: 1_015_808, excluded_strata: 0 }
    );
    assert_eq!(measurement.verdict(), Verdict::Inconclusive(Doubt::Noisy));
}

#[test]
fn plugin_samples_above_sixty_seconds_still_fail() {
    let records = quartets("2026-11-21", 2, Gate::UiEqCpu, |_, arm| constant(&[(if arm == Arm::B { 61_000_000_000 } else { 2_000_000 }, 50)]));
    let measurement = measured(&records);
    assert_eq!((measurement.low, measurement.high), (59_997_984_768, i64::MAX));
    assert_eq!(measurement.verdict(), Verdict::Fail);
    let swapped = quartets("2026-11-21", 2, Gate::UiEqCpu, |_, arm| constant(&[(if arm == Arm::A { 61_000_000_000 } else { 2_000_000 }, 50)]));
    assert_eq!(decide(&swapped, &eligible()), Ok(Decision::Withheld(Doubt::Noisy)));
}

#[test]
fn unequal_strata_give_the_stratified_standard_error() {
    let events: [&[u64]; 2] = [&[0, 1_000], &[100, 105, 110, 115, 120, 125, 130, 135, 140, 145]];
    let mut records = Vec::new();
    for (stratum, night) in events.iter().zip(["2026-11-20", "2026-11-21"]) {
        records.extend(quartets(night, stratum.len() as u32, Gate::UiEqMissed, |block, arm| {
            let plugin = if arm == Arm::B { stratum[block as usize] } else { 0 };
            Samples::Count(Count::new(plugin, 10_000).unwrap())
        }));
    }
    let total = 12.0;
    let analytic = events
        .iter()
        .map(|stratum| {
            let rates: Vec<f64> = stratum.iter().map(|&events| events as f64 * 1e5).collect();
            let size = rates.len() as f64;
            let mean = rates.iter().sum::<f64>() / size;
            let variance = rates.iter().map(|rate| (rate - mean) * (rate - mean)).sum::<f64>() / (size - 1.0);
            (size / total) * (size / total) * variance / size
        })
        .sum::<f64>()
        .sqrt();
    let measurement = measured(&records);
    let bootstrap = (measurement.high - measurement.low) as f64 / 2.0 / 3.169;
    assert!((bootstrap / analytic - 1.0).abs() < 0.03, "bootstrap {bootstrap} against analytic {analytic}");
}

#[test]
fn a_stratum_of_one_unit_is_excluded_and_reported() {
    let mut records = night(&mut SplitMix64(71), &STEADY, "2026-11-20", 3, Duration::ZERO);
    let lone = night(&mut SplitMix64(72), &STEADY, "2026-11-21", 1, Duration::ZERO);
    assert_eq!(decide(&lone, &eligible()), Ok(Decision::Withheld(Doubt::Noisy)));
    records.extend(lone);
    let measurement = measured(&records);
    assert_eq!((measurement.excluded_strata, measurement.verdict()), (1, Verdict::Pass));
}

#[test]
fn conf_calibrate_eligibility_needs_59_clean_a_a_verdicts_and_behaving_controls() {
    let records = night(&mut SplitMix64(41), &STEADY, "2026-11-21", 2, Duration::ZERO);
    let decided = |calibrations: Vec<Calibration>| decide(&records, &calibrations);
    let clean = || vec![Verdict::Pass; AA_VERDICTS];
    let mut one_fail = clean();
    one_fail[30] = Verdict::Fail;
    let mut misbehaved = calibration(Gate::UiEqCpu, clean());
    misbehaved.controls.injected_delay = Verdict::Inconclusive(Doubt::Noisy);
    let mut other_image = calibration(Gate::UiEqCpu, clean());
    other_image.runner_image = id("macos-14-arm64");

    assert_eq!(calibration(Gate::UiEqCpu, vec![Verdict::Pass; 58]).eligibility(), Eligibility::TooFewAa(58));
    assert_eq!(calibration(Gate::UiEqCpu, one_fail.clone()).eligibility(), Eligibility::AaFailed(1));
    assert_eq!(misbehaved.eligibility(), Eligibility::ControlsMisbehaved);
    let no_runner = Ok(Decision::Withheld(Doubt::NoRunner));
    assert_eq!(decided(vec![calibration(Gate::UiEqCpu, vec![Verdict::Pass; 58])]), no_runner);
    assert_eq!(decided(vec![calibration(Gate::UiEqCpu, one_fail)]), no_runner);
    assert_eq!(decided(vec![misbehaved]), no_runner);
    assert_eq!(decided(vec![other_image, calibration(Gate::UiEqLatency, clean())]), no_runner);
    let mut inconclusive_aa = clean();
    inconclusive_aa[0] = Verdict::Inconclusive(Doubt::Noisy);
    assert!(matches!(decided(vec![calibration(Gate::UiEqCpu, inconclusive_aa)]), Ok(Decision::Measured(_))));
}

#[test]
fn a_sixth_night_is_over_budget() {
    let mut rng = SplitMix64(51);
    let nights: Vec<String> = (1..=LOOKS + 1).map(|day| format!("2026-11-{day:02}")).collect();
    let mut records: Vec<RunRecord> = Vec::new();
    for name in &nights[..LOOKS] {
        records.extend(night(&mut rng, &STEADY, name, 2, Duration::ZERO));
    }
    assert_eq!(decide(&records, &eligible()).map(|decision| decision.verdict()), Ok(Verdict::Pass));
    records.extend(night(&mut rng, &STEADY, &nights[LOOKS], 2, Duration::ZERO));
    assert_eq!(decide(&records, &eligible()), Ok(Decision::Withheld(Doubt::Budget)));
}

#[test]
fn pools_that_cannot_pair_are_errors() {
    let records = night(&mut SplitMix64(61), &Load { frames: 20, ..STEADY }, "2026-11-21", 2, Duration::ZERO);
    let without_last_b: Vec<RunRecord> = records.iter().filter(|record| !(record.block == 1 && record.arm == Arm::B)).cloned().collect();
    let mut with_aa = records.clone();
    with_aa[0].arm = Arm::AA;
    let mut other_fixture = records.clone();
    other_fixture[5].workload.fixture_hash = id("sha256:0000");
    let mut counted = records.clone();
    counted[2].samples = Samples::Count(Count::new(1, 2).unwrap());

    assert_eq!(decide(&[], &eligible()), Err(PairError::NoRecords));
    assert_eq!(decide(&without_last_b, &eligible()), Err(PairError::Unpaired { night: id("2026-11-21"), runner_image: id(IMAGE), block: 1 }));
    assert_eq!(decide(&with_aa, &eligible()), Err(PairError::Arms(vec![Arm::A, Arm::B, Arm::AA])));
    assert_eq!(decide(&other_fixture, &eligible()), Err(PairError::MixedDigest { run_id: id("2026-11-21-1-1") }));
    assert_eq!(decide(&counted, &eligible()), Err(PairError::SamplesKind { run_id: id("2026-11-21-0-2") }));
}

#[test]
fn records_print_as_one_line_and_parse_back() {
    let missed = record("2026-11-21", 3, 1, Arm::B, Gate::UiEqMissed, Samples::Count(Count::new(12, 7_200).unwrap()));
    let line = "craft-pair/1 test_id=CONF-aa-sequential run_id=2026-11-21-3-1 app_pin=pc-359051359 fw_rev=cl-9b33fe822 os=macos-15 \
                runner_image=macos-15-arm64 night=2026-11-21 fixture_hash=sha256:9f2c mode=wasm backend=metal load=idle scale=6mp block=3 \
                arm=B gate=UiEqMissed refresh_hz=120 samples=count:12/7200";
    assert_eq!(missed.to_string(), line);
    assert_eq!(line.parse::<RunRecord>(), Ok(missed.clone()));
    let latency = constant(&[(0, 2), (1_000, 1), (61_000_000_000, 3)]);
    assert_eq!(latency.to_string(), "hist:0:2,62:1,1729:3");
    let mut records = night(&mut SplitMix64(81), &STEADY, "2026-11-21", 2, Duration::ZERO);
    records.push(missed);
    let text = print_records(&records);
    assert_eq!(text.lines().count(), 9);
    assert_eq!(parse_records(&text), Ok(records));
    assert_eq!("hist:".parse::<Hist>(), Ok(Hist::new()));
}

#[test]
fn malformed_lines_are_rejected_with_their_line_and_problem() {
    let good = record("n1", 0, 0, Arm::A, Gate::UiEqCpu, constant(&[(2_000_000, 1)])).to_string();
    let edited = |from: &str, to: &str| good.replacen(from, to, 1);
    let cases = [
        (edited("craft-pair/1", "craft-pair/2"), Problem::Version),
        (edited("app_pin=pc-359051359 ", ""), Problem::Field("app_pin")),
        (edited("os=macos-15", "os=mac os"), Problem::Field("runner_image")),
        (edited("night=n1", "night="), Problem::Value("night")),
        (edited("block=0", "block=007"), Problem::Value("block")),
        (edited("block=0", "block=-1"), Problem::Value("block")),
        (edited("arm=A", "arm=C"), Problem::Value("arm")),
        (edited("gate=UiEqCpu", "gate=UiEqcpu"), Problem::Value("gate")),
        (edited("refresh_hz=120", "refresh_hz=0"), Problem::Value("refresh_hz")),
        (edited("samples=hist:", "samples=hist:1730:1,"), Problem::Samples),
        (edited("samples=hist:", "samples=hist:1700:1,"), Problem::Samples),
        (edited("samples=hist:", "samples=hist:0:0,"), Problem::Samples),
        (edited("samples=hist:", "samples=hist:0:18446744073709551615,1:1,"), Problem::Samples),
        (edited("samples=hist:", "samples=count:3/2 hist:"), Problem::Samples),
        (edited("samples=hist:", "samples=count:3/4 "), Problem::Trailing),
        (record("n1", 0, 0, Arm::A, Gate::UiEqMissed, constant(&[(2_000_000, 1)])).to_string(), Problem::SamplesKind),
        (format!("{good} extra=1"), Problem::Trailing),
        (format!("{good}{}", ",9".repeat(40_000)), Problem::TooLong),
    ];
    for (line, problem) in cases {
        assert_eq!(line.parse::<RunRecord>().map(|_| ()), Err(LineError { line: 1, problem }), "{line:.200}");
    }
    assert_eq!(parse_records(&format!("{good}\n\n{good}\n")), Err(LineError { line: 2, problem: Problem::EmptyLine }));
    assert_eq!(parse_records(&format!("{good}\n{}\n", edited("arm=A", "arm=Z"))), Err(LineError { line: 2, problem: Problem::Value("arm") }));
    assert_eq!(Ident::new(&"x".repeat(129)), None);
}

#[test]
fn pools_past_the_record_or_byte_cap_are_rejected() {
    let line = record("n1", 0, 0, Arm::A, Gate::UiEqMissed, Samples::Count(Count::new(1, 2).unwrap())).to_string();
    let at_cap = format!("{line}\n").repeat(MAX_RECORDS);
    assert_eq!(parse_records(&at_cap).map(|records| records.len()), Ok(MAX_RECORDS));
    assert_eq!(parse_records(&format!("{at_cap}{line}\n")), Err(LineError { line: MAX_RECORDS + 1, problem: Problem::TooManyRecords }));
    let mut oversized = "\n".repeat(3);
    oversized.push_str(&"x".repeat(MAX_POOL_BYTES - 2));
    assert_eq!(parse_records(&oversized), Err(LineError { line: 4, problem: Problem::PoolTooLarge }));
}

const NIGHTLY_STRESS: Load = Load { block_noise: 0.25, ..STEADY };
const NIGHTLY_REALISTIC: Load = Load { block_noise: 0.1, frames: 1_000, ..STEADY };

struct Scenario {
    load: &'static Load,
    units_per_night: u32,
    shift: Duration,
}

fn first_decisive_verdict(trial: u64, scenario: &Scenario) -> Verdict {
    let mut rng = SplitMix64((1_000_003 * (trial + 1)) ^ scenario.shift.as_nanos() as u64);
    let mut pooled = Vec::new();
    let mut verdict = Verdict::Inconclusive(Doubt::Noisy);
    for look in 1..=LOOKS {
        pooled.extend(night(&mut rng, scenario.load, &format!("night-{look}"), scenario.units_per_night, scenario.shift));
        verdict = decide(&pooled, &eligible()).unwrap().verdict();
        if matches!(verdict, Verdict::Pass | Verdict::Fail) {
            break;
        }
    }
    verdict
}

fn run(trials: u64, scenario: &Scenario, wanted: Verdict) -> usize {
    let workers = std::thread::available_parallelism().map_or(1, usize::from);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|worker| {
                scope.spawn(move || {
                    (worker as u64..trials).step_by(workers).filter(|trial| first_decisive_verdict(*trial, scenario) == wanted).count()
                })
            })
            .collect();
        handles.into_iter().map(|handle| handle.join().unwrap()).sum()
    })
}

fn clopper_pearson_upper(events: usize, trials: usize) -> f64 {
    let at_most = |p: f64| {
        let mut term = (1.0 - p).powi(trials as i32);
        let mut total = term;
        for k in 0..events {
            term *= (trials - k) as f64 / (k + 1) as f64 * p / (1.0 - p);
            total += term;
        }
        total
    };
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..60 {
        let middle = (low + high) / 2.0;
        if at_most(middle) > 0.05 {
            low = middle;
        } else {
            high = middle;
        }
    }
    high
}

#[test]
fn clopper_pearson_matches_known_bounds() {
    assert!((clopper_pearson_upper(0, 59) - 0.0495).abs() < 0.0005);
    assert!((clopper_pearson_upper(3, 300) - 0.0258).abs() < 0.0005);
}

#[test]
#[ignore = "CONF-aa-sequential: up to 4,000 verdicts; CI job testkit-simulation runs it with cargo test --release -p craft-testkit -- --ignored conf_aa_sequential"]
fn conf_aa_sequential_false_verdicts_and_power_within_five_looks() {
    let trials = 300;
    let allow = Duration::from_millis(1);
    let stress = |shift| Scenario { load: &NIGHTLY_STRESS, units_per_night: 3, shift };
    let false_fails = run(trials, &stress(Duration::ZERO), Verdict::Fail);
    let false_passes = run(trials, &stress(allow * 6 / 5), Verdict::Pass);
    let power = run(200, &Scenario { load: &NIGHTLY_REALISTIC, units_per_night: 4, shift: allow * 2 }, Verdict::Fail);
    println!("CONF-aa-sequential: false Fail {false_fails}/{trials}, false Pass {false_passes}/{trials}, Fail at 2x {power}/200");
    assert!(clopper_pearson_upper(false_fails, trials as usize) <= 0.05, "false Fail {false_fails} of {trials}");
    assert!(clopper_pearson_upper(false_passes, trials as usize) <= 0.05, "false Pass {false_passes} of {trials}");
    assert!(power >= 160, "Fail at twice the allowance in only {power} of 200");
}
