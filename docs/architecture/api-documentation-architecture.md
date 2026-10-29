# OmniKubernetes Architecture Specification: Enterprise API Documentation & OpenAPI Aggregation Standard

- **Document Version:** `1.0.0`
- **Status:** Approved / Enterprise Architecture Standard
- **Author:** System Architecture Team
- **Target Platform:** Kubernetes Microservices, Envoy Gateway / API Gateway, Rust (Axum/Actix), Node.js, Spring Boot, Go, OpenAPI 3.0/3.1 & Swagger UI

---

## 1. Executive Summary & Vấn Đề Kiến Trúc (Problem Statement)

Trong kiến trúc phân tán (**Microservices Architecture on Kubernetes**), một trong những thách thức phổ biến nhất nhưng thường bị giải quyết cẩu thả là: **"Quản lý và hiển thị tài liệu API (Swagger/OpenAPI) ở đâu để vừa đảm bảo tính độc lập của từng service, vừa mang lại trải nghiệm tra cứu tập trung cho Client (Frontend, Mobile, QA, Đối tác)?"**

### 1.1. Phân tích 3 hướng tiếp cận kinh điển

| Tiêu chí so sánh                       | Cách 1: Đặt phân tán riêng rẽ tại từng Backend                                                      | Cách 2: Viết/Đặt toàn bộ tập trung tại API Gateway                                                          | Cách 3: Chuẩn Kiến trúc (Decentralized Definition + Centralized Aggregation)                                                              |
| :------------------------------------- | :-------------------------------------------------------------------------------------------------- | :---------------------------------------------------------------------------------------------------------- | :---------------------------------------------------------------------------------------------------------------------------------------- |
| **Cơ chế hoạt động**                   | Mỗi microservice tự host Swagger UI riêng (vd: `service-a:8081/swagger`, `service-b:8082/swagger`). | Team Gateway tự viết OpenAPI specs hoặc gộp thủ công toàn bộ spec vào Gateway repo.                         | **Backend tự sinh Spec JSON/YAML (`/v3/api-docs`); Gateway chỉ đóng vai trò Aggregator và Reverse Proxy hiển thị 1 Swagger UI duy nhất.** |
| **Single Source of Truth**             | Tốt (code ở đâu doc ở đó).                                                                          | **Rất tệ (Anti-pattern)**: Tài liệu tách rời code backend, luôn bị lệch pha (out-of-sync) khi DTO thay đổi. | **Tuyệt đối**: Code và tài liệu cùng một nguồn, tự động đồng bộ qua CI/CD hoặc runtime.                                                   |
| **Developer Experience (Frontend/QA)** | **Cực hình**: Phải nhớ hàng chục URL/Port, tự chuyển đổi tab, không test đồng bộ được.              | Tốt: Chỉ cần vào 1 URL Gateway.                                                                             | **Xuất sắc**: 1 URL duy nhất, có Dropdown chuyển đổi service, hỗ trợ "Try It Out" qua Gateway.                                            |
| **Bảo mật mạng (Security)**            | **Nguy hiểm**: Phải mở NodePort/LoadBalancer vào từng service nội bộ.                               | An toàn: Chỉ mở Gateway.                                                                                    | **An toàn tối đa**: Toàn bộ Backend nằm trong Private Subnet/K8s ClusterIP, chỉ Gateway mở port.                                          |
| **Quyền sở hữu (Ownership)**           | Team backend tự chịu trách nhiệm.                                                                   | Gateway team trở thành điểm nghẽn (bottleneck) của toàn công ty.                                            | Phân quyền chuẩn: Backend team sở hữu spec, DevOps/Gateway team sở hữu hạ tầng tổng hợp.                                                  |

> **KẾT LUẬN KIẾN TRÚC:**  
> **Tuyệt đối không áp dụng Cách 1 và Cách 2.**  
> Chuẩn mực công nghiệp bắt buộc là **Cách 3: Định nghĩa phân tán tại từng Microservice – Tập hợp và hiển thị tập trung tại API Gateway / Developer Portal**.

---

## 2. Các nguyên lý thiết kế cốt lõi (Core Architectural Tenets)

1. **Single Source of Truth (Nguồn chân lý duy nhất):**
    - OpenAPI Specification bắt buộc phải gắn liền với mã nguồn của Service (Code-First bằng annotations/types hoặc Contract-First nằm cùng Git repo của service đó).
    - Tuyệt đối không cho phép Gateway team viết hộ hay sửa đổi API Spec của các Domain Services.

2. **Decoupled Lifecycle & Dynamic Discovery (Vòng đời độc lập):**
    - Khi một Backend Service triển khai phiên bản mới hoặc cập nhật DTO, tài liệu tại Gateway phải tự động phản ánh thay đổi mà **không cần sửa code, không cần build lại, và không cần restart API Gateway**.

