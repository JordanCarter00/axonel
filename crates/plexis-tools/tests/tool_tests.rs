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
    let write_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "action": "write_file",
            "path": "test_dir/hello.txt",
            "content": "Hello Plexis Sandbox!"
        }),
        sandbox.clone(),
        dir.path().to_path_buf(),
    );
    let write_res = fs_tool.execute(&write_ctx).await.expect("write file");
    assert_eq!(write_res.exit_code, Some(0));

    // 2. Read file back
    let read_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "action": "read_file",
            "path": "test_dir/hello.txt"
        }),
        sandbox.clone(),
        dir.path().to_path_buf(),
    );
    let read_res = fs_tool.execute(&read_ctx).await.expect("read file");
    assert_eq!(read_res.stdout.as_deref(), Some("Hello Plexis Sandbox!"));

    // 3. File exists check
    let exists_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "action": "file_exists",
            "path": "test_dir/hello.txt"
        }),
        sandbox.clone(),
        dir.path().to_path_buf(),
    );
    let exists_res = fs_tool.execute(&exists_ctx).await.expect("exists");
    assert_eq!(exists_res.data["exists"], true);

    // 4. Adversarial Directory Traversal Attempt: escaping sandbox root must be DENIED
    let traversal_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "action": "read_file",
            "path": "../../../etc/passwd"
        }),
        sandbox.clone(),
        dir.path().to_path_buf(),
    );
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
    let ok_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "command": "echo 'Sandboxed execution successful'"
        }),
        sandbox.clone(),
        dir.path().to_path_buf(),
    );
    let ok_res = shell_tool.execute(&ok_ctx).await.expect("shell execute");
    assert!(ok_res
        .stdout
        .expect("stdout")
        .contains("Sandboxed execution successful"));
    assert_eq!(ok_res.exit_code, Some(0));

    // 2. Disallowed command in whitelist is rejected
    let denied_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "command": "rm -rf /"
        }),
        sandbox.clone(),
        dir.path().to_path_buf(),
    );
    let denied_err = shell_tool.execute(&denied_ctx).await.unwrap_err();
    assert!(matches!(denied_err, ToolError::PermissionDenied(_)));

    // 3. Timeout enforcement
    let timeout_policy = SandboxPolicy::new(dir.path()).with_limits(ResourceLimits {
        max_duration: std::time::Duration::from_millis(100),
        max_output_bytes: 1024,
        max_file_size_bytes: 1024,
    });
    let timeout_sandbox = Arc::new(Sandbox::new(dir.path()).with_policy(timeout_policy));
    let timeout_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "command": "sleep 2"
        }),
        timeout_sandbox,
        dir.path().to_path_buf(),
    );
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

    let ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "action": "write_file",
            "path": "result.json",
            "content": "{\"status\":\"ok\"}"
        }),
        sandbox,
        dir.path().to_path_buf(),
    );

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

