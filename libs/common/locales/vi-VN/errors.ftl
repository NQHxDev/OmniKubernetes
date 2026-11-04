# Chuỗi thông báo lỗi (vi-VN)

# RFC 9457 HTTP status
err-bad-request-title = Yêu cầu không hợp lệ
err-bad-request-detail = Yêu cầu không thể xử lý do cú pháp hoặc tham số không hợp lệ.

err-unauthorized-title = Chưa xác thực
err-unauthorized-detail = Yêu cầu thông tin định danh xác thực hoặc thông tin xác thực không hợp lệ.

err-forbidden-title = Không có quyền truy cập
err-forbidden-detail = Bạn không có quyền truy cập vào tài nguyên này.

err-not-found-title = Không tìm thấy tài nguyên
err-not-found-detail = Tài nguyên yêu cầu không tồn tại hoặc đã bị xóa.

err-method-not-allowed-title = Phương thức không được hỗ trợ
err-method-not-allowed-detail = Phương thức HTTP không được hỗ trợ cho endpoint này.

err-conflict-title = Xung đột dữ liệu
err-conflict-detail = Phát sinh xung đột với trạng thái hiện tại của tài nguyên đích.

err-validation-failed-title = Dữ liệu yêu cầu không hợp lệ
err-validation-failed-detail = Có {$count} trường dữ liệu không vượt qua kiểm tra ràng buộc nghiệp vụ.

err-rate-limited-title = Vượt quá giới hạn yêu cầu
err-rate-limited-detail = Vượt quá tần suất yêu cầu cho phép. Vui lòng thử lại sau {$retry_after} giây.

err-internal-server-title = Lỗi máy chủ nội bộ
err-internal-server-detail = Hệ thống đang gặp gián đoạn tạm thời. Vui lòng thử lại sau.
err_internal_server = Hệ thống đang gặp gián đoạn tạm thời. Vui lòng thử lại sau.

err-service-unavailable-title = Dịch vụ tạm thời không khả dụng
err-service-unavailable-detail = Dịch vụ đang trong quá trình bảo trì hoặc quá tải. Vui lòng thử lại sau.

# Ràng buộc validation (RFC 9457 invalid_params)
val-required = Trường '{$name}' là bắt buộc.
val-invalid-email = Địa chỉ email không đúng định dạng.
val-string-min-length = {$name} phải có tối thiểu {$min} ký tự (hiện tại có {$actual} ký tự).
val-string-max-length = {$name} không được vượt quá {$max} ký tự (hiện tại có {$actual} ký tự).
val-number-range = Trường '{$name}' phải nằm trong khoảng từ {$min} đến {$max}.
val-pattern-mismatch = Trường '{$name}' không khớp định dạng yêu cầu.
val-invalid-uuid = Trường '{$name}' phải là định dạng UUID hợp lệ.
val-invalid-format = Định dạng trường '{$name}' không hợp lệ.

# Mã lỗi nghiệp vụ
INVALID_EMAIL_FORMAT = Địa chỉ email không đúng định dạng.
STRING_MIN_LENGTH = {$name} phải có tối thiểu {$min} ký tự (hiện tại có {$actual} ký tự).
NUMBER_MIN_VALUE = Số lượng sản phẩm trong đơn hàng phải lớn hơn 0.
INVALID_POSTAL_CODE = Mã bưu chính không đúng định dạng.
ERR_VALIDATION_FAILED = Kiểm tra ràng buộc nghiệp vụ không thành công.

# Lỗi hạ tầng OmniKubernetes
err-cluster-unreachable = Không thể kết nối tới cụm '{$cluster_name}' tại địa chỉ '{$endpoint}'.
err-namespace-not-found = Namespace '{$namespace}' không tồn tại trong cụm '{$cluster_name}'.
err-pod-quota-exceeded = Không thể tạo pod: Vượt quá giới hạn trong namespace '{$namespace}' (tối đa: {$limit}, hiện tại: {$current}).
err-deployment-failed = Triển khai '{$deployment}' trong namespace '{$namespace}' thất bại: {$reason}.
err-resource-conflict = Tài nguyên '{$resource}' thuộc loại '{$kind}' đã tồn tại trong namespace '{$namespace}'.

