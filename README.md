# Robot-Agent-Monitor Rust

Workspace cho backend Rust của Robot-Agent-Monitor. Giữ URL gốc và cấu hình reverse proxy của frontend khi chuyển backend.

Cấu trúc workspace được đối chiếu trực tiếp với `NestedProjects` trong `../Robot-Agent-Monitor/BackEnd/RobotHandMonitor/RobotAgentMonitor.sln`. Hướng dependency giữa các crate khớp các `ProjectReference` của từng `.csproj`.

```text
Robot-Agent-Monitor-Rust/
├── RobotAgentMonitor/
├── Business/
│   └── CRUDBusiness/
├── Infrastructure/
│   ├── DataAccess/
│   ├── RosBridge/
│   └── ThirdPartyClient/
├── Entities/
│   ├── CommonLib/
│   ├── DbObjects/
│   ├── ApiObject/
│   └── Ros2InterfaceMessage/
└── UnitTest/
```

`ThirdPartyClient` thuộc `Infrastructure`; `Ros2InterfaceMessage` thuộc `Entities`. `UnitTest` là nhóm chứa script kiểm tra; solution C# hiện chưa có project test bên trong nhóm này.

| Project | Trách nhiệm trong Rust |
| --- | --- |
| RobotAgentMonitor | REST, SignalR, auth, `StartupApp` và vòng đời ứng dụng |
| CRUDBusiness | `BaseBL<T>`, các `*BL`, `RobotRosConfiguration`, `CameraBL` |
| DataAccess | `DbManagement`, `DapperExtensions`, `BaseDA<T>`, các `*DA` |
| RosBridge | `RobotRos` tạo handle generic; `CameraRos` dùng ROS service client cho encoder |
| CommonLib | Cấu hình ROS, tên bảng, cấu hình integration, upload và lỗi dùng chung |
| DbObjects | `DbBaseObject`, các model và metadata database |
| ApiObject | DTO request/response CRUD và camera |
| ThirdPartyClient | `MediaClient` |
| Ros2InterfaceMessage | Message ROS 2 có kiểu cụ thể: diagnostic, joint state, odometry và TF |

`RobotAgentMonitor/src/startup_app.rs` khởi tạo các service dùng chung qua `Arc`. Host sử dụng `MediaClient` được CRUDBusiness re-export để giữ đúng các dependency trực tiếp của project C#, vốn hỗ trợ tham chiếu assembly bắc cầu.

API gọi BL, BL gọi DA, DA dùng `DapperExtensions` để thao tác SQLite. `DapperExtensions` là extension trait trên `rusqlite::Connection`; cũng có thể gọi qua transaction nhờ Rust tự dereference. Metadata `DbTableAttribute` nằm tại từng model để xác định bảng, field và khóa.

Các lớp C# chỉ kế thừa BaseDA/BaseBL được biểu diễn bằng type alias cùng tên trong file riêng, ví dụ `SensorDA = BaseDA<Sensor>` và `SensorBL = BaseBL<Sensor>`. `RobotConfigurationBL` là struct riêng vì có validation URDF và phối hợp transaction với lưu file. Các tên hiện có như `JointInteface` và `JointGroupMembers` được giữ nguyên. Tên thư mục project giữ đúng tên trong solution. Tên file và method dùng snake_case của Rust.

DB query trả model có kiểu cụ thể. Các thao tác ghi nhận `Record` để giữ hành vi update một phần theo những field client đã gửi. `CrudServices` chọn BL tương ứng với resource tại biên HTTP. Mỗi DA mở connection theo operation; các instance dùng chung chỉ giữ cấu hình và dependency.

## REST

Bốn operation hiện dùng cùng endpoint với backend C#.

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

