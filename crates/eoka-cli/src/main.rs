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
use eoka_protocol::{manifest_for_operations, OperationCapability, Response, ResponseMeta};
use eoka_tack::EokaToolFilter;
use launch_spec::LaunchSpec;
use serde_json::json;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let agent_mode = cli.agent;
    let json_mode = cli.json || agent_mode;
    if cli.daemon {
        let spec = match launch::resolve_launch_spec(&cli) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[eoka] {}", e);
                std::process::exit(1);
            }
        };
        // The client already passes the suffixed session name (e.g. `foo-live`
        // for connect mode); re-applying effective_session here double-appends
        // the suffix (`foo-live-live`) and the socket never matches what the
        // client waits for.
        if let Err(e) = daemon::run(&cli.session, spec).await {
            eprintln!("[eoka] daemon error: {}", e);
            std::process::exit(1);
        }
        return;
    }
    let spec = match launch::resolve_launch_spec(&cli) {
        Ok(s) => s,
        Err(e) => {
            let response = Response::err(e);
            if json_mode {
                output::print_response(&response, true);
            } else if let Some(error) = response.error {
                eprintln!("Error: {}", error);
            }
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
                    .unwrap_or_else(protocol::Response::err)
                    .with_meta(response_meta(&effective_session, "captcha_solve"));
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
                }))
                .with_meta(response_meta(&effective_session, "status"));
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
        Command::Doctor => {
            let response = doctor_response(&effective_session, &spec);
            output::print_response(&response, json_mode);
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
                }
                .with_meta(response_meta(&effective_session, "kill"));
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
                }
                .with_meta(response_meta(&effective_session, "cdp_url"));
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
            all_tools,
            capabilities,
        } => {
            let filter = match tack_filter(*all_tools, capabilities) {
                Ok(filter) => filter,
                Err(error) => {
                    let response =
                        Response::err(error).with_meta(response_meta(&effective_session, "tack"));
                    output::print_response(&response, json_mode || *raw_json);
                    std::process::exit(1);
                }
            };
            let response = match code::run_code_command(
                &effective_session,
                spec.clone(),
                code.as_deref(),
                file.as_ref().map(|path| path.as_path()),
                *timeout_ms,
                *raw_json,
                filter,
            )
            .await
            {
                Ok(response) => response,
                Err(error) => protocol::Response::err(error),
            }
            .with_meta(response_meta(&effective_session, "tack"));
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
        }
        .with_meta(response_meta(&effective_session, "batch"));
        output::print_response(&response, json_mode);
        if !response.ok {
            std::process::exit(1);
        }
        return;
    }
    let request = command_request::command_to_request(&command, agent_mode);
    let request_cmd = request.cmd();

    let response = match client::send_command(&effective_session, request, spec.clone()).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
    .with_meta(response_meta(&effective_session, request_cmd));

    output::print_response(&response, json_mode);

    if !response.ok {
        std::process::exit(1);
    }
    if matches!(command, Command::Close) {
        let _ = client::kill_daemon(&effective_session);
    }
}

fn response_meta(session: &str, cmd: &str) -> ResponseMeta {
    ResponseMeta {
        session: Some(session.to_string()),
        cmd: Some(cmd.to_string()),
        socket: Some(session::socket_path(session).to_string_lossy().to_string()),
        log: Some(
            session::socket_path(session)
                .with_extension("log")
                .to_string_lossy()
                .to_string(),
        ),
    }
}

fn tack_filter(all_tools: bool, capabilities: &[String]) -> Result<EokaToolFilter, String> {
    if all_tools {
        return Ok(EokaToolFilter::all_non_lifecycle());
    }
    let mut filter = EokaToolFilter::default_agent();
    for capability in capabilities {
        let parsed = OperationCapability::parse(capability).ok_or_else(|| {
            format!(
                "Unknown capability '{}'. Use navigation, observation, interaction, javascript, browser-state, tabs, spa, wasm, network, policy, media, or captcha.",
                capability
            )
        })?;
        filter = filter.include_capability(parsed);
    }
    Ok(filter)
}

fn doctor_response(session: &str, spec: &LaunchSpec) -> Response {
    let socket = session::socket_path(session);
    let pid = session::pid_path(session);
    let log = socket.with_extension("log");
    let cdp = match spec {
        LaunchSpec::Connect { ws_url } => json!({
            "mode": "connect",
            "ws_url": ws_url,
        }),
        LaunchSpec::Launch { headless, .. } => json!({
            "mode": "launch",
            "headless": headless,
        }),
    };
    let chrome_9222 = eoka::cdp::discover::discover_browser_ws("127.0.0.1", 9222).ok();
    Response::ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "session": session,
        "runtime_dir": socket.parent().map(|path| path.to_string_lossy().to_string()),
        "socket": socket.to_string_lossy(),
        "socket_exists": socket.exists(),
        "daemon_running": client::is_daemon_running(session),
        "pid_file": pid.to_string_lossy(),
        "pid_file_exists": pid.exists(),
        "log": log.to_string_lossy(),
        "log_exists": log.exists(),
        "launch": cdp,
        "chrome_9222": chrome_9222,
        "tack_runtime": "quickjs",
        "tools": {
            "default": manifest_for_operations("eoka", false).len(),
            "including_opt_in": manifest_for_operations("eoka", true).len(),
        }
    }))
    .with_meta(response_meta(session, "doctor"))
}

fn parse_port_from_ws(ws_url: &str) -> Option<u16> {
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
