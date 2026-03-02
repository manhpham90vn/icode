## 1. Luồng hoạt động tổng quan (The Flow)

Hệ thống Telegram gồm 2 luồng chính giao tiếp với nhau:
1. **Luồng Cảnh báo (Outbound)**: Claude Code -> System -> Telegram Message.
2. **Luồng Lệnh (Inbound)**: Telegram Message -> System -> Claude Code (Terminal).

---

## 2. Chi tiết Luồng Cảnh Báo (Outbound)

Luồng này được kích hoạt tự động mỗi khi Claude Code dừng lại (chờ lệnh tiếp hoặc đã làm xong tác vụ).

### Bước 2.1: Kích hoạt (Claude Hooks)
Người dùng cần cấu hình file `~/.claude/settings.json` để thêm hook.
Khi sự kiện `Stop` hoặc `SubagentStop` xảy ra, Claude Code sẽ tự động chạy một script CLI (ví dụ: `python notify.py completed`).

### Bước 2.2: Đọc dữ liệu Terminal (Scraper/Monitor)
Do Claude Code chạy trên terminal, ta cần một cách đọc màn hình terminal đó.
- Công cụ tìm kiếm session hiện tại. Nếu dùng **tmux**, chạy lệnh `tmux display-message -p "#S"` để lấy tên session (VD: `claude-session`).
- Chạy `tmux capture-pane -t <session_name> -p` để lấy toàn bộ text hiện có trên màn hình.
- **Phân tách nội dung (Parsing)**: Dùng Regex duyệt qua nội dung trên để tìm xem *User vừa hỏi gì* và *Claude vừa trả lời gì*.
  - Prompt thường bắt đầu bằng ký tự `> `
  - Output của Claude thường nằm trong các ký tự vẽ bảng box (`╭─`, `╰─`) hoặc bắt đầu bằng `⏺ `.

### Bước 2.3: Tạo Phiên (Session & Token)
- Random một `TOKEN` 8 ký tự (VD: `A1B2C3D4`).
- Lưu cục bộ một file JSON (VD: `data/sessions/<uuid>.json`) chứa:
  - `token`
  - `tmuxSession` (tên của màn hình terminal)
  - Thời gian hết hạn (thường là sau 24h).

### Bước 2.4: Gửi Telegram Message
Gọi API [sendMessage](file:///home/manh/Project/self/Claude-Code-Remote/src/channels/telegram/webhook.js#285-300) của Telegram dạng `POST https://api.telegram.org/bot<BOT_TOKEN>/sendMessage`.
Nội dung (Markdown) bao gồm: Tiêu đề trạng thái, Tóm tắt câu hỏi - câu trả lời vừa parse ở Bước 2.2, và Token.
**Lưu ý UI Telegram**: Gửi kèm `inline_keyboard` (2 nút bấm) chứa `callback_data` như `personal:<TOKEN>`. Khi bấm vào, Bot sẽ tự chat hướng dẫn form nhập lệnh bắt đầu bằng `/cmd <TOKEN>` để người dùng copy làm mẫu cho nhanh.

---

## 3. Chi tiết Luồng Lệnh (Inbound)

Luồng này luôn chạy ngầm để ứng trực nhận tin nhắn từ người dùng.

### Bước 3.1: Telegram Webhook / Polling Server
Khởi chạy một Web Server (VD: Express NodeJS hoặc FastAPI Python) nhận HTTP request tại `/webhook/telegram` (hoặc dùng thư viện built-in Polling).

### Bước 3.2: Xác thực (Authentication)
Duyệt object Message gửi lên từ Telegram. 
Kiểm tra biến `message.chat.id` hoặc `message.from.id` xem có nằm trong **Whitelist** đã khai báo ở `.env` hay không. Ngăn chặn người lạ dùng bot.

### Bước 3.3: Map lệnh với Session
Tìm kiếm chuỗi tin nhắn xem có chứa mẫu `<TOKEN> <prompt>` hoặc `/cmd <TOKEN> <prompt>` không.
- Bóc tách token. Mở thư mục `data/sessions/` để đối chiếu xem token này có hợp lệ không.
- Nếu token hợp lệ, ta lấy được thông tin `tmuxSession` từ file log cục bộ đó.

### Bước 3.4: Tiêm lệnh vào Terminal (Command Injection)
Đây là khâu quyết định. Biết được tên tmux session, hệ thống tiến hành "gõ phím" ảo từ xa.
- Gọi shell command: 
  - `tmux send-keys -t <tmux_session> '<Nội dung prompt đã escape>'`
  - `tmux send-keys -t <tmux_session> Enter`
*(Nếu không dùng tmux mà dùng PTY, công cụ sẽ ghi thẳng prompt vào file descriptor `pty` giả lập).*

### Bước 3.5: Thông báo thành công
Sau khi gọi lệnh tiêm phím, Server tự động gọi lại Telegram API để Reply tin nhắn của user: `✅ Lệnh đã được đẩy thành công... (Claude đang xử lý)`.