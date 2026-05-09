//! Server CLI handlers - production API server and admin init.

use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::{info, warn};

const RESULT_INACTIVE_TTL_SECS: i64 = 2 * 60 * 60;
const DEFAULT_INITIAL_ADMIN_KEY: &str = "123456";
const DEPLOYMENT_ADMIN_KEY_ENVS: [&str; 2] =
    ["SCHOLARLENS_ADMIN_PASSWORD", "SCHOLARLENS_ADMIN_KEY"];

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeploymentAdminKey {
    source: String,
    value: String,
}

// ============================================================================
// Production API Server
// ============================================================================

/// Run the production API server with full features
pub async fn run_api_server(
    port_override: Option<u16>,
    host_override: Option<String>,
    serve_static: Option<String>,
) -> Result<()> {
    use rscholar::db::{api_keys, init_pool, DbConfig};
    use rscholar::server::{
        config::ServerConfig, recovery, routes::create_router, state::AppState,
    };

    // Initialize database
    let db_config = DbConfig::default();
    let db = init_pool(&db_config).map_err(|e| anyhow::anyhow!("Database error: {}", e))?;

    // Load configuration from file
    let mut config = ServerConfig::load_from_file("config.toml")
        .map_err(|e| anyhow::anyhow!("Configuration error: {}", e))?;

    // Apply command-line overrides
    if let Some(p) = port_override {
        config.server.port = p;
    }
    if let Some(h) = host_override {
        config.server.host = h;
    }

    let addr = config
        .socket_addr()
        .map_err(|e| anyhow::anyhow!("Invalid address: {}", e))?;

    // Check if admin key exists (async)
    let conn = db
        .get()
        .await
        .map_err(|e| anyhow::anyhow!("DB connection error: {}", e))?;
    let has_admin = conn
        .interact(|conn| api_keys::has_admin_key(conn))
        .await
        .map_err(|e| anyhow::anyhow!("DB interact error: {}", e))?
        .map_err(|e| anyhow::anyhow!("DB error: {}", e))?;
    if let Some(configured) = deployment_admin_key_from_env() {
        let key_name = "Admin".to_string();
        let key_value = configured.value.clone();
        conn.interact(move |conn| api_keys::set_admin_key(conn, &key_name, &key_value, 1000))
            .await
            .map_err(|e| anyhow::anyhow!("DB interact error: {}", e))?
            .map_err(|e| anyhow::anyhow!("Failed to set deployment admin password: {}", e))?;

        println!("Admin password synchronized from {}.", configured.source);
    } else if !has_admin {
        let configured = initial_admin_key_from_pairs(std::iter::empty::<(&str, &str)>());
        let key_name = "Admin".to_string();
        let key_value = configured.value.clone();
        conn.interact(move |conn| {
            api_keys::create_with_key(conn, &key_name, true, 1000, &key_value)
        })
        .await
        .map_err(|e| anyhow::anyhow!("DB interact error: {}", e))?
        .map_err(|e| anyhow::anyhow!("Failed to create initial admin password: {}", e))?;

        if configured.source == "default" {
            println!(
                "Initial admin password created: {}",
                DEFAULT_INITIAL_ADMIN_KEY
            );
        } else {
            println!("Initial admin password created from {}.", configured.source);
        }
    }
    drop(conn);

    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║                ScholarLens API Server                        ║");
    println!("╠═══════════════════════════════════════════════════════════╣");
    println!(
        "║  Version:     {}                                    ║",
        env!("CARGO_PKG_VERSION")
    );
    println!("║  Endpoint:    http://{}                      ║", addr);
    println!("║  Database:    {}                   ║", db_config.path);
    println!("║  Auth:        Cloudflare WAF (external)                  ║");
    println!(
        "║  Admin API:   {}                                    ║",
        if config.server.admin_enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    if let Some(ref dir) = serve_static {
        println!("║  Static:      {} (SPA mode)                     ║", dir);
    }
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!();

    info!(
        host = %config.server.host,
        port = config.server.port,
        db_path = %db_config.path,
        static_dir = ?serve_static,
        "Starting production API server"
    );

    // Initialize LLM filter via centralized LLM module factory
    let llm_filter = rscholar::llm::LlmRelevanceFilter::build_from_config(&config.llm)
        .map_err(|e| anyhow::anyhow!("LLM configuration error: {}", e))?;

    // Initialize ranking microservice once (persistent key pool + FIFO scheduler)
    let ranking_service = if config.easyscholar.keys.is_empty() {
        None
    } else {
        let scheduler_mode = match config.ranking.scheduler_mode.to_lowercase().as_str() {
            "easy_backfill" => rscholar::rankings::SchedulerMode::EasyBackfill,
            other => {
                return Err(anyhow::anyhow!(
                    "Unsupported ranking.scheduler_mode '{}', expected 'easy_backfill'",
                    other
                ));
            }
        };

        let options = rscholar::rankings::RankingServiceOptions {
            queue_capacity: config.ranking.queue_capacity,
            lease_policy: rscholar::rankings::LeasePolicy {
                min_chunk: config.ranking.min_chunk,
                max_chunk: config.ranking.max_chunk,
            },
            max_concurrent_jobs: config.ranking.max_concurrent_jobs,
            scheduler_mode,
            target_duration_sec: config.ranking.target_duration_sec,
            eta_scale: config.ranking.eta_scale,
            heartbeat_ms: config.ranking.heartbeat_ms,
            job_timeout_min_sec: config.ranking.job_timeout_min_sec,
            key_health_policy: rscholar::rankings::KeyHealthPolicy {
                fail_threshold: config.ranking.key_fail_threshold,
                cooldown_secs: config.ranking.key_cooldown_sec,
                stale_ttl_secs: config.ranking.key_stale_ttl_sec,
            },
        };
        let service = rscholar::rankings::RankingService::new(
            &config.easyscholar.keys,
            Some(db.clone()),
            options,
        )
        .map_err(|e| anyhow::anyhow!("Ranking service initialization error: {}", e))?;
        Some(Arc::new(service))
    };

    // Create application state with database
    let state = AppState::new(config.clone(), db, llm_filter, ranking_service);

    let has_db_llm_providers = state
        .run_db(rscholar::db::llm_providers::has_any)
        .await
        .map_err(|e| anyhow::anyhow!("LLM provider DB check error: {}", e))?;
    if has_db_llm_providers {
        state
            .reload_llm_from_db()
            .await
            .map_err(|e| anyhow::anyhow!("LLM provider reload error: {}", e))?;
    }

    // Recover interrupted tasks (mark RUNNING tasks as FAILED after restart)
    let recovered = recovery::recover_interrupted_tasks(&state.db).await;
    if recovered > 0 {
        info!(count = recovered, "Recovered interrupted tasks");
    }

    // Create router with all middleware (with optional static file serving)
    let app = create_router(state.clone(), serve_static.clone());

    // Start cleanup task for cloud/server-side results.
    // Completed/failed task results are retained until they have not been accessed for 2 hours.
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            cleanup_state
                .task_store
                .cleanup_completed(RESULT_INACTIVE_TTL_SECS as u64);

            match cleanup_state
                .run_db(|conn| {
                    rscholar::db::tasks::cleanup_inactive(conn, RESULT_INACTIVE_TTL_SECS)
                })
                .await
            {
                Ok(expired) => {
                    if !expired.is_empty() {
                        delete_expired_result_files(&expired);
                        info!(
                            removed = expired.len(),
                            ttl_secs = RESULT_INACTIVE_TTL_SECS,
                            "Cleaned up inactive task results"
                        );
                    }
                }
                Err(error) => {
                    warn!(error = %error, "Failed to clean up inactive task results");
                }
            }
        }
    });

    // Bind and serve
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(addr = %addr, "Server listening");

    // Graceful shutdown on Ctrl+C
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Server error")?;

    info!("Server shutdown complete");
    Ok(())
}

