# Robot-Agent-Monitor Rust

Backend Rust của Robot-Agent-Monitor: REST CRUD trên SQLite, cầu nối ROS 2 qua `rclrs` và WebSocket realtime cho frontend. Repo tự đứng được, không cần repo nào khác để build, test hay chạy.

```text
Robot-Agent-Monitor-Rust/
├── RobotAgentMonitor/
├── Business/
│   └── CRUDBusiness/
├── Infrastructure/
│   ├── DataAccess/
│   └── RosBridge/
├── Entities/
│   ├── CommonLib/
│   ├── DbObjects/
│   ├── ApiObject/
│   └── Ros2InterfaceMessage/
└── UnitTest/
```

`UnitTest` chứa các script kiểm tra HTTP và Isaac Sim. Vị trí crate và hướng dependency giữa các crate được kiểm tra bằng `.github/scripts/check_architecture.py`.

| Project | Trách nhiệm |
| --- | --- |
| RobotAgentMonitor | REST, WebSocket ROS, `StartupApp` và vòng đời ứng dụng |
| CRUDBusiness | `BaseBL<T>`, các `*BL`, `RobotRosConfiguration`, `RobotRosBL` |
| DataAccess | `DbManagement`, `DapperExtensions`, `BaseDA<T>`, các `*DA` |
| RosBridge | `RobotRos` tạo handle generic của rclrs |
| CommonLib | Cấu hình ROS, tên bảng, upload, log và lỗi dùng chung |
| DbObjects | `DbBaseObject`, các model và metadata database |
| ApiObject | DTO request/response CRUD |
| Ros2InterfaceMessage | Message ROS 2 có kiểu cụ thể: diagnostic, joint state, odometry, TF và clock |

`RobotAgentMonitor/src/startup_app.rs` khởi tạo các service dùng chung qua `Arc`.

API gọi BL, BL gọi DA, DA dùng `DapperExtensions` để thao tác SQLite. `DapperExtensions` là extension trait trên `rusqlite::Connection`; cũng có thể gọi qua transaction nhờ Rust tự dereference. Metadata `DbTableAttribute` nằm tại từng model để xác định bảng, field và khóa.

Entity chỉ cần CRUD chung được khai báo bằng type alias trong file riêng, ví dụ `SensorDA = BaseDA<Sensor>` và `SensorBL = BaseBL<Sensor>`. `RobotConfigurationBL` là struct riêng vì có validation URDF và phối hợp transaction với lưu file. Các tên hiện có như `JointInteface` và `JointGroupMembers` được giữ nguyên. Tên file và method dùng snake_case của Rust.

DB query trả model có kiểu cụ thể. Các thao tác ghi nhận `Record` để giữ hành vi update một phần theo những field client đã gửi. `CrudServices` chọn BL tương ứng với resource tại biên HTTP. Mỗi DA mở connection theo operation; các instance dùng chung chỉ giữ cấu hình và dependency.

## REST

| Method | Path | Request |
| --- | --- | --- |
| POST | `/api/v1/{resource}/create` | JSON `{requestId,data}` hoặc multipart đối với robot configuration |
| PUT | `/api/v1/{resource}/update` | JSON `{requestId,data}` |
| DELETE | `/api/v1/{resource}/delete` | JSON `{requestId,data}` |
| GET | `/api/v1/{resource}/query?ids=1&ids=2` | Lặp tham số `ids` |

Resources gồm robot-profiles, robot-configurations, joint-interfaces, sensors, sensor-streams, joint-groups, joint-group-members, pose-presets, pose-preset-joints, episodes và episode-annotations. Write response có `requestId`, `code` và `message`, thêm `recordId` cho create. Query trả mảng record; trường hợp không có dữ liệu trả HTTP 400 cùng query envelope.

Robot configuration create nhận multipart `RequestId`, `Data.RobotProfileId`, `Data.Name` và `AttachFiles`. URDF được kiểm tra trước khi lưu vào `robots/{profileId}/{configurationId}/{filename}` dưới storage root. DA dùng schema SQLite Monitor đang có. Với `joint-group-members` và `pose-preset-joints`, API biểu diễn `id` qua SQLite rowid vì bảng dùng khóa ghép.

## Chạy

```sh
export ROBOT_AGENT_DATABASE=/path/to/robot-agent.db
export URDF_DATA_ROOT=/path/to/data
cargo run --locked -p robot-agent
```

