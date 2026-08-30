use std::process::Command as StdCommand;
use std::time::Duration;
use tokio::net::UnixStream;

use crate::launch_spec::LaunchSpec;
use crate::session;
use eoka_protocol::{
    read_msg, write_msg, CaptchaInjectArgs, ClearFlagArgs, CloneFromArgs, ConsoleArgs,
    DeleteCookieArgs, DomainArgs, EmulateArgs, FakeCameraArgs, FetchArgs, FillArgs, HeadersArgs,
    IdArgs, InterceptAddArgs, KeyArgs, LoadStateArgs, ModeArgs, NetworkExportArgs, NetworkLogArgs,
    NetworkRecordStartArgs, NetworkShowArgs, NetworkWaitArgs, ObserveArgs, OpenArgs, PathArgs,
    PathStringArgs, Request, Response, ScreenshotArgs, ScriptArgs, SelectArgs, SetCookieArgs,
    SetStorageArgs, SnapshotArgs, StorageArgs, TabIdArgs, TabNewArgs, TargetArgs, TextArgs,
    WaitArgs, WasmFindArgs, WasmReadArgs, WasmWriteArgs,
};

#[derive(Debug, Clone)]
pub struct EokaClient {
    session_name: String,
    spec: LaunchSpec,
}

impl EokaClient {
    pub fn new(session_name: impl Into<String>, spec: LaunchSpec) -> Self {
        Self {
            session_name: session_name.into(),
            spec,
        }
    }

    pub fn session_name(&self) -> &str {
        &self.session_name
    }

    pub fn launch_spec(&self) -> &LaunchSpec {
        &self.spec
    }

    pub async fn call(&self, request: Request) -> anyhow::Result<Response> {
        send_command(&self.session_name, request, self.spec.clone()).await
    }

    pub async fn open(&self, args: OpenArgs) -> anyhow::Result<Response> {
        self.call(Request::Open(args)).await
    }

    pub async fn back(&self) -> anyhow::Result<Response> {
        self.call(Request::Back).await
    }

    pub async fn forward(&self) -> anyhow::Result<Response> {
        self.call(Request::Forward).await
    }

    pub async fn reload(&self) -> anyhow::Result<Response> {
        self.call(Request::Reload).await
    }

    pub async fn snapshot(&self, args: SnapshotArgs) -> anyhow::Result<Response> {
        self.call(Request::Snapshot(args)).await
    }

    pub async fn observe(&self, args: ObserveArgs) -> anyhow::Result<Response> {
        self.call(Request::Observe(args)).await
    }

    pub async fn screenshot(&self, args: ScreenshotArgs) -> anyhow::Result<Response> {
        self.call(Request::Screenshot(args)).await
    }

    pub async fn emulate(&self, args: EmulateArgs) -> anyhow::Result<Response> {
        self.call(Request::Emulate(args)).await
    }

    pub async fn info(&self) -> anyhow::Result<Response> {
        self.call(Request::Info).await
    }

    pub async fn text(&self) -> anyhow::Result<Response> {
        self.call(Request::Text).await
    }

    pub async fn find(&self, args: TextArgs) -> anyhow::Result<Response> {
        self.call(Request::Find(args)).await
    }

    pub async fn click(&self, args: TargetArgs) -> anyhow::Result<Response> {
        self.call(Request::Click(args)).await
    }

    pub async fn double_click(&self, args: TargetArgs) -> anyhow::Result<Response> {
        self.call(Request::DblClick(args)).await
    }

    pub async fn fill(&self, args: FillArgs) -> anyhow::Result<Response> {
        self.call(Request::Fill(args)).await
    }

    pub async fn select(&self, args: SelectArgs) -> anyhow::Result<Response> {
        self.call(Request::Select(args)).await
    }

    pub async fn hover(&self, args: TargetArgs) -> anyhow::Result<Response> {
        self.call(Request::Hover(args)).await
    }

    pub async fn key(&self, args: KeyArgs) -> anyhow::Result<Response> {
        self.call(Request::Key(args)).await
    }

    pub async fn scroll(&self, args: TargetArgs) -> anyhow::Result<Response> {
        self.call(Request::Scroll(args)).await
    }

    pub async fn eval(&self, args: ScriptArgs) -> anyhow::Result<Response> {
        self.call(Request::Eval(args)).await
    }

