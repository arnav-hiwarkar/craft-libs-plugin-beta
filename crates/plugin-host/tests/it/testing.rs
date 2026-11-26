use std::collections::BTreeMap;

use craft_plugin::{Channel, Overlay, PanelPlace, Place, Shape, ToolEventKind, UiEventKind, Value};
use craft_plugin_host::AuxPlanes;
use craft_plugin_host::testing::{Pattern, Picture, RENDER_LAYER, TestPlugin, check_frame, json, pointer, updating_frames};

use super::{Kind, bundle};

fn probe() -> (tempfile::TempDir, TestPlugin) {
    let (dir, bundle) = bundle(Kind::Wasm, &["document.write"]);
    let plugin = TestPlugin::load(&bundle).unwrap();
    (dir, plugin)
}

#[test]
fn a_stored_frame_passes_and_a_changed_render_writes_the_actual_and_a_difference_image_under_target() {
    let (_bundle, plugin) = probe();
    let work = tempfile::tempdir().unwrap();
    let full = plugin.render("invert").on(Pattern::Checker).size(64, 32);
    full.run().unwrap().save(&work.path().join("tests/frames/invert.png")).unwrap();
    assert_eq!(check_frame(work.path(), "invert", &full), Ok(()));

    let half = plugin.render("invert").on(Pattern::Checker).size(64, 32).set("amount", 0.5);
    let error = check_frame(work.path(), "invert", &half).unwrap_err();
    assert!(error.contains("2048 of 2048 pixels are off by more than 1/255 (largest 128/255, first at (0, 0))"), "{error}");
    assert!(error.contains("UPDATE_FRAMES=1"), "{error}");
    let actual = Picture::open(&work.path().join("target/frames/invert.actual.png")).unwrap();
    assert_eq!(actual.at(0, 0), [128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0, 1.0]);
    let difference = Picture::open(&work.path().join("target/frames/invert.diff.png")).unwrap();
    assert_eq!(difference.at(5, 5), [1.0, 0.0, 0.0, 1.0]);

    assert_eq!(check_frame(work.path(), "invert", &full), Ok(()));
    assert!(!work.path().join("target/frames/invert.diff.png").exists());
    assert_eq!(std::fs::read_dir(work.path().join("tests/frames")).unwrap().count(), 1);
}

#[test]
fn a_pointwise_effect_that_changes_its_rows_in_place_renders_the_same_in_bands_as_whole() {
    let (_bundle, plugin) = probe();
    let picture = plugin.render("halve").on(Pattern::Checker).size(40, 257).run().unwrap();
    let (half, dark) = ([0.5, 0.5, 0.5, 1.0], [0.0, 0.0, 0.0, 1.0]);
    assert_eq!([(0, 200), (39, 200), (0, 230), (39, 230)].map(|(x, y)| picture.at(x, y)), [half, dark, dark, half]);
}

#[test]
fn only_update_frames_set_to_1_updates_stored_frames() {
    let asked = [None, Some("0"), Some("false"), Some("yes"), Some(""), Some("1")];
    assert_eq!(asked.map(|value| updating_frames(value.map(std::ffi::OsStr::new))), [false, false, false, false, false, true]);
}

