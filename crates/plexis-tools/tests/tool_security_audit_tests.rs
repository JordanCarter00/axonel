use std::fs;
use std::time::Duration;
use tempfile::TempDir;

use plexis_core::ids::{AgentId, ExecutionId, TaskId};
use plexis_tools::builtin::FilesystemTool;
use plexis_tools::redaction::SecretRedactor;
use plexis_tools::{
    CommandSpec, ExecutionBackend, HostProcessBackend, Sandbox, Tool, ToolError,
    ToolInvocationContext,
};

#[test]
fn test_symlink_escape_rejected_by_sandbox() {
    let outside_dir = TempDir::new().unwrap();
    let sandbox_dir = TempDir::new().unwrap();

    // Create a sensitive file outside sandbox
    let secret_file = outside_dir.path().join("secret.conf");
    fs::write(&secret_file, "TOP_SECRET_PASSWORD=12345").unwrap();

    // Create a symlink inside sandbox pointing outside
    let link_path = sandbox_dir.path().join("escaped_link");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&secret_file, &link_path).unwrap();

    let sandbox = Sandbox::new(sandbox_dir.path());

    // Attempting to resolve the symlink path must be rejected!
    let res = sandbox.resolve_safe_path("escaped_link", false);
    assert!(
        res.is_err(),
        "Symlink pointing outside sandbox must be rejected"
    );

    match res.unwrap_err() {
        ToolError::PermissionDenied(msg) => {
            assert!(
                msg.contains("Symlink traversal escape detected") || msg.contains("escapes"),
                "Expected symlink escape rejection message, got: {}",
                msg
            );
        }
        other => panic!("Expected PermissionDenied, got {:?}", other),
    }

    // Now test FilesystemTool execution with the symlink
    let fs_tool = FilesystemTool;
    let ctx = ToolInvocationContext::new(
        AgentId::new(),
        ExecutionId::new(),
        TaskId::new(),
        serde_json::json!({
            "action": "read_file",
            "path": "escaped_link"
        }),
        std::sync::Arc::new(sandbox),
        sandbox_dir.path().to_path_buf(),
    );

    let rt = tokio::runtime::Runtime::new().unwrap();
    let exec_res = rt.block_on(fs_tool.execute(&ctx));
    assert!(
        exec_res.is_err(),
        "FilesystemTool must reject symlink escape"
    );
}

#[tokio::test]
async fn test_process_backend_kill_on_drop_and_timeout() {
    let temp = TempDir::new().unwrap();
    let backend = HostProcessBackend::new();

    // Run a command with 50ms timeout that sleeps for 5 seconds
    let spec = CommandSpec::new("sleep 5", temp.path()).with_timeout(Duration::from_millis(50));

    let res = backend.execute(spec).await;
    assert!(res.is_err());
    match res.unwrap_err() {
        ToolError::Timeout(d) => assert_eq!(d, Duration::from_millis(50)),
        other => panic!("Expected ToolError::Timeout, got {:?}", other),
    }
}

#[test]
fn test_automated_secret_redaction_patterns() {
    let raw = "Deployment log:\n\
               Authorization: Bearer sk-ant-api03-secret1234567890abcdef\n\
               OpenAI API Key: sk-1234567890abcdef1234567890\n\
               GitHub: ghp_abcdefghijklmnopqrstuvwxyz012345\n\
               AWS: AKIAIOSFODNN7EXAMPLE\n\
               Plain text: All systems operational.";

    let redacted = SecretRedactor::redact_patterns(raw);

    assert!(redacted.contains("Authorization: Bearer [REDACTED]"));
    assert!(redacted.contains("OpenAI API Key: [REDACTED_API_KEY]"));
    assert!(redacted.contains("GitHub: [REDACTED_GH_TOKEN]"));
    assert!(redacted.contains("AWS: [REDACTED_AWS_KEY]"));
    assert!(!redacted.contains("sk-ant-api03"));
    assert!(!redacted.contains("ghp_"));
    assert!(!redacted.contains("AKIAIOSFODNN7EXAMPLE"));
    assert!(redacted.contains("Plain text: All systems operational."));
}