3. **Zero Internal Exposure (Không lộ mạng nội bộ):**
    - Các Backend Services chỉ được giao tiếp qua Kubernetes `ClusterIP` hoặc Internal VPC Subnet.
    - Trình duyệt Client không bao giờ được phép và không thể kết nối trực tiếp vào IP nội bộ của Backend để lấy tài liệu. Toàn bộ Spec JSON và API Traffic đều phải đi qua API Gateway.

4. **Strict Path-Prefix Awareness (Tôn trọng tiền tố định tuyến):**
    - Backend service đứng sau Gateway tại đường dẫn prefix (ví dụ `/api/v1/orders`). Khi thực hiện tính năng "Try It Out" trên Swagger UI, request phải trỏ chính xác về `/api/v1/orders/...`, không bao giờ được trỏ nhầm về root `/...`.

5. **Environment Isolation & Security Hardening:**
    - Swagger UI và Spec endpoints phải được kiểm soát nghiêm ngặt theo môi trường:
        - **Local / Dev**: Bật mở tự do.
        - **Staging / UAT**: Bật nhưng bảo vệ bằng VPN / IP Whitelist hoặc Gateway Basic Auth.
        - **Production**: **Mặc định TẮT hoàn toàn**. Đối với External Open API, phải sử dụng Public Developer Portal riêng biệt đã qua lọc bỏ internal endpoints.

---

## 3. Kiến trúc tổng thể & Mô hình luồng dữ liệu (Architecture Topology)

### 3.1. Sơ đồ kiến trúc (System Topology Diagram)

```text
                                  CLIENT BROWSERS
                    (Frontend Devs / Mobile Devs / QA / Partners)
                                         │
                                         ▼ HTTPS (:443)
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                                 KUBERNETES INGRESS / GATEWAY LAYER                     │
│                                                                                        │
│   Public Hostname: api.omnikubernetes.com                                              │
│                                                                                        │
│   ┌────────────────────────────────────────────────────────────────────────────────┐   │
│   │                           INGRESS / ENVOY API GATEWAY                          │   │
│   │                                                                                │   │
│   │  - /swagger-ui/*        ──> Swagger UI Aggregator (Static Assets Engine)       │   │
│   │  - /swagger-config.json ──> Gateway Aggregated Service Catalog Registry        │   │
│   │  - /api/v1/auth/*       ──> Strip Prefix (/api/v1/auth) -> auth-service:8080   │   │
│   │  - /api/v1/orders/*     ──> Strip Prefix (/api/v1/orders) -> order-service:8080 │   │
│   │  - /api/v1/payments/*   ──> Strip Prefix (/api/v1/payments) -> payment-svc:8080│   │
│   └──────────────────────────────────────┬─────────────────────────────────────────┘   │
└──────────────────────────────────────────┼─────────────────────────────────────────────┘
                                           │
         ┌─────────────────────────────────┼─────────────────────────────────┐
         │ KUBERNETES PRIVATE CLUSTER NETWORK (ClusterIP Services Only)      │
         │                                                                   │
         ▼                                 ▼                                 ▼
┌─────────────────────────┐       ┌─────────────────────────┐       ┌─────────────────────────┐
│      auth-service       │       │      order-service      │       │     payment-service     │
│ (Rust / Axum / Actix)   │       │   (Go / Node / Java)    │       │   (Rust / Axum)         │
│                         │       │                         │       │                         │
│ Port: 8080              │       │ Port: 8080              │       │ Port: 8080              │
│ Pod Internal Routes:    │       │ Pod Internal Routes:    │       │ Pod Internal Routes:    │
│ - /v3/api-docs (JSON)   │       │ - /v3/api-docs (JSON)   │       │ - /v3/api-docs (JSON)   │
│ - /login, /register...  │       │ - /orders, /orders/:id  │       │ - /charges, /refunds... │
│                         │       │                         │       │                         │
│ [Swagger UI: DISABLED]  │       │ [Swagger UI: DISABLED]  │       │ [Swagger UI: DISABLED]  │
└─────────────────────────┘       └─────────────────────────┘       └─────────────────────────┘
```

### 3.2. Luồng thực thi chi tiết (Execution Flow)

