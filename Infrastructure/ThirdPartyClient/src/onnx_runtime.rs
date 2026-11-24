use crate::traits::VlaRuntimeTrait;
use anyhow::{Context, Result, bail, ensure};
use robot_common_lib::{
    vla::{VlaError, VlaImage, VlaInference, VlaJoints, VlaObservation, vla_err},
    vla_package::{VlaManifest, VlaManifestInput, VlaNormalization},
};
use std::{collections::HashMap, path::Path};

#[cfg(feature = "onnx")]
use std::{
    borrow::Cow,
    env, fs,
    sync::{Arc, Mutex, OnceLock},
    time::Instant,
};

#[cfg(feature = "onnx")]
type LoadedSession = (
    String,                        // package_sha256
    VlaManifest,                   // manifest
    ort::session::Session,         // ONNX Runtime session
    Option<tokenizers::Tokenizer>, // optional tokenizer
    Option<VlaNormalization>,      // optional normalization
    Vec<String>,                   // configured/active providers
);

#[cfg(feature = "onnx")]
static ORT_INITIALIZED: OnceLock<Result<(), String>> = OnceLock::new();

pub struct OnnxVlaRuntime {
    preferred_providers: Vec<String>,
    #[cfg(feature = "onnx")]
    state: Arc<Mutex<Option<LoadedSession>>>,
}

impl OnnxVlaRuntime {
    pub fn new(providers: Vec<String>) -> Self {
        let preferred_providers = if providers.is_empty() {
            vec!["tensorrt".into(), "cuda".into(), "cpu".into()]
        } else {
            providers
        };
        Self {
            preferred_providers,
            #[cfg(feature = "onnx")]
            state: Arc::new(Mutex::new(None)),
        }
    }

    pub fn from_env_providers(raw: &str) -> Self {
        Self::new(parse_providers(raw))
    }
}

pub fn parse_providers(raw: &str) -> Vec<String> {
    let parsed: Vec<String> = raw
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| matches!(s.as_str(), "tensorrt" | "cuda" | "cpu"))
        .collect();
    if parsed.is_empty() {
        vec!["tensorrt".into(), "cuda".into(), "cpu".into()]
    } else {
        parsed
    }
}

/// Reorders observed joints to match `manifest.robot.joint_names` and applies optional normalization.
pub fn prepare_state(
    manifest: &VlaManifest,
    joints: &VlaJoints,
    normalization: Option<&VlaNormalization>,
) -> Result<Vec<f32>> {
    joints.validate()?;
    let mut index = HashMap::with_capacity(joints.name.len());
    for (name, pos) in joints.name.iter().zip(joints.position.iter()) {
        index.insert(name.as_str(), *pos as f32);
    }
    let mut state = Vec::with_capacity(manifest.robot.joint_names.len());
    for expected in &manifest.robot.joint_names {
        let value = *index
            .get(expected.as_str())
            .with_context(|| format!("Missing required joint '{expected}' in observation"))?;
        state.push(value);
    }
    if let Some(norm) = normalization {
        if let Some(mean) = &norm.state_mean {
            ensure!(
                mean.len() == state.len(),
                "normalization.stateMean length mismatch"
            );
            for (v, m) in state.iter_mut().zip(mean.iter()) {
                *v -= *m;
            }
        }
        if let Some(std) = &norm.state_std {
            ensure!(
                std.len() == state.len(),
                "normalization.stateStd length mismatch"
            );
            for (v, s) in state.iter_mut().zip(std.iter()) {
                ensure!(s.is_finite() && *s != 0.0, "Invalid normalization.stateStd");
                *v /= *s;
            }
        }
    }
    Ok(state)
}

