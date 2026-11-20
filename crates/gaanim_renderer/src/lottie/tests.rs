use super::*;

fn asset() -> Arc<LottieAsset> {
    Arc::new(LottieAsset {
        path: PathBuf::from("test.json"),
        source_digest: 0,
        composition: velato::Composition {
            frames: 10.0..70.0,
            frame_rate: 30.0,
            width: 100,
            height: 50,
            ..Default::default()
        },
        image_layers: Vec::new(),
        solid_layers: Vec::new(),
        warnings: Vec::new(),
    })
}

#[test]
fn playback_maps_scene_time_to_source_frames() {
    let view = ImageView {
        source_x: 0.0,
        source_y: 0.0,
        source_width: 100.0,
        source_height: 50.0,
        display_width: 100.0,
        display_height: 50.0,
        scale_x: 1.0,
        scale_y: 1.0,
        quality: Default::default(),
    };
    let mut playback =
        LottiePlayback::new(asset(), view, 0.5, Some(1.0), false, 2.0).expect("valid playback");
    playback.active = true;
    playback.scene_start = 3.0;
    assert_eq!(playback.source_frame(3.0), 25.0);
    assert_eq!(playback.source_frame(3.25), 40.0);
    assert!(playback.source_frame(4.0) < 55.0);
}

#[test]
fn unsupported_shapes_are_removed_with_a_warning() {
    let mut json = serde_json::json!({
        "layers": [{"ty": 4, "shapes": [{"ty": "el"}, {"ty": "zz"}]}]
    });
    let warnings = compatibility_warnings(&mut json);
    assert!(
        warnings
            .iter()
            .any(|warning| warning.starts_with("unknown_shape:"))
    );
    assert_eq!(json["layers"][0]["shapes"].as_array().unwrap().len(), 1);
}

#[test]
fn gradient_alpha_stops_are_preserved_for_static_and_animated_gradients() {
    let static_values = serde_json::json!([
        0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.5, 0.0, 1.0, 1.0
    ]);
    let animated_values =
        serde_json::json!([0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.25, 1.0, 0.75, 0.0]);
    let mut json = serde_json::json!({
        "v": "5.7.4",
        "fr": 30.0,
        "ip": 0.0,
        "op": 30.0,
        "w": 100,
        "h": 100,
        "layers": [{
            "ddd": 0,
            "ty": 4,
            "ind": 1,
            "st": 0.0,
            "ip": 0.0,
            "op": 30.0,
            "ks": {
                "a": {"a": 0, "k": [0.0, 0.0]},
                "p": {"a": 0, "k": [0.0, 0.0]},
                "s": {"a": 0, "k": [100.0, 100.0]},
                "r": {"a": 0, "k": 0.0},
                "o": {"a": 0, "k": 100.0}
            },
            "shapes": [
                {
                    "ty": "rc",
                    "p": {"a": 0, "k": [50.0, 50.0]},
                    "s": {"a": 0, "k": [100.0, 100.0]},
                    "r": {"a": 0, "k": 0.0}
                },
                {
                    "ty": "gf",
                    "g": {"p": 2, "k": {"a": 0, "k": static_values}},
                    "s": {"a": 0, "k": [0.0, 50.0]},
                    "e": {"a": 0, "k": [100.0, 50.0]},
                    "t": 1,
                    "o": {"a": 0, "k": 100.0},
                    "r": 1
                },
                {
                    "ty": "gf",
                    "g": {"p": 2, "k": {"a": 1, "k": [
                        {"t": 0.0, "s": animated_values.clone()},
                        {"t": 30.0, "s": animated_values}
                    ]}},
                    "s": {"a": 0, "k": [0.0, 50.0]},
                    "e": {"a": 0, "k": [100.0, 50.0]},
                    "t": 1,
                    "o": {"a": 0, "k": 100.0},
                    "r": 1
                }
            ]
        }]
    });

    let warnings = compatibility_warnings(&mut json);
    let static_gradient = &json["layers"][0]["shapes"][1]["g"];
    let animated_gradient = &json["layers"][0]["shapes"][2]["g"];
    let static_values = static_gradient["k"]["k"].as_array().unwrap();

    assert_eq!(static_gradient["p"], 5);
    assert_eq!(static_values.len(), 30);
    assert_eq!(static_values[8], 0.5);
    assert_eq!(static_values[9], 0.5);
    assert_eq!(static_values[10], 0.0);
    assert_eq!(static_values[11], 0.5);
    assert_eq!(static_values[25], 0.0);
    assert_eq!(animated_gradient["p"], 4);
    assert_eq!(
        animated_gradient["k"]["k"][0]["s"]
            .as_array()
            .unwrap()
            .len(),
        24
    );
    assert!(
        warnings
            .iter()
            .all(|warning| !warning.starts_with("gradient_color_stops:"))
    );
    assert!(velato::Composition::from_json(json).is_ok());
}