```text
Browser / Client               API Gateway / Ingress              Backend Microservice
       │                                │                                      │
       │─── 1. GET /swagger-ui/ ───────>│                                      │
       │<── 2. Render UI Shell ─────────│                                      │
       │                                │                                      │
       │─── 3. GET /swagger-config ────>│                                      │
       │<── 4. Return Service List ─────│ (Returns urls: [Auth, Order, ...])   │
       │                                │                                      │
       │─── 5. Select "Orders" ────────>│                                      │
       │    GET /api/v1/orders/         │                                      │
       │        v3/api-docs             │─── 6. Rewrite & Strip Prefix ───────>│
       │                                │       GET /v3/api-docs               │
       │                                │       Header: X-Forwarded-Prefix:    │
       │                                │               /api/v1/orders         │
       │                                │<── 7. Return OpenAPI JSON spec ──────│
       │<── 8. Render Endpoints ────────│                                      │
       │                                │                                      │
       │─── 9. Execute "Try It Out" ────│                                      │
       │    POST /api/v1/orders/create  │                                      │
       │    (Bearer JWT Header)         │─── 10. Rewrite & Forward ───────────>│
       │                                │        POST /create                  │
       │                                │<── 11. Business Response 201 ────────│
       │<── 12. Display JSON result ────│                                      │
```

---

## 4. Chi tiết triển khai phía Backend Microservices

Mỗi Backend Microservice có trách nhiệm tự sinh OpenAPI Specification tại endpoint nội bộ `/v3/api-docs` (hoặc `/openapi.json`).

### 4.1. Quy tắc cấu hình Backend

1. **Chỉ sinh Spec JSON/YAML, tắt Swagger UI HTML:**
    - Việc load HTML, JS, CSS của Swagger UI trên từng pod backend gây lãng phí bộ nhớ RAM (khoảng 20–50MB heap cho UI assets) và mở rộng bề mặt tấn công bảo mật không cần thiết.
2. **Kích hoạt hỗ trợ Forwarded Headers (RFC 7239):**
    - Backend phải đọc được các header `X-Forwarded-Prefix`, `X-Forwarded-Host`, `X-Forwarded-Proto` do Gateway truyền vào để tự động tính toán Server Base URL chính xác.
3. **Thống nhất tiền tố định tuyến với chiến lược Ingress Gateway:**
    - **Khi triển khai Chiến lược A (Khuyên dùng - Strip Prefix):** Backend đăng ký các router độc lập tại root (ví dụ `/v3/api-docs`, `/orders`, `/login`). Backend không cần gắn cứng tiền tố `/api/v1/...` trong code, mà dựa vào header `X-Forwarded-Prefix` do Ingress tiêm vào (hoặc khai báo `servers` tương đối) để Swagger UI tính toán URL chính xác khi gọi qua Gateway.
    - **Khi triển khai Chiến lược B (Preserve Prefix):** Backend bắt buộc phải mount toàn bộ routes (kể cả `/v3/api-docs`) dưới tiền tố định tuyến đầy đủ `/api/v1/<service>/...`.

### 4.2. Code mẫu chuẩn theo từng Tech Stack

#### A. Rust (Axum Framework với `utoipa`)

Trong hệ sinh thái Rust hiệu năng cao của OmniKubernetes:

```rust
use axum::{routing::get, Json, Router};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::handlers::order::create_order,
        crate::handlers::order::get_order_by_id
    ),
    components(
        schemas(
            crate::models::order::CreateOrderRequest,
            crate::models::order::OrderResponse,
            crate::models::order::OrderStatus
        )
    ),
    tags(
        (name = "Orders", description = "Order Processing & Lifecycle Management APIs")
    ),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

pub fn create_docs_router() -> Router {
    // Expose raw JSON OpenAPI spec tại endpoint nội bộ.
    // Khi triển khai Chiến lược A (Strip Prefix):
    // Ingress rewrite /api/v1/orders/v3/api-docs -> /v3/api-docs chuyển tiếp tới handler này.
    Router::new().route("/v3/api-docs", get(|| async {
        Json(ApiDoc::openapi())
    }))
}
```

#### B. Node.js (NestJS Framework)

```typescript
import { NestFactory } from "@nestjs/core";
import { SwaggerModule, DocumentBuilder } from "@nestjs/swagger";
import { AppModule } from "./app.module";

async function bootstrap() {
    const app = await NestFactory.create(AppModule);

    // Cấu hình OpenAPI Spec
    const config = new DocumentBuilder()
        .setTitle("Order Microservice")
        .setDescription("Order Domain API Specifications")
        .setVersion("1.0.0")
        .addServer("/api/v1/orders", "Through OmniKubernetes API Gateway")
        .addBearerAuth()
        .build();

    const document = SwaggerModule.createDocument(app, config);

    // Chỉ expose raw JSON, không gọi SwaggerModule.setup() để mount HTML
    app.getHttpAdapter().get("/v3/api-docs", (req, res) => {
        res.json(document);
    });

    await app.listen(8080);
}
bootstrap();
```

#### C. Java (Spring Boot với `springdoc-openapi`)

Cấu hình trong `application.yml`:

```yaml
server:
    port: 8080
    # Kích hoạt framework tự động nhận diện X-Forwarded-Prefix từ Gateway
    forward-headers-strategy: framework

springdoc:
    api-docs:
        enabled: true
        path: /v3/api-docs
    # Tắt Swagger UI HTML ở backend
    swagger-ui:
        enabled: false
```

