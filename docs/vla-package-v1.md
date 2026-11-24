# VLA Model Package Format (`formatVersion: 1`)

> **Lưu ý:** Đây là **định dạng gói tạm** cho phía Robot Agent (`261124-ddab`). Chuẩn export cuối cùng sẽ được đồng bộ với server ở ticket luồng server → robot. Toàn bộ logic đọc/kiểm tra gói được gom trong một module duy nhất: `Entities/CommonLib/src/vla_package.rs`.

## Cấu trúc file `.zip`

Tất cả các file đặt trực tiếp ở gốc file zip (không lồng thư mục gốc thừa):

```text
<code>-<version>.zip
├── manifest.json         # bắt buộc
├── model.onnx            # bắt buộc (tên khớp onnx.file trong manifest)
├── model.onnx.data       # tùy chọn (external weights cho model lớn)
├── tokenizer.json        # bắt buộc nếu manifest có input kind="task_tokens"
└── normalization.json    # tùy chọn (mean/std cho state và action)
```

### Quy tắc an toàn khi giải nén

- Tối đa **32 entry** trong file zip.
- Từ chối path traversal (`..`, đường dẫn tuyệt đối, dấu `\` hoặc `:`), symlink và entry có mật khẩu/mã hóa.
- Tổng dung lượng ghi thực tế khi giải nén không vượt `VLA_MAX_UNPACKED_BYTES`.
- Chỉ chấp nhận phần mở rộng: `.json`, `.onnx`, `.data`, `.txt`, `.md`.
- Mọi file khai báo trong `manifest.files` được kiểm tra lại `sha256` sau khi giải nén.

## Schema `manifest.json`

`manifest.json` sử dụng `camelCase` và từ chối field không xác định (`deny_unknown_fields`).

```json
{
  "formatVersion": 1,
  "code": "yellowarm-smolvla",
  "name": "YellowArm pick cube",
  "modelType": "smolvla",
  "version": "2026-11-20",
  "robot": {
    "jointNames": ["joint1", "joint2"]
  },
  "onnx": {
    "file": "model.onnx"
  },
  "inputs": [
    { "name": "state", "kind": "joint_position", "dtype": "float32" },
    {
      "name": "image_front",
      "kind": "image",
      "camera": "front",
      "width": 224,
      "height": 224,
      "encoding": "rgb8",
      "layout": "NCHW",
      "scale": "0_1"
    },
    { "name": "input_ids", "kind": "task_tokens", "dtype": "int64", "maxLength": 64 },
    { "name": "attention_mask", "kind": "task_attention_mask", "dtype": "int64" }
  ],
  "outputs": [
    { "name": "actions", "kind": "joint_position_chunk", "dtype": "float32" },
    { "name": "done", "kind": "done_flag", "optional": true }
  ],
  "normalization": "normalization.json",
  "control": {
    "chunkSize": 10
  },
  "files": {
    "model.onnx": "<64-hex-sha256>"
  }
}
```

### Ràng buộc nghiệp vụ khi cài vào một URDF

1. `robot.jointNames` không rỗng, không trùng và phải là **tập con** các joint không phải `fixed` trong URDF đã chọn.
2. Mọi `camera` trong `inputs` có `kind == "image"` phải khớp một bản ghi `sensors` (đang bật) có `sensors.code == camera` và ít nhất một `sensor_streams` (đang bật) có `data_type == "sensor_msgs/msg/Image"` thuộc URDF đó.
3. Encoding ảnh hỗ trợ: `rgb8`, `bgr8`, `mono8`. Layout: `NCHW` hoặc `NHWC`. Scale: `0_1`, `neg1_1`, `imagenet`, `raw`.
4. Cặp `(robot_configuration_id, code, version)` là duy nhất trong bảng `vla_models`.

## Schema `normalization.json` (tùy chọn)

```json
{
  "stateMean": [0.0, 0.0],
  "stateStd": [1.0, 1.0],
  "actionMean": [0.0, 0.0],
  "actionStd": [1.0, 1.0]
}
```