    pub async fn exec(&self, args: ScriptArgs) -> anyhow::Result<Response> {
        self.call(Request::Exec(args)).await
    }

    pub async fn fetch(&self, args: FetchArgs) -> anyhow::Result<Response> {
        self.call(Request::Fetch(args)).await
    }

    pub async fn cookies(&self) -> anyhow::Result<Response> {
        self.call(Request::Cookies).await
    }

    pub async fn set_cookie(&self, args: SetCookieArgs) -> anyhow::Result<Response> {
        self.call(Request::SetCookie(args)).await
    }

    pub async fn delete_cookie(&self, args: DeleteCookieArgs) -> anyhow::Result<Response> {
        self.call(Request::DeleteCookie(args)).await
    }

    pub async fn clear_cookies(&self) -> anyhow::Result<Response> {
        self.call(Request::ClearCookies).await
    }

    pub async fn storage(&self, args: StorageArgs) -> anyhow::Result<Response> {
        self.call(Request::Storage(args)).await
    }

    pub async fn set_storage(&self, args: SetStorageArgs) -> anyhow::Result<Response> {
        self.call(Request::SetStorage(args)).await
    }

    pub async fn dump_storage(&self) -> anyhow::Result<Response> {
        self.call(Request::DumpStorage).await
    }

    pub async fn save_state(&self, args: PathArgs) -> anyhow::Result<Response> {
        self.call(Request::SaveState(args)).await
    }

    pub async fn load_state(&self, args: LoadStateArgs) -> anyhow::Result<Response> {
        self.call(Request::LoadState(args)).await
    }

    pub async fn headers(&self, args: HeadersArgs) -> anyhow::Result<Response> {
        self.call(Request::Headers(args)).await
    }

    pub async fn console(&self, args: ConsoleArgs) -> anyhow::Result<Response> {
        self.call(Request::Console(args)).await
    }

    pub async fn errors(&self, args: ClearFlagArgs) -> anyhow::Result<Response> {
        self.call(Request::Errors(args)).await
    }

    pub async fn tab_list(&self) -> anyhow::Result<Response> {
        self.call(Request::TabList).await
    }

    pub async fn tab_new(&self, args: TabNewArgs) -> anyhow::Result<Response> {
        self.call(Request::TabNew(args)).await
    }

    pub async fn tab_switch(&self, args: TabIdArgs) -> anyhow::Result<Response> {
        self.call(Request::TabSwitch(args)).await
    }

    pub async fn tab_close(&self, args: TabIdArgs) -> anyhow::Result<Response> {
        self.call(Request::TabClose(args)).await
    }

    pub async fn tab_attach(&self, args: TabIdArgs) -> anyhow::Result<Response> {
        self.call(Request::TabAttach(args)).await
    }

    pub async fn clone_from(&self, args: CloneFromArgs) -> anyhow::Result<Response> {
        self.call(Request::CloneFrom(args)).await
    }

    pub async fn wait(&self, args: WaitArgs) -> anyhow::Result<Response> {
        self.call(Request::Wait(args)).await
    }

    pub async fn spa_info(&self) -> anyhow::Result<Response> {
        self.call(Request::SpaInfo).await
    }

    pub async fn spa_navigate(&self, args: PathStringArgs) -> anyhow::Result<Response> {
        self.call(Request::SpaNavigate(args)).await
    }

    pub async fn fake_camera(&self, args: FakeCameraArgs) -> anyhow::Result<Response> {
        self.call(Request::FakeCamera(args)).await
    }

    pub async fn wasm_info(&self) -> anyhow::Result<Response> {
        self.call(Request::WasmInfo).await
    }

    pub async fn wasm_read(&self, args: WasmReadArgs) -> anyhow::Result<Response> {
        self.call(Request::WasmRead(args)).await
    }

    pub async fn wasm_write(&self, args: WasmWriteArgs) -> anyhow::Result<Response> {
        self.call(Request::WasmWrite(args)).await
    }

    pub async fn wasm_find(&self, args: WasmFindArgs) -> anyhow::Result<Response> {
        self.call(Request::WasmFind(args)).await
    }

    pub async fn intercept_add(&self, args: InterceptAddArgs) -> anyhow::Result<Response> {
        self.call(Request::InterceptAdd(args)).await
    }

