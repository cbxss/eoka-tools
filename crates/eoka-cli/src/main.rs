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
use serde_json::{json, Value};
use std::io::Read;

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
    let json_mode = cli.json;

    // Handle commands that don't need the daemon at all.
    match &command {
        Command::Captcha {
            action: action @ CaptchaAction::Solve { .. },
        } => {
            let response = solve_captcha_command(action, &effective_session, spec.clone())
                .await
                .unwrap_or_else(protocol::Response::err);
            output::print_response(&response, json_mode);
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

    if let Command::Batch { input, file, bail } = &command {
        let response = match run_batch(
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
    let request = command_to_request(&command);

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

async fn solve_captcha_command(
    action: &CaptchaAction,
    session_name: &str,
    spec: LaunchSpec,
) -> Result<protocol::Response, String> {
    let mut value = solve_captcha(action).await?;
    let CaptchaAction::Solve {
        captcha_type,
        inject,
        inject_callback,
        click_after,
        ..
    } = action
    else {
        unreachable!("only solve actions reach solve_captcha_command")
    };

    if !inject {
        return Ok(protocol::Response::ok(value));
    }

    let token = captcha_solution_token(&value)?.to_string();
    let inject_type = captcha_inject_kind_from_solve(captcha_type)?;
    let response = client::send_command(
        session_name,
        captcha_inject_request(
            &token,
            inject_type,
            inject_callback.as_deref(),
            click_after.as_deref(),
        ),
        spec,
    )
    .await
    .map_err(|e| e.to_string())?;

    if !response.ok {
        return Err(format!(
            "Captcha solved but injection failed: {}",
            response.error.unwrap_or_else(|| "unknown error".into())
        ));
    }

    if let Value::Object(ref mut object) = value {
        object.insert("injected".into(), response.data.unwrap_or(Value::Null));
    }
    Ok(protocol::Response::ok(value))
}

async fn solve_captcha(action: &CaptchaAction) -> Result<serde_json::Value, String> {
    let CaptchaAction::Solve {
        captcha_type,
        website_url,
        website_key,
        api_key,
        page_action,
        min_score,
        enterprise_payload,
        api_domain,
        iv,
        context,
        captcha_script,
        challenge_script,
        inject: _,
        inject_callback: _,
        click_after: _,
    } = action
    else {
        unreachable!("only solve actions reach solve_captcha")
    };
    let api_key = api_key
        .as_deref()
        .ok_or("Anti-Captcha key required: use --api-key or ANTI_CAPTCHA_KEY")?;
    let solver = captcha::AntiCaptcha::new(api_key);
    let solution = match captcha_type.to_lowercase().as_str() {
        "hcaptcha" => solver.solve_hcaptcha(website_url, website_key).await,
        "recaptcha_v2" => solver.solve_recaptcha_v2(website_url, website_key).await,
        "recaptcha_v2_enterprise" => {
            let enterprise_payload = enterprise_payload
                .as_deref()
                .map(|payload| {
                    serde_json::from_str(payload)
                        .map_err(|error| format!("invalid --enterprise-payload JSON: {error}"))
                })
                .transpose()?;
            solver
                .solve_recaptcha_v2_enterprise(
                    website_url,
                    website_key,
                    enterprise_payload,
                    api_domain.as_deref(),
                )
                .await
        }
        "recaptcha_v3" => solver.solve_recaptcha_v3(website_url, website_key, page_action.as_deref().unwrap_or("submit"), min_score.unwrap_or(0.3)).await,
        "amazon_waf" => solver.solve_amazon_waf(
            website_url, website_key,
            iv.as_deref().ok_or("amazon_waf requires --iv")?,
            context.as_deref().ok_or("amazon_waf requires --context")?,
            captcha_script.as_deref(), challenge_script.as_deref(),
        ).await,
        _ => return Err(format!("Unknown CAPTCHA type '{captcha_type}'. Use hcaptcha, recaptcha_v2, recaptcha_v2_enterprise, recaptcha_v3, or amazon_waf.")),
    }.map_err(|e| e.to_string())?;
    Ok(json!({ "token": solution.token(), "user_agent": solution.user_agent }))
}

fn captcha_solution_token(value: &Value) -> Result<&str, String> {
    value
        .get("token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| "Captcha solution contained no token to inject".into())
}

fn captcha_inject_kind_from_solve(captcha_type: &str) -> Result<&'static str, String> {
    match captcha_type.to_lowercase().as_str() {
        "hcaptcha" => Ok("hcaptcha"),
        "recaptcha" | "recaptcha_v2" | "recaptcha_v2_enterprise" | "recaptcha_v3" | "recaptcha_enterprise" => Ok("recaptcha"),
        other => Err(format!(
            "--inject is not supported for captcha type '{other}'. Use hcaptcha, recaptcha_v2, or recaptcha_v3."
        )),
    }
}

fn captcha_inject_request(
    token: &str,
    captcha_type: &str,
    callback: Option<&str>,
    click_after: Option<&str>,
) -> Request {
    Request {
        cmd: "captcha_inject".into(),
        args: json!({
            "token": token,
            "captcha_type": captcha_type,
            "callback": callback,
            "click_after": click_after,
        }),
    }
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

async fn run_batch(
    session_name: &str,
    spec: LaunchSpec,
    input: Option<&str>,
    file: Option<&std::path::PathBuf>,
    bail: bool,
) -> Result<protocol::Response, String> {
    let source = read_batch_source(input, file)?;
    let requests = parse_batch_requests(&source)?;
    let mut responses = Vec::with_capacity(requests.len());
    let mut first_error = None;
    let mut shutdown_daemon = false;

    for request in requests {
        let request_cmd = request.cmd.clone();
        let response = client::send_command(session_name, request, spec.clone())
            .await
            .map_err(|e| e.to_string())?;
        if !response.ok && first_error.is_none() {
            first_error = response.error.clone();
        }
        let step = batch_step_effect(&request_cmd, &response, bail);
        shutdown_daemon |= step.shutdown_daemon;
        responses.push(response);
        if step.stop {
            break;
        }
    }

    if shutdown_daemon {
        let _ = client::kill_daemon(session_name);
    }

    let all_ok = first_error.is_none();
    let data = serde_json::to_value(responses).map_err(|e| e.to_string())?;
    Ok(protocol::Response {
        ok: all_ok,
        data: Some(data),
        error: first_error,
    })
}

fn read_batch_source(
    input: Option<&str>,
    file: Option<&std::path::PathBuf>,
) -> Result<String, String> {
    if let Some(path) = file {
        return std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read batch file '{}': {}", path.display(), e));
    }
    if let Some(input) = input {
        return Ok(input.to_string());
    }

    let mut source = String::new();
    std::io::stdin()
        .read_to_string(&mut source)
        .map_err(|e| format!("Failed to read batch JSON from stdin: {}", e))?;
    if source.trim().is_empty() {
        return Err("Batch JSON is empty. Pass JSON as an argument, via --file, or stdin.".into());
    }
    Ok(source)
}

fn parse_batch_requests(source: &str) -> Result<Vec<Request>, String> {
    let value: Value =
        serde_json::from_str(source).map_err(|e| format!("Invalid batch JSON: {}", e))?;
    let steps = match value {
        Value::Array(steps) => steps,
        other => vec![other],
    };

    steps
        .into_iter()
        .enumerate()
        .map(|(idx, step)| batch_step_to_request(idx, step))
        .collect()
}

fn batch_step_to_request(idx: usize, step: Value) -> Result<Request, String> {
    let mut obj = match step {
        Value::Object(obj) => obj,
        _ => return Err(format!("Batch step {} must be an object", idx + 1)),
    };

    if let Some(cmd) = obj.remove("cmd") {
        let cmd = cmd
            .as_str()
            .ok_or_else(|| format!("Batch step {} field 'cmd' must be a string", idx + 1))?;
        let cmd = normalize_batch_cmd(cmd);
        let args = obj.remove("args").unwrap_or_else(|| json!({}));
        return Ok(Request {
            args: normalize_batch_args(idx, &cmd, args)?,
            cmd,
        });
    }

    if obj.len() != 1 {
        return Err(format!(
            "Batch step {} must contain either {{\"cmd\":...}} or exactly one shorthand command",
            idx + 1
        ));
    }

    let (cmd, value) = obj.into_iter().next().expect("len checked");
    let cmd = normalize_batch_cmd(&cmd);
    let args = normalize_batch_args(idx, &cmd, value)?;
    Ok(Request { cmd, args })
}

fn normalize_batch_cmd(cmd: &str) -> String {
    match cmd.replace('-', "_").as_str() {
        "double_click" => "dblclick".into(),
        "set_cookie" => "set_cookie".into(),
        "delete_cookie" => "delete_cookie".into(),
        "clear_cookies" => "clear_cookies".into(),
        "set_storage" => "set_storage".into(),
        "dump_storage" => "dump_storage".into(),
        "save_state" => "save_state".into(),
        "load_state" => "load_state".into(),
        "captcha_inject" => "captcha_inject".into(),
        "spa_info" => "spa_info".into(),
        "spa_navigate" => "spa_navigate".into(),
        "fake_camera" => "fake_camera".into(),
        other => other.into(),
    }
}

fn normalize_batch_args(idx: usize, cmd: &str, value: Value) -> Result<Value, String> {
    if value.is_null() {
        return Ok(json!({}));
    }
    if value.is_object() {
        return Ok(value);
    }

    match cmd {
        "open" | "fetch" => string_arg(idx, cmd, value, "url"),
        "eval" | "exec" => string_arg(idx, cmd, value, "code"),
        "click" | "dblclick" | "hover" | "scroll" => string_arg(idx, cmd, value, "target"),
        "key" => string_arg(idx, cmd, value, "key"),
        "find" => string_arg(idx, cmd, value, "text"),
        "captcha_inject" => string_arg(idx, cmd, value, "token"),
        "spa_navigate" => string_arg(idx, cmd, value, "path"),
        "tab_new" => string_arg(idx, cmd, value, "url"),
        "tab_switch" | "tab_close" | "tab_attach" => string_arg(idx, cmd, value, "tab_id"),
        "wait" => wait_arg(idx, value),
        "fill" => pair_arg(idx, cmd, value, "target", "text"),
        "select" => pair_arg(idx, cmd, value, "target", "value"),
        _ => Err(format!(
            "Batch step {} command '{}' requires object args",
            idx + 1,
            cmd
        )),
    }
}

fn string_arg(idx: usize, cmd: &str, value: Value, key: &str) -> Result<Value, String> {
    value.as_str().map(|v| json!({ key: v })).ok_or_else(|| {
        format!(
            "Batch step {} command '{}' requires string or object args",
            idx + 1,
            cmd
        )
    })
}

fn wait_arg(idx: usize, value: Value) -> Result<Value, String> {
    if let Some(ms) = value.as_u64() {
        return Ok(json!({ "ms": ms }));
    }
    if let Some(text) = value.as_str() {
        return Ok(json!({ "text": text }));
    }
    Err(format!(
        "Batch step {} command 'wait' requires milliseconds, text, or object args",
        idx + 1
    ))
}

fn pair_arg(
    idx: usize,
    cmd: &str,
    value: Value,
    first_key: &str,
    second_key: &str,
) -> Result<Value, String> {
    let values = value.as_array().ok_or_else(|| {
        format!(
            "Batch step {} command '{}' requires [\"{}\", \"{}\"] or object args",
            idx + 1,
            cmd,
            first_key,
            second_key
        )
    })?;
    if values.len() != 2 {
        return Err(format!(
            "Batch step {} command '{}' requires exactly two array values",
            idx + 1,
            cmd
        ));
    }
    let first = values[0].as_str().ok_or_else(|| {
        format!(
            "Batch step {} command '{}' first array value must be a string",
            idx + 1,
            cmd
        )
    })?;
    let second = values[1].as_str().ok_or_else(|| {
        format!(
            "Batch step {} command '{}' second array value must be a string",
            idx + 1,
            cmd
        )
    })?;
    Ok(json!({ first_key: first, second_key: second }))
}

struct BatchStepEffect {
    stop: bool,
    shutdown_daemon: bool,
}

fn batch_step_effect(cmd: &str, response: &protocol::Response, bail: bool) -> BatchStepEffect {
    let shutdown_daemon = cmd == "close" && response.ok;
    BatchStepEffect {
        stop: shutdown_daemon || (bail && !response.ok),
        shutdown_daemon,
    }
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
            load_state,
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
            if let Some(path) = load_state {
                args["load_state"] = json!(path.to_string_lossy());
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
            body_only,
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
            if *body_only {
                args["body_only"] = json!(true);
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
        Command::Batch {
            input: _,
            file: _,
            bail: _,
        } => {
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

        // CAPTCHA
        Command::Captcha {
            action:
                CaptchaAction::Inject {
                    token,
                    captcha_type,
                    callback,
                    click_after,
                },
        } => captcha_inject_request(
            token,
            captcha_type,
            callback.as_deref(),
            click_after.as_deref(),
        ),

        // Commands handled before daemon startup.
        Command::Captcha {
            action: CaptchaAction::Solve { .. },
        }
        | Command::Status
        | Command::Kill
        | Command::CdpUrl { .. } => {
            unreachable!()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed_command(args: &[&str]) -> (Cli, Command) {
        let mut cli = Cli::try_parse_from(args).unwrap();
        let command = cli.command.take().unwrap();
        (cli, command)
    }

    #[test]
    fn click_accepts_bracketed_observe_target_at_cli_layer() {
        let (_cli, command) = parsed_command(&["eoka", "click", "[38]"]);
        let request = command_to_request(&command);

        assert_eq!(request.cmd, "click");
        assert_eq!(request.args["target"], "[38]");
    }

    #[test]
    fn fetch_raw_sets_body_only_and_preserves_max_body() {
        let (_cli, command) = parsed_command(&[
            "eoka",
            "fetch",
            "https://example.com/app.js",
            "--raw",
            "--max-body",
            "16",
        ]);
        let request = command_to_request(&command);

        assert_eq!(request.cmd, "fetch");
        assert_eq!(request.args["body_only"], true);
        assert_eq!(request.args["max_body"], 16);
    }

    #[test]
    fn open_load_state_maps_to_daemon_request() {
        let (_cli, command) = parsed_command(&[
            "eoka",
            "open",
            "/camping/campsites/71576",
            "--load-state",
            "auth.json",
        ]);
        let request = command_to_request(&command);

        assert_eq!(request.cmd, "open");
        assert_eq!(request.args["url"], "/camping/campsites/71576");
        assert_eq!(request.args["load_state"], "auth.json");
    }

    #[test]
    fn captcha_inject_maps_to_daemon_request() {
        let (_cli, command) = parsed_command(&[
            "eoka",
            "captcha",
            "inject",
            "token-123",
            "--captcha-type",
            "recaptcha",
            "--callback",
            "window.onCaptcha",
            "--click-after",
            "text:Continue Booking",
        ]);
        let request = command_to_request(&command);

        assert_eq!(request.cmd, "captcha_inject");
        assert_eq!(request.args["token"], "token-123");
        assert_eq!(request.args["captcha_type"], "recaptcha");
        assert_eq!(request.args["callback"], "window.onCaptcha");
        assert_eq!(request.args["click_after"], "text:Continue Booking");
    }

    #[test]
    fn captcha_solve_accepts_inject_flags() {
        let (_cli, command) = parsed_command(&[
            "eoka",
            "captcha",
            "solve",
            "--captcha-type",
            "recaptcha_v3",
            "--website-url",
            "https://example.com",
            "--website-key",
            "site-key",
            "--api-key",
            "api-key",
            "--inject",
            "--inject-callback",
            "window.onCaptcha",
            "--click-after",
            "text:Continue Booking",
        ]);

        match command {
            Command::Captcha {
                action:
                    CaptchaAction::Solve {
                        inject,
                        inject_callback,
                        click_after,
                        ..
                    },
            } => {
                assert!(inject);
                assert_eq!(inject_callback.as_deref(), Some("window.onCaptcha"));
                assert_eq!(click_after.as_deref(), Some("text:Continue Booking"));
            }
            _ => panic!("expected captcha solve command"),
        }
    }

    #[test]
    fn captcha_solve_inject_kind_is_limited_to_browser_token_types() {
        assert_eq!(
            captcha_inject_kind_from_solve("recaptcha_v2").unwrap(),
            "recaptcha"
        );
        assert_eq!(
            captcha_inject_kind_from_solve("recaptcha_v3").unwrap(),
            "recaptcha"
        );
        assert_eq!(
            captcha_inject_kind_from_solve("recaptcha_v2_enterprise").unwrap(),
            "recaptcha"
        );
        assert_eq!(
            captcha_inject_kind_from_solve("hcaptcha").unwrap(),
            "hcaptcha"
        );
        assert!(captcha_inject_kind_from_solve("amazon_waf").is_err());
    }

    #[test]
    fn captcha_solution_token_rejects_missing_token() {
        assert_eq!(
            captcha_solution_token(&json!({ "token": "abc" })).unwrap(),
            "abc"
        );
        assert!(captcha_solution_token(&json!({ "token": null })).is_err());
        assert!(captcha_solution_token(&json!({ "token": "" })).is_err());
    }

    #[test]
    fn batch_cli_accepts_inline_json_argument() {
        let (_cli, command) = parsed_command(&["eoka", "batch", r#"[{"info":null}]"#]);

        match command {
            Command::Batch { input, file, bail } => {
                assert_eq!(input.as_deref(), Some(r#"[{"info":null}]"#));
                assert!(file.is_none());
                assert!(!bail);
            }
            _ => panic!("expected batch command"),
        }
    }

    #[test]
    fn batch_shorthand_matches_recreation_flow_shape() {
        let requests = parse_batch_requests(
            r#"[
                {"open":"https://www.recreation.gov/camping/campsites/71576?start_date=2026-08-02&end_date=2026-08-03"},
                {"wait":3000},
                {"eval":"\"patched\""},
                {"click":"Add to Cart"},
                {"wait":5000},
                {"info":null}
            ]"#,
        )
        .unwrap();

        assert_eq!(requests.len(), 6);
        assert_eq!(requests[0].cmd, "open");
        assert_eq!(
            requests[0].args["url"],
            "https://www.recreation.gov/camping/campsites/71576?start_date=2026-08-02&end_date=2026-08-03"
        );
        assert_eq!(requests[1].cmd, "wait");
        assert_eq!(requests[1].args["ms"], 3000);
        assert_eq!(requests[2].cmd, "eval");
        assert_eq!(requests[2].args["code"], "\"patched\"");
        assert_eq!(requests[3].cmd, "click");
        assert_eq!(requests[3].args["target"], "Add to Cart");
        assert_eq!(requests[5].cmd, "info");
        assert_eq!(requests[5].args, json!({}));
    }

    #[test]
    fn batch_accepts_canonical_cmd_args_steps() {
        let requests =
            parse_batch_requests(r#"[{"cmd":"eval","args":{"code":"window.location.href"}}]"#)
                .unwrap();

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].cmd, "eval");
        assert_eq!(requests[0].args["code"], "window.location.href");
    }

    #[test]
    fn batch_canonical_hyphen_aliases_use_normalized_args() {
        let requests = parse_batch_requests(r#"[{"cmd":"spa-navigate","args":"/cart"}]"#).unwrap();

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].cmd, "spa_navigate");
        assert_eq!(requests[0].args["path"], "/cart");
    }

    #[test]
    fn batch_accepts_captcha_inject_shorthand() {
        let requests = parse_batch_requests(r#"[{"captcha-inject":"token-123"}]"#).unwrap();

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].cmd, "captcha_inject");
        assert_eq!(requests[0].args["token"], "token-123");
    }

    #[test]
    fn batch_close_stops_and_marks_daemon_for_shutdown() {
        let effect = batch_step_effect(
            "close",
            &protocol::Response::ok_text("Browser closed"),
            false,
        );

        assert!(effect.stop);
        assert!(effect.shutdown_daemon);
    }

    #[test]
    fn batch_bail_still_stops_on_error_without_daemon_shutdown() {
        let effect = batch_step_effect("click", &protocol::Response::err("not found"), true);

        assert!(effect.stop);
        assert!(!effect.shutdown_daemon);
    }

    #[test]
    fn batch_rejects_ambiguous_shorthand_step() {
        let err = parse_batch_requests(r#"[{"click":"A","eval":"B"}]"#).unwrap_err();

        assert!(err.contains("exactly one shorthand command"));
    }

    #[test]
    fn batch_rejects_scalar_step() {
        let err = parse_batch_requests(r#"["click Add"]"#).unwrap_err();

        assert!(err.contains("must be an object"));
    }

    #[test]
    fn standard_intercept_subcommand_accepts_trailing_json_flag() {
        let (cli, command) = parsed_command(&["eoka", "intercept", "add", "*api*", "--json"]);
        let request = command_to_request(&command);

        assert!(cli.json);
        assert_eq!(request.cmd, "intercept_add");
        assert_eq!(request.args["url_pattern"], "*api*");
    }

    #[test]
    fn deprecated_json_intercept_spec_is_rejected() {
        let err = match Cli::try_parse_from([
            "eoka",
            "intercept",
            r#"{"urlPattern":"*api*","action":"log"}"#,
            "--json",
        ]) {
            Ok(_) => panic!("deprecated JSON intercept spec should be rejected"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("unrecognized subcommand"));
    }
}
