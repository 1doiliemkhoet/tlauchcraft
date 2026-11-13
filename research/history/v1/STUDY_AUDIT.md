# TLauncher — kiểm toán dữ liệu Study & bàn giao kỹ thuật

**Nguồn được kiểm tra:** `re tlaucher.zip` do người dùng cung cấp, ngày 13/11/2026. Phân tích chỉ đọc các file JSON, Markdown và log; không chạy `TLauncher.exe` và không có bản nhị phân này trong archive.

## Kết luận

Dữ liệu hiện đủ để nhận diện sơ bộ kiến trúc executable và ghi lại việc Craft Anything đã tiến hành khảo sát, **chưa đủ để tái tạo giao diện và cơ chế TLauncher** hoặc kết luận chương trình có/không có malware. `app/` rỗng; `investigation.cany/investigation/findings.md` ghi `_None yet._`; `handoff.md` ghi `_No handoff yet._`.

Việc Ghidra hoàn tất 66 phút được ghi trong `capture/README.md`; ZIP **không chứa** bản database/project Ghidra hoặc pseudocode đã xuất. Không thể khẳng định việc sao chép ZIP sẽ cho phép khôi phục cache Ghidra trên máy khác. Hãy giữ nguyên project và thư mục cache trên PC gốc.

## Số liệu xác minh từ evidence

| Mục | Kết quả |
|---|---|
| Định dạng / hệ điều hành | PE x86_64, Windows GUI, 64-bit |
| SHA-256 của executable gốc (theo evidence) | `c053b982e7ed3ab28eddcc1c74f418bdea7425e1462535a319fde6b2f73197b2` |
| Kích thước mục tiêu theo log | 72,155,352 byte (~68.81 MiB) |
| Entry point | `0x1421ba730` |
| Sections | 8; đáng chú ý `.text` ~35.38 MB, `.svm_hea` ~35.26 MB |
| Hàm phát hiện | 37,349 (chưa đầy đủ; dừng ở hạn mức 6 triệu instructions) |
| Strings phát hiện | 193,969 (mới có số đếm, không có danh sách chuỗi) |
| Imports / libraries | 621 imports từ 35 libraries |
| Exports | 702; chi tiết 200 exports đầu tiên đã lưu trong ZIP |
| Evidence | 5 JSON files |
| Source được build | Không: `app/` rỗng |
| Thống kê AI (usage.json) | 1,401,909 input tokens; 1,335 output tokens; `cost_usd: 0` theo ứng dụng |

### Kiến trúc: quan sát và suy luận

**Đã quan sát:** PE có section `.svm_hea` và tên exported functions `IsolateEnterStub__JavaMainWrapper__run`, `IsolateEnterStub__CEntryPointNativeFunctions__createIsolate`, `JNI_OnLoad_javafx_font`, `JNI_OnLoad_prism_d3d`. Bảng export khai báo tên executable nội bộ `starter-core-1.269-native.exe`.

**Suy luận có cơ sở, chưa xác thực với code:** Khả năng cao executable là Java application sử dụng JavaFX, được biên dịch thành GraalVM Native Image. Đừng coi mọi phần trong binary là code launcher ứng dụng; lượng lớn code có thể đến từ Java runtime / GraalVM / JavaFX.

**Không được kết luận:** Import `WINHTTP.dll`, `CRYPT32.dll` hoặc JNI có nghĩa là có malware. Cần thêm endpoint, hành vi, persistence, file changes và luồng dữ liệu để đánh giá.

## Hồ sơ evidence

- `ev_65f88fc9…`: `core_target_triage`, mô tả PE/JNI/entry point, dựa trên phân tích sơ bộ.
- `ev_12919a19…`: `core_overview_show`, counts và các cảnh báo `instruction_limit`, `indirect_jumps_unresolved`.
- `ev_3c022a6b…`: `native_sections_list`, 8 sections (bao gồm `.svm_hea`).
- `ev_63647226…`: `native_offered_symbols_list`, có chi tiết 200/702 exports.
- `ev_e74a42fe…`: `core_evidence_browse`, phần tổng quan đã xem.

Log `build-activity.log` cũng ghi `native_pe_resources_list` không chạy được do file 72,155,352 byte > giới hạn 67,108,864 byte của công cụ đó. Đây là giới hạn của tool, **không có nghĩa PE bị hỏng**.

## Những thứ thiếu để xây dựng bản thay thế đúng yêu cầu

1. **Giao diện:** screenshot cửa sổ chính, danh sách phiên bản, tài khoản, cài đặt, tải/cài đặt, lỗi và tiến độ.
2. **Chức năng:** yêu cầu chính xác về login (Microsoft OAuth hay tài khoản khác), Minecraft version manifest, Forge/Fabric/OptiFine, Java runtime, JVM arguments, game directory, downloads, progress, logging.
3. **Phân tích thực thi:** code của các hàm đặc thù launcher (không phải runtime), các string liên quan settings/server/version/download, và xrefs đến chúng.
4. **Đánh giá an toàn:** kết quả kiểm tra signature, network destinations, persistence, startup tasks, auto-updater, file writes, các module được tải động; cần một máy ảo cách ly trước khi quan sát chạy thật.
5. **Ghidra data:** cache/database ngoài ZIP nếu muốn dùng lại phân tích native và decompile từng hàm mà không phải scan lại.

## Lộ trình xây bản mới an toàn

- **Giai đoạn A:** Định nghĩa UI và tính năng từ screenshots và nguồn quan sát hợp lệ. Tạo một bản mô phỏng giao diện không kết nối mạng.
- **Giai đoạn B:** Cài đặt chức năng quản lý phiên bản và Java theo API/manifest chính thức, tải qua HTTPS với xác minh hash, quản lý thư mục và JVM args.
- **Giai đoạn C:** Tích hợp Microsoft OAuth theo luồng được hỗ trợ (không lưu mật khẩu thô, không tái sử dụng token của launcher gốc), launch pipeline và logging.
- **Giai đoạn D:** Kiểm thử trên Windows với dữ liệu test, rà soát dependency, xác minh không có adware/telemetry ngoài chức năng cần thiết.

**Stack đề nghị:** Rust + egui/eframe, đúng brief hiện tại. Không nên copy decompiled code, assets độc quyền hay thông tin xác thực vào ứng dụng mới.

## Bước tiếp theo tiết kiệm API

Không cần gọi tiếp Gemini trong Craft Anything để lấy lại dữ liệu của ZIP: đã có các evidence sẵn. Để truy xuất **thêm** thông tin trực tiếp từ binary trên máy của người dùng, Craft Anything CLI có các tool `native_strings_find`, `native_functions_search`, `native_function_pseudocode`, `core_feature_find`, `native_window_capture`. Xem `craft-anything-cli <tool-name-with-dashes> --help` để lấy tham số đúng cho bản cài đặt, rồi xuất từng phần liên quan thay vì cố nhét 240–254K tokens vào một request. **Không chạy executable nghi vấn trên máy chính** để lấy screenshot; ưu tiên ảnh có sẵn hoặc Windows VM/sandbox.

## Giới hạn phạm vi kiểm toán

Tài liệu này không kết luận TLauncher là malware hay sạch; không kiểm tra trực tiếp executable (không có trong archive); không xây dựng hay biên dịch ứng dụng mới; và chưa xác thực cache Ghidra nằm ở vị trí nào trên PC của người dùng.