#[tokio::test]
async fn test_filesystem_slicing_patching_and_safe_overwrite() {
    let dir = tempdir().expect("tempdir");
    let sandbox = Arc::new(Sandbox::new(dir.path()));
    let agent_id = AgentId::new();
    let exec_id = ExecutionId::new();
    let task_id = TaskId::new();
    let fs_tool = FilesystemTool;

    // 1. Write initial multi-line file
    let content = "line 1\nline 2\nline 3\nline 4\nline 5";
    let write_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "action": "write_file",
            "path": "sample.txt",
            "content": content
        }),
        sandbox.clone(),
        dir.path().to_path_buf(),
    );
    fs_tool.execute(&write_ctx).await.expect("initial write");

    // 2. Safe overwrite = false must error if file exists
    let no_overwrite_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "action": "write_file",
            "path": "sample.txt",
            "content": "new text",
            "overwrite": false
        }),
        sandbox.clone(),
        dir.path().to_path_buf(),
    );
    let overwrite_err = fs_tool.execute(&no_overwrite_ctx).await.unwrap_err();
    assert!(matches!(overwrite_err, ToolError::ExecutionFailed(_)));

    // 3. Sliced read with line numbers (lines 2-4)
    let slice_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "action": "read_file",
            "path": "sample.txt",
            "start_line": 2,
            "end_line": 4,
            "numbered": true
        }),
        sandbox.clone(),
        dir.path().to_path_buf(),
    );
    let slice_res = fs_tool.execute(&slice_ctx).await.expect("slice read");
    let stdout = slice_res.stdout.expect("stdout");
    assert!(stdout.contains("   2: line 2"));
    assert!(stdout.contains("   3: line 3"));
    assert!(stdout.contains("   4: line 4"));
    assert!(!stdout.contains("line 1"));
    assert!(!stdout.contains("line 5"));

    // 4. Multi-file read
    let second_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "action": "write_file",
            "path": "second.txt",
            "content": "second content"
        }),
        sandbox.clone(),
        dir.path().to_path_buf(),
    );
    fs_tool.execute(&second_ctx).await.expect("second write");

    let multi_read_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "action": "read_multiple_files",
            "paths": ["sample.txt", "second.txt"]
        }),
        sandbox.clone(),
        dir.path().to_path_buf(),
    );
    let multi_res = fs_tool.execute(&multi_read_ctx).await.expect("multi read");
    assert_eq!(multi_res.data["count"], 2);
    assert!(multi_res.data["files"]["sample.txt"]["content"]
        .as_str()
        .unwrap()
        .contains("line 1"));
    assert_eq!(
        multi_res.data["files"]["second.txt"]["content"],
        "second content"
    );

    // 5. Apply patch via search/replace keys
    let patch_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "action": "apply_patch",
            "path": "sample.txt",
            "search": "line 3",
            "replace": "line 3 MODIFIED"
        }),
        sandbox.clone(),
        dir.path().to_path_buf(),
    );
    fs_tool.execute(&patch_ctx).await.expect("patch applied");

    let read_back_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "action": "read_file",
            "path": "sample.txt"
        }),
        sandbox.clone(),
        dir.path().to_path_buf(),
    );
    let read_back = fs_tool.execute(&read_back_ctx).await.expect("read back");
    assert!(read_back.stdout.unwrap().contains("line 3 MODIFIED"));
}

#[tokio::test]
async fn test_git_tool_checkout_branch_and_conflicts() {
    let dir = tempdir().expect("tempdir");
    let sandbox = Arc::new(Sandbox::new(dir.path()));
    let agent_id = AgentId::new();
    let exec_id = ExecutionId::new();
    let task_id = TaskId::new();
    let git_tool = plexis_tools::GitTool;

    // Initialize git repository
    let _ = std::process::Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .expect("git init");
    let _ = std::process::Command::new("git")
        .args(["config", "user.name", "Tester"])
        .current_dir(dir.path())
        .output();
    let _ = std::process::Command::new("git")
        .args(["config", "user.email", "test@test.local"])
        .current_dir(dir.path())
        .output();

    // Initial commit
    std::fs::write(dir.path().join("file.txt"), "initial content\n").expect("write file");
    let commit_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "action": "commit",
            "message": "initial commit"
        }),
        sandbox.clone(),
        dir.path().to_path_buf(),
    );
    git_tool.execute(&commit_ctx).await.expect("git commit");

    // Checkout new branch
    let checkout_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "action": "checkout",
            "branch": "feature-patch-1",
            "create_branch": true
        }),
        sandbox.clone(),
        dir.path().to_path_buf(),
    );
    let checkout_res = git_tool.execute(&checkout_ctx).await.expect("checkout -b");
    assert_eq!(checkout_res.exit_code, Some(0));

    // List branches
    let branch_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "action": "branch"
        }),
        sandbox.clone(),
        dir.path().to_path_buf(),
    );
    let branch_res = git_tool.execute(&branch_ctx).await.expect("list branches");
    assert!(branch_res.stdout.unwrap().contains("feature-patch-1"));

    // Check conflicts (none expected)
    let conflict_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "action": "conflicts"
        }),
        sandbox.clone(),
        dir.path().to_path_buf(),
    );
    let conflict_res = git_tool.execute(&conflict_ctx).await.expect("conflicts");
    assert_eq!(conflict_res.data["has_conflicts"], false);
}
