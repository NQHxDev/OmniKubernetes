# Thông báo thành công (vi-VN)

# Thao tác CRUD
msg-operation-success = Thao tác hoàn tất thành công.
msg-resource-created = Tạo mới tài nguyên thành công.
msg-resource-updated = Cập nhật tài nguyên thành công.
msg-resource-deleted = Xóa tài nguyên thành công.

# Thao tác có placeholder
msg-entity-created = Tạo mới {$entity} '{$id}' thành công.
msg-entity-updated = Cập nhật {$entity} '{$id}' thành công.
msg-entity-deleted = Xóa {$entity} '{$id}' thành công.
msg-welcome-user = Chào mừng trở lại, {$name}! Lần đăng nhập gần nhất: {$last_login}.
msg-batch-processed = Đã xử lý thành công {$count} mục trong {$duration_ms}ms.

# Nghiệp vụ
msg-user-fetch-success = Lấy thông tin người dùng thành công.
msg-user-register-success = Đăng ký tài khoản người dùng '{$username}' thành công.
msg-user-update-success = Cập nhật hồ sơ người dùng thành công.
msg-user-delete-success = Xóa tài khoản người dùng '{$username}' thành công.

msg-order-list-success = Truy vấn danh sách đơn hàng thành công.
msg-order-fetch-success = Lấy thông tin đơn hàng '{$order_id}' thành công.
msg-order-create-success = Tạo đơn hàng '{$order_id}' thành công.
msg-order-cancel-success = Hủy đơn hàng '{$order_id}' thành công.

msg-payment-process-success = Xử lý thanh toán {$amount} {$currency} thành công.
msg-payment-refund-success = Khởi tạo hoàn tiền {$amount} {$currency} thành công.

msg-audit-event-success = Truy vấn dòng sự kiện kiểm toán thành công.
msg-notification-sent = Gửi thông báo tới '{$recipient}' thành công.

# Hạ tầng OmniKubernetes
msg-cluster-connected = Kết nối thành công tới cụm '{$cluster_name}' (phiên bản Kubernetes v{$version}).
msg-cluster-disconnected = Đã ngắt kết nối khỏi cụm '{$cluster_name}'.
msg-deployment-scaled = Đã thay đổi số lượng replica của deployment '{$deployment}' trong namespace '{$namespace}' từ {$from_replicas} thành {$to_replicas}.
msg-pod-restarted = Khởi động lại pod '{$pod_name}' trong namespace '{$namespace}' thành công.
msg-node-drained = Đã hoàn tất drain node '{$node_name}' (đã di dời {$evicted_pods} pod).
msg-namespace-cleanup = Đã dọn dẹp {$count} tài nguyên trong namespace '{$namespace}'.
msg-configmap-reloaded = Tải lại ConfigMap '{$name}' thành công (phiên bản {$version}).