/// Converts rgb8/bgr8/mono8 -> RGB, resizes to manifest target size, converts layout and scale.
pub fn preprocess_image(image: &VlaImage, input: &VlaManifestInput) -> Result<Vec<f32>> {
    ensure!(
        image.width > 0 && image.height > 0,
        "Invalid camera image dimensions"
    );
    let src_w = image.width as usize;
    let src_h = image.height as usize;
    let pixels = src_w.checked_mul(src_h).context("Image size overflow")?;

    let rgb: Vec<u8> = match image.encoding.as_str() {
        "rgb8" => {
            ensure!(
                image.data.len() >= pixels * 3,
                "rgb8 image data shorter than width * height * 3"
            );
            image.data[..pixels * 3].to_vec()
        }
        "bgr8" => {
            ensure!(
                image.data.len() >= pixels * 3,
                "bgr8 image data shorter than width * height * 3"
            );
            let mut out = Vec::with_capacity(pixels * 3);
            for px in image.data[..pixels * 3].chunks_exact(3) {
                out.push(px[2]);
                out.push(px[1]);
                out.push(px[0]);
            }
            out
        }
        "mono8" => {
            ensure!(
                image.data.len() >= pixels,
                "mono8 image data shorter than width * height"
            );
            let mut out = Vec::with_capacity(pixels * 3);
            for &g in &image.data[..pixels] {
                out.push(g);
                out.push(g);
                out.push(g);
            }
            out
        }
        other => bail!("Unsupported camera image encoding: {other}"),
    };

    let dst_w = input.width.context("Missing target width")? as usize;
    let dst_h = input.height.context("Missing target height")? as usize;
    let resized = resize_rgb(&rgb, src_w, src_h, dst_w, dst_h)?;

    let scale = input.scale.as_deref().unwrap_or("0_1");
    let layout = input.layout.as_deref().unwrap_or("NCHW");
    let dst_pixels = dst_w * dst_h;
    let mut tensor = vec![0.0f32; dst_pixels * 3];

    let imagenet_mean = [0.485f32, 0.456, 0.406];
    let imagenet_std = [0.229f32, 0.224, 0.225];

    for y in 0..dst_h {
        for x in 0..dst_w {
            let src_idx = (y * dst_w + x) * 3;
            for c in 0..3 {
                let raw = f32::from(resized[src_idx + c]);
                let val = match scale {
                    "0_1" => raw / 255.0,
                    "neg1_1" => (raw / 127.5) - 1.0,
                    "imagenet" => ((raw / 255.0) - imagenet_mean[c]) / imagenet_std[c],
                    "raw" => raw,
                    other => bail!("Unsupported image scale: {other}"),
                };
                let dst_idx = match layout {
                    "NCHW" => c * dst_pixels + y * dst_w + x,
                    "NHWC" => (y * dst_w + x) * 3 + c,
                    other => bail!("Unsupported image layout: {other}"),
                };
                tensor[dst_idx] = val;
            }
        }
    }
    Ok(tensor)
}

#[cfg(feature = "onnx")]
fn resize_rgb(rgb: &[u8], src_w: usize, src_h: usize, dst_w: usize, dst_h: usize) -> Result<Vec<u8>> {
    if src_w == dst_w && src_h == dst_h {
        return Ok(rgb.to_vec());
    }
    let img = image::RgbImage::from_raw(src_w as u32, src_h as u32, rgb.to_vec())
        .context("Failed to construct RgbImage for resize")?;
    let resized = image::imageops::resize(
        &img,
        dst_w as u32,
        dst_h as u32,
        image::imageops::FilterType::Triangle,
    );
    Ok(resized.into_raw())
}

#[cfg(not(feature = "onnx"))]
fn resize_rgb(rgb: &[u8], src_w: usize, src_h: usize, dst_w: usize, dst_h: usize) -> Result<Vec<u8>> {
    if src_w == dst_w && src_h == dst_h {
        return Ok(rgb.to_vec());
    }
    let mut out = vec![0u8; dst_w * dst_h * 3];
    for y in 0..dst_h {
        let sy = (y * src_h) / dst_h;
        for x in 0..dst_w {
            let sx = (x * src_w) / dst_w;
            let src_idx = (sy * src_w + sx) * 3;
            let dst_idx = (y * dst_w + x) * 3;
            out[dst_idx..dst_idx + 3].copy_from_slice(&rgb[src_idx..src_idx + 3]);
        }
    }
    Ok(out)
}

