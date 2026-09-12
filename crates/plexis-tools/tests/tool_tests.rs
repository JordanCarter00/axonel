use std::sync::Arc;
use tempfile::tempdir;

use plexis_core::ids::{AgentId, ExecutionId, TaskId};
use plexis_tools::{
    FilesystemTool, ResourceLimits, Sandbox, SandboxPolicy, ShellTool, Tool, ToolError,
    ToolInvocationContext, ToolRegistry,
};

#[tokio::test]
async fn test_filesystem_tool_lifecycle_and_traversal_prevention() {
    let dir = tempdir().expect("tempdir");
    let sandbox = Arc::new(Sandbox::new(dir.path()));

    let agent_id = AgentId::new();
    let exec_id = ExecutionId::new();
    let task_id = TaskId::new();

    let fs_tool = FilesystemTool;

    // 1. Write file
    let write_ctx = ToolInvocationContext {
        agent_id,
        execution_id: exec_id,
        task_id,
        arguments: serde_json::json!({
            "action": "write_file",
            "path": "test_dir/hello.txt",
            "content": "Hello Plexis Sandbox!"
        }),
        sandbox: sandbox.clone(),
        working_directory: dir.path().to_path_buf(),
    };
    let write_res = fs_tool.execute(&write_ctx).await.expect("write file");
    assert_eq!(write_res.exit_code, Some(0));

    // 2. Read file back
    let read_ctx = ToolInvocationContext {
        agent_id,
        execution_id: exec_id,
        task_id,
        arguments: serde_json::json!({
            "action": "read_file",
            "path": "test_dir/hello.txt"
        }),
        sandbox: sandbox.clone(),
        working_directory: dir.path().to_path_buf(),
    };
    let read_res = fs_tool.execute(&read_ctx).await.expect("read file");
    assert_eq!(read_res.stdout.as_deref(), Some("Hello Plexis Sandbox!"));

    // 3. File exists check
    let exists_ctx = ToolInvocationContext {
        agent_id,
        execution_id: exec_id,
        task_id,
        arguments: serde_json::json!({
            "action": "file_exists",
            "path": "test_dir/hello.txt"
        }),
        sandbox: sandbox.clone(),
        working_directory: dir.path().to_path_buf(),
    };
    let exists_res = fs_tool.execute(&exists_ctx).await.expect("exists");
    assert_eq!(exists_res.data["exists"], true);

    // 4. Adversarial Directory Traversal Attempt: escaping sandbox root must be DENIED
    let traversal_ctx = ToolInvocationContext {
        agent_id,
        execution_id: exec_id,
        task_id,
        arguments: serde_json::json!({
            "action": "read_file",
            "path": "../../../etc/passwd"
        }),
        sandbox: sandbox.clone(),
        working_directory: dir.path().to_path_buf(),
    };
    let traversal_err = fs_tool.execute(&traversal_ctx).await.unwrap_err();
    assert!(
        matches!(traversal_err, ToolError::PermissionDenied(_)),
        "Path traversal escape must return ToolError::PermissionDenied"
    );
}

#[tokio::test]
async fn test_shell_tool_execution_timeout_and_whitelist() {
    let dir = tempdir().expect("tempdir");
    let mut policy = SandboxPolicy::new(dir.path());
    // Whitelist only echo and ls
    policy.allowed_commands = Some(vec!["echo".to_string(), "ls".to_string()]);
    let sandbox = Arc::new(Sandbox::new(dir.path()).with_policy(policy));

    let agent_id = AgentId::new();
    let exec_id = ExecutionId::new();
    let task_id = TaskId::new();
    let shell_tool = ShellTool;

    // 1. Allowed command executes successfully
    let ok_ctx = ToolInvocationContext {
        agent_id,
        execution_id: exec_id,
        task_id,
        arguments: serde_json::json!({
            "command": "echo 'Sandboxed execution successful'"
        }),
        sandbox: sandbox.clone(),
        working_directory: dir.path().to_path_buf(),
    };
    let ok_res = shell_tool.execute(&ok_ctx).await.expect("shell execute");
    assert!(ok_res
        .stdout
        .expect("stdout")
        .contains("Sandboxed execution successful"));
    assert_eq!(ok_res.exit_code, Some(0));

    // 2. Disallowed command in whitelist is rejected
    let denied_ctx = ToolInvocationContext {
        agent_id,
        execution_id: exec_id,
        task_id,
        arguments: serde_json::json!({
            "command": "rm -rf /"
        }),
        sandbox: sandbox.clone(),
        working_directory: dir.path().to_path_buf(),
    };
    let denied_err = shell_tool.execute(&denied_ctx).await.unwrap_err();
    assert!(matches!(denied_err, ToolError::PermissionDenied(_)));

    // 3. Timeout enforcement
    let timeout_policy = SandboxPolicy::new(dir.path()).with_limits(ResourceLimits {
        max_duration: std::time::Duration::from_millis(100),
        max_output_bytes: 1024,
        max_file_size_bytes: 1024,
    });
    let timeout_sandbox = Arc::new(Sandbox::new(dir.path()).with_policy(timeout_policy));
    let timeout_ctx = ToolInvocationContext {
        agent_id,
        execution_id: exec_id,
        task_id,
        arguments: serde_json::json!({
            "command": "sleep 2"
        }),
        sandbox: timeout_sandbox,
        working_directory: dir.path().to_path_buf(),
    };
    let timeout_err = shell_tool.execute(&timeout_ctx).await.unwrap_err();
    assert!(matches!(timeout_err, ToolError::Timeout(_)));
}

#[tokio::test]
async fn test_tool_registry_telemetry_audit_record() {
    let dir = tempdir().expect("tempdir");
    let sandbox = Arc::new(Sandbox::new(dir.path()));
    let registry = ToolRegistry::standard_suite();

    let agent_id = AgentId::new();
    let exec_id = ExecutionId::new();
    let task_id = TaskId::new();

    let ctx = ToolInvocationContext {
        agent_id,
        execution_id: exec_id,
        task_id,
        arguments: serde_json::json!({
            "action": "write_file",
            "path": "result.json",
            "content": "{\"status\":\"ok\"}"
        }),
        sandbox,
        working_directory: dir.path().to_path_buf(),
    };

    let (record, result) = registry.invoke("filesystem", &ctx).await;
    assert!(result.is_ok());

    // Verify all 10 telemetry fields
    assert_eq!(record.agent_id, agent_id);
    assert_eq!(record.execution_id, exec_id);
    assert_eq!(record.task_id, task_id);
    assert_eq!(record.tool_name, "filesystem");
    assert!(record.authorization_result.is_allowed());
    assert!(record.ended_at.is_some());
    assert!(record.ended_at.unwrap() >= record.started_at);
    assert!(record.result.is_some());
    assert!(record.failure.is_none());
}
