use robot_common_lib::{
    vla::{VlaError, VlaImage, VlaJoints},
    vla_package::{
        VlaManifest, VlaManifestControl, VlaManifestInput, VlaManifestOnnx, VlaManifestOutput,
        VlaManifestRobot, VlaNormalization,
    },
};
use robot_third_party_client::{
    OnnxVlaRuntime,
    onnx_runtime::{decode_actions, parse_providers, prepare_state, preprocess_image},
    traits::VlaRuntimeTrait,
};
use std::{collections::BTreeMap, path::Path};

fn sample_manifest() -> VlaManifest {
    VlaManifest {
        format_version: 1,
        code: "yellowarm-test".into(),
        name: "YellowArm Test".into(),
        model_type: "smolvla".into(),
        version: "1.0.0".into(),
        robot: VlaManifestRobot {
            joint_names: vec!["joint1".into(), "joint2".into()],
        },
        onnx: VlaManifestOnnx {
            file: "model.onnx".into(),
        },
        inputs: vec![
            VlaManifestInput {
                name: "state".into(),
                kind: "joint_position".into(),
                dtype: Some("float32".into()),
                camera: None,
                width: None,
                height: None,
                encoding: None,
                layout: None,
                scale: None,
                max_length: None,
            },
            VlaManifestInput {
                name: "image_front".into(),
                kind: "image".into(),
                dtype: Some("float32".into()),
                camera: Some("front".into()),
                width: Some(2),
                height: Some(2),
                encoding: Some("rgb8".into()),
                layout: Some("NCHW".into()),
                scale: Some("0_1".into()),
                max_length: None,
            },
        ],
        outputs: vec![VlaManifestOutput {
            name: "actions".into(),
            kind: "joint_position_chunk".into(),
            dtype: Some("float32".into()),
            optional: Some(false),
        }],
        normalization: None,
        control: Some(VlaManifestControl {
            chunk_size: Some(2),
        }),
        files: BTreeMap::new(),
    }
}

#[test]
fn reorders_observed_joints_and_normalizes_state() {
    let manifest = sample_manifest();
    let observed = VlaJoints {
        name: vec!["joint2".into(), "unused".into(), "joint1".into()],
        position: vec![2.0, 99.0, 1.0],
    };
    let norm = VlaNormalization {
        state_mean: Some(vec![0.5, 1.0]),
        state_std: Some(vec![0.5, 2.0]),
        action_mean: Some(vec![0.5, 1.0]),
        action_std: Some(vec![0.5, 2.0]),
    };

    let state = prepare_state(&manifest, &observed, Some(&norm)).unwrap();
    assert_eq!(state, vec![1.0, 0.5]);

    let decoded = decode_actions(&manifest, &[1.0, 0.5, 2.0, 1.0, 3.0, 1.5], 3, Some(&norm))
        .unwrap();
    // Truncated to control.chunkSize = 2.
    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].name, vec!["joint1", "joint2"]);
    assert_eq!(decoded[0].position, vec![1.0, 2.0]);
    assert_eq!(decoded[1].position, vec![1.5, 3.0]);
}

#[test]
fn converts_bgr8_to_rgb_nchw_scaled() {
    let manifest = sample_manifest();
    let img_input = &manifest.inputs[1];
    // 2x2 BGR image where every pixel is (B=0, G=127, R=255) -> RGB (255, 127, 0).
    let bgr = VlaImage {
        width: 2,
        height: 2,
        encoding: "bgr8".into(),
        data: vec![0, 127, 255].repeat(4),
    };
    let tensor = preprocess_image(&bgr, img_input).unwrap();
    assert_eq!(tensor.len(), 3 * 2 * 2);
    // Channel 0 (R) should be ~1.0, Channel 1 (G) ~127/255, Channel 2 (B) 0.0.
    for i in 0..4 {
        assert!((tensor[i] - 1.0).abs() < 1e-5);
        assert!((tensor[4 + i] - (127.0 / 255.0)).abs() < 1e-5);
        assert!(tensor[8 + i].abs() < 1e-5);
    }
}

#[test]
fn parses_provider_list_with_fallback() {
    assert_eq!(parse_providers("tensorrt, cuda, cpu"), vec!["tensorrt", "cuda", "cpu"]);
    assert_eq!(parse_providers("unknown"), vec!["tensorrt", "cuda", "cpu"]);
}

#[cfg(not(feature = "onnx"))]
#[test]
fn without_onnx_feature_load_reports_unavailable() {
    let runtime = OnnxVlaRuntime::from_env_providers("cpu");
    assert!(!runtime.is_available());
    let err = runtime
        .load(Path::new("."), &sample_manifest(), &"a".repeat(64))
        .unwrap_err();
    assert_eq!(err.downcast_ref::<VlaError>(), Some(&VlaError::Unavailable));
    assert!(err.to_string().contains("ONNX runtime not built"));
}
