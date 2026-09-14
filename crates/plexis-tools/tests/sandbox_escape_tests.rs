use std::time::Duration;
use tempfile::tempdir;

use plexis_tools::backend::{BubblewrapBackend, CommandSpec, ExecutionBackend, HostProcessBackend};

#[tokio::test]
async fn test_host_process_backend_boundary_assessment() {
    let dir = tempdir().unwrap();
    let backend = HostProcessBackend::new();

    // 1. Host backend clears environment variables completely
    let spec_env = CommandSpec::new("echo $HOST_SECRET_TOKEN", dir.path())
        .with_timeout(Duration::from_secs(5));
    let res = backend.execute(spec_env).await.expect("execute");
    assert_eq!(res.stdout.trim(), "");

    // 2. Host backend injects authorized env vars
    let spec_inject = CommandSpec::new("echo $MY_VAR", dir.path())
        .with_env("MY_VAR", "injected_secret")
        .with_timeout(Duration::from_secs(5));
    let res = backend.execute(spec_inject).await.expect("execute");
    assert_eq!(res.stdout.trim(), "injected_secret");

    // 3. Honest evaluation: Host backend has NO filesystem mount isolation
    assert!(!backend.has_filesystem_isolation());
    assert!(!backend.has_network_isolation());
}

#[tokio::test]
async fn test_bubblewrap_backend_sandbox_confinement_and_escapes() {
    if !BubblewrapBackend::is_available() {
        eprintln!("bwrap not available on this host; skipping bubblewrap test");
        return;
    }

    let bwrap = BubblewrapBackend::new().expect("init bubblewrap");
    assert!(bwrap.has_filesystem_isolation());
    assert!(bwrap.has_network_isolation());

    let dir = tempdir().unwrap();
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    // 1. Writing inside workspace succeeds
    let write_ok = CommandSpec::new("echo 'safe data' > test.txt && cat test.txt", &workspace);
    let res_ok = bwrap
        .execute(write_ok)
        .await
        .expect("execute inside workspace");
    assert_eq!(res_ok.exit_code, 0);
    assert!(res_ok.stdout.contains("safe data"));

    // 2. Escape attempt 1: Writing outside workspace to read-only mount (/usr)
    let escape_write_usr = CommandSpec::new("touch /usr/malicious.txt", &workspace);
    let res_usr = bwrap
        .execute(escape_write_usr)
        .await
        .expect("execute write usr");
    assert_ne!(res_usr.exit_code, 0, "Writing to /usr in sandbox must fail");
    assert!(
        res_usr.stderr.to_lowercase().contains("read-only")
            || res_usr.stderr.to_lowercase().contains("permission denied"),
        "Expected read-only filesystem error, got: {}",
        res_usr.stderr
    );

    // 3. Escape attempt 2: Writing to host $HOME outside workspace
    if let Ok(home) = std::env::var("HOME") {
        let escape_home = CommandSpec::new(format!("touch '{}/escape_test.tmp'", home), &workspace);
        let res_home = bwrap
            .execute(escape_home)
            .await
            .expect("execute write home");
        assert_ne!(
            res_home.exit_code, 0,
            "Writing to $HOME must fail in sandbox"
        );
        assert!(
            res_home.stderr.to_lowercase().contains("read-only")
                || res_home.stderr.to_lowercase().contains("permission denied")
                || res_home.stderr.to_lowercase().contains("no such file"),
            "Expected failure writing to $HOME, got: {}",
            res_home.stderr
        );
    }

    // 4. Escape attempt 3: Reading unauthorized system secrets (/etc/shadow)
    let escape_read_shadow = CommandSpec::new("cat /etc/shadow", &workspace);
    let res_shadow = bwrap
        .execute(escape_read_shadow)
        .await
        .expect("execute read shadow");
    assert_ne!(res_shadow.exit_code, 0, "Reading /etc/shadow must fail");

    // 5. Escape attempt 4: Outbound network egress blocked under network namespace isolation
    let escape_net = CommandSpec::new(
        "curl -s -m 2 https://1.1.1.1 2>&1 || nc -z -w 1 1.1.1.1 80 2>&1 || echo 'NETWORK_FAILED'",
        &workspace,
    )
    .with_timeout(Duration::from_secs(5));
    let res_net = bwrap
        .execute(escape_net)
        .await
        .expect("execute network check");
    // With --unshare-all / --unshare-net, network devices are loopback-only
    let out = format!("{} {}", res_net.stdout, res_net.stderr);
    assert!(
        out.contains("NETWORK_FAILED")
            || out.contains("Network is unreachable")
            || out.contains("Could not resolve")
            || out.contains("failed"),
        "Network access must be blocked in bubblewrap sandbox, got: {}",
        out
    );
}