fn delete_expired_result_files(expired: &[rscholar::db::tasks::ExpiredTaskRecord]) {
    for record in expired {
        let Some(csv_path) = record.csv_path.as_deref() else {
            continue;
        };
        let path = std::path::Path::new(csv_path);
        if let Err(error) = std::fs::remove_file(path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                warn!(
                    task_id = %record.task_id,
                    path = %path.display(),
                    error = %error,
                    "Failed to delete expired result file"
                );
            }
            continue;
        }

        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::remove_dir(parent) {
                if error.kind() != std::io::ErrorKind::NotFound
                    && error.kind() != std::io::ErrorKind::DirectoryNotEmpty
                {
                    warn!(
                        task_id = %record.task_id,
                        path = %parent.display(),
                        error = %error,
                        "Failed to delete expired result directory"
                    );
                }
            }
        }
    }
}

/// Wait for shutdown signal (Ctrl+C)
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received, stopping server...");
}

// ============================================================================
// Admin Key Initialization
// ============================================================================

fn initial_admin_key_from_env() -> DeploymentAdminKey {
    deployment_admin_key_from_env().unwrap_or_else(|| DeploymentAdminKey {
        source: "default".to_string(),
        value: DEFAULT_INITIAL_ADMIN_KEY.to_string(),
    })
}

fn deployment_admin_key_from_env() -> Option<DeploymentAdminKey> {
    let pairs = DEPLOYMENT_ADMIN_KEY_ENVS
        .iter()
        .filter_map(|source| std::env::var(source).ok().map(|value| (*source, value)));
    deployment_admin_key_from_pairs(pairs)
}

