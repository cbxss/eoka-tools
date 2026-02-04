use clap::Parser;
use std::path::PathBuf;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(name = "eoka-runner")]
#[command(about = "Config-based browser automation")]
#[command(version)]
struct Cli {
    /// Config file to run
    config: PathBuf,

    /// Run in headless mode (overrides config)
    #[arg(long)]
    headless: bool,

    /// Set a parameter (can be used multiple times)
    #[arg(short = 'P', long = "param", value_name = "KEY=VALUE")]
    params: Vec<String>,

    /// Verbose output (-v for info, -vv for debug)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Validate config without running
    #[arg(long)]
    check: bool,

    /// Quiet mode (only errors)
    #[arg(short, long)]
    quiet: bool,
}

#[tokio::main]
async fn main() -> eoka_runner::Result<()> {
    let cli = Cli::parse();

    // Set up logging based on verbosity
    let level = if cli.quiet {
        Level::ERROR
    } else {
        match cli.verbose {
            0 => Level::WARN,
            1 => Level::INFO,
            _ => Level::DEBUG,
        }
    };

    FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .compact()
        .init();

    // Parse parameters
    let params = eoka_runner::Params::from_args(&cli.params)?;

    // Load and validate config with parameters
    let mut config = eoka_runner::Config::load_with_params(&cli.config, &params)?;

    if cli.check {
        println!("Config valid: {}", config.name);
        println!("  Target: {}", config.target.url);
        println!("  Actions: {}", config.actions.len());
        if !config.params.is_empty() {
            println!("  Parameters: {}", config.params.len());
            for (name, def) in &config.params {
                let req = if def.required { " (required)" } else { "" };
                let desc = def.description.as_deref().unwrap_or("");
                println!("    - {}{}: {}", name, req, desc);
            }
        }
        if let Some(ref success) = config.success {
            let count = success.any.as_ref().map(|v| v.len()).unwrap_or(0)
                + success.all.as_ref().map(|v| v.len()).unwrap_or(0);
            println!("  Success conditions: {}", count);
        }
        if let Some(ref on_failure) = config.on_failure {
            if let Some(ref retry) = on_failure.retry {
                println!("  Retry attempts: {}", retry.attempts);
            }
        }
        return Ok(());
    }

    // Override headless if specified
    if cli.headless {
        config.browser.headless = true;
    }

    println!("Running: {}", config.name);

    // Get base path for resolving includes (directory containing the config file)
    let base_path = cli
        .config
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));

    let mut runner = eoka_runner::Runner::new(&config.browser).await?;
    let result = runner.run_with_base_path(&config, base_path).await?;

    // Print result
    println!();
    if result.success {
        println!("✓ Success");
    } else {
        println!("✗ Failed");
        if let Some(ref error) = result.error {
            println!("  Error: {}", error);
        }
    }
    println!("  Actions: {}", result.actions_executed);
    println!("  Duration: {}ms", result.duration_ms);
    if result.retries > 0 {
        println!("  Retries: {}", result.retries);
    }

    runner.close().await?;

    if !result.success {
        std::process::exit(1);
    }

    Ok(())
}