Mặc định backend bind `127.0.0.1:8080` qua `AGENT_LISTEN`. Dùng reverse proxy HTTPS có hỗ trợ WebSocket để giữ cùng base URL, port và đường dẫn mà frontend đang gọi. Reverse proxy cần giữ public Host hoặc cấu hình origin cụ thể. CRUD có cùng yêu cầu truy cập như backend C# và nên chỉ được expose trong phạm vi mạng dự kiến.

- Trên Linux, đặt `ROS_ENABLED=true` để chạy ROS. Khi chưa đặt `ROBOT_CONFIGURATION_ID`, agent subscribe mặc định `/diagnostics`, `/joint_states`, `/odom`, `/tf` và `/tf_static`. `/tf_static` dùng Reliable + Transient Local. Nếu đặt `ROBOT_CONFIGURATION_ID`, agent lấy danh sách topic đang bật từ SQLite thay cho danh sách mặc định. `ROS_DOMAIN_ID` và `ROS_NAMESPACE` mặc định là `0` và `/`. Khởi động lại agent khi thay cấu hình.
- `rclrs 0.8` dùng ROS 2 RCL/RMW của môi trường đã source. `RobotRos` cung cấp `publisher<T>`, `subscriber<T>`, `service_client<S>` và `service_server<S>`, trả về handle của chính thư viện. Mỗi service Rust giữ `Arc<RobotRos>` cùng các handle nó sử dụng. `node()` cho phép dùng các factory khác của thư viện, gồm action client/server. Message nhận được được chuyển qua event có kiểu cụ thể. Luồng nhận dành cho consumer đang kết nối, có buffer 256 message; consumer chậm nhận thông báo `messages_dropped` để biết đã bỏ lỡ dữ liệu. Cấu hình DDS qua RMW và cấu hình vendor ROS 2 tương ứng.
- Monitor HTTP đăng ký sẵn `diagnostic_msgs/msg/DiagnosticArray`, `diagnostic_msgs/msg/DiagnosticStatus`, `sensor_msgs/msg/JointState`, `nav_msgs/msg/Odometry` và `tf2_msgs/msg/TFMessage` bằng `RobotRosBL::register<T>()` ở StartupApp. Muốn đưa type mới lên HTTP thì implement `RosMessageType` và đăng ký thêm type đó. Service Rust dùng handle trực tiếp chỉ cần trait `Message` của thư viện. Phần cấu hình database hiện mới hỗ trợ topic subscribe/publish. Build mặc định chỉ chạy CRUD/JSON với `ROS_ENABLED=false`; bật ROS khi build bằng feature `ros2`. Không có backend ROS giả khi thiếu ROS 2.
- Encoder camera cần feature `camera-ros` (bao gồm `ros2`) và Rust bindings được generate từ `vmo_camera_interface/srv/StreamEncoder` thật của robot. Các biến `CAMERA_HEAD_ENCODER_SERVICE`, `CAMERA_FRONT_ENCODER_SERVICE`, `CAMERA_BACK_ENCODER_SERVICE` ghi đè `/{camera}_camera/encoder_control`. Request hiện dùng các field `camera`, `action`, `width`, `height`; response dùng `success`, `message`. Cần đối chiếu interface robot khi build; không còn tự encode CDR hay chọn service mapping thủ công.
- Chạy MediaServer của bản C# với cấu hình RTP/ICE hiện tại. Đặt cùng `MEDIA_ACCESS_TOKEN` ở agent và MediaServer, tối thiểu 32 ký tự. Agent gọi `MEDIA_URL` mặc định `http://127.0.0.1:8090/`.
- Camera dùng `/hubs/camera` và `/hubs/camera/negotiate` qua SignalR JSON/WebSocket. Khai báo origin web trong `REALTIME_ALLOWED_ORIGINS`.
- UI mở WebSocket `/api/ros/ws` qua proxy frontend. Backend gửi `status`, `ros_message` và `messages_dropped` ngay sau khi kết nối. Frame `publish` chứa `id`, `topic`, `messageType` và `data`, rồi nhận `publish_result` cùng `id`. ROS và camera không yêu cầu đăng nhập trong bản LAN hiện tại.
- `POST /api/ros/publish` nhận JSON có kiểu cụ thể, ví dụ `{"topic":"/joint_states_out","messageType":"sensor_msgs/msg/JointState","data":{"name":["joint1"],"position":[0.5]}}`. Topic phải được cấu hình làm publisher với cùng message type. `JointState` chấp nhận các mảng position/velocity/effort rỗng hoặc cùng độ dài với name.
- API snapshot cũ đã được bỏ. Các consumer nhận trực tiếp message qua `RobotRosBL::subscribe_messages()` hoặc WebSocket.