fn initial_admin_key_from_pairs<I, K, V>(pairs: I) -> DeploymentAdminKey
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    deployment_admin_key_from_pairs(pairs).unwrap_or_else(|| DeploymentAdminKey {
        source: "default".to_string(),
        value: DEFAULT_INITIAL_ADMIN_KEY.to_string(),
    })
}

fn deployment_admin_key_from_pairs<I, K, V>(pairs: I) -> Option<DeploymentAdminKey>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    let values = pairs
        .into_iter()
        .map(|(source, value)| {
            (
                source.as_ref().to_string(),
                value.as_ref().trim().to_string(),
            )
        })
        .collect::<Vec<_>>();

    for preferred_source in DEPLOYMENT_ADMIN_KEY_ENVS {
        if let Some((source, value)) = values
            .iter()
            .find(|(source, value)| source == preferred_source && !value.is_empty())
        {
            return Some(DeploymentAdminKey {
                source: source.clone(),
                value: value.clone(),
            });
        }
    }

    None
}

/// Initialize the first admin API key
pub async fn init_admin_key(name: &str, custom_key: Option<&str>) -> Result<()> {
    use rscholar::db::{api_keys, init_pool, DbConfig};

    println!("Initializing admin API key...\n");

    // Initialize database
    let db_config = DbConfig::default();
    let db = init_pool(&db_config).map_err(|e| anyhow::anyhow!("Database error: {}", e))?;

    let conn = db
        .get()
        .await
        .map_err(|e| anyhow::anyhow!("DB connection error: {}", e))?;

    // Check if admin already exists
    let has_admin = conn
        .interact(|conn| api_keys::has_admin_key(conn))
        .await
        .map_err(|e| anyhow::anyhow!("DB interact error: {}", e))?
        .map_err(|e| anyhow::anyhow!("DB error: {}", e))?;

    if has_admin {
        println!("⚠️  An admin key already exists.");
        println!("   Use the admin endpoints to manage additional keys.");
        return Ok(());
    }

    // Create admin key
    let configured = custom_key
        .map(|value| DeploymentAdminKey {
            source: "--key".to_string(),
            value: value.trim().to_string(),
        })
        .unwrap_or_else(initial_admin_key_from_env);
    let key_name = name.to_string();
    let key_value = configured.value.clone();
    let created = conn
        .interact(move |conn| api_keys::create_with_key(conn, &key_name, true, 1000, &key_value))
        .await
        .map_err(|e| anyhow::anyhow!("DB interact error: {}", e))?
        .map_err(|e| anyhow::anyhow!("Failed to create key: {}", e))?;

    use secrecy::ExposeSecret;

    println!("╔═══════════════════════════════════════════════════════════════════╗");
    println!("║                    ADMIN API KEY CREATED                           ║");
    println!("╠═══════════════════════════════════════════════════════════════════╣");
    println!(
        "║  Name:       {}                                         ║",
        created.name
    );
    println!(
        "║  ID:         {}                                   ║",
        created.id
    );
    println!("║                                                                   ║");
    println!("║  🔑 API Key: {}  ║", created.key.expose_secret());
    println!("║                                                                   ║");
    println!("║  ⚠️  SAVE THIS KEY! It will not be shown again.                    ║");
    println!("╚═══════════════════════════════════════════════════════════════════╝");
    println!();
    println!("Use this key in the X-API-Key header for admin endpoints.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deployment_admin_key_prefers_password_env() {
        let configured = deployment_admin_key_from_pairs([
            ("SCHOLARLENS_ADMIN_KEY", "fallback-admin-key"),
            ("SCHOLARLENS_ADMIN_PASSWORD", " deploy-admin-password-2026 "),
        ])
        .expect("deployment key");

        assert_eq!(configured.source, "SCHOLARLENS_ADMIN_PASSWORD");
        assert_eq!(configured.value, "deploy-admin-password-2026");
    }

    #[test]
    fn test_deployment_admin_key_ignores_empty_values() {
        let configured = deployment_admin_key_from_pairs([
            ("SCHOLARLENS_ADMIN_PASSWORD", "   "),
            ("SCHOLARLENS_ADMIN_KEY", "fallback-admin-key"),
        ])
        .expect("fallback key");

        assert_eq!(configured.source, "SCHOLARLENS_ADMIN_KEY");
        assert_eq!(configured.value, "fallback-admin-key");
    }

    #[test]
    fn test_initial_admin_key_defaults_to_123456() {
        let configured = initial_admin_key_from_pairs([
            ("SCHOLARLENS_ADMIN_PASSWORD", ""),
            ("SCHOLARLENS_ADMIN_KEY", "   "),
        ]);

        assert_eq!(configured.source, "default");
        assert_eq!(configured.value, "123456");
    }
}