/// Denormalizes flat action chunk `[steps * num_joints]` and truncates to `control.chunkSize`.
pub fn decode_actions(
    manifest: &VlaManifest,
    flat: &[f32],
    steps: usize,
    normalization: Option<&VlaNormalization>,
) -> Result<Vec<VlaJoints>> {
    let joints = &manifest.robot.joint_names;
    let num_joints = joints.len();
    ensure!(
        flat.len() >= steps * num_joints,
        "Action output tensor smaller than steps * num_joints"
    );
    let max_chunk = manifest
        .control
        .as_ref()
        .and_then(|c| c.chunk_size)
        .unwrap_or(steps);
    let take_steps = steps.min(max_chunk);
    let mut actions = Vec::with_capacity(take_steps);
    for step in 0..take_steps {
        let offset = step * num_joints;
        let mut position = Vec::with_capacity(num_joints);
        for j in 0..num_joints {
            let mut val = flat[offset + j];
            if let Some(norm) = normalization {
                if let Some(std) = &norm.action_std {
                    ensure!(
                        std.len() == num_joints,
                        "normalization.actionStd length mismatch"
                    );
                    val *= std[j];
                }
                if let Some(mean) = &norm.action_mean {
                    ensure!(
                        mean.len() == num_joints,
                        "normalization.actionMean length mismatch"
                    );
                    val += mean[j];
                }
            }
            ensure!(val.is_finite(), "Non-finite action value from ONNX model");
            position.push(f64::from(val));
        }
        let joints_step = VlaJoints {
            name: joints.clone(),
            position,
        };
        joints_step.validate()?;
        actions.push(joints_step);
    }
    Ok(actions)
}

#[cfg(feature = "onnx")]
fn ensure_ort_initialized() -> Result<()> {
    let res = ORT_INITIALIZED.get_or_init(|| {
        if let Ok(dylib) = env::var("ORT_DYLIB_PATH")
            && !dylib.trim().is_empty()
        {
            ort::init_from(dylib)
                .map(|builder| {
                    let _ = builder.commit();
                })
                .map_err(|err| format!("Failed to load ONNX Runtime library: {err}"))
        } else {
            let _ = ort::init().commit();
            Ok(())
        }
    });
    match res {
        Ok(()) => Ok(()),
        Err(msg) => Err(vla_err(VlaError::Unavailable, msg.clone())),
    }
}

#[cfg(feature = "onnx")]
fn build_execution_providers(
    preferred: &[String],
) -> (Vec<ort::execution_providers::ExecutionProviderDispatch>, Vec<String>) {
    let mut eps = Vec::new();
    let mut active = Vec::new();
    for provider in preferred {
        match provider.as_str() {
            "tensorrt" => {
                eps.push(ort::execution_providers::TensorRTExecutionProvider::default().build());
                active.push("tensorrt".into());
            }
            "cuda" => {
                eps.push(ort::execution_providers::CUDAExecutionProvider::default().build());
                active.push("cuda".into());
            }
            "cpu" => {
                eps.push(ort::execution_providers::CPUExecutionProvider::default().build());
                active.push("cpu".into());
            }
            _ => {}
        }
    }
    if !active.iter().any(|p| p == "cpu") {
        eps.push(ort::execution_providers::CPUExecutionProvider::default().build());
        active.push("cpu".into());
    }
    (eps, active)
}

