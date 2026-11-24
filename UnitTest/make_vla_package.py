"""Generate a tiny valid VLA ONNX model zip package (formatVersion 1) for tests.

Requires `onnx` (optional: if `--dummy-onnx` is passed, writes placeholder bytes for
zip/HTTP tests that do not execute ONNX Runtime).
"""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile


def build_tiny_onnx_bytes(num_joints: int, chunk_size: int, image_size: int) -> bytes:
    import onnx
    from onnx import TensorProto, helper

    # Output actions = repeat(state, chunk_size) + mean(image) * 0.0 + sum(input_ids) * 0.0
    # Keeps all inputs connected so ORT validates their names and shapes.
    state = helper.make_tensor_value_info("state", TensorProto.FLOAT, [1, num_joints])
    image = helper.make_tensor_value_info(
        "image_front", TensorProto.FLOAT, [1, 3, image_size, image_size]
    )
    input_ids = helper.make_tensor_value_info("input_ids", TensorProto.INT64, [1, 16])

    actions = helper.make_tensor_value_info(
        "actions", TensorProto.FLOAT, [1, chunk_size, num_joints]
    )
    done = helper.make_tensor_value_info("done", TensorProto.BOOL, [1])

    zero_scale = helper.make_tensor("zero_scale", TensorProto.FLOAT, [1], [0.0])
    repeats = helper.make_tensor("repeats", TensorProto.INT64, [3], [1, chunk_size, 1])
    unsq_axes = helper.make_tensor("unsq_axes", TensorProto.INT64, [1], [1])
    done_const = helper.make_tensor("done_val", TensorProto.BOOL, [1], [True])

    nodes = [
        helper.make_node("ReduceMean", ["image_front"], ["img_mean"], keepdims=0),
        helper.make_node("Mul", ["img_mean", "zero_scale"], ["img_zero"]),
        helper.make_node("Cast", ["input_ids"], ["tok_f"], to=TensorProto.FLOAT),
        helper.make_node("ReduceMean", ["tok_f"], ["tok_mean"], keepdims=0),
        helper.make_node("Mul", ["tok_mean", "zero_scale"], ["tok_zero"]),
        helper.make_node("Add", ["state", "img_zero"], ["s1"]),
        helper.make_node("Add", ["s1", "tok_zero"], ["s2"]),
        helper.make_node("Unsqueeze", ["s2", "unsq_axes"], ["s3"]),
        helper.make_node("Tile", ["s3", "repeats"], ["actions"]),
        helper.make_node("Identity", ["done_val"], ["done"]),
    ]

    graph = helper.make_graph(
        nodes,
        "tiny_vla_model",
        [state, image, input_ids],
        [actions, done],
        initializer=[zero_scale, repeats, unsq_axes, done_const],
    )
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)])
    model.ir_version = 9
    onnx.checker.check_model(model)
    return model.SerializeToString()


def minimal_tokenizer_json() -> bytes:
    tokenizer = {
        "version": "1.0",
        "truncation": None,
        "padding": None,
        "added_tokens": [],
        "normalizer": None,
        "pre_tokenizer": {"type": "Whitespace"},
        "post_processor": None,
        "decoder": None,
        "model": {
            "type": "WordLevel",
            "vocab": {"<unk>": 0, "pick": 1, "cube": 2, "move": 3, "hold": 4},
            "unk_token": "<unk>",
        },
    }
    return json.dumps(tokenizer, indent=2).encode("utf-8")


def create_package(
    output_path: Path,
    code: str = "yellowarm-smolvla",
    name: str = "YellowArm Test VLA",
    model_type: str = "smolvla",
    version: str = "1.0.0",
    joints: list[str] | None = None,
    camera: str | None = "front",
    chunk_size: int = 4,
    image_size: int = 8,
    dummy_onnx: bool = False,
) -> Path:
    joint_names = joints or ["joint1", "joint2"]
    onnx_bytes = (
        b"dummy-onnx-model-payload"
        if dummy_onnx
        else build_tiny_onnx_bytes(len(joint_names), chunk_size, image_size)
    )
    onnx_sha = hashlib.sha256(onnx_bytes).hexdigest()

    inputs = [
        {"name": "state", "kind": "joint_position", "dtype": "float32"},
    ]
    if camera is not None:
        inputs.append(
            {
                "name": "image_front",
                "kind": "image",
                "camera": camera,
                "width": image_size,
                "height": image_size,
                "encoding": "rgb8",
                "layout": "NCHW",
                "scale": "0_1",
            }
        )
    inputs.append(
        {"name": "input_ids", "kind": "task_tokens", "dtype": "int64", "maxLength": 16}
    )

    manifest = {
        "formatVersion": 1,
        "code": code,
        "name": name,
        "modelType": model_type,
        "version": version,
        "robot": {"jointNames": joint_names},
        "onnx": {"file": "model.onnx"},
        "inputs": inputs,
        "outputs": [
            {"name": "actions", "kind": "joint_position_chunk", "dtype": "float32"},
            {"name": "done", "kind": "done_flag", "optional": True},
        ],
        "control": {"chunkSize": chunk_size},
        "files": {"model.onnx": onnx_sha},
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output_path, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        zf.writestr("manifest.json", json.dumps(manifest, indent=2))
        zf.writestr("model.onnx", onnx_bytes)
        zf.writestr("tokenizer.json", minimal_tokenizer_json())
    return output_path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--code", default="yellowarm-smolvla")
    parser.add_argument("--name", default="YellowArm Test VLA")
    parser.add_argument("--model-type", default="smolvla")
    parser.add_argument("--version", default="1.0.0")
    parser.add_argument("--joints", nargs="+", default=["joint1", "joint2"])
    parser.add_argument("--camera", default="front")
    parser.add_argument("--no-camera", action="store_true")
    parser.add_argument("--chunk-size", type=int, default=4)
    parser.add_argument("--image-size", type=int, default=8)
    parser.add_argument("--dummy-onnx", action="store_true")
    args = parser.parse_args()

    out = create_package(
        args.output,
        code=args.code,
        name=args.name,
        model_type=args.model_type,
        version=args.version,
        joints=args.joints,
        camera=None if args.no_camera else args.camera,
        chunk_size=args.chunk_size,
        image_size=args.image_size,
        dummy_onnx=args.dummy_onnx,
    )
    print(f"Created VLA package: {out}")


if __name__ == "__main__":
    main()