#[test]
fn a_frame_name_that_is_not_a_plain_stem_is_refused_and_nothing_is_written() {
    let (_bundle, plugin) = probe();
    let work = tempfile::tempdir().unwrap();
    let render = plugin.render("invert").on(Pattern::Checker).size(8, 8);
    for name in ["../preview", "full.actual", "a/b", ""] {
        let error = check_frame(work.path(), name, &render).unwrap_err();
        assert_eq!(error, format!("frame name `{name}` must be a plain name of letters, digits, `-` and `_`, such as `full`"));
    }
    assert_eq!(std::fs::read_dir(work.path()).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn a_stored_frame_that_is_a_link_is_refused_and_its_target_is_not_written() {
    let (_bundle, plugin) = probe();
    let work = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(work.path().join("tests/frames")).unwrap();
    let link = work.path().join("tests/frames/full.png");
    std::os::unix::fs::symlink(outside.path().join("victim.png"), &link).unwrap();
    let error = check_frame(work.path(), "full", &plugin.render("invert").on(Pattern::Checker).size(8, 8)).unwrap_err();
    assert_eq!(error, format!("{} is a link; stored frames must be plain files", link.display()));
    assert!(!outside.path().join("victim.png").exists());
}

#[test]
fn a_misspelt_parameter_or_effect_names_what_the_plugin_has() {
    let (_bundle, plugin) = probe();
    let error = plugin.render("invert").set("amout", 1.0).run().unwrap_err();
    assert_eq!(error, r#"effect `invert` has no parameter `amout`; its parameters are ["amount"]"#);
    let error = plugin.render("invrt").run().unwrap_err();
    assert!(error.starts_with("this plugin has no effect `invrt`; its effects are [\"invert\""), "{error}");
}

#[test]
fn a_panel_is_clicked_by_its_label_and_a_command_is_recorded_with_its_parameters() {
    let (_bundle, plugin) = probe();
    let mut panel = plugin.panel("counter").unwrap();
    panel.click("Count").unwrap();
    panel.click("Count").unwrap();
    assert!(panel.texts().contains(&"2 clicks at speed 0".to_string()), "{:?}", panel.texts());
    let error = panel.click("Cont").unwrap_err();
    assert!(error.starts_with("panel `counter` shows no widget labelled `Cont`; it shows [\"Count\""), "{error}");
    plugin.command("edit").unwrap();
    assert_eq!(plugin.commands(), [("layer.newNull".to_string(), serde_json::json!({"name": "From Plugin"}))]);
}

#[test]
fn a_failed_call_is_journalled_with_the_call_and_the_effect() {
    let (_bundle, plugin) = probe();
    let error = plugin.render("read-inputs").run().unwrap_err();
    assert_eq!(error, "no matte layer");
    assert_eq!(plugin.logs(), ["render of effect `read-inputs` failed: no matte layer"]);
}

#[test]
fn the_app_can_warn_into_the_plugins_own_log() {
    let (_bundle, plugin) = probe();
    plugin.plugin().warn("effect `invert`: its GPU shader is turned off".into());
    assert_eq!(plugin.logs(), ["effect `invert`: its GPU shader is turned off"]);
}

#[test]
fn a_test_render_clamps_to_the_hard_range_and_refuses_text_for_a_number_like_the_app() {
    let (_bundle, plugin) = probe();
    let render = |amount: f64| plugin.render("invert").on(Pattern::Checker).size(2, 1).set("amount", amount).run().unwrap();
    assert_eq!(render(2.0).pixels, [[0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]]);
    assert_eq!(render(2.0), render(1.0));
    let error = plugin.render("invert").set("amount", "x").run().unwrap_err();
    assert_eq!(error, r#"parameter `amount` takes a number, not the text "x""#);
}

#[test]
fn an_effect_that_changes_with_time_must_say_so() {
    let (_bundle, plugin) = probe();
    assert_eq!(plugin.render("drift").time_check(), Ok(()));
    assert_eq!(plugin.render("invert").time_check(), Ok(()));
    let error = plugin.render("silent-drift").time_check().unwrap_err();
    assert!(error.contains("effect `silent-drift` renders differently at 0 s and at 0.5 s") && error.contains("`.time_dependent()`"), "{error}");
}

#[test]
fn an_effect_that_reads_the_scene_at_another_time_without_declaring_other_frames_fails_and_says_what_to_declare() {
    let (_bundle, plugin) = probe();
    let error = plugin.render("scene-later").size(8, 8).run().unwrap_err();
    assert!(error.contains("effect `scene-later` must declare `.reads_other_frames()`"), "{error}");
}

#[test]
fn a_gradient_past_the_stop_limit_reaches_the_plugin_cut_and_is_warned_about_once_per_value() {
    let (_bundle, plugin) = probe();
    let ramp = |count: usize| craft_plugin::Gradient {
        colors: (0..count).map(|i| (i as f64 / 69.0, [i as f64 / 69.0, 0.0, 0.0, 1.0])).collect(),
        opacities: vec![(0.0, 1.0), (1.0, 1.0)],
    };
    let red = |count: usize| plugin.render("ramp").size(1, 1).set("ramp", ramp(count)).set("at", 1.0).float().run().unwrap().at(0, 0)[0];
    assert_eq!(red(70), 63.0 / 69.0);
    assert_eq!(red(70), 63.0 / 69.0);
    let warning =
        "effect `ramp`: gradient parameter `ramp` has 6 stops beyond the limit of 64; the plugin received only the first 64 colour and opacity stops";
    assert_eq!(plugin.logs(), [warning]);
    assert_eq!(red(64), 63.0 / 69.0);
    assert_eq!(red(71), 63.0 / 69.0);
    let second = warning.replace("has 6 stops", "has 7 stops");
    assert_eq!(plugin.logs(), [warning, second.as_str()]);
}

#[test]
fn an_effect_at_state_version_1_moves_a_value_saved_under_its_old_name_and_leaves_newer_values_alone() {
    let (_bundle, plugin) = probe();
    let saved = BTreeMap::from([("amt".to_string(), Value::Number(0.25)), ("unknown".to_string(), Value::Bool(true))]);
    let migrated = plugin.migrate("renamed", 0, saved.clone()).unwrap();
    assert_eq!(migrated, BTreeMap::from([("strength".to_string(), Value::Number(0.25)), ("unknown".to_string(), Value::Bool(true))]));
    assert_eq!(plugin.migrate("renamed", 1, saved.clone()).unwrap(), saved);
    assert_eq!(plugin.migrate("invert", 0, saved.clone()).unwrap(), saved);
}

#[test]
fn a_generator_that_reports_a_drawn_rectangle_renders_as_a_whole_frame_with_the_rest_transparent() {
    let (_bundle, plugin) = probe();
    let picture = plugin.render("sprite").size(400, 300).set("drawn", true).run().unwrap();
    assert_eq!((picture.width, picture.height), (400, 300));
    assert_eq!(picture.at(0, 0), [0.0; 4]);
    assert_eq!(picture.at(99, 150), [0.0; 4]);
    assert_eq!(picture.at(100, 50), [0.0, 0.0, 1.0, 1.0]);
}

#[test]
fn a_test_serves_its_own_frames_and_layer_data_and_can_cancel_a_task() {
    let (_dir, bundle) = bundle(Kind::Wasm, &["document.read", "document.write"]);
    let plugin = TestPlugin::load(&bundle).unwrap();
    plugin.frames(|request| {
        let side = (40.0 * request.scale) as u32;
        Ok(Picture { width: side, height: side / 2, pixels: vec![[request.time as f32, 0.0, 0.0, 1.0]; (side * side / 2) as usize] })
    });
    let read = plugin.task("frame", json!({"time": 0.5, "scale": 0.5})).unwrap();
    assert_eq!(read, json!({"width": 20, "height": 10, "center": [0.5, 0.0, 0.0, 1.0]}));
    plugin.set_data(Place::OwnLayer, "tint", &[255]).unwrap();
    assert_eq!(plugin.data(Place::Layer(RENDER_LAYER), "tint"), Some(vec![255]));
    let tinted = plugin.render("tinted").on(Pattern::Checker).size(4, 4).run().unwrap();
    assert_eq!(tinted.at(1, 1)[0], 1.0);
    let started = plugin.start("patient", json!({})).unwrap();
    assert!(plugin.cancel(started));
    assert_eq!(plugin.wait(started), Err(format!("task {started} was cancelled")));
}

#[test]
fn the_test_host_refuses_what_the_app_refuses_and_fixtures_can_fail() {
    let (_dir, bundle) = bundle(Kind::Wasm, &["document.read", "document.write"]);
    let plugin = TestPlugin::load(&bundle).unwrap();
    for n in 0..craft_plugin::MAX_DATA_KEYS {
        plugin.set_data(Place::Layer(3), &format!("k{n}"), &[]).unwrap();
    }
    assert!(plugin.set_data(Place::Layer(3), "one-more", &[]).unwrap_err().contains("64 keys"));
    plugin.set_data(Place::Layer(4), "one-more", &[]).unwrap();
    assert!(plugin.task("frame", json!({"time": 0.0, "scale": 0.0})).unwrap_err().contains("scale above 0"));
    assert!(plugin.task("frame", json!({"time": 0.0, "scale": 1.0, "source": true})).unwrap_err().contains("names its layer"));
    plugin.frames(|_| Err("no layer 9".into()));
    assert_eq!(plugin.task("frame", json!({"time": 0.0, "scale": 1.0})), Err("no layer 9".into()));
    plugin.reply("layer.newNull", |_| Err("the composition is gone".into()));
    assert!(plugin.task("edit", json!({})).unwrap_err().contains("the composition is gone"));
}

#[test]
fn an_effect_reads_the_depth_and_normals_given_with_its_layer_and_nothing_on_a_plain_layer() {
    let (_bundle, plugin) = probe();
    let depth: Vec<f32> = (0..512).map(|i| (i / 2) as f32).collect();
    let normal: Vec<f32> = (0..512).flat_map(|i| [0.0, 0.0, -((i % 2) as f32)]).collect();
    let aux = AuxPlanes { width: 2, height: 256, planes: vec![(Channel::Normal, normal), (Channel::Depth, depth), (Channel::Uv, vec![0.0; 1024])] };
    let read = plugin.render("aux-depth").size(2, 256).aux(aux).float().run().unwrap();
    assert_eq!([read.at(0, 70), read.at(1, 200)], [[70.0 / 1000.0, 0.0, 0.0, 1.0], [200.0 / 1000.0, 1.0, 0.0, 1.0]]);
    let plain = plugin.render("aux-depth").size(3, 2).float().run().unwrap();
    assert_eq!(plain.at(1, 1), [0.0, 0.0, 1.0, 1.0]);
}

#[test]
fn an_effect_reading_a_channel_it_did_not_declare_fails_and_says_what_to_declare() {
    let (_bundle, plugin) = probe();
    let error = plugin.render("aux-undeclared").size(4, 4).run().unwrap_err();
    assert!(error.contains("effect `aux-undeclared` must declare `.reads_aux([Channel::Uv])`"), "{error}");
}

#[test]
fn a_plan_reads_source_frames_at_other_times_from_the_host_and_must_declare_it() {
    let (_bundle, plugin) = probe();
    let shifted = |shift: f32| {
        let mut picture = Picture::new(64, 48);
        for (i, pixel) in picture.pixels.iter_mut().enumerate() {
            let (x, y) = ((i % 64) as f32 - shift, (i / 64) as f32);
            let v = 0.5 + 0.2 * (0.4 * x).sin() * (0.3 * y).cos() + 0.1 * (0.15 * x + 0.1 * y).sin();
            *pixel = [v, v, v, 1.0];
        }
        picture
    };
    let (now, earlier) = (shifted(3.0), shifted(0.0));
    let frames = move |time: f64| if time < 0.75 { shifted(0.0) } else { shifted(3.0) };
    let rebuilt = plugin.render("rewind").input(now.clone()).source_at(frames).time(1.0).run().unwrap();
    let mean_error = |a: &Picture, b: &Picture| {
        let inner: Vec<f32> = (0..a.pixels.len())
            .filter(|i| (8..56).contains(&(i % 64)) && (8..40).contains(&(i / 64)))
            .map(|i| (a.pixels[i][0] - b.pixels[i][0]).abs())
            .collect();
        inner.iter().sum::<f32>() / inner.len() as f32
    };
    let (to_now, to_earlier) = (mean_error(&rebuilt, &now), mean_error(&rebuilt, &earlier));
    assert!(
        to_now < 0.01 && to_earlier > 10.0 * to_now,
        "the earlier frame warped by the flow matches now ({to_now}), not the earlier frame ({to_earlier})"
    );
    let error = plugin.render("silent-rewind").input(now).time(1.0).run().unwrap_err();
    assert!(error.contains("declare `.reads_other_frames()`"), "{error}");
}

#[test]
fn a_button_press_runs_the_effects_host_commands_and_the_button_sends_no_value_to_a_render() {
    let (_bundle, plugin) = probe();
    plugin.press("switches", "reset").unwrap();
    let paths: Vec<serde_json::Value> = plugin.commands().into_iter().map(|(_, params)| params["path"].clone()).collect();
    assert_eq!(paths, ["effects/@1/softness", "effects/@1/amount"]);
    let error = plugin.render("switches").set("reset", true).run().unwrap_err();
    assert_eq!(error, "parameter `reset` takes no value, as it is a button, not true");
    let error = plugin.press("plain", "go").unwrap_err();
    assert!(error.contains("no `Effect::button`"), "{error}");
}

fn written(overlay: Overlay) -> Vec<String> {
    overlay
        .marks
        .into_iter()
        .filter_map(|mark| match mark.shape {
            Shape::Text { text, .. } => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn a_sandboxed_panel_sees_a_static_its_command_set_while_its_tool_and_overlay_run_on_instances_of_their_own() {
    let (_bundle, plugin) = probe();
    plugin.command("bump").unwrap();
    plugin.command("bump").unwrap();
    let panel = plugin.panel("shared").unwrap().texts();
    let tool = written(plugin.tool("census", pointer(ToolEventKind::Move, [0.0; 2])).unwrap());
    let overlay = written(plugin.render("peek").overlay().unwrap());
    assert_eq!((panel, tool, overlay), (vec!["shared 2".to_string()], vec!["shared 0".to_string()], vec!["shared 0".to_string()]));
}

#[test]
fn a_tool_reads_the_pen_pressure_tilt_and_end_of_a_move_and_a_mouse_move_has_no_pen() {
    let (_bundle, plugin) = probe();
    let pen = craft_plugin::Pen { pressure: 0.5, tilt: [0.25, -0.5], eraser: true, rotation: 0.0 };
    let drawn = written(
        plugin.tool("stamp", craft_plugin::ToolEvent { pen: Some(pen), pressure: 0.5, ..pointer(ToolEventKind::Move, [40.0, 30.0]) }).unwrap(),
    );
    assert_eq!(drawn, ["Move 40.0 30.0 pen 0.50 0.25 -0.50 eraser at 0.50"]);
    let drawn = written(plugin.tool("stamp", pointer(ToolEventKind::Move, [50.0, 35.0])).unwrap());
    assert_eq!(drawn, ["Move 40.0 30.0 pen 0.50 0.25 -0.50 eraser at 0.50", "Move 50.0 35.0"]);
}

#[test]
fn a_tree_panel_opens_folders_from_toggles_and_its_ten_thousand_row_table_builds_only_its_view() {
    use craft_plugin::ListEvent::{Activate, Show, Toggle};
    let (_bundle, plugin) = probe();
    let mut panel = plugin.panel("tree").unwrap();
    assert_eq!(panel.rows(0).unwrap(), (0, vec!["Folder 0".to_string(), "Folder 1".into(), "Folder 2".into()]));
    panel.list_event(0, Toggle { row: 0, open: true }).unwrap();
    panel.list_event(0, Toggle { row: 1, open: true }).unwrap();
    let shown = ["Folder 0", "Shot 0.0", "Take 0.0.0", "Take 0.0.1", "Shot 0.1", "Folder 1", "Folder 2"];
    assert_eq!(panel.rows(0).unwrap(), (0, shown.map(String::from).to_vec()));
    assert!(panel.texts().contains(&"opened Shot 0.0".to_string()), "{:?}", panel.texts());
    panel.list_event(0, Toggle { row: 0, open: false }).unwrap();
    assert_eq!(panel.rows(0).unwrap().1.len(), 3);
    panel.list_event(1, Show { first: 9_990, count: 10 }).unwrap();
    let (first, rows) = panel.rows(1).unwrap();
    assert_eq!((first, rows.len(), rows[5].as_str()), (9_990, 10, "take 9995\t35.5 s\t*"));
    panel.list_event(1, Activate(9_995)).unwrap();
    assert!(panel.texts().contains(&"activated take 9995".to_string()), "{:?}", panel.texts());
}

#[test]
fn a_status_item_and_a_stamp_option_bar_take_their_places_and_each_notes_panel_keeps_its_own_text() {
    let (_bundle, plugin) = probe();
    let found = plugin.plugin().contributions();
    let place = |id: &str| found.panels.iter().find(|panel| panel.id == id).map(|panel| (panel.place, panel.multiple, panel.icon.clone()));
    assert_eq!(
        [place("status"), place("stamp-options"), place("notes")],
        [
            Some((PanelPlace::Status, false, Some("bell".into()))),
            Some((PanelPlace::Bar, false, None)),
            Some((PanelPlace::Bottom, true, Some("text".into())))
        ]
    );
    let options = found.tools.iter().find(|tool| tool.id == "stamp").and_then(|tool| tool.options.clone());
    assert_eq!(options.as_deref(), Some("stamp-options"));
    let mut status = plugin.panel("status").unwrap();
    status.click("Ping").unwrap();
    assert_eq!(status.texts(), ["Ping", "1 pings"]);
    let mut one = plugin.panel_for("notes", "1").unwrap();
    let mut two = plugin.panel_for("notes", "2").unwrap();
    one.send("Note", UiEventKind::Text("milk".into())).unwrap();
    two.send("Note", UiEventKind::Text("eggs".into())).unwrap();
    one.redraw().unwrap();
    assert_eq!((one.texts(), two.texts()), (vec!["Note".to_string(), "note 1: milk".into()], vec!["Note".to_string(), "note 2: eggs".into()]));
}

#[test]
fn the_controls_panel_takes_toggle_radio_text_area_menu_curve_and_gradient_events_by_label() {
    let (_bundle, plugin) = probe();
    let mut panel = plugin.panel("controls").unwrap();
    let status = |panel: &craft_plugin_host::testing::PanelTest| panel.texts().into_iter().find(|text| text.starts_with("live ")).unwrap_or_default();
    assert!(panel.widgets().iter().any(|widget| matches!(widget, craft_plugin::Widget::Focus { .. })), "the first call asks for the focus");
    panel.send("Live", craft_plugin::UiEventKind::Bool(true)).unwrap();
    panel.send("Mode", craft_plugin::UiEventKind::Number(1.0)).unwrap();
    panel.send("Notes", craft_plugin::UiEventKind::Text("one\ntwo".into())).unwrap();
    assert_eq!(status(&panel), r#"live true mode 1 pinned false notes "one\ntwo" said "#);
    assert!(!panel.widgets().iter().any(|widget| matches!(widget, craft_plugin::Widget::Focus { .. })), "later calls leave the focus alone");
    panel.click("Clear notes").unwrap();
    panel.pick("More", "Pinned").unwrap();
    panel.pick("More", "Hard").unwrap();
    panel.pick("", "Say hello").unwrap();
    assert_eq!(status(&panel), r#"live true mode 2 pinned true notes "" said hello"#);
    let error = panel.pick("More", "Sort").unwrap_err();
    assert_eq!(error, "a menu `More` of panel `controls` has no item `Sort`");
    panel.send("Shape", craft_plugin::UiEventKind::Points(vec![[0.0, 0.0], [0.5, 0.7], [1.0, 1.0]])).unwrap();
    let ramp = craft_plugin::Gradient::new(&[(0.0, [0.0; 4]), (0.5, [1.0; 4]), (1.0, [0.0; 4])]);
    panel.send("Ramp", craft_plugin::UiEventKind::Gradient(ramp)).unwrap();
    let texts = panel.texts();
    assert!(texts.contains(&"3 points".to_string()) && texts.contains(&"3 stops".to_string()), "{texts:?}");
    assert!(texts.contains(&"Probe controls".to_string()), "{texts:?}");
}

#[test]
fn the_preview_panel_asks_the_app_for_a_frame_widget_and_never_reads_pixels_itself() {
    let (_bundle, plugin) = probe();
    let read = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let counted = read.clone();
    plugin.frames(move |_| {
        counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Err("the preview must not read frames".into())
    });
    let mut panel = plugin.panel("preview").unwrap();
    let frame = |panel: &craft_plugin_host::testing::PanelTest| {
        panel.widgets().iter().find_map(|widget| match widget {
            craft_plugin::Widget::Frame { request, size, .. } => Some((request.comp.clone(), request.time, request.scale, *size)),
            _ => None,
        })
    };
    assert_eq!(frame(&panel), Some((None, 0.0, 0.25, [160.0, 90.0])));
    panel.send("Composition", UiEventKind::Text("Shot 2".into())).unwrap();
    assert_eq!(frame(&panel), Some((Some("Shot 2".into()), 0.0, 0.25, [160.0, 90.0])));
    panel.click_frame(0).unwrap();
    panel.click_frame(0).unwrap();
    assert!(panel.texts().contains(&"2 clicks".to_string()), "{:?}", panel.texts());
    assert_eq!(panel.click_frame(1).unwrap_err(), "panel `preview` shows 1 frames, so it has no frame 1");
    assert_eq!(read.load(std::sync::atomic::Ordering::Relaxed), 0);
}

#[test]
fn a_pad_right_click_reaches_a_plugin_at_ui_level_two_and_never_one_at_the_old_level() {
    use craft_plugin::{Pointer, PointerPhase, UiEvent, Widget};
    use craft_plugin_host::Retained;
    let (_bundle, plugin) = probe();
    let level = plugin.plugin().contributions().ui_level;
    let said = |mut retained: Retained, kind: fn(Pointer) -> UiEventKind| {
        let mut panel = plugin.panel("pad").unwrap();
        let pad = panel.widgets().iter().find_map(|widget| if let Widget::Canvas { id, .. } = widget { Some(*id) } else { None }).unwrap();
        retained.push(vec![UiEvent { id: pad, kind: kind(Pointer::new([10.0, 20.0], PointerPhase::Down)) }], "");
        panel.input(retained.start([320.0, 480.0], std::time::Duration::ZERO).unwrap()).unwrap();
        panel.texts().into_iter().find(|text| text.starts_with("pad")).unwrap_or_default()
    };
    let new = || Retained::with_level(level);
    assert_eq!(
        [said(Retained::new(), UiEventKind::Context), said(new(), UiEventKind::Context), said(new(), UiEventKind::Open)],
        ["pad  ink [0.0, 0.0, 0.0, 1.0]", "pad context 10 20 ink [0.0, 0.0, 0.0, 1.0]", "pad open 10 20 ink [0.0, 0.0, 0.0, 1.0]"]
    );
    let mut panel = plugin.panel("pad").unwrap();
    panel.send("Ink", UiEventKind::Color([1.0, 0.0, 0.0, 1.0])).unwrap();
    assert_eq!(panel.texts().last().map(String::as_str), Some("pad open 10 20 ink [1.0, 0.0, 0.0, 1.0]"));
}
