# YellowArm Two ORCA — Isaac Sim → Rust

Scene: robots/yellowarm_orca_dual/usd/YellowArm_Two_ORCA_ros.usda (từ gốc workspace).

| Topic | Isaac Sim | Backend Rust | Message |
| --- | --- | --- | --- |
| /yellowarm_sim/joint_states | publish | subscribe | sensor_msgs/msg/JointState |
| /yellowarm_sim/clock | publish | subscribe | rosgraph_msgs/msg/Clock |
| /yellowarm_sim/joint_command | subscribe | publisher handle | sensor_msgs/msg/JointState |

Graph chạy khi nhấn Play, dùng articulation /World/robot/Geometry và ROS_DOMAIN_ID từ môi trường. Scene hiện không có ROS publisher diagnostics, odometry, TF hoặc camera. Frame/header do publisher của Isaac cung cấp; không tự đặt frame_id giả trong backend. Lệnh của scene chỉ nối name/position vào ArticulationController; velocity/effort command không được nối.

## Chạy backend trên Jetson

Source ROS Humble và workspace chứa Rust-generated interfaces (nếu không nằm sẵn trong ROS install). Cần rosgraph_msgs bên cạnh các package đã dùng trong README Rust.

```bash
source /opt/ros/humble/setup.bash
# source /path/to/generated-rust-interfaces/install/setup.bash
cargo build -p robot-agent --features ros2 --locked
export ROBOT_AGENT_DATABASE=/absolute/path/to/existing-monitor.db
export URDF_DATA_ROOT=/absolute/path/to/robot-assets
bash deploy/yellowarm-sim/run.sh
```

Launcher bật ROS và chọn ros-topics.json, mặc định domain 0 giống Isaac Sim khi không đặt biến. Nếu dùng domain khác, export ROS_DOMAIN_ID giống nhau trên cả hai máy trước khi khởi động. Không dùng ROS_LOCALHOST_ONLY=1 cho LAN. Cấu hình DDS/network thuộc môi trường deploy; không cần rosbridge_websocket hoặc Zenoh cho kết nối rclrs/DDS này.

Nếu ROBOT_CONFIGURATION_ID được đặt, StartupApp gộp topic từ DB với file deploy. Cùng topic/hướng/type chỉ tạo một endpoint; khác type báo lỗi. Không thay schema hoặc các endpoint CRUD. Nếu không đặt ROS_TOPICS_FILE, hành vi cấu hình mặc định/DB trước đây giữ nguyên.

## Chạy scene trên PC

Mở đúng USD, bật isaacsim.ros2.bridge và nhấn Play. Ví dụ domain 0 với PowerShell, đặt trước khi mở Isaac:

```powershell
$env:ROS_DOMAIN_ID='0'
$env:ROS_LOCALHOST_ONLY='0'
$env:ROS_AUTOMATIC_DISCOVERY_RANGE='SUBNET'
# Khởi động Isaac Sim từ shell này, mở USD rồi Play.
```

Trong frontend: cấu hình Joint Interface subscribe /yellowarm_sim/joint_states, type sensor_msgs/msg/JointState. Nếu cần điều khiển sau, cấu hình publish /yellowarm_sim/joint_command cùng type và default command. Nạp URDF tương ứng vào robot configuration để viewer hiển thị đúng joint. Không coi việc backend subscribe topic là đã có URDF/model trong database.

## Xác nhận nhận dữ liệu

Log backend in một lần cho mỗi topic khi nhận message đầu tiên. GET /api/ros/status bổ sung traffic: receivedMessages, lastReceivedUnixMs, lastMessageAgeMs theo topic. WebSocket /api/ros/ws phát joint state với payload topic/messageType/data và nhận frame publish. Đặt REALTIME_ALLOWED_ORIGINS theo địa chỉ frontend. Traffic chỉ lưu bộ đếm/thời điểm, không cache snapshot payload.

Probe không cần DB hay token, dùng cùng RobotRosBL/rclrs thật, chỉ subscribe và không gửi lệnh:

```bash
ROS_DOMAIN_ID=0 cargo run --locked -p robot-agent --example ros_receive_probe --features ros2
# Mặc định đợi 5 JointState (48 joints, position hữu hạn) và 5 Clock trong 60s.
# ROS_PROBE_TIMEOUT_SECONDS, ROS_EXPECTED_JOINTS và ROS_RECEIVE_REPORT có thể ghi đè.
```

Thử headless read-only bằng Python của Isaac Sim sau setup_ros_env.bat:

```text
python.bat <rust-workspace>/UnitTest/isaac_scene_publisher.py --stage <workspace>/robots/yellowarm_orca_dual/usd/YellowArm_Two_ORCA_ros.usda --seconds 180
```

Script chỉ mở/Play/Stop trong tiến trình test, không ghi lại USD. stdout/stderr cần redirect vào logs/ khi chạy nền.

## Đã thử thật — 2026-11-07

- PC: Isaac Sim 6.0.1, internal ROS Jazzy/Fast DDS. Jetson: Ubuntu 22.04, ROS Humble/Fast DDS, Rust 1.99.1, rclrs 0.8.
- Smoke dùng domain 117 tách khỏi domain mặc định. Qua LAN nhận 48 joint, 5 JointState và 5 Clock; probe exit 0.
- Binary backend HTTP chính cũng nhận cả hai topic; /api/health trả status ok với SQLite fixture riêng và list profiles trả mảng rỗng.
- Build ros2 thật thành công. Không dùng shim hoặc mock ROS. Không gửi joint_command và không sửa scene.
- Log/report lưu ở logs/yellowarm_rust_* từ gốc workspace. Tiến trình test giới hạn thời gian và tự dừng; không cài systemd/service hoặc thay DB triển khai.
