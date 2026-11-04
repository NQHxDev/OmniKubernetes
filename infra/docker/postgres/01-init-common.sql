-- infra/docker/postgres/01-init-common.sql
-- Nền tảng chung cho cụm PostgreSQL cục bộ: Khởi tạo các extensions dùng chung và cấu hình baseline.
-- Khi triển khai từng microservice cụ thể, các tệp SQL khởi tạo DB/User độc quyền sẽ được bổ sung tương ứng.

\c omni_default;

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