#[test]
fn omitted_transform_defaults_load_and_render_in_layers_groups_and_precompositions() {
    let layer = serde_json::json!({
        "ty": 4, "ind": 1, "st": 0, "ip": 0, "op": 30,
        "ks": {"o": {"a": 0, "k": 75}},
        "shapes": [{"ty": "gr", "it": [
            {"ty": "gr", "it": [
                {"ty": "rc", "p": {"a": 0, "k": [50, 50]},
                 "s": {"a": 0, "k": [20, 20]}, "r": {"a": 0, "k": 0}},
                {"ty": "fl", "c": {"a": 0, "k": [1, 0, 0, 1]},
                 "o": {"a": 0, "k": 100}, "r": 1},
                {"ty": "tr", "o": {"a": 0, "k": 50}}
            ]},
            {"ty": "tr"}
        ]}]
    });
    for in_precomposition in [false, true] {
        let mut json = serde_json::json!({
            "v": "5.7.5", "fr": 30, "ip": 0, "op": 30, "w": 100, "h": 100,
            "layers": [layer.clone()]
        });
        if in_precomposition {
            json["assets"] = serde_json::json!([{"id": "nested", "layers": [layer.clone()]}]);
            json["layers"] = serde_json::json!([{
                "ty": 0, "ind": 1, "st": 0, "ip": 0, "op": 30,
                "refId": "nested", "w": 100, "h": 100, "ks": {}
            }]);
        }
        let error = velato::Composition::from_json(json.clone()).unwrap_err();
        assert!(error.to_string().contains("missing field `p`"));
        assert!(compatibility_warnings(&mut json).is_empty());
        let composition = velato::Composition::from_json(json).expect("identity defaults parse");
        assert_eq!(
            composition.layers[0].transform.evaluate(0.0).into_owned(),
            vello::kurbo::Affine::IDENTITY,
            "omitted position and scale must preserve placement and size"
        );
        let mut renderer = velato::Renderer::new();
        for frame in [0.0, 15.0, 29.0] {
            let scene = renderer.render_to_vello_scene(
                &composition,
                frame,
                vello::kurbo::Affine::IDENTITY,
                1.0,
            );
            assert!(
                !scene.encoding().path_data.is_empty(),
                "the rectangle must render"
            );
        }
    }
}

#[test]
fn existing_transform_positions_and_scales_are_preserved() {
    let positions = [
        serde_json::json!({"a": 0, "k": [10, 20]}),
        serde_json::json!({"a": 1, "k": [
            {"t": 0, "s": [10, 20]}, {"t": 30, "s": [30, 40]}
        ]}),
    ];
    for position in positions {
        for shape_transform in [false, true] {
            let mut transform = serde_json::json!({"p": position.clone(), "s": position.clone()});
            sanitize_transform(Some(&mut transform), shape_transform, &mut BTreeMap::new());
            assert_eq!(transform["p"], position);
            assert_eq!(transform["s"], position);
        }
    }
}

