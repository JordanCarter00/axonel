//! CLI subcommands implementation for Plexis.
//!
//! Subcommands:
//! - `init [path]`: Initializes a workspace in the target directory, detecting project characteristics.
//! - `serve`: Launches the operational HTTP server control plane.
//! - `status`: Inspects authoritative storage and prints system and workspace operational status.

use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use plexis_core::Workspace;
use plexis_storage::traits::{AgentStore, TaskStore, WorkflowStore, WorkspaceStore};
use plexis_storage::SqliteStore;

use crate::routes::create_router;
use crate::state::AppState;

#[derive(Parser, Debug)]
#[command(name = "plexis")]
#[command(about = "Plexis — Autonomous Software Engineering Workspace & Control Plane", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize a project workspace for Plexis
    Init {
        /// Target directory to initialize as workspace (defaults to current working directory)
        path: Option<PathBuf>,
        /// Friendly workspace name
        #[arg(short, long)]
        name: Option<String>,
        /// Path to the authoritative database
        #[arg(long, default_value = "plexis.db")]
        db: String,
    },
    /// Start the Plexis operational control plane server
    Serve {
        /// Bind host address
        #[arg(long, default_value = "0.0.0.0")]
        host: String,
        /// Listen port
        #[arg(short, long, default_value_t = 3000)]
        port: u16,
        /// Path to SQLite database file or :memory:
        #[arg(long, default_value = "plexis.db")]
        db: String,
        /// Optional authentication bearer token
        #[arg(long)]
        auth_token: Option<String>,
    },
    /// Inspect system and workspace operational status
    Status {
        /// Path to SQLite database file
        #[arg(long, default_value = "plexis.db")]
        db: String,
    },
}

pub async fn run_cli() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init { path, name, db }) => {
            init_workspace(path, name, &db).await?;
        }
        Some(Commands::Serve {
            host,
            port,
            db,
            auth_token,
        }) => {
            run_server(&host, port, &db, auth_token).await?;
        }
        Some(Commands::Status { db }) => {
            show_status(&db).await?;
        }
        None => {
            // Default behavior: run server with environment/default settings
            let db_path =
                std::env::var("PLEXIS_DB_PATH").unwrap_or_else(|_| "plexis.db".to_string());
            let port = std::env::var("PORT")
                .ok()
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(3000);
            let auth_token = std::env::var("PLEXIS_AUTH_TOKEN").ok();
            run_server("0.0.0.0", port, &db_path, auth_token).await?;
        }
    }

    Ok(())
}

async fn init_workspace(
    target_path: Option<PathBuf>,
    custom_name: Option<String>,
    db_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let target = target_path
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let canonical = std::fs::canonicalize(&target).unwrap_or(target);
    let ws_name = custom_name.unwrap_or_else(|| {
        canonical
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace")
            .to_string()
    });

    println!("Initializing Plexis workspace in: {}", canonical.display());

    // Create .plexis directory and config
    let plexis_dir = canonical.join(".plexis");
    std::fs::create_dir_all(&plexis_dir)?;

    let config_file = plexis_dir.join("config.json");
    if !config_file.exists() {
        let default_config = serde_json::json!({
            "name": ws_name,
            "version": "1.0",
            "security": {
                "max_execution_time_secs": 1800,
                "allow_network": false,
                "require_approval_for_destructive": true
            }
        });
        std::fs::write(&config_file, serde_json::to_string_pretty(&default_config)?)?;
        println!("Created {}", config_file.display());
    }

    // Register workspace in store
    let store = if db_path == ":memory:" {
        SqliteStore::open_in_memory()?
    } else {
        SqliteStore::open(db_path)?
    };

    let mut ws = Workspace::new(&ws_name, canonical.clone());
    if let Ok(meta) = crate::git::discover_git_metadata(&canonical) {
        ws.vcs = meta;
    }
    ws.metadata = serde_json::json!({ "is_default": true });

    store.create_workspace(&ws).await?;
    println!(
        "Registered workspace '{}' [{}] in {}",
        ws.name, ws.id, db_path
    );
    println!("Workspace initialized successfully.");

    Ok(())
}

async fn show_status(db_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !Path::new(db_path).exists() && db_path != ":memory:" {
        println!("Database not found at: {}", db_path);
        println!("Run 'plexis init' or start 'plexis serve' to create one.");
        return Ok(());
    }

    let store = SqliteStore::open(db_path)?;
    let workspaces = store.list_workspaces().await?;
    let workflows = store.list_workflows().await?;
    let mut total_tasks = 0;
    for wf in &workflows {
        if let Ok(tasks) = store.list_tasks_by_workflow(&wf.id).await {
            total_tasks += tasks.len();
        }
    }
    let agents = store.list_agents().await?;

    println!("============================================================");
    println!("                 PLEXIS SYSTEM STATUS                       ");
    println!("============================================================");
    println!("Authoritative Database : {}", db_path);
    println!("Total Workspaces       : {}", workspaces.len());
    println!("Total Workflows        : {}", workflows.len());
    println!("Total Tasks            : {}", total_tasks);
    println!("Active Agents          : {}", agents.len());
    println!("------------------------------------------------------------");
    if workspaces.is_empty() {
        println!("No workspaces registered. Use 'plexis init' to register one.");
    } else {
        println!("Workspaces:");
        for ws in &workspaces {
            let default_flag = if ws
                .metadata
                .get("is_default")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                " (default)"
            } else {
                ""
            };
            let vcs = if let Some(ref b) = ws.vcs.branch {
                let head = ws.vcs.head_sha.as_deref().unwrap_or("unknown");
                format!(" [git: {}@{}]", b, &head[..7.min(head.len())])
            } else {
                String::new()
            };
            println!(
                "  • {} [{}]{} -> {}{}",
                ws.name,
                ws.id,
                default_flag,
                ws.canonical_path.display(),
                vcs
            );
        }
    }
    println!("============================================================");

    Ok(())
}

async fn run_server(
    host: &str,
    port: u16,
    db_path: &str,
    auth_token: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Opening authoritative storage at: {}", db_path);
    let store = if db_path == ":memory:" {
        SqliteStore::open_in_memory()?
    } else {
        SqliteStore::open(db_path)?
    };

    let mut state = AppState::new(store);
    if let Some(ref tok) = auth_token {
        if !tok.trim().is_empty() {
            info!("Hardened token authentication enabled");
            state = state.with_auth_token(auth_token.clone());
        }
    }

    let app = create_router(state);

    let ip: std::net::IpAddr = host
        .parse()
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
    let addr = SocketAddr::from((ip, port));
    info!("Plexis API server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
