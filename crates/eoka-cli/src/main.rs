mod batch;
mod captcha_cmd;
mod cli;
mod client;
mod code;
mod command_request;
mod daemon;
mod handler;
mod launch;
mod launch_spec;
mod output;
mod protocol;
mod session;
mod sessions;

use clap::Parser;
use cli::{CaptchaAction, Cli, Command, ToolsAction};
use eoka_protocol::manifest_for_operations;
use launch_spec::LaunchSpec;
use serde_json::json;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Daemon mode (internal — launched by client). The daemon process re-parses
    // the same global flags, so --cdp / --from-profile / --headed are visible here.
    if cli.daemon {
        let spec = match launch::resolve_launch_spec(&cli) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[eoka] {}", e);
                std::process::exit(1);
            }
        };
        if let Err(e) = daemon::run(&launch::effective_session(&cli, &spec), spec).await {
            eprintln!("[eoka] daemon error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    // Resolve LaunchSpec eagerly — invalid --cdp args fail before we touch the daemon.
    let spec = match launch::resolve_launch_spec(&cli) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
    let effective_session = launch::effective_session(&cli, &spec);

    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            use clap::CommandFactory;
            if let Err(e) = Cli::command().print_help() {
                eprintln!("Error: failed to print help: {}", e);
                std::process::exit(1);
            }
            println!();
            return;
        }
    };
    let json_mode = cli.json;

    // Handle commands that don't need the daemon at all.
    match &command {
        Command::Sessions => {
            sessions::print_sessions(json_mode);
            return;
        }
        Command::Captcha {
            action: action @ CaptchaAction::Solve(_),
        } => {
            let response =
                captcha_cmd::solve_captcha_command(action, &effective_session, spec.clone())
                    .await
                    .unwrap_or_else(protocol::Response::err);
            output::print_response(&response, json_mode);
            if !response.ok {
                std::process::exit(1);
            }
            return;
        }
        Command::Status => {
            let running = client::is_daemon_running(&effective_session);
            if json_mode {
                let response = protocol::Response::ok(json!({
                    "running": running,
                    "session": effective_session,
                    "socket": session::socket_path(&effective_session).to_string_lossy(),
                }));
                output::print_response(&response, true);
            } else if running {
                println!("Daemon running (session={})", effective_session);
                println!(
                    "Socket: {}",
                    session::socket_path(&effective_session).display()
                );
            } else {
                println!("No daemon running (session={})", effective_session);
            }
            return;
        }
        Command::Kill => {
            let result = client::kill_daemon(&effective_session);
            if json_mode {
                let response = match &result {
                    Ok(killed) => protocol::Response::ok(json!({
                        "killed": killed,
                        "session": effective_session,
                    })),
                    Err(e) => protocol::Response::err(e.to_string()),
                };
                output::print_response(&response, true);
            } else {
                match &result {
                    Ok(true) => println!("Daemon killed (session={})", effective_session),
                    Ok(false) => println!("No daemon running (session={})", effective_session),
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            if result.is_err() {
                std::process::exit(1);
            }
            return;
        }
        Command::CdpUrl { port } => {
            let p = port
                .or_else(|| match &spec {
                    LaunchSpec::Connect { ws_url } => parse_port_from_ws(ws_url),
                    _ => None,
                })
                .unwrap_or(9222);
            let result = eoka::cdp::discover::discover_browser_ws("127.0.0.1", p);
            if json_mode {
                let response = match &result {
                    Ok(url) => protocol::Response::ok_text(url.clone()),
                    Err(e) => protocol::Response::err(e.to_string()),
                };
                output::print_response(&response, true);
            } else {
                match &result {
                    Ok(url) => println!("{}", url),
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            if result.is_err() {
                std::process::exit(1);
            }
            return;
        }
        Command::Tack {
            code,
            file,
            timeout_ms,
            raw_json,
        } => {
            let response = match code::run_code_command(
                &effective_session,
                spec.clone(),
                code.as_deref(),
                file.as_ref().map(|path| path.as_path()),
                *timeout_ms,
                *raw_json,
            )
            .await
            {
                Ok(response) => response,
                Err(error) => protocol::Response::err(error),
            };
            if *raw_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&response).unwrap_or_default()
                );
            } else {
                output::print_response(&response, json_mode);
            }
            if !response.ok {
                std::process::exit(1);
            }
            return;
        }
        Command::Tools {
            action: ToolsAction::Manifest { all, json },
        } => {
            print_tools_manifest(*all, *json || json_mode);
            return;
        }
        _ => {}
    }

    if let Command::Batch { input, file, bail } = &command {
        let response = match batch::run_batch(
            &effective_session,
            spec.clone(),
            input.as_deref(),
            file.as_ref(),
            *bail,
        )
        .await
        {
            Ok(resp) => resp,
            Err(error) => protocol::Response::err(error),
        };
        output::print_response(&response, json_mode);
        if !response.ok {
            std::process::exit(1);
        }
        return;
    }

    // Build request from command (clone-from is handled inside the daemon so
    // it can hydrate the active session).
    let request = command_request::command_to_request(&command);

    let response = match client::send_command(&effective_session, request, spec.clone()).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    output::print_response(&response, json_mode);

    if !response.ok {
        std::process::exit(1);
    }

    // `close` shuts down the daemon too (whether it owns Chrome or not).
    if matches!(command, Command::Close) {
        let _ = client::kill_daemon(&effective_session);
    }
}

fn parse_port_from_ws(ws_url: &str) -> Option<u16> {
    // ws://127.0.0.1:9222/devtools/...
    let after_scheme = ws_url
        .trim_start_matches("ws://")
        .trim_start_matches("wss://");
    let host_port = after_scheme.split('/').next()?;
    host_port.rsplit(':').next()?.parse().ok()
}

fn print_tools_manifest(include_opt_in: bool, json_mode: bool) {
    let manifest = manifest_for_operations("eoka", include_opt_in);
    if json_mode {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "tools": manifest })).unwrap_or_default()
        );
        return;
    }
    for tool in manifest {
        println!("{}\t{}", tool.path, tool.description);
    }
}