#[test]
fn layer_split_position_is_supported_without_a_warning() {
    let mut json = serde_json::json!({
        "layers": [{
            "ty": 3,
            "ks": {
                "p": {
                    "s": true,
                    "x": {"a": 0, "k": 10.0},
                    "y": {"a": 0, "k": 20.0}
                },
                "r": {"a": 0, "k": 0.0}
            }
        }]
    });

    let warnings = compatibility_warnings(&mut json);

    assert!(
        warnings
            .iter()
            .all(|warning| !warning.starts_with("split_position:"))
    );
}

#[test]
fn known_velato_import_panics_are_sanitized() {
    let mut json = serde_json::json!({
        "v": "5.7.4",
        "fr": 30.0,
        "ip": 0.0,
        "op": 30.0,
        "w": 100,
        "h": 100,
        "assets": [{
            "id": "image_0",
            "w": 10,
            "h": 10,
            "u": "images/",
            "p": "dot.png",
            "e": 0
        }],
        "layers": [{
            "ddd": 0,
            "ty": 4,
            "ind": 1,
            "st": 0.0,
            "ip": 0.0,
            "op": 30.0,
            "bm": 16,
            "ks": {
                "a": {"a": 0, "k": [0.0, 0.0]},
                "p": {
                    "s": true,
                    "x": {"a": 0, "k": 50.0},
                    "y": {"a": 0, "k": 50.0}
                },
                "s": {"a": 0, "k": [100.0, 100.0]},
                "o": {"a": 0, "k": 100.0}
            },
            "shapes": [{
                "ty": "gr",
                "it": [
                    {
                        "ty": "el",
                        "p": {"a": 0, "k": [0.0, 0.0]},
                        "s": {"a": 0, "k": [20.0, 20.0]}
                    },
                    {
                        "ty": "fl",
                        "c": {"a": 0, "k": [1.0, 0.0, 0.0, 1.0]},
                        "o": {"a": 0, "k": 100.0}
                    },
                    {
                        "ty": "tr",
                        "p": {
                            "s": true,
                            "x": {"a": 0, "k": 10.0},
                            "y": {"a": 0, "k": 20.0}
                        },
                        "a": {"a": 0, "k": [0.0, 0.0]},
                        "s": {"a": 0, "k": [100.0, 100.0]},
                        "r": {
                            "x": {"a": 0, "k": 0.0},
                            "y": {"a": 0, "k": 0.0},
                            "z": {"a": 0, "k": 15.0},
                            "or": {"a": 0, "k": [0.0, 0.0, 0.0]}
                        },
                        "o": {"a": 0, "k": 100.0}
                    }
                ]
            }]
        }]
    });

    let warnings = compatibility_warnings(&mut json);
    assert_eq!(json["assets"].as_array().unwrap().len(), 0);
    let result = std::panic::catch_unwind(|| velato::Composition::from_json(json));

    assert!(result.is_ok(), "sanitized Lottie conversion must not panic");
    assert!(result.unwrap().is_ok(), "sanitized Lottie must parse");
    assert!(
        warnings
            .iter()
            .any(|warning| warning.starts_with("blend_mode:"))
    );
    assert!(
        warnings
            .iter()
            .any(|warning| warning.starts_with("split_rotation:"))
    );
}