---

## 5. Chi tiết triển khai phía API Gateway & Ingress (Centralized Layer)

Gateway đóng vai trò là "Single Pane of Glass" (Cửa sổ duy nhất). Tại đây, ta triển khai một Swagger UI Engine duy nhất đọc cấu hình từ file danh mục `swagger-config.json`.

### 5.1. Định dạng chuẩn của `swagger-config.json`

File cấu hình này cho phép Swagger UI hiển thị Dropdown danh sách toàn bộ các services trong hệ thống:

```json
{
    "urls": [
        {
            "name": "01. Auth & IAM Service",
            "url": "/api/v1/auth/v3/api-docs"
        },
        {
            "name": "02. Order Processing Service",
            "url": "/api/v1/orders/v3/api-docs"
        },
        {
            "name": "03. Payment & Ledger Service",
            "url": "/api/v1/payments/v3/api-docs"
        },
        {
            "name": "04. Product & Catalog Service",
            "url": "/api/v1/products/v3/api-docs"
        },
        {
            "name": "05. Notification Service",
            "url": "/api/v1/notifications/v3/api-docs"
        }
    ],
    "layout": "StandaloneLayout",
    "deepLinking": true,
    "persistAuthorization": true,
    "displayRequestDuration": true,
    "filter": true
}
```

> **Giải thích các tham số tối ưu UX:**
>
> - `persistAuthorization: true`: Giúp lập trình viên khi dán Bearer Token (JWT) một lần sẽ **không bị mất token** khi chuyển đổi qua lại giữa các service trên Dropdown.
> - `filter: true`: Bật ô tìm kiếm nhanh endpoint ngay trên đầu trang.
> - `displayRequestDuration: true`: Hiển thị chính xác thời gian phản hồi (latency tính bằng ms) khi bấm Execute.

---

### 5.2. Triển khai trên Kubernetes với Ingress & Swagger UI Deployment

Đây là phương án chuẩn Cloud-Native nhất: Chạy một Deployment Swagger UI siêu nhẹ (~15MB RAM) trong cụm K8s, tích hợp trực tiếp qua Ingress.

#### 1. Swagger UI Deployment & ConfigMap (`infra/kubernetes/deployments/swagger-ui.yaml`)

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
    name: swagger-ui-config
    namespace: omni-system
data:
    swagger-config.json: |
        {
          "urls": [
            {"name": "Auth Service", "url": "/api/v1/auth/v3/api-docs"},
            {"name": "Order Service", "url": "/api/v1/orders/v3/api-docs"},
            {"name": "Payment Service", "url": "/api/v1/payments/v3/api-docs"},
            {"name": "Product Service", "url": "/api/v1/products/v3/api-docs"}
          ],
          "deepLinking": true,
          "persistAuthorization": true,
          "displayRequestDuration": true
        }
---
apiVersion: apps/v1
kind: Deployment
metadata:
    name: swagger-ui-aggregator
    namespace: omni-system
spec:
    replicas: 1
    selector:
        matchLabels:
            app: swagger-ui-aggregator
    template:
        metadata:
            labels:
                app: swagger-ui-aggregator
        spec:
            containers:
                - name: swagger-ui
                  image: swaggerapi/swagger-ui:v5.17.14
                  ports:
                      - containerPort: 8080
                  env:
                      - name: CONFIG_URL
                        value: "/swagger-ui/config/swagger-config.json"
                  volumeMounts:
                      - name: config-volume
                        mountPath: /usr/share/nginx/html/swagger-ui/config
            volumes:
                - name: config-volume
                  configMap:
                      name: swagger-ui-config
---
apiVersion: v1
kind: Service
metadata:
    name: swagger-ui-service
    namespace: omni-system
spec:
    selector:
        app: swagger-ui-aggregator
    ports:
        - port: 8080
          targetPort: 8080
