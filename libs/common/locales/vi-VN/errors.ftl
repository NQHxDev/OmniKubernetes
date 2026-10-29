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