#[test]
fn external_image_layer_is_decoded_and_added_to_the_vello_scene() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "gaanim-lottie-image-{}-{unique}",
        std::process::id()
    ));
    let image_dir = root.join("images");
    std::fs::create_dir_all(&image_dir).unwrap();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
        .save(image_dir.join("dot.png"))
        .unwrap();
    let json_path = root.join("image.json");
    std::fs::write(
        &json_path,
        serde_json::to_vec(&serde_json::json!({
            "v": "5.7.4",
            "fr": 30.0,
            "ip": 0.0,
            "op": 30.0,
            "w": 100,
            "h": 100,
            "assets": [{
                "id": "image_0",
                "w": 20,
                "h": 10,
                "u": "images/",
                "p": "dot.png",
                "e": 0
            }],
            "layers": [{
                "ddd": 0,
                "ty": 2,
                "ind": 1,
                "st": 0.0,
                "ip": 0.0,
                "op": 30.0,
                "refId": "image_0",
                "ks": {
                    "a": {"a": 0, "k": [0.0, 0.0]},
                    "p": {"a": 0, "k": [10.0, 20.0]},
                    "s": {"a": 0, "k": [100.0, 100.0]},
                    "r": {"a": 0, "k": 0.0},
                    "o": {"a": 0, "k": 100.0}
                }
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let asset = LottieAsset::load(&json_path).unwrap();
    assert_eq!(asset.image_layers.len(), 1);
    assert!(
        asset
            .warnings()
            .iter()
            .all(|warning| !warning.starts_with("image_layer:"))
    );
    let view = ImageView {
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
    let playback = LottiePlayback::new(asset, view, 0.0, None, false, 1.0).unwrap();
    let player = LottiePlayer::new(playback);
    assert_eq!(player.scene().encoding().resources.patches.len(), 1);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn image_and_solid_layers_inside_a_precomposition_are_rendered() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "gaanim-lottie-precomp-{}-{unique}",
        std::process::id()
    ));
    let image_dir = root.join("images");
    std::fs::create_dir_all(&image_dir).unwrap();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
        .save(image_dir.join("dot.png"))
        .unwrap();
    let json_path = root.join("precomp.json");
    let transform = serde_json::json!({
        "a": {"a": 0, "k": [0.0, 0.0]},
        "p": {"a": 0, "k": [10.0, 20.0]},
        "s": {"a": 0, "k": [100.0, 100.0]},
        "r": {"a": 0, "k": 0.0},
        "o": {"a": 0, "k": 100.0}
    });
    std::fs::write(
        &json_path,
        serde_json::to_vec(&serde_json::json!({
            "v": "5.7.4",
            "fr": 30.0,
            "ip": 0.0,
            "op": 30.0,
            "w": 100,
            "h": 100,
            "assets": [
                {
                    "id": "image_0",
                    "w": 20,
                    "h": 10,
                    "u": "images/",
                    "p": "dot.png",
                    "e": 0
                },
                {
                    "id": "precomp_0",
                    "layers": [
                        {
                            "ddd": 0,
                            "ty": 1,
                            "ind": 1,
                            "nm": "Nested Layer",
                            "st": 0.0,
                            "ip": 0.0,
                            "op": 30.0,
                            "sw": 30.0,
                            "sh": 15.0,
                            "sc": "#336699",
                            "ks": transform.clone()
                        },
                        {
                            "ddd": 0,
                            "ty": 2,
                            "ind": 2,
                            "nm": "Nested Layer",
                            "st": 0.0,
                            "ip": 0.0,
                            "op": 30.0,
                            "refId": "image_0",
                            "ks": transform.clone()
                        }
                    ]
                }
            ],
            "layers": [{
                "ddd": 0,
                "ty": 0,
                "ind": 1,
                "nm": "Container",
                "st": 0.0,
                "ip": 0.0,
                "op": 30.0,
                "sr": 1.0,
                "w": 100.0,
                "h": 100.0,
                "refId": "precomp_0",
                "ks": transform
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let asset = LottieAsset::load(&json_path).unwrap();
    assert_eq!(
        asset.image_layers[0].precomposition.as_deref(),
        Some("precomp_0")
    );
    assert_eq!(
        asset.solid_layers[0].precomposition.as_deref(),
        Some("precomp_0")
    );
    assert!(
        asset
            .warnings()
            .iter()
            .all(|warning| !warning.starts_with("nested_image_layer:"))
    );
    let view = ImageView {
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
    let without_custom_content = Arc::new(LottieAsset {
        path: json_path.clone(),
        source_digest: asset.source_digest,
        composition: asset.composition.clone(),
        image_layers: Vec::new(),
        solid_layers: Vec::new(),
        warnings: Vec::new(),
    });
    let without_custom_content = LottiePlayer::new(
        LottiePlayback::new(without_custom_content, view, 0.0, None, false, 1.0).unwrap(),
    );
    let player =
        LottiePlayer::new(LottiePlayback::new(asset, view, 0.0, None, false, 1.0).unwrap());

    assert_eq!(player.scene().encoding().resources.patches.len(), 1);
    assert!(
        player.scene().encoding().draw_tags.len()
            > without_custom_content.scene().encoding().draw_tags.len()
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn solid_layer_is_added_to_the_vello_scene() {
    let mut json = serde_json::json!({
        "v": "5.7.4",
        "fr": 30.0,
        "ip": 0.0,
        "op": 30.0,
        "w": 100,
        "h": 100,
        "layers": [{
            "ddd": 0,
            "ty": 1,
            "ind": 1,
            "st": 0.0,
            "ip": 0.0,
            "op": 30.0,
            "sw": 20.0,
            "sh": 10.0,
            "sc": "#336699",
            "ks": {
                "a": {"a": 0, "k": [0.0, 0.0]},
                "p": {"a": 0, "k": [10.0, 20.0]},
                "s": {"a": 0, "k": [100.0, 100.0]},
                "r": {"a": 0, "k": 0.0},
                "o": {"a": 0, "k": 50.0}
            }
        }]
    });

    let mut warnings = compatibility_warnings(&mut json);
    let solid_layers = solid_layer_specs(&json, &mut warnings);
    let composition = velato::Composition::from_json(json).unwrap();
    assert_eq!(solid_layers.len(), 1);
    assert!(warnings.is_empty());

    let view = ImageView {
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
    let without_solid = Arc::new(LottieAsset {
        path: PathBuf::from("solid.json"),
        source_digest: 0,
        composition: composition.clone(),
        image_layers: Vec::new(),
        solid_layers: Vec::new(),
        warnings: Vec::new(),
    });
    let without_solid =
        LottiePlayer::new(LottiePlayback::new(without_solid, view, 0.0, None, false, 1.0).unwrap());
    let with_solid = Arc::new(LottieAsset {
        path: PathBuf::from("solid.json"),
        source_digest: 0,
        composition,
        image_layers: Vec::new(),
        solid_layers,
        warnings: Vec::new(),
    });
    let with_solid =
        LottiePlayer::new(LottiePlayback::new(with_solid, view, 0.0, None, false, 1.0).unwrap());

    assert!(
        with_solid.scene().encoding().draw_tags.len()
            > without_solid.scene().encoding().draw_tags.len()
    );
}

#[test]
fn derived_geometry_plugin_samples_lottie_for_headless_seek() {
    let view = ImageView {
        source_x: 0.0,
        source_y: 0.0,
        source_width: 100.0,
        source_height: 50.0,
        display_width: 100.0,
        display_height: 50.0,
        scale_x: 1.0,
        scale_y: 1.0,
        quality: Default::default(),
    };
    let mut playback =
        LottiePlayback::new(asset(), view, 0.0, None, false, 1.0).expect("valid playback");
    playback.active = true;
    let player = LottiePlayer::new(playback);
    let initial_frame = player.sampled_frame;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(gaanim_animation::PlaybackState::default())
        .add_plugins(crate::GaanimDerivedGeometryPlugin);
    let entity = app.world_mut().spawn(player).id();
    app.world_mut()
        .resource_mut::<gaanim_animation::PlaybackState>()
        .current_time = 1.0;
    app.update();

    let sampled_frame = app
        .world()
        .get::<LottiePlayer>(entity)
        .expect("Lottie player should remain spawned")
        .sampled_frame;
    assert_ne!(sampled_frame.to_bits(), initial_frame.to_bits());
}