Xem phần triển khai encoder và MediaServer ở `../Robot-Agent-Monitor/BackEnd/RobotHandMonitor/ROS_WEBRTC.md`.

Kiểm tra API sau khi build bằng `python UnitTest/http_compatibility.py --executable target/debug/robot-agent` (Windows dùng `target/debug/robot-agent.exe`). Script tạo SQLite và storage tạm, kiểm tra 11 resource. Có thể thêm `--baseline /path/to/old-agent` để so sánh status và JSON với executable trước refactor.

Kiểm tra tổ chức layer bằng `python UnitTest/solution_layout.py`. Script đọc trực tiếp solution, các ProjectReference và Cargo metadata để phát hiện khác biệt về nhóm project hoặc hướng dependency.

Kiểm tra DTO/validation bằng `cargo test -p robot-ros2-interface-message --locked`. Mã hóa wire và type support do rclrs cùng rosidl-generated bindings đảm nhiệm.

## Build với ROS 2

Cài ROS 2 cùng toolchain Rust theo [hướng dẫn ros2-rust](https://github.com/ros2-rust/ros2_rust/blob/main/docs/building.md). Generate Rust interfaces bằng `rosidl_generator_rs` cho `builtin_interfaces`, `std_msgs`, `geometry_msgs`, `diagnostic_msgs`, `sensor_msgs`, `nav_msgs`, `rosgraph_msgs` và `tf2_msgs`. Source cả ROS distribution và workspace chứa các Rust bindings đó; chỉ cài message package dạng binary của ROS chưa đủ. `ros-env` lấy interfaces từ `AMENT_PREFIX_PATH`; các bindings cần hỗ trợ feature `serde`.

```sh
source /opt/ros/$ROS_DISTRO/setup.bash
source /path/to/ros-rust-workspace/install/setup.bash
cargo build --locked -p robot-agent --features ros2
ROS_ENABLED=true ./target/debug/robot-agent
# Có generated vmo_camera_interface: dùng --features camera-ros
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

Trên Windows, build/Clippy mặc định và test DTO/HTTP hoạt động khi không có ROS đầy đủ. Build ros2 cùng transport subscribe đã được kiểm chứng trên Jetson/ROS Humble bằng dữ liệu thật của Isaac Sim PC, ngày 2026-11-07. Phần service/camera-ros và điều khiển qua LAN cần bài thử riêng; xem hướng dẫn YellowArm bên dưới.

## Isaac Sim YellowArm Two ORCA

Cấu hình và lệnh chạy tại [deploy/yellowarm-sim/README.md](deploy/yellowarm-sim/README.md). StartupApp nhận ROS_TOPICS_FILE và gộp với cấu hình DB nếu có. Profile này nhận /yellowarm_sim/joint_states và /yellowarm_sim/clock, đồng thời tạo publisher /yellowarm_sim/joint_command; message Clock dùng rosidl-generated rosgraph_msgs.

RosStatus có thêm traffic theo topic (số message, thời điểm nhận, tuổi dữ liệu), không lưu snapshot. Đã kiểm chứng rclrs thật từ Isaac Sim PC qua LAN tới Jetson ngày 2026-11-07; xem log và điều kiện cụ thể trong README deploy. Kết quả này thay cho giới hạn kiểm chứng Windows/shim của giai đoạn trước ở trên.
