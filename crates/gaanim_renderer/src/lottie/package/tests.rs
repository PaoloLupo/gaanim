use super::*;
use std::io::{Cursor, Write};
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn animation() -> Value {
    json!({"v":"5.7.4","w":100,"h":100,"fr":20,"ip":10,"op":50,"assets":[],"layers":[
        {"ty":4,"ind":1,"ip":10,"op":50,"st":0,"ks":{"o":{"a":0,"k":100},"p":{"a":0,"k":[50,50]},"s":{"a":0,"k":[100,100]},"a":{"a":0,"k":[0,0]},"r":{"a":0,"k":0}},
         "shapes":[{"ty":"rc","p":{"a":0,"k":[0,0]},"s":{"a":0,"k":[40,40]},"r":{"a":0,"k":0}},
         {"ty":"fl","c":{"sid":"color","a":0,"k":[1,0,0]},"o":{"a":0,"k":100},"r":1}]}
    ],"markers":[{"cm":"short","tm":20,"dr":10}]})
}

fn machine() -> Value {
    json!({"initial":"idle","inputs":[{"type":"Boolean","name":"active","value":false},{"type":"Event","name":"reset"},
        {"type":"Numeric","name":"count","value":0},{"type":"Numeric","name":"threshold","value":2},{"type":"String","name":"label","value":"idle"}],
        "states":[
        {"name":"idle","type":"PlaybackState","animation":"one","transitions":[{"type":"Transition","toState":"active","guards":[{"type":"Boolean","inputName":"active","conditionType":"Equal","compareTo":true}]}]},
        {"name":"active","type":"PlaybackState","animation":"two","mode":"Reverse","transitions":[{"type":"Transition","toState":"done","guards":[{"type":"Event","inputName":"reset"}]}]},
        {"name":"done","type":"PlaybackState","animation":"one","final":true,"autoplay":false}
    ]})
}

fn fixture(version: &str, change: impl FnOnce(&mut HashMap<String, Value>)) -> Fixture {
    let dir = if version == "1" { "animations" } else { "a" };
    let mut files = HashMap::from([
        (
            "manifest.json".into(),
            json!({"version":version,"animations":[{"id":"one"},{"id":"two"}],"themes":[{"id":"blue"}],"stateMachines":[{"id":"main"}],"initial":{"animation":"two"}}),
        ),
        (format!("{dir}/one.json"), animation()),
        (format!("{dir}/two.json"), animation()),
        (
            "t/blue.json".into(),
            json!({"rules":[{"id":"color","type":"Color","value":[0,0,1]}]}),
        ),
        ("s/main.json".into(), machine()),
    ]);
    change(&mut files);
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, value) in files {
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer
            .write_all(&serde_json::to_vec(&value).unwrap())
            .unwrap();
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "gaanim-dotlottie-{}-{}.lottie",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, writer.finish().unwrap().into_inner()).unwrap();
    Fixture(path)
}

