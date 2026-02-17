//! MCP-connected Harness operator TUI.

use anyhow::{Result, anyhow, bail};
use harness_tui::TuiRunner;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, PartialEq)]
struct Args {
    server_url: String,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            server_url: "http://127.0.0.1:3000/sse".to_string(),
        }
    }
}

impl Args {
    fn parse() -> Result<Self> {
        Self::parse_from(std::env::args().skip(1))
    }

    fn parse_from<I>(iter: I) -> Result<Self>
    where
        I: IntoIterator<Item = String>,
    {
        let mut args = Self::default();
        let mut it = iter.into_iter();

        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--server-url" => {
                    args.server_url = it
                        .next()
                        .ok_or_else(|| anyhow!("--server-url requires a value"))?;
                }
                "-h" | "--help" => {
                    print_usage();
                    std::process::exit(0);
                }
                _ => bail!("unknown argument: {arg}"),
            }
        }

        Ok(args)
    }
}

fn print_usage() {
    println!(
        "\
Harness MCP Operator TUI

Usage:
  cargo run --bin harness-tui -- [options]

Options:
  --server-url <URL>    MCP SSE endpoint URL (default: http://127.0.0.1:3000/sse)
  -h, --help            Show this help
"
    );
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Args::parse()?;
    let mut runner = TuiRunner::connect(args.server_url).await?;
    runner.run().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_parse_defaults() {
        let args = Args::parse_from(Vec::<String>::new()).expect("parse defaults");
        assert_eq!(args.server_url, "http://127.0.0.1:3000/sse");
    }

    #[test]
    fn args_parse_override_server_url() {
        let args = Args::parse_from(vec![
            "--server-url".to_string(),
            "http://localhost:4000/sse".to_string(),
        ])
        .expect("parse override");

        assert_eq!(args.server_url, "http://localhost:4000/sse");
    }

    #[test]
    fn args_parse_rejects_unknown_flag() {
        let err = Args::parse_from(vec!["--wat".to_string()]).expect_err("must fail");
        assert!(err.to_string().contains("unknown argument"));
    }
}