    pub async fn intercept_list(&self) -> anyhow::Result<Response> {
        self.call(Request::InterceptList).await
    }

    pub async fn intercept_remove(&self, args: IdArgs) -> anyhow::Result<Response> {
        self.call(Request::InterceptRemove(args)).await
    }

    pub async fn intercept_log(&self, args: ClearFlagArgs) -> anyhow::Result<Response> {
        self.call(Request::InterceptLog(args)).await
    }

    pub async fn network_record_start(
        &self,
        args: NetworkRecordStartArgs,
    ) -> anyhow::Result<Response> {
        self.call(Request::NetworkRecordStart(args)).await
    }

    pub async fn network_record_stop(&self) -> anyhow::Result<Response> {
        self.call(Request::NetworkRecordStop).await
    }

    pub async fn network_record_status(&self) -> anyhow::Result<Response> {
        self.call(Request::NetworkRecordStatus).await
    }

    pub async fn network_log(&self, args: NetworkLogArgs) -> anyhow::Result<Response> {
        self.call(Request::NetworkLog(args)).await
    }

    pub async fn network_show(&self, args: NetworkShowArgs) -> anyhow::Result<Response> {
        self.call(Request::NetworkShow(args)).await
    }

    pub async fn network_wait(&self, args: NetworkWaitArgs) -> anyhow::Result<Response> {
        self.call(Request::NetworkWait(args)).await
    }

    pub async fn network_export(&self, args: NetworkExportArgs) -> anyhow::Result<Response> {
        self.call(Request::NetworkExport(args)).await
    }

    pub async fn network_clear(&self) -> anyhow::Result<Response> {
        self.call(Request::NetworkClear).await
    }

    pub async fn js_mode(&self, args: ModeArgs) -> anyhow::Result<Response> {
        self.call(Request::JsMode(args)).await
    }

    pub async fn js_allow(&self, args: DomainArgs) -> anyhow::Result<Response> {
        self.call(Request::JsAllow(args)).await
    }

    pub async fn js_block(&self, args: DomainArgs) -> anyhow::Result<Response> {
        self.call(Request::JsBlock(args)).await
    }

    pub async fn js_remove(&self, args: DomainArgs) -> anyhow::Result<Response> {
        self.call(Request::JsRemove(args)).await
    }

    pub async fn js_list(&self) -> anyhow::Result<Response> {
        self.call(Request::JsList).await
    }

    pub async fn close(&self) -> anyhow::Result<Response> {
        self.call(Request::Close).await
    }

    pub async fn shutdown(&self) -> anyhow::Result<Response> {
        self.call(Request::Shutdown).await
    }

    pub async fn captcha_inject(&self, args: CaptchaInjectArgs) -> anyhow::Result<Response> {
        self.call(Request::CaptchaInject(args)).await
    }
}
pub async fn send_command(
    session_name: &str,
    request: Request,
    spec: LaunchSpec,
) -> anyhow::Result<Response> {
    let response = send_command_once(session_name, request.clone(), &spec).await?;
    if should_restart_headed_daemon(&response, &spec) {
        let _ = kill_daemon(session_name);
        let retry = send_command_once(session_name, request, &spec).await?;
        return Ok(retry);
    }
    Ok(response)
}

async fn send_command_once(
    session_name: &str,
    request: Request,
    spec: &LaunchSpec,
) -> anyhow::Result<Response> {
    let sock = session::socket_path(session_name);
    let stream = match UnixStream::connect(&sock).await {
        Ok(s) => s,
        Err(_) => {
            launch_daemon(session_name, spec)?;
            wait_for_socket(session_name).await?
        }
    };

    let (mut reader, mut writer) = stream.into_split();
    write_msg(&mut writer, &request).await?;
    let response: Response = read_msg(&mut reader).await?;
    Ok(response)
}

