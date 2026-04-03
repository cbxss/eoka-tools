mod cli;
mod client;
mod daemon;
mod handler;
mod output;
mod protocol;
mod session;

use clap::Parser;
use cli::{Cli, Command, InterceptAction, TabAction, WasmAction};
use protocol::Request;
use serde_json::json;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Daemon mode (internal — launched by client)
    if cli.daemon {
        if let Err(e) = daemon::run(&cli.session, !cli.headed).await {
            eprintln!("[eoka] daemon error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            // No command — print help
            use clap::CommandFactory;
            Cli::command().print_help().unwrap();
            println!();
            return;
        }
    };

    // Handle local commands (no daemon needed)
    match &command {
        Command::Status => {
            if client::is_daemon_running(&cli.session) {
                println!("Daemon running (session={})", cli.session);
                println!("Socket: {}", session::socket_path(&cli.session).display());
            } else {
                println!("No daemon running (session={})", cli.session);
            }
            return;
        }
        Command::Kill => {
            match client::kill_daemon(&cli.session) {
                Ok(true) => println!("Daemon killed (session={})", cli.session),
                Ok(false) => println!("No daemon running (session={})", cli.session),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
            return;
        }
        _ => {}
    }

    // Build request from command
    let request = command_to_request(&command);

    // Send to daemon (auto-launches if needed)
    let headless = !cli.headed;
    let response = match client::send_command(&cli.session, request, headless).await {
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

    // If close command succeeded, also kill daemon
    if matches!(command, Command::Close) {
        let _ = client::kill_daemon(&cli.session);
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
        Command::Eval { code, file, no_return, max_size } => Request {
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

        // Status/Kill handled above
        Command::Status | Command::Kill => unreachable!(),
    }
}
