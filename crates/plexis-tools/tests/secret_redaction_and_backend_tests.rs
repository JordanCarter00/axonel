use std::sync::Arc;
use tempfile::tempdir;

use plexis_core::ids::{AgentId, ExecutionId, TaskId};
use plexis_tools::{
    InMemorySecretStore, Sandbox, SandboxPolicy, SecretAcl, ShellTool, Tool, ToolInvocationContext,
};

#[tokio::test]
async fn test_secret_authorization_and_automatic_redaction() {
    let dir = tempdir().unwrap();
    let working_dir = dir.path().to_path_buf();
    let policy = SandboxPolicy::new(working_dir.clone());
    let sandbox = Arc::new(Sandbox::new(working_dir.clone()).with_policy(policy));

    let authorized_agent = AgentId::new();
    let unauthorized_agent = AgentId::new();
    let task_id = TaskId::new();

    // 1. Setup secret store with ACL
    let secret_store = Arc::new(InMemorySecretStore::new());
    let secret_value = "super_secret_production_token_xyz987";
    secret_store.set_secret(
        "PROD_API_TOKEN",
        secret_value,
        SecretAcl::new()
            .with_agent(authorized_agent)
            .with_task(task_id)
            .with_tool("shell"),
    );

    let shell_tool = ShellTool;

    // 2. Execute with authorized agent:
    // Command echoes the env var: $PROD_API_TOKEN
    let auth_ctx = ToolInvocationContext::new(
        authorized_agent,
        ExecutionId::new(),
        task_id,
        serde_json::json!({
            "command": "echo \"Token value is: $PROD_API_TOKEN\""
        }),
        sandbox.clone(),
        working_dir.clone(),
    )
    .with_secret_store(secret_store.clone());

    let output = shell_tool.execute(&auth_ctx).await.expect("execute shell");
    let stdout = output.stdout.expect("stdout");

    // CRITICAL: The secret was present in process env (so the text was formed),
    // but the output was automatically REDACTED before returning to the caller!
    assert!(
        stdout.contains("Token value is: [REDACTED]"),
        "Stdout should have redacted secret, got: {}",
        stdout
    );
    assert!(
        !stdout.contains(secret_value),
        "Stdout must NEVER contain the raw secret value!"
    );

    // Verify structured json data is also redacted
    let data_str = serde_json::to_string(&output.data).unwrap();
    assert!(
        !data_str.contains(secret_value),
        "Data payload must never contain raw secret!"
    );

    // 3. Execute with unauthorized agent:
    // The secret is not injected into the process env at all
    let unauth_ctx = ToolInvocationContext::new(
        unauthorized_agent,
        ExecutionId::new(),
        task_id,
        serde_json::json!({
            "command": "echo \"Token value is: $PROD_API_TOKEN\""
        }),
        sandbox,
        working_dir,
    )
    .with_secret_store(secret_store.clone());

    let unauth_output = shell_tool
        .execute(&unauth_ctx)
        .await
        .expect("execute shell");
    let unauth_stdout = unauth_output.stdout.expect("stdout");
    assert_eq!(unauth_stdout.trim(), "Token value is:");
}

#[tokio::test]
async fn test_host_environment_stripping() {
    let dir = tempdir().unwrap();
    let working_dir = dir.path().to_path_buf();
    let policy = SandboxPolicy::new(working_dir.clone());
    let sandbox = Arc::new(Sandbox::new(working_dir.clone()).with_policy(policy));

    // Set a parent process environment variable that should NOT leak
    std::env::set_var("PLEXIS_HOST_LEAK_TEST_VAR", "secret_host_value_123");

    let shell_tool = ShellTool;
    let ctx = ToolInvocationContext::new(
        AgentId::new(),
        ExecutionId::new(),
        TaskId::new(),
        serde_json::json!({
            "command": "echo \"Host var: $PLEXIS_HOST_LEAK_TEST_VAR\""
        }),
        sandbox,
        working_dir,
    );

    let output = shell_tool.execute(&ctx).await.expect("execute shell");
    let stdout = output.stdout.expect("stdout");
    assert_eq!(
        stdout.trim(),
        "Host var:",
        "Host process environment variable must be stripped and not visible to tool"
    );
}
