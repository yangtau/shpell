mod auth;
mod config;
mod provider;
mod shell;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "x", version, about = "Write shell commands in natural language")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate a shell command from a natural language description
    Gen {
        /// Target shell the command will run in
        #[arg(long, default_value = "zsh")]
        shell: String,
        /// Natural language description of the command
        #[arg(required = true, num_args = 1.., trailing_var_arg = true)]
        query: Vec<String>,
    },
    /// Manage LLM provider credentials
    Auth {
        #[command(subcommand)]
        cmd: AuthCmd,
    },
    /// Print the shell integration script (e.g. `eval "$(x init zsh)"`)
    Init {
        /// Shell to integrate with (currently only: zsh)
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

const SUBCOMMANDS: &[&str] = &["gen", "auth", "init", "help", "-h", "--help", "-V", "--version"];

fn main() {
    // `x <free text>` is shorthand for `x gen <free text>`.
    let mut args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && !SUBCOMMANDS.contains(&args[1].as_str()) {
        args.insert(1, "gen".into());
    }
    let cli = Cli::parse_from(args);
    if let Err(e) = run(cli) {
        eprintln!("x: {e:#}");
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
            let command = provider.generate(&req)?;
            println!("{command}");
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
