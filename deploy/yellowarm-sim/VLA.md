# VLA ONNX Runtime -> ROS 2

Rust Agent nạp và chạy trực tiếp model VLA định dạng **ONNX** (không còn inference server HTTP ngoài). Mỗi model được đóng gói thành một file `.zip` theo [định dạng gói tạm `formatVersion: 1`](../../docs/vla-package-v1.md), lưu metadata trong bảng SQLite `vla_models` (tự tạo bằng `CREATE TABLE IF NOT EXISTS` khi khởi động) và gắn chặt với một cấu hình URDF (`robot_configurations`).

> **Giới hạn hiện tại:** Runtime hỗ trợ các model ONNX chạy một bước forward cho mỗi chu kỳ inference (nhận joint state + ảnh camera + token lệnh, trả action chunk và cờ `done` tùy chọn). Các kiến trúc đa bước khử nhiễu (flow-matching nhiều vòng bên trong) cần được export gộp vòng lặp trong graph ONNX trước khi đóng gói. Chưa có auth; vẫn giữ kiểm tra `Origin` trên các route ghi.

## Biến môi trường

| Biến | Mặc định | Ý nghĩa |
| --- | --- | --- |
| `VLA_DATA_ROOT` | Theo `URDF_DATA_ROOT` (`data`) | Thư mục gốc lưu gói VLA đã giải nén (`vla/{configId}/{modelId}`) và `.staging/` |
| `VLA_MAX_PACKAGE_BYTES` | `4294967296` (4 GiB) | Giới hạn dung lượng file `.zip` khi upload |
| `VLA_MAX_UNPACKED_BYTES` | `8589934592` (8 GiB) | Giới hạn tổng số byte thực sự ghi ra đĩa khi giải nén |
| `VLA_ONNX_PROVIDERS` | `tensorrt,cuda,cpu` | Thứ tự ưu tiên Execution Provider cho `ort` (rơi về `cpu` nếu GPU không khả dụng) |
| `ORT_DYLIB_PATH` | chưa đặt | Đường dẫn tới `libonnxruntime.so` (`ort` dùng `load-dynamic`) |
| `VLA_IMAGE_MAX_AGE_MS` | `500` | Tuổi tối đa của frame ảnh camera từ ROS topic (quá hạn trả 503) |
| `VLA_TIMEOUT_MS` | `600000` | Hạn tối đa cho toàn bộ lệnh chạy VLA (`< 750000`) |
| `VLA_COMMAND_TOPIC` | `/yellowarm_sim/joint_command` | Topic `sensor_msgs/msg/JointState` publisher |
| `VLA_STATE_TOPIC` | `/yellowarm_sim/joint_states` | Topic `sensor_msgs/msg/JointState` subscriber |
| `VLA_MAX_STEPS` | `1000` | Số bước action tối đa publish cho một lệnh |
| `VLA_STEP_PERIOD_MS` | `100` | Khoảng nghỉ (ms) giữa hai action liên tiếp |
| `ROBOT_CONFIGURATION_ID` | chưa đặt | Nếu đặt, lệnh chạy VLA phải khớp đúng URDF configuration này |

> **Breaking changes so với bản cũ:** Đã gỡ `VLA_URL`, `VLA_ACCESS_TOKEN`, `VLA_MODELS_FILE`, `vla-models.json` và `fake_vla_server.py`. Body của `POST /api/vla/commands` đổi từ `{profileId, task}` sang `{robotConfigurationId, modelId, task}`.

## Camera qua ROS 2

- Mỗi input `kind: "image"` trong `manifest.json` khai báo tên `camera` (ví dụ `"front"`).
- Agent ánh xạ `camera` sang bản ghi `sensors` đang bật (`sensors.code == camera`) của URDF đó và lấy topic từ `sensor_streams` đang bật có `data_type == "sensor_msgs/msg/Image"`.
- Hỗ trợ encoding `rgb8`, `bgr8`, `mono8` (chưa hỗ trợ `CompressedImage`). Agent tự chuyển sang RGB, resize, đổi layout (`NCHW`/`NHWC`) và scale theo manifest.
- Subscription chạy trực tiếp trên `RobotRos::subscriber::<Image>` (QoS best-effort), chỉ giữ frame mới nhất trong `watch` channel.

## HTTP API

| Method | Path | Mô tả |
| --- | --- | --- |
| `GET` | `/api/vla/models?robotConfigurationId=&q=` | Liệt kê / tìm kiếm model theo `code`, `displayName`, `version`, `modelType` |
| `GET` | `/api/vla/models/{id}` | Chi tiết model kèm `manifest` đã parse |
| `POST` | `/api/vla/models` | Upload multipart: `robotConfigurationId` + file `package` (`.zip`) |
| `PUT` | `/api/vla/models/{id}` | Sửa `displayName`, `description` |
| `DELETE` | `/api/vla/models/{id}` | Xóa model (trả `409` nếu đang nạp hoặc đang chạy) |
| `GET` | `/api/vla/status` | Trạng thái runtime `{loadedModelId, configurationId, running, providers}` |
| `POST` | `/api/vla/commands` | Chạy lệnh `{robotConfigurationId, modelId, task}` → `{publishedSteps}` |
| `POST` | `/api/vla/stop` | Dừng lệnh đang chạy → `{stopping}` |

Mã lỗi JSON `{ "message": "..." }`: `400` gói/request không hợp lệ, `403` origin không hợp lệ, `404` không tìm thấy, `409` trùng version / đang dùng / sai URDF / đang bận, `413` gói quá lớn, `503` ROS / camera / ONNX runtime chưa sẵn sàng, `504` timeout.

## Cài đặt ONNX Runtime trên Jetson (JetPack 7.2 / aarch64)

1. Kiểm tra phiên bản CUDA và TensorRT thực tế trên Jetson (`nvcc --version`, `dpkg -l | grep nvinfer`).
2. Nếu chưa có bản dựng sẵn `libonnxruntime.so` khớp CUDA/TensorRT của JetPack 7.2, tự build từ mã nguồn `onnxruntime` cho aarch64 với `--use_cuda --use_tensorrt --build_shared_lib --parallel`.
3. Đặt đường dẫn thư viện động và build backend với cả `ros2` và `onnx`:

```bash
source /opt/ros/$ROS_DISTRO/setup.bash
# source /path/to/ros-rust-workspace/install/setup.bash
export ORT_DYLIB_PATH=/usr/local/lib/libonnxruntime.so
export VLA_ONNX_PROVIDERS=tensorrt,cuda,cpu
cargo build -p robot-agent --features "ros2 onnx" --locked
bash deploy/yellowarm-sim/run.sh
```

Nếu dùng reverse proxy (Nginx/Caddy) trước frontend/backend, tăng giới hạn kích thước upload (ví dụ `client_max_body_size 4G;` và tăng `proxy_read_timeout`).

## Kiểm thử

```bash
# Sinh gói mẫu (thêm --dummy-onnx nếu chưa cài python onnx)
python UnitTest/make_vla_package.py --output /tmp/sample-vla.zip

# Kiểm thử HTTP thật với agent đã build
python UnitTest/vla_http.py --executable target/debug/robot-agent
```