```

#### 2. Cấu hình NGINX Ingress Routing & Chiến lược Path Routing

##### A. Phân tích các lỗi kỹ thuật & Lệch Path Routing (Critical Issues & Root Cause)

Khi triển khai API Documentation và Ingress Routing trên môi trường Kubernetes thực tế, có hai lỗi kỹ thuật kinh điển thường xuyên gây ra mã lỗi `404 Not Found`:

1. **Lỗi biến `$uri` trong NGINX Ingress (`proxy_set_header X-Forwarded-Prefix $uri;`):**
    - Biến `$uri` trong NGINX phản ánh **toàn bộ request path hiện tại** đã qua chuẩn hóa (ví dụ: `/api/v1/orders/v3/api-docs` hoặc `/api/v1/orders/123/items`), hoàn toàn **không phải** là routing prefix của microservice (`/api/v1/orders`).
    - Nếu gửi header `X-Forwarded-Prefix: $uri`, Backend sẽ nhận prefix sai lệch theo từng request gọi đến. Khi Backend hoặc Swagger UI dựa vào header này để dựng Base URL, đường dẫn sẽ bị nhân đôi hoặc méo mó (ví dụ: `/api/v1/orders/123/items/orders/123/items`), gây lỗi `404 Not Found` ngay lập tức.
    - **Nguyên tắc chuẩn:** Header `X-Forwarded-Prefix` bắt buộc phải là tiền tố route gốc cố định của microservice đó (ví dụ: `/api/v1/orders`, `/api/v1/auth`).

2. **Lỗi thiếu Rewrite Path (`nginx.ingress.kubernetes.io/rewrite-target: /$2`):**
    - Cấu hình Ingress sử dụng Regex matching: `- path: /api/v1/auth(/|$)(.*)`.
    - Nếu thiếu annotation `nginx.ingress.kubernetes.io/rewrite-target: /$2`, NGINX Ingress sẽ chuyển tiếp **nguyên văn** URI `/api/v1/auth/v3/api-docs` vào Backend pod `auth-service`.
    - Trong khi đó, Backend (Axum, NestJS, Spring Boot tại Mục 4.2) chỉ đăng ký router nội bộ tại `/v3/api-docs` (hoặc `/login`, `/orders`...).
    - **Hậu quả:** Backend pod trả về `404 Not Found`. API Gateway không thể kéo được OpenAPI JSON spec của Backend, khiến Swagger UI báo lỗi "Failed to load API definition" ngay khi mở trang.

3. **Phạm vi tác dụng của Annotation trong Ingress Controller (Ingress Resource-Level Scope):**
    - Trong `ingress-nginx`, các annotation như `nginx.ingress.kubernetes.io/rewrite-target` và `nginx.ingress.kubernetes.io/x-forwarded-prefix` có phạm vi tác dụng trên **toàn bộ tài nguyên Ingress**, không thể cấu hình riêng rẽ cho từng item trong mảng `paths` nếu các path có tiền tố khác nhau.
    - Gom chung nhiều microservices với các routing prefix khác nhau (`/api/v1/auth`, `/api/v1/orders`) vào một Ingress resource duy nhất sẽ gây xung đột giá trị header `X-Forwarded-Prefix`.
    - **Giải pháp kiến trúc chuẩn:** Áp dụng mô hình **Decoupled Ingress per Microservice Pattern** (mỗi service sở hữu một Ingress manifest riêng biệt) nhằm đảm bảo tính tự trị (Microservice Autonomy) và cấu hình chính xác từng route.

---

##### B. Thống nhất 2 chiến lược kiến trúc (Architecture Strategies)

| Tiêu chí | Chiến lược A: Gateway Rewrite (Strip Prefix) - Khuyên dùng | Chiến lược B: Preserve Prefix (Backend-Aware Routing) |
| :--- | :--- | :--- |
| **Cơ chế hoạt động** | Ingress gọt bỏ tiền tố `/api/v1/<service>` trước khi chuyển tiếp vào Backend pod (`/api/v1/orders/v3/api-docs` -> `/v3/api-docs`). Gateway chủ động tiêm header `X-Forwarded-Prefix: /api/v1/<service>`. | Ingress giữ nguyên toàn bộ request path (`/api/v1/orders/v3/api-docs` giữ nguyên khi vào pod). Không dùng rewrite target. |
| **Trách nhiệm Backend** | Backend code độc lập hoàn toàn với routing prefix của Gateway. Chỉ cần lắng nghe tại `/v3/api-docs` và các root endpoints (`/`, `/123`). | Backend bắt buộc phải tự mount toàn bộ router và OpenAPI doc endpoint dưới tiền tố đầy đủ `/api/v1/<service>/...`. |
| **Tính độc lập (Decoupling)** | **Rất cao (Loose Coupling):** Có thể chạy test service độc lập ở Local, Docker Compose hoặc K8s mà không cần đổi base path trong code. Thay đổi Gateway routing không ảnh hưởng code backend. | **Thấp (Tight Coupling):** Backend gắn chặt với cấu trúc URL bên ngoài của Gateway. Nếu đổi prefix routing, phải sửa code và redeploy backend. |
| **Cấu hình Ingress K8s** | Cần annotation `rewrite-target: /$2` và `x-forwarded-prefix`. Khuyến nghị tách Ingress theo từng microservice để tránh xung đột header. | Ingress cấu hình đơn giản (`pathType: Prefix`), có thể gộp chung trong 1 Ingress manifest duy nhất. |
| **Khuyến nghị áp dụng** | **Chuẩn khuyến nghị (Recommended Standard)** cho toàn bộ hệ thống OmniKubernetes. | Phương án thay thế (Alternative) nếu framework backend hỗ trợ cấu hình context-path tự động (ví dụ `server.servlet.context-path` trong Spring Boot). |

---

##### C. Cấu hình chuẩn theo Chiến lược A (Khuyên dùng - Decoupled Ingress Manifests)

Theo chuẩn kiến trúc của OmniKubernetes, mỗi service được cấu hình Ingress riêng biệt để đảm bảo tính tự trị, kiểm soát chính xác quy tắc rewrite và header `X-Forwarded-Prefix`.

###### 1. Ingress cho Swagger UI Shell (`infra/kubernetes/ingress/swagger-ui-ingress.yaml`)

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: omni-swagger-ui-ingress
  namespace: omni-system
  annotations:
    kubernetes.io/ingress.class: "nginx"
    nginx.ingress.kubernetes.io/proxy-connect-timeout: "15"
    nginx.ingress.kubernetes.io/proxy-read-timeout: "60"
spec:
  rules:
    - host: api.omnikubernetes.com
      http:
        paths:
          # Đẩy trực tiếp vào Swagger UI Aggregator Pod (không rewrite)
          - path: /swagger-ui
            pathType: Prefix
            backend:
              service:
                name: swagger-ui-service
                port:
                  number: 8080
```

