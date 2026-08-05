//! `captcha solve`/`captcha inject` orchestration: calling out to the
//! Anti-Captcha solver and, for `solve --inject`, forwarding the token to
//! the daemon in one step.

use serde_json::{json, Value};

use crate::cli::{CaptchaAction, SolveArgs};
use crate::launch_spec::LaunchSpec;
use crate::{client, protocol};

pub(crate) async fn solve_captcha_command(
    action: &CaptchaAction,
    session_name: &str,
    spec: LaunchSpec,
) -> Result<protocol::Response, String> {
    let mut value = solve_captcha(action).await?;
    let CaptchaAction::Solve(args) = action else {
        unreachable!("only solve actions reach solve_captcha_command")
    };
    let SolveArgs {
        captcha_type,
        inject,
        inject_callback,
        click_after,
        ..
    } = args.as_ref();

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
    let CaptchaAction::Solve(args) = action else {
        unreachable!("only solve actions reach solve_captcha")
    };
    let SolveArgs {
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
    } = args.as_ref();
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

pub(crate) fn captcha_inject_request(
    token: &str,
    captcha_type: &str,
    callback: Option<&str>,
    click_after: Option<&str>,
) -> protocol::Request {
    protocol::Request {
        cmd: "captcha_inject".into(),
        args: json!({
            "token": token,
            "captcha_type": captcha_type,
            "callback": callback,
            "click_after": click_after,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::test_support::parsed_command;
    use crate::cli::Command;

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
                action: CaptchaAction::Solve(args),
            } => {
                assert!(args.inject);
                assert_eq!(args.inject_callback.as_deref(), Some("window.onCaptcha"));
                assert_eq!(args.click_after.as_deref(), Some("text:Continue Booking"));
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
}
