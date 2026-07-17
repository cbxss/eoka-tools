mod cli;
mod client;
mod daemon;
mod handler;
mod launch_spec;
mod output;
mod protocol;
mod session;

use clap::Parser;
use cli::{CaptchaAction, Cli, Command, InterceptAction, TabAction, WasmAction};
use launch_spec::LaunchSpec;
use protocol::Request;
use serde_json::json;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Daemon mode (internal — launched by client). The daemon process re-parses
    // the same global flags, so --cdp / --from-profile / --headed are visible here.
    if cli.daemon {
        let spec = match resolve_launch_spec(&cli) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[eoka] {}", e);
                std::process::exit(1);
            }
        };
        if let Err(e) = daemon::run(&effective_session(&cli, &spec), spec).await {
            eprintln!("[eoka] daemon error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    // Resolve LaunchSpec eagerly — invalid --cdp args fail before we touch the daemon.
    let spec = match resolve_launch_spec(&cli) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
    let effective_session = effective_session(&cli, &spec);

    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            use clap::CommandFactory;
            Cli::command().print_help().unwrap();
            println!();
            return;
        }
    };

    // Handle commands that don't need the daemon at all.
    match &command {
        Command::Captcha { action } => {
            let response = match solve_captcha(action).await {
                Ok(value) => protocol::Response::ok(value),
                Err(error) => protocol::Response::err(error),
            };
            output::print_response(&response, cli.json);
            if !response.ok {
                std::process::exit(1);
            }
            return;
        }
        Command::Status => {
            if client::is_daemon_running(&effective_session) {
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
            match client::kill_daemon(&effective_session) {
                Ok(true) => println!("Daemon killed (session={})", effective_session),
                Ok(false) => println!("No daemon running (session={})", effective_session),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
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
            match eoka::cdp::discover::discover_browser_ws("127.0.0.1", p) {
                Ok(url) => println!("{}", url),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
            return;
        }
        _ => {}
    }

    // Build request from command (clone-from is handled inside the daemon so
    // it can hydrate the active session).
    let request = command_to_request(&command);

    let response = match client::send_command(&effective_session, request, spec.clone()).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    output::print_response(&response, cli.json);

    if !response.ok {
        std::process::exit(1);
    }

    // `close` shuts down the daemon too (whether it owns Chrome or not).
    if matches!(command, Command::Close) {
        let _ = client::kill_daemon(&effective_session);
    }
}

async fn solve_captcha(action: &CaptchaAction) -> Result<serde_json::Value, String> {
    let CaptchaAction::Solve {
        captcha_type,
        website_url,
        website_key,
        api_key,
        page_action,
        min_score,
        iv,
        context,
        captcha_script,
        challenge_script,
    } = action;
    let api_key = api_key
        .as_deref()
        .ok_or("Anti-Captcha key required: use --api-key or ANTI_CAPTCHA_KEY")?;
    let solver = captcha::AntiCaptcha::new(api_key);
    let solution = match captcha_type.to_lowercase().as_str() {
        "hcaptcha" => solver.solve_hcaptcha(website_url, website_key).await,
        "recaptcha_v2" => solver.solve_recaptcha_v2(website_url, website_key).await,
        "recaptcha_v3" => solver.solve_recaptcha_v3(website_url, website_key, page_action.as_deref().unwrap_or("submit"), min_score.unwrap_or(0.3)).await,
        "amazon_waf" => solver.solve_amazon_waf(
            website_url, website_key,
            iv.as_deref().ok_or("amazon_waf requires --iv")?,
            context.as_deref().ok_or("amazon_waf requires --context")?,
            captcha_script.as_deref(), challenge_script.as_deref(),
        ).await,
        _ => return Err(format!("Unknown CAPTCHA type '{captcha_type}'. Use hcaptcha, recaptcha_v2, recaptcha_v3, or amazon_waf.")),
    }.map_err(|e| e.to_string())?;
    Ok(json!({ "token": solution.token(), "user_agent": solution.user_agent }))
}

/// Map CLI flags to a `LaunchSpec`. Resolves --cdp ports to ws:// URLs eagerly.
fn resolve_launch_spec(cli: &Cli) -> Result<LaunchSpec, String> {
    if let Some(spec) = &cli.cdp {
        let ws_url = launch_spec::resolve_cdp_spec(spec)?;
        return Ok(LaunchSpec::Connect { ws_url });
    }
    if cli.auto_connect {
        let (_port, ws_url) = launch_spec::auto_connect()?;
        return Ok(LaunchSpec::Connect { ws_url });
    }
    Ok(LaunchSpec::Launch {
        headless: !cli.headed,
        from_profile: cli
            .from_profile
            .as_deref()
            .map(resolve_profile_spec)
            .transpose()?,
        clone_state_from: cli.clone_state_from.clone(),
        no_stealth: cli.no_stealth,
    })
}

fn resolve_profile_spec(spec: &str) -> Result<std::path::PathBuf, String> {
    if spec == "auto" {
        return handler::profile::default_profile_dir()
            .ok_or_else(|| "Could not autodetect Chrome profile dir".to_string());
    }
    let p = std::path::PathBuf::from(spec);
    if !p.exists() {
        return Err(format!("Profile path does not exist: {}", p.display()));
    }
    Ok(p)
}

fn effective_session(cli: &Cli, spec: &LaunchSpec) -> String {
    format!("{}{}", cli.session, launch_spec::session_suffix(spec))
}

fn parse_port_from_ws(ws_url: &str) -> Option<u16> {
    // ws://127.0.0.1:9222/devtools/...
    let after_scheme = ws_url
        .trim_start_matches("ws://")
        .trim_start_matches("wss://");
    let host_port = after_scheme.split('/').next()?;
    host_port.rsplit(':').next()?.parse().ok()
}

fn command_to_request(cmd: &Command) -> Request {
    match cmd {
        // Navigation
        Command::Open {
            url,
            headers,
            user_agent,
            bypass_csp,
            inject_js,
        } => {
            let mut args = json!({ "url": url });
            if let Some(h) = headers {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(h) {
                    args["headers"] = parsed;
                }
            }
            if let Some(ua) = user_agent {
                args["user_agent"] = json!(ua);
            }
            if *bypass_csp {
                args["bypass_csp"] = json!(true);
            }
            if let Some(js) = inject_js {
                args["inject_js"] = json!(js);
            }
            Request {
                cmd: "open".into(),
                args,
            }
        }
        Command::Back => Request {
            cmd: "back".into(),
            args: json!({}),
        },
        Command::Forward => Request {
            cmd: "forward".into(),
            args: json!({}),
        },
        Command::Reload => Request {
            cmd: "reload".into(),
            args: json!({}),
        },

        // Observation
        Command::Snapshot { interactive, all } => Request {
            cmd: "snapshot".into(),
            args: json!({ "interactive": interactive, "all": all }),
        },
        Command::Observe { filter, max } => Request {
            cmd: "observe".into(),
            args: json!({ "filter": filter, "max": max }),
        },
        Command::Screenshot { output, annotate } => Request {
            cmd: "screenshot".into(),
            args: json!({
                "output": output.as_ref().map(|p| p.to_string_lossy().to_string()),
                "annotate": annotate,
            }),
        },
        Command::Emulate {
            width,
            height,
            dpr,
            desktop,
            reset,
        } => Request {
            cmd: "emulate".into(),
            args: json!({
                "width": width,
                "height": height,
                "dpr": dpr,
                "desktop": desktop,
                "reset": reset,
            }),
        },
        Command::Info => Request {
            cmd: "info".into(),
            args: json!({}),
        },
        Command::Text => Request {
            cmd: "text".into(),
            args: json!({}),
        },
        Command::Find { text } => Request {
            cmd: "find".into(),
            args: json!({ "text": text }),
        },

        // Actions
        Command::Click { target } => Request {
            cmd: "click".into(),
            args: json!({ "target": target }),
        },
        Command::DoubleClick { target } => Request {
            cmd: "dblclick".into(),
            args: json!({ "target": target }),
        },
        Command::Fill { target, text } => Request {
            cmd: "fill".into(),
            args: json!({ "target": target, "text": text }),
        },
        Command::Select { target, value } => Request {
            cmd: "select".into(),
            args: json!({ "target": target, "value": value }),
        },
        Command::Hover { target } => Request {
            cmd: "hover".into(),
            args: json!({ "target": target }),
        },
        Command::Key { key } => Request {
            cmd: "key".into(),
            args: json!({ "key": key }),
        },
        Command::Scroll { target } => Request {
            cmd: "scroll".into(),
            args: json!({ "target": target }),
        },

        // JavaScript
        Command::Eval {
            code,
            file,
            no_return,
            max_size,
        } => Request {
            cmd: if *no_return { "exec" } else { "eval" }.into(),
            args: json!({
                "code": code,
                "file": file.as_ref().map(|p| p.to_string_lossy().to_string()),
                "max_size": max_size,
            }),
        },
        Command::Exec { code, file } => Request {
            cmd: "exec".into(),
            args: json!({
                "code": code,
                "file": file.as_ref().map(|p| p.to_string_lossy().to_string()),
            }),
        },

        // Network
        Command::Fetch {
            url,
            method,
            headers,
            body,
            redirect,
            max_body,
        } => {
            let mut args = json!({ "url": url });
            if let Some(m) = method {
                args["method"] = json!(m);
            }
            if let Some(h) = headers {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(h) {
                    args["headers"] = parsed;
                }
            }
            if let Some(b) = body {
                args["body"] = json!(b);
            }
            if let Some(r) = redirect {
                args["redirect"] = json!(r);
            }
            if let Some(mb) = max_body {
                args["max_body"] = json!(mb);
            }
            Request {
                cmd: "fetch".into(),
                args,
            }
        }

        // Cookies
        Command::Cookies => Request {
            cmd: "cookies".into(),
            args: json!({}),
        },
        Command::SetCookie {
            name,
            value,
            domain,
            path,
        } => Request {
            cmd: "set_cookie".into(),
            args: json!({
                "name": name,
                "value": value,
                "domain": domain,
                "path": path,
            }),
        },
        Command::DeleteCookie { name, domain } => Request {
            cmd: "delete_cookie".into(),
            args: json!({ "name": name, "domain": domain }),
        },
        Command::ClearCookies => Request {
            cmd: "clear_cookies".into(),
            args: json!({}),
        },

        // Storage
        Command::Storage {
            key,
            session_storage,
        } => Request {
            cmd: "storage".into(),
            args: json!({ "key": key, "session_storage": session_storage }),
        },
        Command::SetStorage {
            key,
            value,
            session_storage,
        } => Request {
            cmd: "set_storage".into(),
            args: json!({ "key": key, "value": value, "session_storage": session_storage }),
        },
        Command::DumpStorage => Request {
            cmd: "dump_storage".into(),
            args: json!({}),
        },

        // State
        Command::SaveState { path } => Request {
            cmd: "save_state".into(),
            args: json!({ "path": path.to_string_lossy() }),
        },
        Command::LoadState { path, no_navigate } => Request {
            cmd: "load_state".into(),
            args: json!({ "path": path.to_string_lossy(), "no_navigate": no_navigate }),
        },

        // Headers
        Command::Headers { headers_json } => Request {
            cmd: "headers".into(),
            args: json!({ "headers_json": headers_json }),
        },

        // Console/Errors
        Command::Console { clear, level } => Request {
            cmd: "console".into(),
            args: json!({ "clear": clear, "level": level }),
        },
        Command::Errors { clear } => Request {
            cmd: "errors".into(),
            args: json!({ "clear": clear }),
        },

        // Tabs
        Command::Tab { action } => match action {
            TabAction::List => Request {
                cmd: "tab_list".into(),
                args: json!({}),
            },
            TabAction::New { url } => Request {
                cmd: "tab_new".into(),
                args: json!({ "url": url }),
            },
            TabAction::Switch { tab_id } => Request {
                cmd: "tab_switch".into(),
                args: json!({ "tab_id": tab_id }),
            },
            TabAction::Close { tab_id } => Request {
                cmd: "tab_close".into(),
                args: json!({ "tab_id": tab_id }),
            },
            TabAction::Attach { tab_id } => Request {
                cmd: "tab_attach".into(),
                args: json!({ "tab_id": tab_id }),
            },
        },

        // Wait
        Command::Wait {
            ms,
            text,
            url,
            load,
            timeout,
        } => Request {
            cmd: "wait".into(),
            args: json!({
                "ms": ms,
                "text": text,
                "url": url,
                "load": load,
                "timeout": timeout,
            }),
        },

        // Batch
        Command::Batch { json: _, bail: _ } => {
            // Batch reads from stdin — handled specially
            // For now, just pass through
            Request {
                cmd: "batch".into(),
                args: json!({}),
            }
        }

        // SPA
        Command::SpaInfo => Request {
            cmd: "spa_info".into(),
            args: json!({}),
        },
        Command::SpaNavigate { path } => Request {
            cmd: "spa_navigate".into(),
            args: json!({ "path": path }),
        },

        // Fake Camera
        Command::FakeCamera { file, loop_video } => Request {
            cmd: "fake_camera".into(),
            args: json!({
                "file": file.to_string_lossy(),
                "loop_video": loop_video,
            }),
        },

        // WASM
        Command::Wasm { action } => match action {
            WasmAction::Info => Request {
                cmd: "wasm_info".into(),
                args: json!({}),
            },
            WasmAction::Read { addr, len, memory } => Request {
                cmd: "wasm_read".into(),
                args: json!({ "addr": addr, "len": len, "memory": memory }),
            },
            WasmAction::Write { addr, hex, memory } => Request {
                cmd: "wasm_write".into(),
                args: json!({ "addr": addr, "hex": hex, "memory": memory }),
            },
            WasmAction::Find {
                pattern,
                start,
                end,
                max,
                memory,
            } => Request {
                cmd: "wasm_find".into(),
                args: json!({
                    "pattern": pattern,
                    "start": start,
                    "end": end,
                    "max": max,
                    "memory": memory,
                }),
            },
        },

        // Intercept
        Command::Intercept { action } => match action {
            InterceptAction::Add {
                url_pattern,
                capture,
                respond,
                status,
            } => Request {
                cmd: "intercept_add".into(),
                args: json!({
                    "url_pattern": url_pattern,
                    "capture": capture.as_ref().map(|p| p.to_string_lossy().to_string()),
                    "respond": respond.as_ref().map(|p| p.to_string_lossy().to_string()),
                    "status": status,
                }),
            },
            InterceptAction::List => Request {
                cmd: "intercept_list".into(),
                args: json!({}),
            },
            InterceptAction::Remove { id } => Request {
                cmd: "intercept_remove".into(),
                args: json!({ "id": id }),
            },
            InterceptAction::Log { clear } => Request {
                cmd: "intercept_log".into(),
                args: json!({ "clear": clear }),
            },
        },

        // Close
        Command::Close => Request {
            cmd: "close".into(),
            args: json!({}),
        },

        // Clone-from (live snapshot import)
        Command::CloneFrom { source, to } => Request {
            cmd: "clone_from".into(),
            args: json!({
                "source": source,
                "to": to.as_ref().map(|p| p.to_string_lossy().to_string()),
            }),
        },

        // Commands handled before daemon startup.
        Command::Captcha { .. } | Command::Status | Command::Kill | Command::CdpUrl { .. } => {
            unreachable!()
        }
    }
}
