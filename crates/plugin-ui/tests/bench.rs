use std::time::{Duration, Instant};

use craft_plugin::{Item, PanelReply, Pointer, PointerPhase, Ui, UiEvent, UiEventKind, UiInput, Widget};
use craft_plugin_host::Retained;
use craft_plugin_ui::conformance::{Bare, theme};
use craft_plugin_ui::draw;
use egui_kittest::Harness;

const RUNS: usize = 7;
const SIZE: [f32; 2] = [640.0, 900.0];
const SECTIONS: usize = 25;
const ROWS: usize = 100_000;
const FRAMES: usize = 200;

fn panel(input: &UiInput) -> PanelReply {
    let (mut volume, mut count, mut name, mut on, mut choice, mut picked) = (0.5, 3.0, String::from("untitled"), true, 1usize, None::<usize>);
    let mut ui = Ui::new(input);
    for section in 0..SECTIONS {
        ui.label(&format!("Section {section}"));
        ui.button("Apply");
        ui.slider("Amount", &mut volume, 0.0..=1.0);
        ui.number("Count", &mut count, 1.0);
        ui.text_edit("name", &mut name, "name");
        ui.toggle("Enabled", &mut on);
        ui.popup("Mode", &mut choice, &["Add", "Multiply", "Screen"]);
        ui.row(|ui| {
            ui.label("Row");
            ui.button("Go");
        });
        ui.group("Group", |ui| ui.label("Inside"));
    }
    ui.list("rows", ROWS, &mut picked, |row| Item::cells(&[&format!("Row {row}")]));
    ui.reply()
}

fn count(widgets: &[Widget]) -> usize {
    widgets
        .iter()
        .map(|widget| match widget {
            Widget::Row { children } | Widget::Group { children, .. } | Widget::Column { children } => 1 + count(children),
            _ => 1,
        })
        .sum()
}

fn measure(iterations: usize, mut run: impl FnMut()) -> (Duration, Duration) {
    let mut times: Vec<Duration> = (0..=RUNS)
        .map(|_| {
            let start = Instant::now();
            (0..iterations).for_each(|_| run());
            start.elapsed() / iterations as u32
        })
        .skip(1)
        .collect();
    times.sort();
    (times[RUNS / 2], times[0])
}

fn row(case: &str, per: &str, (median, minimum): (Duration, Duration), note: &str) {
    println!("| {case} | {per} | {:.1} us | {:.1} us | {note} |", median.as_secs_f64() * 1e6, minimum.as_secs_f64() * 1e6);
}

fn motion(x: f32) -> UiEvent {
    UiEvent { id: 7, kind: UiEventKind::Pointer(Pointer::new([x, 4.0], PointerPhase::Hover)) }
}

fn events() -> Vec<UiEvent> {
    let mut events: Vec<UiEvent> = (0..9).map(|x| motion(x as f32)).collect();
    events.push(UiEvent { id: 3, kind: UiEventKind::Click });
    events.extend((9..18).map(|x| motion(x as f32)));
    events.push(UiEvent { id: 3, kind: UiEventKind::Click });
    events
}

fn cycle(retained: &mut Retained, reply: &PanelReply, now: Duration) -> usize {
    retained.push(events(), "");
    let mut calls = 0;
    while retained.start(SIZE, now).is_some() {
        retained.finish(Ok(reply.clone()), now);
        calls += 1;
    }
    calls
}

#[test]
#[ignore]
fn plugin_ui_speed() {
    let input = UiInput { size: SIZE, ..Default::default() };
    let reply = panel(&input);
    assert_eq!(count(&reply.widgets), 300 + 1);
    assert!(matches!(reply.widgets.last(), Some(Widget::List { count, items, .. }) if *count as usize == ROWS && items.len() == 128));
    let encoded = serde_json::to_vec(&reply).unwrap();
    assert_eq!(serde_json::from_slice::<PanelReply>(&encoded).unwrap(), reply);

    println!("| case | per | median | min of {RUNS} | note |");
    println!("|---|---|---|---|---|");
    row("build", "pass", measure(50, || drop(panel(&input))), "301 widgets, list 100000 rows, 128-row window");
    row(
        "wire",
        "encode+decode",
        measure(50, || drop(serde_json::from_slice::<PanelReply>(&serde_json::to_vec(&reply).unwrap()).unwrap())),
        &format!("{} bytes", encoded.len()),
    );

    let mut retained = Retained::new();
    let first = Instant::now().elapsed();
    assert_eq!(cycle(&mut retained, &reply, first), 2);
    row(
        "host",
        "cycle",
        measure(200, || {
            cycle(&mut retained, &reply, first);
        }),
        "20 events, 2 calls, includes one reply clone per call",
    );

    let widgets = reply.widgets.clone();
    let mut harness = Harness::builder().with_size(egui::vec2(SIZE[0], SIZE[1])).build_ui_state(
        move |ui, events: &mut Vec<UiEvent>| events.extend(draw(ui, &widgets, &theme(), "plugin.bench.panel", &mut Bare)),
        Vec::new(),
    );
    harness.run();
    row("paint", "frame", measure(FRAMES, || harness.step()), "kittest, no input, 200 frames per run");

    harness.state_mut().clear();
    (0..FRAMES).for_each(|_| harness.step());
    assert_eq!(harness.state().len(), 0);
    let mut idle = Retained::new();
    assert!(idle.start(SIZE, first).is_some());
    idle.finish(Ok(reply), first);
    assert_eq!((0..FRAMES).filter(|_| idle.start(SIZE, first).is_some()).count(), 0);
    println!("| idle | {FRAMES} frames | 0 events, 0 plugin calls | | asserted |");
}
