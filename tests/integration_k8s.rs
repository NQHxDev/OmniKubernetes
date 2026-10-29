//! Kubernetes Integration Tests
//!
//! These tests verify that the cluster is healthy and Kubernetes manifests
//! can be validated against the API server. They are marked with `#[ignore]`
//! by default and executed in CI with KIND (Kubernetes in Docker).

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// Helper to execute `kubectl` with the given arguments and assert success.
fn run_kubectl(args: &[&str]) -> String {
    let output = Command::new("kubectl")
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("Failed to execute kubectl with args {args:?}: {e}"));

    assert_kubectl_success(&output, &format!("kubectl {}", args.join(" ")));
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Helper to validate a YAML manifest against the Kubernetes API server (`--dry-run=server`).
fn validate_manifest_server_dry_run(manifest_yaml: &str, resource_desc: &str) {
    let mut child = Command::new("kubectl")
        .args(["apply", "-f", "-", "--dry-run=server"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("Failed to spawn kubectl for {resource_desc}: {e}"));

    child
        .stdin
        .take()
        .expect("Failed to open kubectl stdin")
        .write_all(manifest_yaml.as_bytes())
        .unwrap_or_else(|e| panic!("Failed to write {resource_desc} YAML to stdin: {e}"));

    let output = child
        .wait_with_output()
        .unwrap_or_else(|e| panic!("Failed to wait for kubectl ({resource_desc}): {e}"));

    assert_kubectl_success(
        &output,
        &format!("{resource_desc} server dry-run validation"),
    );
}

/// Assert that a `kubectl` command output succeeded, printing `stderr` on failure.
fn assert_kubectl_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed (exit code {:?}):\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
}

/// Test basic Kubernetes cluster connectivity.
#[test]
#[ignore = "Requires a running Kubernetes cluster (executed in CI with KIND)"]
fn test_k8s_cluster_connectivity() {
    let stdout = run_kubectl(&["cluster-info"]);
    assert!(
        stdout.contains("Kubernetes control plane"),
        "Unexpected cluster-info output:\n{stdout}"
    );
}

/// Test that cluster nodes are in the `Ready` state.
#[test]
#[ignore = "Requires a running Kubernetes cluster (executed in CI with KIND)"]
fn test_k8s_nodes_ready() {
    let status = run_kubectl(&[
        "get",
        "nodes",
        "-o",
        "jsonpath={.items[*].status.conditions[?(@.type=='Ready')].status}",
    ]);

    assert!(
        status.split_whitespace().any(|s| s == "True"),
        "No nodes are in Ready state (got: {status:?})"
    );
}

/// Test that core system pods in `kube-system` are running.
#[test]
#[ignore = "Requires a running Kubernetes cluster (executed in CI with KIND)"]
fn test_k8s_system_pods_healthy() {
    let phases = run_kubectl(&[
        "get",
        "pods",
        "-n",
        "kube-system",
        "-o",
        "jsonpath={.items[*].status.phase}",
    ]);

    assert!(
        !phases.trim().is_empty(),
        "No pods found in kube-system namespace"
    );

    for phase in phases.split_whitespace() {
        assert!(
            phase == "Running" || phase == "Succeeded",
            "Unhealthy pod phase detected in kube-system: {phase} (all phases: {phases})"
        );
    }
}

/// Test service discovery mechanism in the default namespace.
#[test]
#[ignore = "Requires a running Kubernetes cluster (executed in CI with KIND)"]
fn test_k8s_service_discovery() {
    let services = run_kubectl(&["get", "services", "--all-namespaces"]);
    assert!(
        services.contains("kubernetes"),
        "Default 'kubernetes' service not found in cluster:\n{services}"
    );
}

/// Test that namespaces can be validated against the API server.
#[test]
#[ignore = "Requires a running Kubernetes cluster (executed in CI with KIND)"]
fn test_k8s_namespace_operations() {
    let test_namespace = "omni-test";
    let stdout = run_kubectl(&[
        "create",
        "namespace",
        test_namespace,
        "--dry-run=server",
        "-o",
        "yaml",
    ]);

    assert!(
        stdout.contains("name: omni-test"),
        "Namespace dry-run output missing expected name:\n{stdout}"
    );
}

/// Test Pod deployment manifest validation against the API server.
#[test]
#[ignore = "Requires a running Kubernetes cluster (executed in CI with KIND)"]
fn test_k8s_pod_deployment() {
    let pod_yaml = r"
apiVersion: v1
kind: Pod
metadata:
  name: test-pod
  labels:
    app: omni-test
spec:
  containers:
  - name: nginx
    image: nginx:alpine
    ports:
    - containerPort: 80
";

    validate_manifest_server_dry_run(pod_yaml, "Pod manifest");
}

/// Test `ConfigMap` manifest validation against the API server.
#[test]
#[ignore = "Requires a running Kubernetes cluster (executed in CI with KIND)"]
fn test_k8s_configmap_operations() {
    let configmap_yaml = r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: test-config
data:
  app.env: "test"
  log.level: "info"
"#;

    validate_manifest_server_dry_run(configmap_yaml, "ConfigMap manifest");
}

/// Test `Secret` manifest validation against the API server.
#[test]
#[ignore = "Requires a running Kubernetes cluster (executed in CI with KIND)"]
fn test_k8s_secret_operations() {
    let secret_yaml = r"
apiVersion: v1
kind: Secret
metadata:
  name: test-secret
type: Opaque
stringData:
  username: testuser
  password: testpass
";

    validate_manifest_server_dry_run(secret_yaml, "Secret manifest");
}

/// Test `ResourceQuota` manifest validation against the API server.
#[test]
#[ignore = "Requires a running Kubernetes cluster (executed in CI with KIND)"]
fn test_k8s_resource_quotas() {
    let quota_yaml = r#"
apiVersion: v1
kind: ResourceQuota
metadata:
  name: test-quota
spec:
  hard:
    requests.cpu: "1"
    requests.memory: 1Gi
    limits.cpu: "2"
    limits.memory: 2Gi
"#;

    validate_manifest_server_dry_run(quota_yaml, "ResourceQuota manifest");
}

/// Validate all Kubernetes manifests under `infra/kubernetes` if any exist.
#[test]
#[ignore = "Requires a running Kubernetes cluster (executed in CI with KIND)"]
fn test_infra_kubernetes_manifests() {
    let k8s_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("infra/kubernetes");
    if !k8s_dir.exists() {
        return;
    }

    let mut manifest_files = Vec::new();
    collect_yaml_files(&k8s_dir, &mut manifest_files);

    for manifest in manifest_files {
        let path_str = manifest.to_str().expect("Invalid UTF-8 path");
        run_kubectl(&["apply", "--dry-run=server", "-f", path_str]);
    }
}

fn collect_yaml_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_yaml_files(&path, out);
        } else if matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("yaml" | "yml")
        ) {
            out.push(path);
        }
    }
}
