use std::collections::BTreeMap;

use craft_plugin::{Channel, Place, Value};
use craft_plugin_host::AuxPlanes;
use craft_plugin_host::testing::{Pattern, Picture, RENDER_LAYER, TestPlugin, check_frame, json, updating_frames};

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
    let depth: Vec<f32> = (0..6).map(|i| 100.0 * i as f32).collect();
    let normal: Vec<f32> = (0..6).flat_map(|i| [0.0, 0.0, -(i as f32) / 10.0]).collect();
    let aux = AuxPlanes { width: 3, height: 2, planes: vec![(Channel::Normal, normal), (Channel::Depth, depth), (Channel::Uv, vec![0.0; 12])] };
    let read = plugin.render("aux-depth").size(3, 2).aux(aux).float().run().unwrap();
    assert_eq!([read.at(0, 0), read.at(2, 1)], [[0.0, 0.0, 0.0, 1.0], [0.5, 0.5, 0.0, 1.0]]);
    let plain = plugin.render("aux-depth").size(3, 2).float().run().unwrap();
    assert_eq!(plain.at(1, 1), [0.0, 0.0, 1.0, 1.0]);
}

#[test]
fn an_effect_reading_a_channel_it_did_not_declare_fails_and_says_what_to_declare() {
    let (_bundle, plugin) = probe();
    let error = plugin.render("aux-undeclared").size(4, 4).run().unwrap_err();
    assert!(error.contains("effect `aux-undeclared` must declare `.reads_aux([Channel::Uv])`"), "{error}");
}