# Phân hệ Nhận dạng & Bảo mật truy cập (omni-auth)
err-auth-missing-credentials-title = Chưa cung cấp thông tin xác thực
err-auth-missing-credentials-detail = Yêu cầu thông tin định danh xác thực trong header Authorization hoặc thông tin đăng nhập.
ERR_AUTH_MISSING_CREDENTIALS = Yêu cầu thông tin định danh xác thực trong header Authorization hoặc thông tin đăng nhập.

err-auth-token-expired-title = Mã xác thực đã hết hạn
err-auth-token-expired-detail = Mã xác thực của bạn đã hết hạn. Vui lòng làm mới phiên làm việc bằng Refresh Token.
ERR_AUTH_TOKEN_EXPIRED = Mã xác thực của bạn đã hết hạn. Vui lòng làm mới phiên làm việc bằng Refresh Token.

err-auth-token-revoked-title = Mã xác thực đã bị thu hồi
err-auth-token-revoked-detail = Mã xác thực đã bị thu hồi do đăng xuất, đổi mật khẩu hoặc phiên làm việc đã bị hủy.
ERR_AUTH_TOKEN_REVOKED = Mã xác thực đã bị thu hồi do đăng xuất, đổi mật khẩu hoặc phiên làm việc đã bị hủy.

err-auth-invalid-signature-title = Chữ ký số không hợp lệ
err-auth-invalid-signature-detail = Chữ ký số của mã xác thực không hợp lệ hoặc dữ liệu đã bị can thiệp trái phép.
ERR_AUTH_INVALID_SIGNATURE = Chữ ký số của mã xác thực không hợp lệ hoặc dữ liệu đã bị can thiệp trái phép.

err-auth-malformed-token-title = Định dạng mã xác thực không hợp lệ
err-auth-malformed-token-detail = Mã xác thực không tuân thủ đúng cấu trúc chuẩn JWT.
ERR_AUTH_MALFORMED_TOKEN = Mã xác thực không tuân thủ đúng cấu trúc chuẩn JWT.

err-auth-invalid-credentials-title = Thông tin đăng nhập không chính xác
err-auth-invalid-credentials-detail = Tên đăng nhập hoặc mật khẩu không chính xác.
ERR_AUTH_INVALID_CREDENTIALS = Tên đăng nhập hoặc mật khẩu không chính xác.

err-auth-insufficient-roles-title = Không đủ quyền theo vai trò
err-auth-insufficient-roles-detail = Bạn không sở hữu vai trò yêu cầu để thực hiện hành động này.
ERR_AUTH_INSUFFICIENT_ROLES = Bạn không sở hữu vai trò yêu cầu để thực hiện hành động này.

err-auth-insufficient-perms-title = Không đủ quyền hạn thực thi
err-auth-insufficient-perms-detail = Bạn thiếu quyền hạn nghiệp vụ cụ thể cần thiết để truy cập tài nguyên.
ERR_AUTH_INSUFFICIENT_PERMS = Bạn thiếu quyền hạn nghiệp vụ cụ thể cần thiết để truy cập tài nguyên.

err-auth-tenant-mismatch-title = Không có quyền truy cập Tenant
err-auth-tenant-mismatch-detail = Bạn không có quyền truy cập vào không gian dữ liệu của tổ chức/tenant này.
ERR_AUTH_TENANT_MISMATCH = Bạn không có quyền truy cập vào không gian dữ liệu của tổ chức/tenant này.

err-auth-signature-expired-title = Chữ ký nội bộ quá hạn
err-auth-signature-expired-detail = Chữ ký xác thực mạng nội bộ đã hết hạn hoặc độ lệch đồng hồ vượt ngưỡng cho phép.
ERR_AUTH_SIGNATURE_EXPIRED = Chữ ký xác thực mạng nội bộ đã hết hạn hoặc độ lệch đồng hồ vượt ngưỡng cho phép.

err-auth-token-theft-detected-title = Phát hiện nguy cơ bảo mật phiên
err-auth-token-theft-detected-detail = Phát hiện hành vi sử dụng lại token làm mới đã bị thu hồi; toàn bộ phiên liên quan đã bị hủy để đảm bảo an toàn.
ERR_AUTH_TOKEN_THEFT_DETECTED = Phát hiện hành vi sử dụng lại token làm mới đã bị thu hồi; toàn bộ phiên liên quan đã bị hủy để đảm bảo an toàn.