#[test]
fn lottie_v1_v2_selection_cache_and_json_equivalence() {
    for version in ["1", "2"] {
        let file = fixture(version, |files| {
            if version == "1" {
                files.get_mut("manifest.json").unwrap()["activeAnimationId"] = json!("one");
                files
                    .get_mut("manifest.json")
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove("initial");
            }
        });
        let session = PackagePlayback::load(&file.0, &Default::default()).unwrap();
        assert_eq!(
            session.animation,
            if version == "1" { "one" } else { "two" }
        );
        let other = PackagePlayback::load(
            &file.0,
            &LottiePackageOptions {
                animation_id: Some("one".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(Arc::ptr_eq(&session.package, &other.package));
        let packaged = other.initial_asset().unwrap();
        let plain = LottieAsset::from_json(PathBuf::from("test.json"), animation(), None).unwrap();
        assert_eq!(packaged.duration(), plain.duration());
        assert_eq!(
            packaged.composition.layers.len(),
            plain.composition.layers.len()
        );
        assert_eq!(packaged.warnings(), plain.warnings());
        let render = |asset| {
            let view = gaanim_objects::prelude::ImageView {
                source_x: 0.0,
                source_y: 0.0,
                source_width: 100.0,
                source_height: 100.0,
                display_width: 100.0,
                display_height: 100.0,
                scale_x: 1.0,
                scale_y: 1.0,
                quality: Default::default(),
            };
            let player = super::super::LottiePlayer::new(
                super::super::LottiePlayback::new(asset, view, 0.0, None, false, 1.0).unwrap(),
            );
            let encoded = player.scene().encoding();
            (
                encoded.path_data.clone(),
                encoded.draw_data.clone(),
                encoded.transforms.clone(),
            )
        };
        assert_eq!(render(packaged), render(plain));
        assert!(
            PackagePlayback::load(
                &file.0,
                &LottiePackageOptions {
                    animation_id: Some("missing".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}

#[test]
fn lottie_machine_commands_are_reversible_and_instances_are_independent() {
    let file = fixture("2", |_| {});
    let mut session = PackagePlayback::load(
        &file.0,
        &LottiePackageOptions {
            state_machine_id: Some("main".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let other = session.clone();
    assert!(
        session
            .record(LottieCommand::Event("reset".into()), 0.0, 0.0, false)
            .is_err()
    );
    assert!(
        session
            .record(
                LottieCommand::Input("active".into(), LottieInput::Numeric(1.0)),
                0.0,
                0.0,
                false
            )
            .is_err()
    );
    assert!(
        session
            .record(
                LottieCommand::Input("count".into(), LottieInput::Numeric(f64::NAN)),
                0.0,
                0.0,
                false
            )
            .is_err()
    );
    session
        .record(
            LottieCommand::Input("active".into(), LottieInput::Boolean(true)),
            2.0,
            1.0,
            true,
        )
        .unwrap();
    session
        .record(LottieCommand::Theme(Some("blue".into())), 2.0, 1.0, true)
        .unwrap();
    session
        .record(LottieCommand::Event("reset".into()), 3.0, 1.0, true)
        .unwrap();
    let frames: Vec<_> = [0.0, 1.5, 2.0, 2.5, 3.0, 4.0]
        .iter()
        .map(|t| session.sample(*t, 1.0).unwrap().frame.unwrap())
        .collect();
    assert_eq!(frames[0], 10.0);
    assert_eq!(frames[1], 20.0);
    assert!((frames[2] - 50.0).abs() < 1e-5);
    assert_eq!(frames[3], 40.0);
    assert_eq!(frames[4], 10.0);
    for (i, t) in [0.0, 1.5, 2.0, 2.5, 3.0, 4.0].iter().enumerate().rev() {
        assert_eq!(session.sample(*t, 1.0).unwrap().frame.unwrap(), frames[i]);
    }
    assert!(!Arc::ptr_eq(
        &session.sample(2.5, 1.0).unwrap().asset,
        &other.sample(2.5, 1.0).unwrap().asset
    ));
    assert!(Arc::ptr_eq(
        &session.sample(1.5, 1.0).unwrap().asset,
        &other.sample(1.5, 1.0).unwrap().asset
    ));
}

#[test]
fn lottie_same_time_events_preserve_declaration_order_and_initial_inputs() {
    let file = fixture("2", |_| {});
    let mut session = PackagePlayback::load(
        &file.0,
        &LottiePackageOptions {
            state_machine_id: Some("main".into()),
            ..Default::default()
        },
    )
    .unwrap();
    session
        .record(
            LottieCommand::Input("active".into(), LottieInput::Boolean(true)),
            0.0,
            0.0,
            false,
        )
        .unwrap();
    assert!((session.sample(2.0, 2.0).unwrap().frame.unwrap() - 50.0).abs() < 1e-5);
    session
        .record(LottieCommand::Event("reset".into()), 2.0, 2.0, true)
        .unwrap();
    assert_eq!(session.sample(2.0, 2.0).unwrap().frame, Some(10.0));
    assert!((session.sample(1.0, 2.0).unwrap().frame.unwrap() - 50.0).abs() < 1e-5);
}

#[test]
fn lottie_unsupported_selected_content_is_rejected_without_blocking_other_animations() {
    for field in ["entryActions", "interactions", "tween", "reserved", "theme"] {
        let file = fixture("2", |files| match field {
            "entryActions" => {
                files.get_mut("s/main.json").unwrap()["states"][0]["entryActions"] =
                    json!([{"type":"SetTheme","value":"blue"}])
            }
            "interactions" => {
                files.get_mut("s/main.json").unwrap()["interactions"] = json!([{"type":"Click"}])
            }
            "tween" => {
                files.get_mut("s/main.json").unwrap()["states"][0]["transitions"][0]["type"] =
                    json!("Tweened")
            }
            "reserved" => {
                files.get_mut("s/main.json").unwrap()["inputs"][2]["name"] = json!("elapsed_time")
            }
            _ => files.get_mut("t/blue.json").unwrap()["rules"][0]["keyframes"] = json!([]),
        });
        assert!(PackagePlayback::load(&file.0, &Default::default()).is_ok());
        let options = if field == "theme" {
            LottiePackageOptions {
                theme_id: Some("blue".into()),
                ..Default::default()
            }
        } else {
            LottiePackageOptions {
                state_machine_id: Some("main".into()),
                ..Default::default()
            }
        };
        assert!(PackagePlayback::load(&file.0, &options).is_err(), "{field}");
    }
}

#[test]
fn lottie_cycles_fail_transactionally() {
    let file = fixture("2", |files| {
        files.get_mut("s/main.json").unwrap()["states"][1]["transitions"] =
            json!([{"type":"Transition","toState":"idle"}]);
    });
    let mut session = PackagePlayback::load(
        &file.0,
        &LottiePackageOptions {
            state_machine_id: Some("main".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        session
            .record(
                LottieCommand::Input("active".into(), LottieInput::Boolean(true)),
                1.0,
                0.0,
                true
            )
            .is_err()
    );
    assert!(session.commands.is_empty());
    assert_eq!(session.sample(1.0, 0.0).unwrap().frame, Some(30.0));
}

#[test]
fn lottie_themes_cover_all_static_vector_rule_types() {
    let mut source = json!({"c":{"sid":"c","a":1,"k":[]},"s":{"sid":"s","k":0},"p":{"sid":"p","k":[]},"v":{"sid":"v","k":[]},"g":{"p":2,"k":{"sid":"g","k":[]}}});
    apply_theme(&mut source,&json!({"rules":[
        {"id":"c","type":"Color","value":[0,1,0]}, {"id":"s","type":"Scalar","value":42},
        {"id":"p","type":"Position","value":[10,20,0]}, {"id":"v","type":"Vector","value":[30,40]},
        {"id":"g","type":"Gradient","value":[{"offset":0,"color":[1,0,0,0.5]},{"offset":0.5,"color":[0,1,0]},{"offset":1,"color":[0,0,1]}]}
    ]}),"one").unwrap();
    assert_eq!(source["c"]["a"], 0);
    assert_eq!(source["s"]["k"], 42);
    assert_eq!(source["p"]["k"], json!([10, 20, 0]));
    assert_eq!(source["v"]["k"], json!([30, 40]));
    assert_eq!(source["g"]["p"], 3);
    assert_eq!(source["g"]["k"]["k"].as_array().unwrap().len(), 18);
}

#[test]
fn lottie_package_paths_cannot_escape_or_read_host_files() {
    for name in [
        "../../outside.png",
        "/absolute.png",
        "C:/image.png",
        "..\\image.png",
        "https://example.com/a.png",
    ] {
        assert!(safe_path(name).is_err(), "{name}");
    }
    let file = fixture("2", |_| {});
    let package = Package::load(&file.0).unwrap();
    assert!(package.image(Some("../../"), "secret.png").is_err());
    assert!(package.image(None, "manifest.json").is_ok()); // only ZIP bytes, never host files
    let missing = fixture("2", |files| {
        files.remove("manifest.json");
    });
    assert!(Package::load(&missing.0).is_err());
}

#[test]
fn lottie_packaged_images_and_manifest_initial_machine() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for path in [
        root.join("examples/assets/dotlottie_demo.lottie"),
        root.join("tests/assets/dotlottie_v1.lottie"),
    ] {
        let asset = LottieAsset::load(&path).unwrap();
        assert_eq!(asset.image_layers.len(), 1);
        let package = Package::load(&path).unwrap();
        assert!(
            package
                .image(None, "pixel.png")
                .unwrap()
                .starts_with(b"\x89PNG")
        );
    }
    let file = fixture("2", |files| {
        files.get_mut("manifest.json").unwrap()["initial"] = json!({"stateMachine":"main"});
    });
    assert!(
        PackagePlayback::load(&file.0, &Default::default())
            .unwrap()
            .is_machine()
    );
    assert!(
        !PackagePlayback::load(
            &file.0,
            &LottiePackageOptions {
                animation_id: Some("one".into()),
                ..Default::default()
            }
        )
        .unwrap()
        .is_machine()
    );
}

#[test]
fn lottie_state_modes_loops_markers_and_background_use_scene_time() {
    for (mode, quarter, end) in [
        ("Forward", 25.0, 30.0),
        ("Reverse", 25.0, 20.0),
        ("Bounce", 25.0, 20.0),
        ("ReverseBounce", 25.0, 30.0),
    ] {
        let file = fixture("2", |files| {
            let state = &mut files.get_mut("s/main.json").unwrap()["states"][0];
            state["mode"] = json!(mode);
            state["segment"] = json!("short");
            state["loop"] = json!(true);
            state["loopCount"] = json!(2);
            state["backgroundColor"] = json!(0xff000080_u32);
        });
        let session = PackagePlayback::load(
            &file.0,
            &LottiePackageOptions {
                state_machine_id: Some("main".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let sample = session.sample(4.25, 4.0).unwrap();
        assert!((sample.frame.unwrap() - quarter).abs() < 1e-5, "{mode}");
        assert_eq!(sample.background, Some(0xff000080));
        assert!(
            (session.sample(20.0, 4.0).unwrap().frame.unwrap() - end).abs() < 1e-5,
            "{mode}"
        );
    }
}

#[test]
fn lottie_global_guards_take_priority_and_numeric_string_references_work() {
    let file = fixture("2", |files| {
        let machine = files.get_mut("s/main.json").unwrap();
        machine["states"][0]["transitions"] = json!([
            {"type":"Transition","toState":"active","guards":[
                {"type":"Numeric","inputName":"count","conditionType":"GreaterThanOrEqual","compareTo":"$threshold"},
                {"type":"String","inputName":"label","conditionType":"NotEqual","compareTo":"idle"}]}]);
        machine["states"].as_array_mut().unwrap().push(json!({"name":"global","type":"GlobalState","transitions":[
            {"type":"Transition","toState":"done","guards":[{"type":"Event","inputName":"reset"}]}]}));
    });
    let options = LottiePackageOptions {
        state_machine_id: Some("main".into()),
        ..Default::default()
    };
    let mut session = PackagePlayback::load(&file.0, &options).unwrap();
    session
        .record(
            LottieCommand::Input("count".into(), LottieInput::Numeric(3.0)),
            1.0,
            0.0,
            true,
        )
        .unwrap();
    assert_eq!(session.sample(1.0, 0.0).unwrap().frame, Some(30.0));
    session
        .record(
            LottieCommand::Input("label".into(), LottieInput::String("go".into())),
            1.0,
            0.0,
            true,
        )
        .unwrap();
    assert!((session.sample(1.0, 0.0).unwrap().frame.unwrap() - 50.0).abs() < 1e-5);
    let mut global = PackagePlayback::load(&file.0, &options).unwrap();
    global
        .record(LottieCommand::Event("reset".into()), 1.0, 0.0, true)
        .unwrap();
    assert_eq!(global.sample(1.0, 0.0).unwrap().frame, Some(10.0));
}

#[test]
fn lottie_invalid_archive_and_missing_animation_are_errors() {
    let file = fixture("2", |files| {
        files.remove("a/two.json");
    });
    assert!(PackagePlayback::load(&file.0, &Default::default()).is_err());
    assert!(
        PackagePlayback::load(
            &file.0,
            &LottiePackageOptions {
                animation_id: Some("one".into()),
                ..Default::default()
            }
        )
        .is_ok()
    );
    let corrupt = fixture("2", |_| {});
    std::fs::write(&corrupt.0, b"this is not a zip file").unwrap();
    assert!(Package::load(&corrupt.0).is_err());
}

#[test]
fn lottie_subframe_markers_do_not_panic_at_the_endpoint() {
    let file = fixture("2", |files| {
        files.get_mut("a/one.json").unwrap()["markers"][0]["dr"] = json!(1e-10);
        files.get_mut("s/main.json").unwrap()["states"][0]["segment"] = json!("short");
    });
    let session = PackagePlayback::load(
        &file.0,
        &LottiePackageOptions {
            state_machine_id: Some("main".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let frame = session.sample(5.0, 0.0).unwrap().frame.unwrap();
    assert!((20.0..20.0 + 1e-10).contains(&frame));
}

#[test]
fn lottie_global_override_does_not_reenter_the_current_playback_state() {
    let file = fixture("2", |files| {
        let machine = files.get_mut("s/main.json").unwrap();
        machine["initial"] = json!("global");
        machine["states"].as_array_mut().unwrap().push(json!({"name":"global","type":"GlobalState","transitions":[
            {"type":"Transition","toState":"active","guards":[{"type":"Boolean","inputName":"active","conditionType":"Equal","compareTo":true}]},
            {"type":"Transition","toState":"idle","guards":[{"type":"Boolean","inputName":"active","conditionType":"Equal","compareTo":false}]}
        ]}));
    });
    let mut session = PackagePlayback::load(
        &file.0,
        &LottiePackageOptions {
            state_machine_id: Some("main".into()),
            ..Default::default()
        },
    )
    .unwrap();
    session
        .record(
            LottieCommand::Input("active".into(), LottieInput::Boolean(true)),
            1.0,
            0.0,
            true,
        )
        .unwrap();
    session
        .record(
            LottieCommand::Input("count".into(), LottieInput::Numeric(5.0)),
            1.5,
            0.0,
            true,
        )
        .unwrap();
    assert_eq!(session.sample(1.5, 0.0).unwrap().frame, Some(40.0));
}
