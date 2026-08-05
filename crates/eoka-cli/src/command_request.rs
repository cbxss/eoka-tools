mod network;

use crate::captcha_cmd::captcha_inject_request;
use crate::cli::{CaptchaAction, Command, JsAction, TabAction, WasmAction};
use crate::protocol::{
    ClearFlagArgs, CloneFromArgs, ConsoleArgs, DeleteCookieArgs, DomainArgs, EmulateArgs,
    FakeCameraArgs, FetchArgs, FillArgs, HeadersArgs, KeyArgs, LoadStateArgs, ModeArgs,
    ObserveArgs, OpenArgs, PathArgs, PathStringArgs, Request, ScreenshotArgs, ScriptArgs,
    SelectArgs, SetCookieArgs, SetStorageArgs, SnapshotArgs, StorageArgs, TabIdArgs, TabNewArgs,
    TargetArgs, TextArgs, WaitArgs, WasmFindArgs, WasmReadArgs, WasmWriteArgs,
};
use network::network_action_to_request;
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
        Command::Open {
            url,
            headers,
            user_agent,
            bypass_csp,
            inject_js,
            load_state,
        } => Request::Open(OpenArgs {
            url: url.clone(),
            headers: headers.as_ref().map(|h| parse_headers_json(h)),
            user_agent: user_agent.clone(),
            bypass_csp: *bypass_csp,
            inject_js: inject_js.clone(),
            load_state: load_state
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
        }),
        Command::Back => Request::Back,
        Command::Forward => Request::Forward,
        Command::Reload => Request::Reload,
        Command::Snapshot { interactive, all } => Request::Snapshot(SnapshotArgs {
            interactive: *interactive,
            all: *all,
        }),
        Command::Observe { filter, max } => Request::Observe(ObserveArgs {
            filter: filter.clone(),
            max: *max,
        }),
        Command::Screenshot { output, annotate } => Request::Screenshot(ScreenshotArgs {
            output: output
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            annotate: *annotate,
        }),
        Command::Emulate {
            width,
            height,
            dpr,
            desktop,
            reset,
        } => Request::Emulate(EmulateArgs {
            width: *width,
            height: *height,
            dpr: *dpr,
            desktop: *desktop,
            reset: *reset,
        }),
        Command::Info => Request::Info,
        Command::Text => Request::Text,
        Command::Find { text } => Request::Find(TextArgs { text: text.clone() }),
        Command::Click { target } => Request::Click(TargetArgs {
            target: target.clone(),
        }),
        Command::DoubleClick { target } => Request::DblClick(TargetArgs {
            target: target.clone(),
        }),
        Command::Fill { target, text } => Request::Fill(FillArgs {
            target: target.clone(),
            text: text.clone(),
        }),
        Command::Select { target, value } => Request::Select(SelectArgs {
            target: target.clone(),
            value: value.clone(),
        }),
        Command::Hover { target } => Request::Hover(TargetArgs {
            target: target.clone(),
        }),
        Command::Key { key } => Request::Key(KeyArgs { key: key.clone() }),
        Command::Scroll { target } => Request::Scroll(TargetArgs {
            target: target.clone(),
        }),
        Command::Eval {
            code,
            file,
            no_return,
            max_size,
        } => {
            let args = ScriptArgs {
                code: code.clone(),
                file: file.as_ref().map(|path| path.to_string_lossy().to_string()),
                max_size: *max_size,
            };
            if *no_return {
                Request::Exec(args)
            } else {
                Request::Eval(args)
            }
        }
        Command::Exec { code, file } => Request::Exec(ScriptArgs {
            code: code.clone(),
            file: file.as_ref().map(|path| path.to_string_lossy().to_string()),
            max_size: None,
        }),
        Command::Fetch {
            url,
            method,
            headers,
            body,
            redirect,
            body_only,
            max_body,
        } => Request::Fetch(FetchArgs {
            url: url.clone(),
            method: method.clone(),
            headers: headers.as_ref().map(|h| parse_headers_json(h)),
            body: body.clone(),
            redirect: redirect.clone(),
            body_only: *body_only,
            max_body: *max_body,
        }),
        Command::Cookies => Request::Cookies,
        Command::SetCookie {
            name,
            value,
            domain,
            path,
        } => Request::SetCookie(SetCookieArgs {
            name: name.clone(),
            value: value.clone(),
            domain: domain.clone(),
            path: path.clone(),
        }),
        Command::DeleteCookie { name, domain } => Request::DeleteCookie(DeleteCookieArgs {
            name: name.clone(),
            domain: domain.clone(),
        }),
        Command::ClearCookies => Request::ClearCookies,
        Command::Storage {
            key,
            session_storage,
        } => Request::Storage(StorageArgs {
            key: key.clone(),
            session_storage: *session_storage,
        }),
        Command::SetStorage {
            key,
            value,
            session_storage,
        } => Request::SetStorage(SetStorageArgs {
            key: key.clone(),
            value: value.clone(),
            session_storage: *session_storage,
        }),
        Command::DumpStorage => Request::DumpStorage,
        Command::SaveState { path } => Request::SaveState(PathArgs {
            path: path.to_string_lossy().to_string(),
        }),
        Command::LoadState { path, no_navigate } => Request::LoadState(LoadStateArgs {
            path: path.to_string_lossy().to_string(),
            no_navigate: *no_navigate,
        }),
        Command::Headers { headers_json } => Request::Headers(HeadersArgs {
            headers_json: headers_json.clone(),
        }),
        Command::Console { clear, level } => Request::Console(ConsoleArgs {
            clear: *clear,
            level: level.clone(),
        }),
        Command::Errors { clear } => Request::Errors(ClearFlagArgs { clear: *clear }),
        Command::Tab { action } => match action {
            TabAction::List => Request::TabList,
            TabAction::New { url } => Request::TabNew(TabNewArgs { url: url.clone() }),
            TabAction::Switch { tab_id } => Request::TabSwitch(TabIdArgs {
                tab_id: tab_id.clone(),
            }),
            TabAction::Close { tab_id } => Request::TabClose(TabIdArgs {
                tab_id: tab_id.clone(),
            }),
            TabAction::Attach { tab_id } => Request::TabAttach(TabIdArgs {
                tab_id: tab_id.clone(),
            }),
        },
        Command::Wait {
            ms,
            text,
            url,
            load,
            timeout,
        } => Request::Wait(WaitArgs {
            ms: *ms,
            text: text.clone(),
            url: url.clone(),
            load: load.clone(),
            timeout: *timeout,
        }),
        Command::SpaInfo => Request::SpaInfo,
        Command::SpaNavigate { path } => {
            Request::SpaNavigate(PathStringArgs { path: path.clone() })
        }
        Command::FakeCamera { file, loop_video } => Request::FakeCamera(FakeCameraArgs {
            file: file.to_string_lossy().to_string(),
            loop_video: *loop_video,
        }),
        Command::Wasm { action } => match action {
            WasmAction::Info => Request::WasmInfo,
            WasmAction::Read { addr, len, memory } => Request::WasmRead(WasmReadArgs {
                addr: addr.clone(),
                len: *len,
                memory: memory.clone(),
            }),
            WasmAction::Write { addr, hex, memory } => Request::WasmWrite(WasmWriteArgs {
                addr: addr.clone(),
                hex: hex.clone(),
                memory: memory.clone(),
            }),
            WasmAction::Find {
                pattern,
                start,
                end,
                max,
                memory,
            } => Request::WasmFind(WasmFindArgs {
                pattern: pattern.clone(),
                start: start.clone(),
                end: end.clone(),
                max: *max,
                memory: memory.clone(),
            }),
        },

        Command::Network { action } => network_action_to_request(action),
        Command::Js { action } => match action {
            JsAction::Mode { mode } => Request::JsMode(ModeArgs { mode: mode.clone() }),
            JsAction::Allow { domain } => Request::JsAllow(DomainArgs {
                domain: domain.clone(),
            }),
            JsAction::Block { domain } => Request::JsBlock(DomainArgs {
                domain: domain.clone(),
            }),
            JsAction::Remove { domain } => Request::JsRemove(DomainArgs {
                domain: domain.clone(),
            }),
            JsAction::List => Request::JsList,
        },
        Command::Close => Request::Close,
        Command::CloneFrom { source, to } => Request::CloneFrom(CloneFromArgs {
            source: source.clone(),
            to: to.as_ref().map(|path| path.to_string_lossy().to_string()),
        }),
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
    use serde_json::json;

    #[test]
    fn click_accepts_bracketed_observe_target_at_cli_layer() {
        let (_cli, command) = parsed_command(&["eoka", "click", "[38]"]);
        let request = command_to_request(&command);

        assert_eq!(request.cmd(), "click");
        assert_eq!(request.args_json()["target"], "[38]");
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

        assert_eq!(request.cmd(), "fetch");
        assert_eq!(request.args_json()["body_only"], true);
        assert_eq!(request.args_json()["max_body"], 16);
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

        assert_eq!(request.cmd(), "open");
        assert_eq!(request.args_json()["url"], "/camping/campsites/71576");
        assert_eq!(request.args_json()["load_state"], "auth.json");
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

        assert_eq!(request.cmd(), "captcha_inject");
        assert_eq!(request.args_json()["token"], "token-123");
        assert_eq!(request.args_json()["captcha_type"], "recaptcha");
        assert_eq!(request.args_json()["callback"], "window.onCaptcha");
        assert_eq!(request.args_json()["click_after"], "text:Continue Booking");
    }

    #[test]
    fn network_intercept_subcommand_accepts_trailing_json_flag() {
        let (cli, command) =
            parsed_command(&["eoka", "network", "intercept", "add", "*api*", "--json"]);
        let request = command_to_request(&command);

        assert!(cli.json);
        assert_eq!(request.cmd(), "intercept_add");
        assert_eq!(request.args_json()["url_pattern"], "*api*");
    }

    #[test]
    fn network_record_start_clear_maps_to_typed_request() {
        let (_cli, command) = parsed_command(&[
            "eoka",
            "network",
            "record",
            "start",
            "--pattern",
            "*/api/*",
            "--clear",
        ]);
        let request = command_to_request(&command);

        assert_eq!(request.cmd(), "network_record_start");
        assert_eq!(request.args_json()["patterns"], json!(["*/api/*"]));
        assert_eq!(request.args_json()["clear"], true);
    }

    #[test]
    fn network_wait_defaults_to_new_entries() {
        let (_cli, command) = parsed_command(&[
            "eoka",
            "network",
            "wait",
            "--pattern",
            "*/api/*",
            "--status",
            "200",
            "--timeout",
            "5000",
        ]);
        let request = command_to_request(&command);

        assert_eq!(request.cmd(), "network_wait");
        assert_eq!(request.args_json()["pattern"], "*/api/*");
        assert_eq!(request.args_json()["status"], 200);
        assert_eq!(request.args_json()["timeout"], 5000);
        assert_eq!(request.args_json()["include_existing"], false);
    }

    #[test]
    fn network_export_json_resolves_path_and_format() {
        let (_cli, command) = parsed_command(&[
            "eoka",
            "network",
            "export",
            "capture.json",
            "--format",
            "json",
        ]);
        let request = command_to_request(&command);

        assert_eq!(request.cmd(), "network_export");
        assert_eq!(request.args_json()["format"], "json");
        assert!(request.args_json()["path"]
            .as_str()
            .unwrap()
            .ends_with("capture.json"));
    }

    #[test]
    fn network_save_har_keeps_legacy_wire_command() {
        let (_cli, command) = parsed_command(&[
            "eoka",
            "network",
            "save-har",
            "capture.har",
            "--settle-ms",
            "1",
        ]);
        let request = command_to_request(&command);

        assert_eq!(request.cmd(), "network_save_har");
        assert_eq!(request.args_json()["format"], "har");
        assert_eq!(request.args_json()["settle_ms"], 1);
    }

    #[test]
    fn top_level_intercept_is_rejected() {
        let err = match Cli::try_parse_from(["eoka", "intercept", "add", "*api*", "--json"]) {
            Ok(_) => panic!("top-level intercept should be rejected"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("unrecognized subcommand"));
    }

    #[test]
    fn deprecated_json_intercept_spec_is_rejected() {
        let err = match Cli::try_parse_from([
            "eoka",
            "network",
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
