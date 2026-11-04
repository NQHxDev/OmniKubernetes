//! In-memory `TestClient` for testing Axum applications without binding physical TCP ports.

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode, header},
};
use tower::ServiceExt;

/// Ergonomic wrapper over an Axum HTTP response.
#[derive(Debug, Clone)]
pub struct TestResponse {
    /// HTTP status code.
    pub status: StatusCode,
    /// Response headers.
    pub headers: HeaderMap,
    /// Raw response body bytes.
    pub body_bytes: Vec<u8>,
}

impl TestResponse {
    /// Returns the HTTP status code.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns the response body parsed as a UTF-8 string.
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body_bytes).to_string()
    }

    /// Deserializes the response body as JSON.
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body_bytes)
    }

    /// Returns a header value by key.
    #[must_use]
    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers.get(key).and_then(|v| v.to_str().ok())
    }
}

/// In-memory testing client executing HTTP requests against an Axum [`Router`].
pub struct TestClient {
    router: Router,
    default_headers: Vec<(HeaderName, HeaderValue)>,
}

impl std::fmt::Debug for TestClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestClient")
            .field("default_headers", &self.default_headers)
            .finish_non_exhaustive()
    }
}

impl TestClient {
    /// Creates a new `TestClient` targeting the given Axum router.
    #[must_use]
    pub fn new(router: Router) -> Self {
        Self {
            router,
            default_headers: Vec::new(),
        }
    }

    /// Attaches a default Bearer JWT token header.
    #[must_use]
    pub fn with_bearer_token(mut self, token: &str) -> Self {
        let val = HeaderValue::from_str(&format!("Bearer {token}"))
            .expect("Invalid bearer token characters");
        self.default_headers.push((header::AUTHORIZATION, val));
        self
    }

    /// Attaches a default header to all requests.
    #[must_use]
    pub fn with_header(mut self, key: HeaderName, value: HeaderValue) -> Self {
        self.default_headers.push((key, value));
        self
    }

    /// Executes an arbitrary HTTP request.
    pub async fn request(&self, method: Method, uri: &str, body: Body) -> TestResponse {
        let mut builder = Request::builder().method(method).uri(uri);
        for (k, v) in &self.default_headers {
            builder = builder.header(k.clone(), v.clone());
        }
        let req = builder.body(body).expect("Failed to build HTTP request");

        let response = self
            .router
            .clone()
            .oneshot(req)
            .await
            .expect("Failed to execute in-memory request");

        let status = response.status();
        let headers = response.headers().clone();
        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("Failed to read in-memory body")
            .to_vec();

        TestResponse {
            status,
            headers,
            body_bytes,
        }
    }

    /// Executes a GET request.
    pub async fn get(&self, uri: &str) -> TestResponse {
        self.request(Method::GET, uri, Body::empty()).await
    }

    /// Executes a POST request with a JSON payload.
    pub async fn post_json<T: serde::Serialize>(&self, uri: &str, payload: &T) -> TestResponse {
        let json_bytes = serde_json::to_vec(payload).expect("Failed to serialize json payload");
        let mut builder = Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json");

        for (k, v) in &self.default_headers {
            builder = builder.header(k.clone(), v.clone());
        }

        let req = builder
            .body(Body::from(json_bytes))
            .expect("Failed to build POST request");

        let response = self
            .router
            .clone()
            .oneshot(req)
            .await
            .expect("Failed to execute in-memory request");

        let status = response.status();
        let headers = response.headers().clone();
        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("Failed to read in-memory body")
            .to_vec();

        TestResponse {
            status,
            headers,
            body_bytes,
        }
    }

    /// Executes a DELETE request.
    pub async fn delete(&self, uri: &str) -> TestResponse {
        self.request(Method::DELETE, uri, Body::empty()).await
    }
}
