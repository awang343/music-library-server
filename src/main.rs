use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

mod api;
mod config;
mod db;
mod libraries;
mod scanner;

#[derive(Parser)]
#[command(name = "muserv", version, about = "Muserv: personal music library server")]
struct Cli {
    /// Path to config.toml. Defaults to $XDG_CONFIG_HOME/muserv/config.toml
    /// (or ~/.config/muserv/config.toml).
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    cmd: Cmd,
}

fn default_config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"));
    base.join("muserv").join("config.toml")
}

#[derive(Subcommand)]
enum Cmd {
    /// Scan all configured libraries and update the database.
    Scan {
        /// Only scan this library (by name). Default scans all.
        #[arg(long)]
        library: Option<String>,
    },
    /// Run the HTTP server.
    Serve {
        /// Run a scan of all libraries before starting the server.
        #[arg(long)]
        scan: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,muserv=debug")),
        )
        .init();

    let cli = Cli::parse();
    let config_path = cli.config.unwrap_or_else(default_config_path);
    let cfg = config::Config::load(&config_path)
        .with_context(|| format!("loading config from {}", config_path.display()))?;
    tracing::info!(?cfg, "loaded config");

    let pool = db::connect(&cfg.db_path).await?;
    let libs = libraries::sync(&pool, &cfg.libraries).await?;
    tracing::info!(count = libs.len(), "libraries synced");

    match cli.cmd {
        Cmd::Scan { library } => {
            let to_scan: Vec<&libraries::Library> = match library.as_deref() {
                Some(name) => libs
                    .iter()
                    .filter(|l| l.name == name)
                    .collect::<Vec<_>>(),
                None => libs.iter().collect(),
            };
            if to_scan.is_empty() {
                anyhow::bail!("no matching library");
            }
            for lib in to_scan {
                println!("== library: {} ({}) ==", lib.name, lib.root_path);
                let stats = scanner::scan(&pool, lib.id, &lib.root()).await?;
                print_scan_summary(&stats);
            }
        }
        Cmd::Serve { scan } => {
            if scan {
                for lib in &libs {
                    println!("== library: {} ({}) ==", lib.name, lib.root_path);
                    let stats = scanner::scan(&pool, lib.id, &lib.root()).await?;
                    print_scan_summary(&stats);
                }
            }
            if cfg.auth_token.is_none() && !cfg.bind.starts_with("127.")
                && !cfg.bind.starts_with("localhost")
                && !cfg.bind.starts_with("[::1]")
            {
                tracing::warn!(bind = %cfg.bind, "auth_token is unset and bind is non-loopback — API is open");
            }
            let router = api::router(pool, cfg.auth_token.clone(), libs);
            let listener = tokio::net::TcpListener::bind(&cfg.bind)
                .await
                .with_context(|| format!("binding {}", cfg.bind))?;
            tracing::info!(addr = %cfg.bind, "listening");
            axum::serve(listener, router).await?;
        }
    }
    Ok(())
}

fn print_scan_summary(stats: &scanner::ScanStats) {
    println!(
        "scan: seen={} inserted={} updated={} unchanged={} removed={} failed={}",
        stats.seen, stats.inserted, stats.updated, stats.unchanged, stats.removed, stats.failed,
    );
    if !stats.failures.is_empty() {
        println!();
        println!(
            "{} file{} skipped (not imported):",
            stats.failures.len(),
            if stats.failures.len() == 1 { "" } else { "s" },
        );
        for f in &stats.failures {
            println!("  {}: {}", f.path.display(), f.reason);
        }
    }
}
