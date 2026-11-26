use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use craft_plugin::Pen;
use egui::Event;

const MOST_QUEUED: usize = 4096;

#[derive(Default)]
struct Readings {
    queued: Vec<Pen>,
    current: Option<Pen>,
}

#[derive(Clone, Default)]
pub struct PenSource(Arc<Mutex<Readings>>);

impl PenSource {
    fn lock(&self) -> MutexGuard<'_, Readings> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set(&self, sample: Option<Pen>) {
        let sample = sample.map(sanitized);
        let mut readings = self.lock();
        if let Some(sample) = sample {
            if readings.queued.len() >= MOST_QUEUED {
                readings.queued.remove(0);
            }
            readings.queued.push(sample);
        }
        readings.current = sample;
    }

    pub fn pens(&self, events: &[Event]) -> Vec<Option<Pen>> {
        let pointer = |event: &&Event| matches!(event, Event::PointerMoved(_) | Event::PointerButton { .. });
        let mut readings = self.lock();
        if !events.iter().any(|event| pointer(&event)) {
            return vec![readings.current; events.len()];
        }
        let mut samples = std::mem::take(&mut readings.queued);
        let latest = readings.current;
        drop(readings);
        if samples.iter().any(touching) {
            samples.retain(touching);
        }
        let current = match samples.last().copied().filter(touching) {
            Some(contact) if !latest.as_ref().is_some_and(touching) => Some(contact),
            _ => latest,
        };
        let moves = events.iter().filter(|event| matches!(event, Event::PointerMoved(_))).count();
        let mut nr = 0;
        events
            .iter()
            .map(|event| match event {
                Event::PointerMoved(_) => {
                    nr += 1;
                    ((nr * samples.len()).div_ceil(moves)).checked_sub(1).and_then(|at| samples.get(at).copied()).or(current)
                }
                Event::PointerButton { pressed: true, .. } => samples.first().copied().or(current),
                _ => current,
            })
            .collect()
    }
}

fn touching(sample: &Pen) -> bool {
    sample.pressure > 0.0
}

fn sanitized(sample: Pen) -> Pen {
    let quarter = std::f32::consts::FRAC_PI_2;
    let pressure = if sample.pressure.is_nan() { 1.0 } else { sample.pressure.clamp(0.0, 1.0) };
    let tilt = sample.tilt.map(|tilt| if tilt.is_finite() { tilt.clamp(-quarter, quarter) } else { 0.0 });
    let rotation = if sample.rotation.is_finite() {
        (sample.rotation + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
    } else {
        0.0
    };
    Pen { pressure, tilt, eraser: sample.eraser, rotation }
}

#[cfg(test)]
mod tests {
    use craft_plugin::Pen;
    use egui::{Event, Modifiers, PointerButton, Pos2};

    use super::PenSource;

    fn pen(pressure: f32) -> Option<Pen> {
        Some(Pen { pressure, tilt: [0.25, -0.5], eraser: false, rotation: 0.0 })
    }

    fn moved() -> Event {
        Event::PointerMoved(Pos2::new(10.0, 10.0))
    }

    fn button(pressed: bool) -> Event {
        Event::PointerButton { pos: Pos2::new(10.0, 10.0), button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
    }

    fn pressures(pens: Vec<Option<Pen>>) -> Vec<Option<f32>> {
        pens.into_iter().map(|pen| pen.map(|pen| pen.pressure)).collect()
    }

    #[test]
    fn a_pens_readings_reach_the_moves_of_the_pass_with_their_pressure_tilt_and_end() {
        let source = PenSource::default();
        source.set(Some(Pen { pressure: 0.0, tilt: [0.0, 0.0], eraser: true, rotation: 0.0 }));
        source.set(Some(Pen { pressure: 0.5, tilt: [0.25, -0.5], eraser: true, rotation: 0.0 }));
        source.set(Some(Pen { pressure: 0.75, tilt: [0.5, 0.125], eraser: true, rotation: 0.0 }));
        assert_eq!(
            source.pens(&[moved(), button(true), moved()]),
            [
                Some(Pen { pressure: 0.5, tilt: [0.25, -0.5], eraser: true, rotation: 0.0 }),
                Some(Pen { pressure: 0.5, tilt: [0.25, -0.5], eraser: true, rotation: 0.0 }),
                Some(Pen { pressure: 0.75, tilt: [0.5, 0.125], eraser: true, rotation: 0.0 }),
            ],
            "the hover reading before the touch is dropped, the press takes the first contact"
        );
        assert_eq!(
            source.pens(&[moved()]),
            [Some(Pen { pressure: 0.75, tilt: [0.5, 0.125], eraser: true, rotation: 0.0 })],
            "no new reading repeats the latest"
        );
    }

    #[test]
    fn readings_spread_evenly_over_the_moves_and_wait_for_a_pass_with_pointer_events() {
        let source = PenSource::default();
        for pressure in [0.25, 0.5, 0.75, 1.0] {
            source.set(pen(pressure));
        }
        let key = Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
        assert_eq!(pressures(source.pens(&[key])), [Some(1.0)], "a pass with no pointer event reads the latest and leaves the queue");
        assert_eq!(pressures(source.pens(&[moved(), moved()])), [Some(0.5), Some(1.0)]);
        source.set(pen(0.2));
        assert_eq!(pressures(source.pens(&[moved(), moved(), moved()])), [Some(0.2), Some(0.2), Some(0.2)]);
    }

    #[test]
    fn a_tap_shorter_than_a_pass_and_a_lift_keep_the_contact_pressure_and_a_mouse_has_no_pen() {
        let source = PenSource::default();
        for pressure in [0.0, 0.6, 0.0] {
            source.set(pen(pressure));
        }
        source.set(None);
        assert_eq!(pressures(source.pens(&[button(true), button(false)])), [Some(0.6), Some(0.6)]);
        source.set(pen(0.0));
        assert_eq!(pressures(source.pens(&[moved()])), [Some(0.0)], "a hovering pen reads pressure 0");
        source.set(None);
        assert_eq!(pressures(source.pens(&[moved(), button(true), button(false)])), [None, None, None]);
    }

    #[test]
    fn readings_out_of_range_are_brought_into_range() {
        let source = PenSource::default();
        source.set(Some(Pen { pressure: f32::NAN, tilt: [f32::INFINITY, -4.0], eraser: false, rotation: 0.0 }));
        source.set(Some(Pen { pressure: 4.0, tilt: [f32::NAN, 4.0], eraser: false, rotation: 0.0 }));
        let quarter = std::f32::consts::FRAC_PI_2;
        assert_eq!(
            source.pens(&[moved(), moved()]),
            [
                Some(Pen { pressure: 1.0, tilt: [0.0, -quarter], eraser: false, rotation: 0.0 }),
                Some(Pen { pressure: 1.0, tilt: [0.0, quarter], eraser: false, rotation: 0.0 })
            ]
        );
    }

    #[test]
    fn a_barrel_rotation_is_kept_inside_one_half_turn_either_way_and_a_broken_one_is_zero() {
        let turn = |rotation| super::sanitized(Pen { pressure: 0.5, rotation, ..Default::default() }).rotation;
        let pi = std::f32::consts::PI;
        assert!((turn(1.5 * pi) + 0.5 * pi).abs() < 1e-5 && (turn(0.25) - 0.25).abs() < 1e-6, "{} {}", turn(1.5 * pi), turn(0.25));
        assert_eq!(turn(f32::NAN), 0.0);
    }
}