#[cfg(feature = "onnx")]
fn validate_session_io(session: &ort::session::Session, manifest: &VlaManifest) -> Result<()> {
    let session_inputs: HashMap<&str, &ort::session::Input> = session
        .inputs
        .iter()
        .map(|input| (input.name.as_str(), input))
        .collect();
    for declared in &manifest.inputs {
        let Some(_found) = session_inputs.get(declared.name.as_str()) else {
            return Err(vla_err(
                VlaError::InvalidPackage,
                format!(
                    "ONNX model is missing declared input '{}'",
                    declared.name
                ),
            ));
        };
    }

    let session_outputs: HashMap<&str, &ort::session::Output> = session
        .outputs
        .iter()
        .map(|output| (output.name.as_str(), output))
        .collect();
    for declared in &manifest.outputs {
        let optional = declared.optional.unwrap_or(false);
        if !optional && !session_outputs.contains_key(declared.name.as_str()) {
            return Err(vla_err(
                VlaError::InvalidPackage,
                format!(
                    "ONNX model is missing required output '{}'",
                    declared.name
                ),
            ));
        }
    }
    Ok(())
}

impl VlaRuntimeTrait for OnnxVlaRuntime {
    fn is_available(&self) -> bool {
        cfg!(feature = "onnx")
    }

    fn load(&self, dir: &Path, manifest: &VlaManifest, package_sha256: &str) -> Result<bool> {
        robot_common_lib::logged_result!("onnx_load", {
            #[cfg(feature = "onnx")]
            {
                ensure_ort_initialized()?;
                let model_path = dir.join(&manifest.onnx.file);
                if !model_path.is_file() {
                    return Err(vla_err(
                        VlaError::InvalidPackage,
                        format!("ONNX model file '{}' not found", manifest.onnx.file),
                    ));
                }

                // Unload previous model before loading the new one (G1).
                self.unload();

                let (eps, active_providers) = build_execution_providers(&self.preferred_providers);
                let session = ort::session::Session::builder()
                    .map_err(|e| vla_err(VlaError::Unavailable, format!("ORT builder error: {e}")))?
                    .with_execution_providers(eps)
                    .map_err(|e| {
                        vla_err(
                            VlaError::Unavailable,
                            format!("ORT execution provider error: {e}"),
                        )
                    })?
                    .commit_from_file(&model_path)
                    .map_err(|e| {
                        vla_err(
                            VlaError::InvalidPackage,
                            format!("Failed to load ONNX model '{}': {e}", manifest.onnx.file),
                        )
                    })?;

                validate_session_io(&session, manifest)?;

                let tokenizer = if manifest.requires_tokenizer() {
                    let tok_path = dir.join("tokenizer.json");
                    let tok = tokenizers::Tokenizer::from_file(&tok_path).map_err(|e| {
                        vla_err(
                            VlaError::InvalidPackage,
                            format!("Failed to load tokenizer.json: {e}"),
                        )
                    })?;
                    Some(tok)
                } else {
                    None
                };

                let normalization = if let Some(norm_file) = &manifest.normalization {
                    let norm_text = fs::read_to_string(dir.join(norm_file)).map_err(|e| {
                        vla_err(
                            VlaError::InvalidPackage,
                            format!("Cannot read normalization file '{norm_file}': {e}"),
                        )
                    })?;
                    let norm: VlaNormalization = serde_json::from_str(&norm_text).map_err(|e| {
                        vla_err(
                            VlaError::InvalidPackage,
                            format!("Invalid normalization JSON '{norm_file}': {e}"),
                        )
                    })?;
                    Some(norm)
                } else {
                    None
                };

                robot_common_lib::operation_log::info(
                    module_path!(),
                    format_args!(
                        "Loaded ONNX VLA model {} ({}) with providers {:?}",
                        manifest.code, manifest.version, active_providers
                    ),
                );

                let mut guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
                *guard = Some((
                    package_sha256.to_owned(),
                    manifest.clone(),
                    session,
                    tokenizer,
                    normalization,
                    active_providers,
                ));
                Ok(true)
            }
            #[cfg(not(feature = "onnx"))]
            {
                let _ = (dir, manifest, package_sha256);
                Err(vla_err(
                    VlaError::Unavailable,
                    "ONNX runtime not built (compile with --features onnx)",
                ))
            }
        })
    }

