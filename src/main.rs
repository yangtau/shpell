mod auth;
mod compose;
mod config;
mod provider;
mod shell;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "shpell", version, about = "Write shell commands in natural language")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate a shell command from a natural language description
    /// (non-interactive, for scripting)
    Gen {
        /// Target shell the command will run in (defaults to $SHELL)
        #[arg(long, default_value_t = detect_shell())]
        shell: String,
        /// Natural language description of the command
        #[arg(required = true, num_args = 1.., trailing_var_arg = true)]
        query: Vec<String>,
    },
    /// Interactive Shpell mode: type a request, watch the command stream in,
    /// press Enter to accept (used by the shell integration's Tab binding;
    /// also what `shpell [request]` runs)
    Compose {
        /// Target shell the command will run in (defaults to $SHELL)
        #[arg(long, default_value_t = detect_shell())]
        shell: String,
        /// Optional first request, submitted immediately on entry
        #[arg(num_args = 0.., trailing_var_arg = true)]
        query: Vec<String>,
    },
    /// Manage LLM provider credentials
    Auth {
        #[command(subcommand)]
        cmd: AuthCmd,
    },
    /// Print the shell integration script (e.g. `eval "$(shpell init zsh)"`)
    Init {
        /// Shell to integrate with (zsh or bash)
        shell: String,
    },
}

#[derive(Subcommand)]
enum AuthCmd {
    /// Log in with your ChatGPT subscription (OAuth)
    Login,
    /// Remove stored credentials
    Logout,
    /// Show current login status
    Status,
}

const SUBCOMMANDS: &[&str] = &[
    "gen", "compose", "auth", "init", "help", "-h", "--help", "-V", "--version",
];

/// Best-effort shell detection for the `--shell` defaults: the basename of
/// $SHELL, falling back to zsh.
fn detect_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .and_then(|s| s.rsplit('/').next().map(str::to_string))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "zsh".into())
}

fn main() {
    // `shpell [free text]` opens interactive Shpell mode, the free text (if
    // any) submitted as the first request. `gen` stays the explicit
    // non-interactive entry point for scripts.
    let mut args: Vec<String> = std::env::args().collect();
    if args.len() == 1 || !SUBCOMMANDS.contains(&args[1].as_str()) {
        args.insert(1, "compose".into());
    }
    let cli = Cli::parse_from(args);
    if let Err(e) = run(cli) {
        eprintln!("shpell: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.cmd {
        Cmd::Gen { shell, query } => {
            let cfg = config::Config::load()?;
            let provider = provider::from_config(&cfg)?;
            let req = provider::GenRequest {
                query: query.join(" "),
                shell,
                os: std::env::consts::OS.to_string(),
                cwd: std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
            };
            let command = provider.generate(&req, &mut |_| {})?;
            println!("{command}");
        }
        Cmd::Compose { shell, query } => {
            let initial = (!query.is_empty()).then(|| query.join(" "));
            compose::run(&shell, initial)?
        }
        Cmd::Auth { cmd } => match cmd {
            AuthCmd::Login => auth::login()?,
            AuthCmd::Logout => auth::logout()?,
            AuthCmd::Status => auth::status()?,
        },
        Cmd::Init { shell } => print!("{}", shell::init_script(&shell)?),
    }
    Ok(())
}