###### 2. Ingress cho Auth Service (`infra/kubernetes/ingress/auth-ingress.yaml`)

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: omni-auth-ingress
  namespace: omni-system
  annotations:
    kubernetes.io/ingress.class: "nginx"
    nginx.ingress.kubernetes.io/use-regex: "true"
    # Rewrite: Bỏ /api/v1/auth, chỉ đẩy phần path phía sau ($2) vào backend
    nginx.ingress.kubernetes.io/rewrite-target: /$2
    # Gán cố định X-Forwarded-Prefix chuẩn cho Auth Service
    nginx.ingress.kubernetes.io/x-forwarded-prefix: "/api/v1/auth"
    nginx.ingress.kubernetes.io/proxy-connect-timeout: "15"
    nginx.ingress.kubernetes.io/proxy-read-timeout: "60"
spec:
  rules:
    - host: api.omnikubernetes.com
      http:
        paths:
          # Match: /api/v1/auth hoặc /api/v1/auth/...
          # Nhóm 1: (/|$), Nhóm 2: (.*)
          - path: /api/v1/auth(/|$)(.*)
            pathType: ImplementationSpecific
            backend:
              service:
                name: auth-service
                port:
                  number: 8080
```

###### 3. Ingress cho Order Service (`infra/kubernetes/ingress/order-ingress.yaml`)

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: omni-order-ingress
  namespace: omni-system
  annotations:
    kubernetes.io/ingress.class: "nginx"
    nginx.ingress.kubernetes.io/use-regex: "true"
    # Rewrite: Bỏ /api/v1/orders, chỉ đẩy phần path phía sau ($2) vào backend
    nginx.ingress.kubernetes.io/rewrite-target: /$2
    # Gán cố định X-Forwarded-Prefix chuẩn cho Order Service
    nginx.ingress.kubernetes.io/x-forwarded-prefix: "/api/v1/orders"
    nginx.ingress.kubernetes.io/proxy-connect-timeout: "15"
    nginx.ingress.kubernetes.io/proxy-read-timeout: "60"
spec:
  rules:
    - host: api.omnikubernetes.com
      http:
        paths:
          # Match: /api/v1/orders hoặc /api/v1/orders/...
          # Nhóm 1: (/|$), Nhóm 2: (.*)
          - path: /api/v1/orders(/|$)(.*)
            pathType: ImplementationSpecific
            backend:
              service:
                name: order-service
                port:
                  number: 8080
```

> **Giải thích cơ chế hoạt động của Regex Capture Groups:**
>
> - Định tuyến regex `/api/v1/orders(/|$)(.*)` chia request path thành 2 capture groups:
>   - Nhóm 1 `(/|$)`: Khớp dấu gạch chéo `/` hoặc ký tự kết thúc chuỗi `$`.
>   - Nhóm 2 `(.*)`: Bắt toàn bộ chuỗi đường dẫn còn lại phía sau prefix.
> - Khi client gọi `GET /api/v1/orders/v3/api-docs`:
>   - `$1 = /`, `$2 = v3/api-docs`.
>   - `rewrite-target: /$2` chuyển đổi request thành `GET /v3/api-docs` gửi vào Pod backend.
>   - Header `X-Forwarded-Prefix: /api/v1/orders` được gửi kèm theo request.
>   - Backend xử lý route `/v3/api-docs`, trả về mã `200 OK` chứa OpenAPI Specification.
> - Khi client gọi `POST /api/v1/orders/create`:
>   - Backend pod nhận `POST /create` kèm header `X-Forwarded-Prefix: /api/v1/orders`.

