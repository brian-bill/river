mod adapters;
mod ai;
mod cli;
mod connection;
mod engine;
mod error;
mod lang;
mod mcp;
mod tui;

use std::collections::HashMap;
use std::io::{self, Write};
use std::path::PathBuf;

use clap::Parser;
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::Terminal;
use tracing::info;
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::adapters::DatabaseAdapter;
use crate::ai::AiClient;
use crate::tui::app;

#[derive(Parser, Debug)]
    #[command(name = "river", version = "0.11.0", about = "Unified Database Access")]
struct Cli {
    #[arg(short, long, default_value = "river.yaml")]
    config: String,
    #[arg(long, default_value = "false")]
    server: bool,
    /// RiverQL script file to run (file-processor mode).
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,
    /// Run silently: execute the script and print nothing to stdout.
    #[arg(short, long)]
    silent: bool,
    /// Export the final result to a file; the extension picks the format
    /// (csv, xlsx, json, txt, xml).
    #[arg(long, value_name = "FILE")]
    out: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let file_appender = tracing_appender::rolling::never(".", "river.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().with_writer(non_blocking))
        .init();

    let cli = Cli::parse();

    info!(
        config = %cli.config,
        "River v0.11.0 — starting..."
    );

    let connections = connection::config::load_config(&cli.config)?;
    let ai_configs_vec = connection::config::load_ai_configs(&cli.config)?;

    let ai_configs: HashMap<String, connection::config::AiConfig> = ai_configs_vec
        .into_iter()
        .map(|c| (c.name.clone(), c))
        .collect();

    if !ai_configs.is_empty() {
        info!(
            "loaded {} AI config(s): {:?}",
            ai_configs.len(),
            ai_configs.keys().collect::<Vec<_>>()
        );
    }

    let ai_client = AiClient::new();

    let mut adapters: HashMap<String, Box<dyn DatabaseAdapter>> = HashMap::new();
    let mut conn_errors: Vec<String> = Vec::new();

    for cfg in &connections {
        match adapters::create_adapter(cfg).await {
            Ok(adapter) => {
                info!("connected to '{}' ({:?})", cfg.name, cfg.kind);
                adapters.insert(cfg.name.clone(), adapter);
            }
            Err(e) => {
                let msg = format!("failed to connect to '{}': {}", cfg.name, e);
                info!("{}", msg);
                conn_errors.push(msg);
            }
        }
    }

    let source_db: Vec<(String, connection::DatabaseKind)> = connections
        .iter()
        .map(|c| (c.name.clone(), c.kind.clone()))
        .collect();

    if cli.server {
        return crate::mcp::run_mcp_server(adapters, source_db, ai_configs, ai_client).await;
    }

    if let Some(file) = cli.file.clone() {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        let stderr = io::stderr();
        let mut stderr = stderr.lock();
        let code = crate::cli::run_file_processor(
            file,
            cli.silent,
            cli.out.clone(),
            &adapters,
            &source_db,
            &ai_configs,
            &ai_client,
            &mut stdout,
            &mut stderr,
        )
        .await;
        let _ = stdout.flush();
        let _ = stderr.flush();
        std::process::exit(code);
    }

    let mut app = app::App::new(adapters, source_db, ai_configs, ai_client, conn_errors);

    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    enable_raw_mode()?;
    stdout.execute(EnableBracketedPaste)?;
    stdout.execute(EnableMouseCapture)?;
    let _ = stdout.execute(PushKeyboardEnhancementFlags(
        KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            | KeyboardEnhancementFlags::REPORT_EVENT_TYPES,
    ));

    let mut terminal = Terminal::new(ratatui::backend::CrosstermBackend::new(stdout))?;

    let result = app::run_event_loop(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    let mut stdout = io::stdout();
    let _ = stdout.execute(DisableBracketedPaste);
    let _ = stdout.execute(DisableMouseCapture);
    let _ = stdout.execute(PopKeyboardEnhancementFlags);
    stdout.execute(LeaveAlternateScreen)?;

    result
}