    fn unload(&self) -> bool {
        #[cfg(feature = "onnx")]
        {
            let mut guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
            guard.take().is_some()
        }
        #[cfg(not(feature = "onnx"))]
        {
            false
        }
    }

    fn loaded(&self) -> Option<String> {
        #[cfg(feature = "onnx")]
        {
            let guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
            guard.as_ref().map(|(sha, ..)| sha.clone())
        }
        #[cfg(not(feature = "onnx"))]
        {
            None
        }
    }

    fn providers(&self) -> Vec<String> {
        #[cfg(feature = "onnx")]
        {
            let guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((_, _, _, _, _, active)) = guard.as_ref() {
                active.clone()
            } else {
                self.preferred_providers.clone()
            }
        }
        #[cfg(not(feature = "onnx"))]
        {
            self.preferred_providers.clone()
        }
    }

    async fn infer(&self, task: &str, observation: &VlaObservation) -> Result<VlaInference> {
        robot_common_lib::logged_result!(async "onnx_infer", {
            #[cfg(feature = "onnx")]
            {
                let state = Arc::clone(&self.state);
                let task = task.to_owned();
                let observation = observation.clone();
                tokio::task::spawn_blocking(move || {
                    let started = Instant::now();
                    let mut guard = state.lock().unwrap_or_else(|e| e.into_inner());
                    let Some((_, manifest, session, tokenizer, normalization, providers)) =
                        guard.as_mut()
                    else {
                        return Err(vla_err(VlaError::Unavailable, "No ONNX VLA model loaded"));
                    };

                    let mut token_ids: Option<Vec<i64>> = None;
                    let mut attention_mask: Option<Vec<i64>> = None;
                    if manifest.requires_tokenizer() {
                        let tok = tokenizer
                            .as_ref()
                            .context("Tokenizer not loaded for task_tokens input")?;
                        let encoding = tok
                            .encode(task.as_str(), true)
                            .map_err(|e| anyhow::anyhow!("Tokenization failed: {e}"))?;
                        let max_len = manifest
                            .inputs
                            .iter()
                            .find(|i| i.kind == "task_tokens")
                            .and_then(|i| i.max_length)
                            .unwrap_or(64);
                        let mut ids: Vec<i64> = encoding
                            .get_ids()
                            .iter()
                            .take(max_len)
                            .map(|&id| i64::from(id))
                            .collect();
                        let mut mask: Vec<i64> = encoding
                            .get_attention_mask()
                            .iter()
                            .take(max_len)
                            .map(|&m| i64::from(m))
                            .collect();
                        ids.resize(max_len, 0);
                        mask.resize(max_len, 0);
                        token_ids = Some(ids);
                        attention_mask = Some(mask);
                    }

                    let mut inputs: Vec<(Cow<'_, str>, ort::value::SessionInputValue<'_>)> =
                        Vec::with_capacity(manifest.inputs.len());

                    for input in &manifest.inputs {
                        match input.kind.as_str() {
                            "joint_position" => {
                                let state_vec = prepare_state(
                                    manifest,
                                    &observation.joints,
                                    normalization.as_ref(),
                                )?;
                                let shape = vec![1usize, state_vec.len()];
                                let tensor = ort::value::Tensor::from_array((shape, state_vec))?;
                                inputs.push((
                                    Cow::Borrowed(input.name.as_str()),
                                    ort::value::SessionInputValue::from(tensor),
                                ));
                            }
                            "image" => {
                                let cam = input
                                    .camera
                                    .as_deref()
                                    .context("Image input missing camera")?;
                                let img = observation.images.get(cam).ok_or_else(|| {
                                    vla_err(
                                        VlaError::Unavailable,
                                        format!("Missing image frame for camera '{cam}'"),
                                    )
                                })?;
                                let img_vec = preprocess_image(img, input)?;
                                let w = input.width.context("Image input missing width")? as usize;
                                let h =
                                    input.height.context("Image input missing height")? as usize;
                                let layout = input.layout.as_deref().unwrap_or("NCHW");
                                let shape = if layout == "NHWC" {
                                    vec![1usize, h, w, 3]
                                } else {
                                    vec![1usize, 3, h, w]
                                };
                                let tensor = ort::value::Tensor::from_array((shape, img_vec))?;
                                inputs.push((
                                    Cow::Borrowed(input.name.as_str()),
                                    ort::value::SessionInputValue::from(tensor),
                                ));
                            }
                            "task_tokens" => {
                                let ids = token_ids.clone().context("Missing task tokens")?;
                                let shape = vec![1usize, ids.len()];
                                let tensor = ort::value::Tensor::from_array((shape, ids))?;
                                inputs.push((
                                    Cow::Borrowed(input.name.as_str()),
                                    ort::value::SessionInputValue::from(tensor),
                                ));
                            }
                            "task_attention_mask" => {
                                let mask =
                                    attention_mask.clone().context("Missing attention mask")?;
                                let shape = vec![1usize, mask.len()];
                                let tensor = ort::value::Tensor::from_array((shape, mask))?;
                                inputs.push((
                                    Cow::Borrowed(input.name.as_str()),
                                    ort::value::SessionInputValue::from(tensor),
                                ));
                            }
                            other => bail!("Unsupported input kind: {other}"),
                        }
                    }

                    let outputs = session
                        .run(ort::session::SessionInputs::ValueMap(inputs))
                        .map_err(|e| {
                            vla_err(VlaError::Unavailable, format!("ONNX inference failed: {e}"))
                        })?;

                    let action_meta = manifest
                        .outputs
                        .iter()
                        .find(|o| o.kind == "joint_position_chunk")
                        .context("Manifest missing joint_position_chunk output")?;
                    let (shape, flat_actions) = outputs[action_meta.name.as_str()]
                        .try_extract_tensor::<f32>()
                        .map_err(|e| {
                            vla_err(
                                VlaError::Unavailable,
                                format!("Invalid actions output tensor: {e}"),
                            )
                        })?;

                    let num_joints = manifest.robot.joint_names.len();
                    let dims: &[i64] = shape.as_ref();
                    let steps = match dims {
                        [1, s, j] if *j as usize == num_joints => *s as usize,
                        [s, j] if *j as usize == num_joints => *s as usize,
                        [1, j] if *j as usize == num_joints => 1,
                        _ => {
                            return Err(vla_err(
                                VlaError::Unavailable,
                                format!("Unexpected actions output shape: {dims:?}"),
                            ));
                        }
                    };
                    let actions =
                        decode_actions(manifest, flat_actions, steps, normalization.as_ref())?;

                    let mut done = false;
                    if let Some(done_meta) =
                        manifest.outputs.iter().find(|o| o.kind == "done_flag")
                        && let Some(done_val) = outputs.get(done_meta.name.as_str())
                    {
                        if let Ok((_, bools)) = done_val.try_extract_tensor::<bool>() {
                            done = bools.first().copied().unwrap_or(false);
                        } else if let Ok((_, floats)) = done_val.try_extract_tensor::<f32>() {
                            done = floats.first().copied().unwrap_or(0.0) >= 0.5;
                        }
                    }

                    let elapsed_ms = started.elapsed().as_millis();
                    robot_common_lib::operation_log::info(
                        module_path!(),
                        format_args!(
                            "ONNX VLA infer finished in {elapsed_ms} ms (providers: {:?}, steps: {})",
                            providers,
                            actions.len()
                        ),
                    );
                    Ok(VlaInference { actions, done })
                })
                .await
                .map_err(|e| vla_err(VlaError::Unavailable, format!("ONNX worker panic: {e}")))?
            }
            #[cfg(not(feature = "onnx"))]
            {
                let _ = (task, observation);
                Err(vla_err(
                    VlaError::Unavailable,
                    "ONNX runtime not built (compile with --features onnx)",
                ))
            }
        })
    }
}
