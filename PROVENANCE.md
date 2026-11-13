# Provenance và giới hạn

Nguồn đầu vào nghiên cứu (chỉ ghi thông tin định danh; **không** upload các binary):

| Nguồn | Mục đích | SHA-256 |
|---|---|---|
| `TLauncher.exe` (2026) | PE, Java/GraalVM native starter | `c053b982e7ed3ab28eddcc1c74f418bdea7425e1462535a319fde6b2f73197b2` |
| `TLauncher-Installer-2.1.0.5.exe` | Installer và embedded Bootstrapper | `25189514c0b7739bf5549331fec12db48ab694770cd20d5f2b23057e73fd1e17` |
| `original-TLauncher-2.923.zip` | Bytecode lịch sử tham khảo | `0e83efed2a35e23fda8ecc87220516b5369e6628f9fd35da4e9082562b52f250` |

Tài liệu sinh trong quá trình phân tích tĩnh và xây dựng yêu cầu. Do không có mã nguồn implementation mới, tài liệu này không chứng minh launcher hoạt động. Mọi thông tin về source v2.923 không được mặc định đúng với binary 2026. Không phân phối lại binary/JAR/asset gốc trong repository này.

Các tài liệu v0–v5 có thể chứa nhận định tiến hóa theo thời gian; **v5 là baseline nghiên cứu hiện tại**, lịch sử chỉ dùng để tham khảo và kiểm toán. Nên kiểm tra lại các dẫn chứng quan trọng ở bytecode/metadata gốc trước khi triển khai.