---

##### D. Cấu hình mẫu theo Chiến lược B (Alternative - Preserve Prefix trong Single Ingress)

Nếu dự án chọn phương án giữ nguyên tiền tố và backend đã chủ động mount toàn bộ router dưới prefix `/api/v1/<service>` (ví dụ trong Spring Boot đặt `server.servlet.context-path=/api/v1/orders`), cấu hình Ingress có thể gộp chung trong 1 file duy nhất mà không cần rewrite:

```yaml
# infra/kubernetes/ingress/single-ingress-preserve-prefix.yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: omni-gateway-ingress-preserve
  namespace: omni-system
  annotations:
    kubernetes.io/ingress.class: "nginx"
    nginx.ingress.kubernetes.io/proxy-connect-timeout: "15"
    nginx.ingress.kubernetes.io/proxy-read-timeout: "60"
spec:
  rules:
    - host: api.omnikubernetes.com
      http:
        paths:
          - path: /swagger-ui
            pathType: Prefix
            backend:
              service:
                name: swagger-ui-service
                port:
                  number: 8080
          # Giữ nguyên path, Backend tự lắng nghe /api/v1/auth/... và /api/v1/auth/v3/api-docs
          - path: /api/v1/auth
            pathType: Prefix
            backend:
              service:
                name: auth-service
                port:
                  number: 8080
          # Giữ nguyên path, Backend tự lắng nghe /api/v1/orders/... và /api/v1/orders/v3/api-docs
          - path: /api/v1/orders
            pathType: Prefix
            backend:
              service:
                name: order-service
                port:
                  number: 8080
```

---

## 6. Xử lý triệt để 5 "cạm bẫy" lớn nhất trong thực tế (Pitfalls & Solutions)

### Bẫy 1: Bấm "Execute" (Try It Out) bị lỗi 404 Not Found

- **Nguyên nhân gốc rễ:**
    1. Swagger UI gửi request đến gốc `/orders/123` thay vì qua tiền tố Gateway `/api/v1/orders/orders/123` (hoặc `/api/v1/orders/123`).
    2. Ingress cấu hình sai header `X-Forwarded-Prefix $uri` khiến prefix bị biến động theo từng endpoint thay vì giữ cố định `/api/v1/orders`.
    3. Ingress thiếu quy tắc `rewrite-target: /$2`, khiến Gateway chuyển tiếp nguyên xi URI có prefix vào backend chỉ lắng nghe root path.
- **Cách khắc phục chuẩn:**
    1. **Tại Ingress Gateway:** Luôn cấu hình gán cứng header `nginx.ingress.kubernetes.io/x-forwarded-prefix: /api/v1/<service>` đi kèm annotation `rewrite-target: /$2` theo đúng **Chiến lược A**.
    2. **Tại OpenAPI Spec Backend:** Trong trường hợp framework Backend không tự động đọc header forwarded, cấu hình tường minh trường `servers` trong OpenAPI Specification của service:
        ```yaml
        servers:
            - url: /api/v1/orders
              description: Relative path via OmniKubernetes API Gateway
        ```
        _Lưu ý: Luôn dùng đường dẫn tương đối `/api/v1/orders` thay vì gắn cứng cả domain `https://api.omnikubernetes.com` để spec tương thích linh hoạt trên cả Local, Dev, Staging và Production._

### Bẫy 2: Lỗi CORS khi tải OpenAPI Spec

- **Hiện tượng:** Mở Swagger UI thì trống rỗng, Console trình duyệt báo đỏ: `Access to fetch at ... from origin ... has been blocked by CORS policy`.
- **Nguyên nhân:** Swagger UI host ở domain A nhưng cấu hình URL spec lại trỏ sang domain B hoặc port khác.
- **Cách khắc phục chuẩn:** **Không bao giờ dùng Absolute URL khác domain.** Luôn cấu hình URL trong `swagger-config.json` dưới dạng Relative Path trên cùng domain của Gateway:
    - Đúng: `"url": "/api/v1/orders/v3/api-docs"`
    - Sai: `"url": "http://order-internal-service:8080/v3/api-docs"`

### Bẫy 3: Đăng nhập một lần dùng cho tất cả (JWT Bearer Persistence)

- **Vấn đề:** Mỗi lần đổi service trên dropdown, dev phải copy/paste lại Authorization Token rất khó chịu.
- **Cách khắc phục:**
    - Kích hoạt `persistAuthorization: true` trong cấu hình Swagger UI.
    - Đồng nhất tên Security Scheme trên toàn bộ các microservices: Tất cả đều đặt tên scheme là `BearerAuth` (chuẩn OpenAPI Security Requirement):
        ```yaml
        components:
            securitySchemes:
                BearerAuth:
                    type: http
                    scheme: bearer
                    bearerFormat: JWT
        ```