fn should_restart_headed_daemon(response: &Response, spec: &LaunchSpec) -> bool {
    let LaunchSpec::Launch {
        headless: false, ..
    } = spec
    else {
        return false;
    };
    if std::env::var_os("DISPLAY").is_none() {
        return false;
    }
    let Some(error) = response.error.as_deref() else {
        return false;
    };
    error.contains("Missing X server or $DISPLAY")
        || error.contains("channel is empty and sending half is closed")
}
async fn wait_for_socket(session_name: &str) -> anyhow::Result<UnixStream> {
    let sock = session::socket_path(session_name);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if tokio::time::Instant::now() > deadline {
            anyhow::bail!(
                "Daemon failed to start (socket {} not found after 5s)",
                sock.display()
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Ok(s) = UnixStream::connect(&sock).await {
            return Ok(s);
        }
    }
}
fn launch_daemon(session_name: &str, spec: &LaunchSpec) -> anyhow::Result<()> {
    session::ensure_runtime_dir()?;

    let exe = std::env::current_exe()?;
    let mut cmd = StdCommand::new(exe);
    cmd.arg("--daemon").arg("--session").arg(session_name);

    match spec {
        LaunchSpec::Launch {
            headless,
            from_profile,
            clone_state_from,
            no_stealth,
            proxy,
            no_js,
            js_allow,
            js_block,
            persist,
            geo_align,
        } => {
            if !*headless {
                cmd.arg("--headed");
            }
            if let Some(p) = from_profile {
                cmd.arg("--from-profile").arg(p);
            }
            if let Some(s) = clone_state_from {
                cmd.arg("--clone-state-from").arg(s);
            }
            if *no_stealth {
                cmd.arg("--no-stealth");
            }
            if *persist {
                cmd.arg("--persist");
            }
            if !*geo_align {
                cmd.arg("--no-geo-align");
            }
            if let Some(p) = proxy {
                cmd.arg("--proxy").arg(p);
            }
            if *no_js {
                cmd.arg("--no-js");
            }
            for domain in js_allow {
                cmd.arg("--js-allow").arg(domain);
            }
            for domain in js_block {
                cmd.arg("--js-block").arg(domain);
            }
        }
        LaunchSpec::Connect { ws_url } => {
            cmd.arg("--cdp").arg(ws_url);
        }
    }

    let log_path = session::socket_path(session_name).with_extension("log");
    let log_file = std::fs::File::create(&log_path)?;

    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(log_file);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    cmd.spawn()?;
    eprintln!(
        "[eoka] daemon starting (session={}, log={})",
        session_name,
        log_path.display()
    );
    Ok(())
}
pub fn is_daemon_running(session_name: &str) -> bool {
    let sock = session::socket_path(session_name);
    std::os::unix::net::UnixStream::connect(&sock).is_ok()
}
pub fn kill_daemon(session_name: &str) -> anyhow::Result<bool> {
    let sock = session::socket_path(session_name);
    if let Ok(stream) = std::os::unix::net::UnixStream::connect(&sock) {
        drop(stream);
    }
    let pid_path = session::pid_path(session_name);
    let killed = if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            StdCommand::new("kill")
                .arg(pid.to_string())
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        } else {
            false
        }
    } else {
        false
    };
    let _ = std::fs::remove_file(&sock);
    let _ = std::fs::remove_file(&pid_path);
    Ok(killed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::UnixListener;

    fn test_client(session: &str) -> EokaClient {
        EokaClient::new(
            session,
            LaunchSpec::Launch {
                headless: true,
                from_profile: None,
                clone_state_from: None,
                no_stealth: false,
                proxy: None,
                no_js: false,
                js_allow: Vec::new(),
                js_block: Vec::new(),
                persist: false,
                geo_align: false,
            },
        )
    }

    #[tokio::test]
    async fn typed_open_helper_sends_protocol_request() {
        let session = format!("eoka-sdk-helper-test-{}", std::process::id());
        session::ensure_runtime_dir().unwrap();
        let sock = session::socket_path(&session);
        let _ = std::fs::remove_file(&sock);
        let listener = UnixListener::bind(&sock).unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (mut reader, mut writer) = stream.into_split();
            let request: Request = read_msg(&mut reader).await.unwrap();
            assert_eq!(request.cmd(), "open");
            assert_eq!(request.args_json()["url"], "https://example.com");
            write_msg(&mut writer, &Response::ok_text("ok"))
                .await
                .unwrap();
        });

        let response = test_client(&session)
            .open(OpenArgs {
                url: "https://example.com".to_string(),
                ..Default::default()
            })
            .await
            .unwrap();

        assert!(response.ok);
        server.await.unwrap();
        let _ = std::fs::remove_file(&sock);
    }
}
