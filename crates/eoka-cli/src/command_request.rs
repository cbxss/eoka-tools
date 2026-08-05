//! Maps a parsed `Command` to the daemon-facing `Request` envelope.

use serde_json::json;

use crate::captcha_cmd::captcha_inject_request;
use crate::cli::{CaptchaAction, Command, InterceptAction, TabAction, WasmAction};
use crate::protocol::Request;

/// Parse a `--headers` JSON string, exiting with a clear error on malformed
/// input instead of silently sending the request with no headers.
fn parse_headers_json(raw: &str) -> serde_json::Value {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: invalid --headers JSON: {}", e);
            std::process::exit(1);
        }
    }
}

pub(crate) fn command_to_request(cmd: &Command) -> Request {
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
                args["headers"] = parse_headers_json(h);
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
                args["headers"] = parse_headers_json(h);
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

        // Commands handled before daemon startup — main() returns early for
        // these (see the early-dispatch match and the `Command::Batch` check
        // above it), so this function is never actually called for them. If
        // this panics, main()'s early-dispatch match no longer intercepts
        // this variant — fix it there rather than adding a Request arm here.
        Command::Captcha {
            action: CaptchaAction::Solve(_),
        }
        | Command::Sessions
        | Command::Status
        | Command::Kill
        | Command::CdpUrl { .. }
        | Command::Batch { .. } => {
            unreachable!(
                "BUG: command should have been handled before command_to_request — see main()'s early-dispatch match"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::test_support::parsed_command;
    use crate::cli::Cli;
    use clap::Parser;

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