### Bẫy 4: Thất thoát dữ liệu nhạy cảm & Lộ API nội bộ (Security Leakage)

- **Vấn đề:** Các endpoint nội bộ như `/admin/metrics`, `/internal/reindex-db` bị phơi bày ra tài liệu.
- **Cách khắc phục:**
    - Áp dụng OpenAPI Grouping / Tagging. Chỉ export các endpoint công khai ra Group `public-v1`.
    - Giấu hoàn toàn các route nội bộ khỏi Swagger Generator bằng annotation (ví dụ trong Rust utoipa: không gắn `#[utoipa::path]` vào private handlers).

### Bẫy 5: Bảo mật môi trường Production (Production Hardening)

- **Nguyên tắc:** **Môi trường Production tuyệt đối không mở Swagger UI công khai.**
- **Giải pháp:**
    1. Trong Ingress Production: Không tạo ingress path `/swagger-ui`.
    2. Nếu bắt buộc cần cho đối tác hoặc nội bộ truy cập:
        - Kích hoạt `Basic Authentication` hoặc `OAuth2 Proxy` chặn trước route `/swagger-ui` ngay tại Ingress.
        - Giới hạn IP Whitelist (chỉ cho phép IP VPN của công ty).

---

## 7. Tiêu chuẩn nâng cấp trong tương lai: Developer Portal (Enterprise Scale)

Khi hệ thống OmniKubernetes vượt ngưỡng **20+ Microservices**, mô hình Swagger UI Aggregator trên Gateway có thể được nâng cấp lên mô hình **Dedicated Developer Portal**:

```text
               ┌────────────────────────────────────────────────────────┐
               │         CI / CD PIPELINE (GitHub Actions / GitLab)     │
               └───────────────────────────┬────────────────────────────┘
                                           │
         ┌─────────────────────────────────┼────────────────────────────────┐
         │ Build Artifact                  │ Build Artifact                 │
         ▼                                 ▼                                ▼
┌─────────────────┐               ┌─────────────────┐              ┌─────────────────┐
│  auth-service   │               │  order-service  │              │ payment-service │
│                 │               │                 │              │                 │
│ Generates:      │               │ Generates:      │              │ Generates:      │
│ openapi.json    │               │ openapi.json    │              │ openapi.json    │
└────────┬────────┘               └────────┬────────┘              └────────┬────────┘
         │                                 │                                │
         └─────────────────────────────────┼────────────────────────────────┘
                                           │ Push Specs at deploy time
                                           ▼
                    ┌──────────────────────────────────────────────┐
                    │     INTERNAL DEVELOPER PORTAL (BACKSTAGE)    │
                    │      or Centralized API Catalog (S3/R2)      │
                    │                                              │
                    │ - Search across all services                 │
                    │ - Mock Server capabilities                   │
                    │ - Breaking Change Detection (oasdiff)        │
                    │ - Contract Testing (Pact / Prism)            │
                    └──────────────────────────────────────────────┘
```

---

## 8. Bảng kiểm tra chất lượng kiến trúc (Architecture Checklist)

Trước khi bàn giao hoặc đưa một service mới lên môi trường tích hợp, kỹ sư phụ trách phải kiểm tra đủ 6 mục sau:

- [ ] **Tài liệu sinh tự động từ code (Zero Manual Specs):** Không có file spec nào được viết thủ công độc lập với code backend.
- [ ] **Giao diện HTML tắt tại Backend:** Backend pod chỉ trả về `application/json` khi gọi `/v3/api-docs`, không tốn tài nguyên render HTML.
- [ ] **Đồng bộ Ingress Rewrite & Header X-Forwarded-Prefix:** Ingress có cấu hình `rewrite-target: /$2` chính xác và tiêm đúng header `X-Forwarded-Prefix` cố định khớp với prefix của service. Kiểm tra endpoint spec qua Gateway (`GET /api/v1/<service>/v3/api-docs`) trả về mã `200 OK`.
- [ ] **Hiển thị đầy đủ trong Gateway Dropdown:** Service đã được đăng ký vào `swagger-config.json` và tải mượt mà trên Gateway UI.
- [ ] **Tính năng "Try It Out" chạy thông suốt:** Gọi test thử 1 API GET và 1 API POST thành công với mã `200/201`, không bị `404 Not Found` do sai prefix.
- [ ] **Duy trì Token xác thực:** Nút `Authorize` màu xanh giữ nguyên trạng thái token khi chuyển đổi qua lại giữa các service trên Dropdown.
- [ ] **Môi trường Production được khóa chặt:** Route `/swagger-ui` trả về `403 Forbidden` hoặc `404 Not Found` khi truy cập từ mạng ngoài trên cụm Production.
