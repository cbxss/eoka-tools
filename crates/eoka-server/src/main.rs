mod dispatch;
mod methods;
mod protocol;
mod state;

use serde_json::Value;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

use protocol::{Request, Response, ServerError};
use state::AppState;

enum ParsedLine {
    Request(Request),
    Malformed { id: Value, error: ServerError },
    Skip,
}

fn parse_line(line: &str) -> ParsedLine {
    let value: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, line, "unparseable input line, skipping");
            return ParsedLine::Skip;
        }
    };
    match serde_json::from_value::<Request>(value.clone()) {
        Ok(req) => ParsedLine::Request(req),
        Err(e) => match value.get("id").cloned() {
            Some(id) => ParsedLine::Malformed {
                id,
                error: ServerError::invalid_params(e.to_string()),
            },
            None => {
                tracing::warn!(error = %e, line, "request has no id, skipping");
                ParsedLine::Skip
            }
        },
    }
}

async fn write_response(stdout: &mut io::Stdout, response: &Response) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec(response)?;
    bytes.push(b'\n');
    stdout.write_all(&bytes).await?;
    stdout.flush().await?;
    Ok(())
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let mut lines = BufReader::new(io::stdin()).lines();
    let mut stdout = io::stdout();
    let mut state = AppState::new();

    loop {
        let line = match lines.next_line().await? {
            Some(line) => line,
            // PROTOCOL.md: stdin EOF is the shutdown signal; exit non-zero
            // since it wasn't a `browser.close`-initiated shutdown.
            None => {
                tracing::info!("stdin closed, exiting");
                anyhow::bail!("stdin closed before browser.close");
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let request = match parse_line(&line) {
            ParsedLine::Skip => continue,
            ParsedLine::Malformed { id, error } => {
                write_response(&mut stdout, &Response::err(id, error)).await?;
                continue;
            }
            ParsedLine::Request(req) => req,
        };

        let is_close = request.method == "browser.close";
        let response = match dispatch::dispatch(&mut state, &request.method, request.params).await {
            Ok(result) => Response::ok(request.id, result),
            Err(error) => Response::err(request.id, error),
        };
        let succeeded = !response.is_error();

        write_response(&mut stdout, &response).await?;

        if is_close && succeeded {
            break;
        }
    }

    Ok(())
}
