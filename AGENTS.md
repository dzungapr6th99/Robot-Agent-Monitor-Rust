# AGENTS.md — Robot-Agent-Monitor-Rust

Áp dụng cho toàn bộ repo backend Rust. Khi làm việc trong workspace My-Humanoid-Hand, áp dụng thêm các quy tắc chung của workspace.

## Kiến trúc

- Repo này độc lập: tài liệu, comment, script và code không tham chiếu tới backend cũ viết bằng ngôn ngữ khác, file solution/project của nó hay giao thức realtime cũ. `check_architecture.py` báo lỗi khi gặp các tham chiếu đó.
- `RobotAgentMonitor/` xử lý giao tiếp HTTP, WebSocket và khởi tạo service.
- `Business/CRUDBusiness/` chứa nghiệp vụ. API gọi Business, Business gọi các adapter Infrastructure theo dependency hiện có.
- `Infrastructure/` chứa các adapter `DataAccess` và `RosBridge`.
- `Entities/` chứa `ApiObject`, `CommonLib`, `DbObjects` và `Ros2InterfaceMessage`.
- Giữ vị trí các crate và các dependency nội bộ theo kiến trúc hiện tại. Xem [README.md](README.md) để biết trách nhiệm từng crate. Bản nháp kiểm tra CI nằm tại `.github/scripts/check_architecture.py`.

## Trait trong Business và Infrastructure

- Struct phải triển khai trait tương ứng và lớp sử dụng gọi phương thức qua trait.
- Đặt file khai báo trait trong `<layer>/<project>/traits/` của crate sở hữu trait, ví dụ `Business/CRUDBusiness/traits/` hoặc `Infrastructure/DataAccess/traits/`.
- Kiểu tiện ích static được miễn nếu là unit struct và chỉ có hàm liên kết không nhận `self`. Constructor và hàm static có thể nằm trong `impl` riêng của struct đã triển khai trait.
- CI kiểm tra vị trí khai báo trait trong các file Rust của hai layer và kiểm tra struct trong `src/` và `traits/` bằng `.github/scripts/check_traits.py`. Script ghi rõ bốn struct dữ liệu hiện hữu được miễn trong giai đoạn chuyển tiếp. Struct mới phải tuân thủ quy tắc trên.
- Alias CRUD dùng trait generic mà kiểu gốc triển khai theo entity.

## Kiểm tra trước commit và merge

- Với mỗi clone dùng để sửa mã, cài hook bằng `pre-commit install --install-hooks` từ gốc repo sau khi cài công cụ `pre-commit`. Hook cần Cargo và chạy cùng hai script kiểm tra layout và trait mà GitHub CI dùng. Chạy `pre-commit run --all-files` để kiểm tra thủ công. Sửa lỗi kiểm tra trước khi commit.
- Khi đưa workspace Rust vào thư mục con của repo lớn hơn, giữ `.github/scripts/` trong workspace Rust. Đặt workflow ở `.github/workflows/` của repo lớn và chỉnh `defaults.run.working-directory` thành đường dẫn tới workspace Rust. Gộp hai hook vào `.pre-commit-config.yaml` ở gốc repo lớn và thêm đường dẫn workspace Rust trước đường dẫn script trong mỗi `entry`.
- Để chặn merge, đặt job `Rust architecture` làm required status check trong ruleset của nhánh đích trên repo chứa workflow. Chỉnh nhánh trong workflow nếu repo dùng nhánh đích khác `main`.

## Xử lý lỗi và logging

- Không throw exception trong luồng nghiệp vụ. Hàm nghiệp vụ có thể lỗi trong Business và Infrastructure dùng `Result`, xử lý và log ở mức `ERROR` tại biên hàm trước khi trả `Err` cho caller. Giữ mã lỗi và ngữ cảnh của lỗi gốc.
- Log `INFO` trước khi xử lý kết nối hoặc request từ service khác và khi bắt đầu thread hoặc task nền. Chỉ ghi metadata cần thiết. Không ghi token hoặc payload.
- Hạn chế kiểu trả về `()` trong nghiệp vụ. Dùng `bool` cho trạng thái, số nguyên cho số lượng hoặc mã kết quả khi có ý nghĩa, hoặc `Result` khi cần truyền lỗi có kiểu. Giữ chữ ký callback và lifecycle do framework quy định.
