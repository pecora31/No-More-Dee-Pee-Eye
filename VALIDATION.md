# No More Dee Pee Eye 0.1.0 alpha — kết quả kiểm tra

Kiểm tra tại máy phát triển ngày 03/10/2026. Người dùng đã xác nhận truy cập Steam và các trang trước đó bị nhà mạng chặn, với tốc độ tốt trong môi trường của họ.

## Đã qua

| Kiểm tra | Kết quả |
| --- | --- |
| `cargo test --locked` | 12/12 test qua |
| `cargo clippy --all-targets --locked -- -D warnings` | Qua, không cảnh báo |
| `cargo fmt --check` | Qua |
| IPC giữa hai tiến trình thật | ACL, PID hai đầu, từ chối schema sai, stop/shutdown |
| Ba preset với `--intercept=0` | Cả ba qua kiểm tra tham số và khởi tạo Lua |
| Bản portable trong đường dẫn tiếng Việt có dấu và khoảng trắng | Kiểm tra lõi và vòng đời chạy thành công |
| Bật → chạy → tắt → bật lại | Thành công với phạm vi `example.com` |
| Mất kênh lệnh từ UI | Helper và lõi kết thúc; không còn tiến trình ứng dụng/winws2 sau test |
| Bốn màn hình bản release | Đã render và xem ảnh thực tế |
| System tray | Tạo được tray; ẩn và mở lại không kết thúc event loop |
| Khởi động ẩn, mở lại bằng lần chạy thứ hai, `--quit` | Qua trong script đo tài nguyên |
| Driver đóng gói | Authenticode `Valid` |

Một lỗi khởi động ẩn trước khi event loop hoạt động đã được sửa. File đo ban đầu ghi 0 là không hợp lệ và đã được thay bằng kết quả đo lại có kiểm tra tiến trình còn sống.

## Tài nguyên đo trước đợt sửa giao diện

Release, software renderer, cửa sổ 1000×710, lõi đang tắt. Đây là Working Set của UI, không phải tổng tài nguyên khi đang xử lý lưu lượng.

| Chỉ số | Giá trị |
| --- | ---: |
| Executable | 7,09 MiB |
| ZIP gồm runtime | 4,79 MiB |
| UI ẩn trong khay | 75,75 MiB Working Set |
| UI đang mở | 79,64 MiB Working Set |
| Private memory khi UI mở | 63,14 MiB |
| CPU khi ẩn, mẫu 10,01 giây | Không ghi nhận mức tăng ở độ phân giải phép đo |

RAM UI mở cao hơn mục tiêu sơ bộ 30–70 MB trong đề xuất; chưa thực hiện tối ưu thêm hoặc benchmark tải mạng. Không suy rộng mẫu CPU ngắn thành cam kết luôn dùng 0% CPU.

## Lõi và artifact

- Zapret2 báo phiên bản `v1.0.5.2`, upstream core commit `6b6c63e3385fa73f8af3be4a69171e947f5a319d`, Lua compatibility 6.
- Windows bundle ghim ở commit `6eb463a6758fb48cd101bc55dfd057e6e9d98af1`.
- Artifact: `dist/NoMoreDeePeeEye-0.1.0-windows-x64.zip`.
- ZIP SHA-256: `b4e8a6f43669f8b020cbb1b5f3576923b173f00e2aca407b8dd40d31df3e561d` (xem checksum chuẩn trong `artifacts/release.sha256`).

Chi tiết máy đọc được nằm trong `artifacts/performance.json`, `artifacts/release-engine-check.txt`, `artifacts/release-engine-smoke.txt`; ảnh ở `artifacts/release-screenshots`.

## Chưa kiểm chứng / ngoài bản đầu

- Tải dài, chuyển Wi-Fi, sleep/resume và IPv6 trên nhiều môi trường; phản hồi truy cập hiện có chỉ áp dụng cho môi trường của người dùng.
- Đăng nhập lại/reboot sau khi người dùng bật tùy chọn mở UI cùng Windows.
- QUIC/UDP, Windows Service tự bật lõi trước đăng nhập, tự tìm chiến lược, tự cập nhật.
- Kiểm tra đầy đủ accessibility bằng trình đọc màn hình, nhiều mức DPI và nhiều tài khoản Windows.

Không bật tùy chọn khởi động cùng Windows trong quá trình kiểm tra. Không thay cấu hình DNS/proxy của máy.


## Cập nhật giao diện

Inter nhúng, bố cục lấy cảm hứng từ WARP: sidebar đen, nền tối phẳng và điểm nhấn xanh dương. No More Dee Pee Eye có biểu tượng trạng thái riêng, không dùng nhận diện Cloudflare. Thông báo thường tự ẩn sau 3 giây; lỗi giữ lại đến khi đóng. Không thay đổi xử lý mạng.

Build release, fmt, clippy và 12 test đều qua. Bốn màn hình release và thao tác ẩn/mở khay đã kiểm tra; ảnh tại artifacts/warp-layout-release. Executable 7.77 MiB, ZIP 5.19 MiB. Số đo RAM phía trên thuộc bản giao diện trước; chưa đo lại.