Mặc định backend bind `127.0.0.1:8080` qua `AGENT_LISTEN`. Dùng reverse proxy HTTPS có hỗ trợ WebSocket để giữ cùng base URL, port và đường dẫn mà frontend đang gọi. Reverse proxy cần giữ public Host hoặc cấu hình origin cụ thể. CRUD không có đăng nhập, chỉ expose trong phạm vi mạng dự kiến.

- Trên Linux, đặt `ROS_ENABLED=true` để chạy ROS. Khi chưa đặt `ROBOT_CONFIGURATION_ID`, agent subscribe mặc định `/diagnostics`, `/joint_states`, `/odom`, `/tf` và `/tf_static`. `/tf_static` dùng Reliable + Transient Local. Nếu đặt `ROBOT_CONFIGURATION_ID`, agent lấy danh sách topic đang bật từ SQLite thay cho danh sách mặc định. `ROS_DOMAIN_ID` và `ROS_NAMESPACE` mặc định là `0` và `/`. Khởi động lại agent khi thay cấu hình.
- `rclrs 0.8` dùng ROS 2 RCL/RMW của môi trường đã source. `RobotRos` cung cấp `publisher<T>`, `subscriber<T>`, `service_client<S>` và `service_server<S>`, trả về handle của chính thư viện. Mỗi service Rust giữ `Arc<RobotRos>` cùng các handle nó sử dụng. `node()` cho phép dùng các factory khác của thư viện, gồm action client/server. Message nhận được được chuyển qua event có kiểu cụ thể. Luồng nhận dành cho consumer đang kết nối, có buffer 256 message; consumer chậm nhận thông báo `messages_dropped` để biết đã bỏ lỡ dữ liệu. Cấu hình DDS qua RMW và cấu hình vendor ROS 2 tương ứng.
- Monitor HTTP đăng ký sẵn `diagnostic_msgs/msg/DiagnosticArray`, `diagnostic_msgs/msg/DiagnosticStatus`, `sensor_msgs/msg/JointState`, `nav_msgs/msg/Odometry`, `tf2_msgs/msg/TFMessage` và `rosgraph_msgs/msg/Clock` bằng `RobotRosBL::register<T>()` ở StartupApp. Muốn đưa type mới lên HTTP thì implement `RosMessageType` và đăng ký thêm type đó. Service Rust dùng handle trực tiếp chỉ cần trait `Message` của thư viện. Phần cấu hình database hiện mới hỗ trợ topic subscribe/publish. Build mặc định chỉ chạy CRUD/JSON với `ROS_ENABLED=false`; bật ROS khi build bằng feature `ros2`. Không có backend ROS giả khi thiếu ROS 2.
- UI mở WebSocket `/api/ros/ws` qua proxy frontend. Backend gửi `status`, `ros_message` và `messages_dropped` ngay sau khi kết nối. Frame `publish` chứa `id`, `topic`, `messageType` và `data`, rồi nhận `publish_result` cùng `id`. Khai báo origin web trong `REALTIME_ALLOWED_ORIGINS`. ROS không yêu cầu đăng nhập trong bản LAN hiện tại.
- `POST /api/ros/publish` nhận JSON có kiểu cụ thể, ví dụ `{"topic":"/joint_states_out","messageType":"sensor_msgs/msg/JointState","data":{"name":["joint1"],"position":[0.5]}}`. Topic phải được cấu hình làm publisher với cùng message type. `JointState` chấp nhận các mảng position/velocity/effort rỗng hoặc cùng độ dài với name.
- Các consumer nhận trực tiếp message qua `RobotRosBL::subscribe_messages()` hoặc WebSocket.

## Camera

Backend hiện chưa có luồng camera. Cụm camera cũ (media server ngoài, encoder service phía robot, hub realtime `/hubs/camera`) đã được gỡ. Luồng thay thế dự kiến: subscribe `sensor_msgs/msg/Image` → encode H264 bằng CPU → WebRTC qua WHEP (`POST /api/camera/{name}/whep`).

## Kiểm tra

- API: sau khi build, chạy `python UnitTest/http_compatibility.py --executable target/debug/robot-agent` (Windows dùng `target/debug/robot-agent.exe`). Script tự tạo SQLite theo schema nhúng trong script và storage tạm, kiểm tra 11 resource. Có thể thêm `--baseline /path/to/old-agent` để so sánh status và JSON với một executable khác.
- Layout, dependency và tham chiếu cấm: `python .github/scripts/check_architecture.py`. Struct/trait: `python .github/scripts/check_traits.py`.
- DTO/validation: `cargo test -p robot-ros2-interface-message --locked`. Mã hóa wire và type support do rclrs cùng rosidl-generated bindings đảm nhiệm.

## Build với ROS 2

Cài ROS 2 cùng toolchain Rust theo [hướng dẫn ros2-rust](https://github.com/ros2-rust/ros2_rust/blob/main/docs/building.md). Generate Rust interfaces bằng `rosidl_generator_rs` cho `builtin_interfaces`, `std_msgs`, `geometry_msgs`, `diagnostic_msgs`, `sensor_msgs`, `nav_msgs`, `rosgraph_msgs` và `tf2_msgs`. Source cả ROS distribution và workspace chứa các Rust bindings đó; chỉ cài message package dạng binary của ROS chưa đủ. `ros-env` lấy interfaces từ `AMENT_PREFIX_PATH`; các bindings cần hỗ trợ feature `serde`.

```sh
source /opt/ros/$ROS_DISTRO/setup.bash
source /path/to/ros-rust-workspace/install/setup.bash
cargo build --locked -p robot-agent --features ros2
ROS_ENABLED=true ./target/debug/robot-agent
```

Không dùng `use_ros_shim` cho runtime. Feature này của upstream chỉ phục vụ tài liệu; build sản phẩm phải liên kết ROS 2 thật. Chưa thêm ros2-client/Zenoh.

Ví dụ service Rust giữ subscription nhận JointState; callback chạy trên executor của node dùng chung. Giữ handle trong struct để subscription tiếp tục tồn tại.

```rust
use robot_ros_bridge::{RobotRos, Subscription, messages::JointState, traits::RobotRosTrait};

fn joint_states(ros: &RobotRos) -> anyhow::Result<Subscription<JointState>> {
    ros.subscriber::<JointState>(
        "/joint_states",
        RobotRos::qos(true, 10, false),
        |message| println!("{} joints", message.name.len()),
    )
}
```

Service VLA có thể giữ `Subscription<Observation>`, `Publisher<Action>` hoặc `Client<Inference>` với generated message/service tùy ý. Gọi `ros.service_client::<Inference>(...)`, `ros.service_server::<Inference>(...)` hoặc lấy `ros.node()?` để dùng các factory/action khác của rclrs. Không cần đăng ký type với HTTP monitor để dùng trực tiếp trong Rust.

`RobotRos` giữ Context, node và executor thread; shutdown dừng executor và join thread. Handle rclrs được chia sẻ qua Arc, không khóa node thủ công. `RosMessageEvent` giữ format JSON của monitor, không có enum tập trung dành cho mọi nghiệp vụ.

Trên Windows, build/Clippy mặc định và test DTO/HTTP hoạt động khi không có ROS đầy đủ. Build ros2 cùng transport subscribe đã được kiểm chứng trên Jetson/ROS Humble bằng dữ liệu thật của Isaac Sim PC, ngày 2026-11-07.

## Deploy lên Jetson

Cách deploy hiện tại: làm theo [deploy/jetson-home/README.md](deploy/jetson-home/README.md) (copy source, build trên Jetson, chạy `start-backend.sh` và `start-frontend.sh`). Thư mục này chỉ được gỡ sau khi script deploy một lệnh ở cấp project Robot đã có và chạy được.

## Isaac Sim YellowArm Two ORCA

Cấu hình và lệnh chạy tại [deploy/yellowarm-sim/README.md](deploy/yellowarm-sim/README.md). StartupApp nhận ROS_TOPICS_FILE và gộp với cấu hình DB nếu có. Profile này nhận /yellowarm_sim/joint_states và /yellowarm_sim/clock, đồng thời tạo publisher /yellowarm_sim/joint_command; message Clock dùng rosidl-generated rosgraph_msgs.

RosStatus có thêm traffic theo topic (số message, thời điểm nhận, tuổi dữ liệu), không lưu snapshot. Đã kiểm chứng rclrs thật từ Isaac Sim PC qua LAN tới Jetson ngày 2026-11-07; xem log và điều kiện cụ thể trong README deploy.
