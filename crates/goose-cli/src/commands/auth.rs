use anyhow::{anyhow, Result};
use console::style;
use serde::Serialize;

use goose::providers::aws_sso::{
    run_end_to_end, DeviceAuth, SsoSummary, DEFAULT_PROFILE_NAME, DEFAULT_SESSION_NAME,
};
use goose::providers::bedrock::BEDROCK_DEFAULT_MODEL;

/// Newline-delimited JSON events emitted on stdout when `--json` is set. The
/// desktop app reads these; humans get the pretty branch instead.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Event<'a> {
    Prompt {
        verification_uri_complete: &'a str,
        user_code: &'a str,
        expires_in: i32,
    },
    Success {
        summary: &'a SsoSummary,
    },
    Error {
        message: String,
    },
}

fn emit(event: &Event<'_>) {
    if let Ok(line) = serde_json::to_string(event) {
        println!("{}", line);
    }
}

pub async fn handle_aws_sso(
    start_url: Option<String>,
    sso_region: Option<String>,
    bedrock_region: Option<String>,
    model: Option<String>,
    json: bool,
) -> Result<()> {
    let result = run(start_url, sso_region, bedrock_region, model, json).await;
    if let Err(ref e) = result {
        if json {
            emit(&Event::Error {
                message: format!("{:#}", e),
            });
        }
    }
    result
}

async fn run(
    start_url: Option<String>,
    sso_region: Option<String>,
    bedrock_region: Option<String>,
    model: Option<String>,
    json: bool,
) -> Result<()> {
    let start_url = start_url
        .or_else(|| std::env::var("GOOSE_AWS_SSO_START_URL").ok())
        .ok_or_else(|| {
            anyhow!(
                "AWS SSO start URL required. Pass --start-url https://<your-org>.awsapps.com/start \
                 or set GOOSE_AWS_SSO_START_URL"
            )
        })?;
    let sso_region = sso_region
        .or_else(|| std::env::var("GOOSE_AWS_SSO_REGION").ok())
        .unwrap_or_else(|| "us-east-1".to_string());
    let bedrock_region = bedrock_region
        .or_else(|| std::env::var("GOOSE_AWS_BEDROCK_REGION").ok())
        .unwrap_or_else(|| sso_region.clone());
    let model = model.unwrap_or_else(|| BEDROCK_DEFAULT_MODEL.to_string());

    if !json {
        println!(
            "{}",
            style("Signing in to AWS SSO — a browser tab will open.").bold()
        );
    }

    let summary = run_end_to_end(&start_url, &sso_region, &bedrock_region, &model, |device| {
        on_prompt(device, json)
    })
    .await?;

    if json {
        emit(&Event::Success { summary: &summary });
    } else {
        print_summary(&summary);
    }
    Ok(())
}

fn on_prompt(device: &DeviceAuth, json: bool) {
    if json {
        emit(&Event::Prompt {
            verification_uri_complete: &device.verification_uri_complete,
            user_code: &device.user_code,
            expires_in: device.expires_in,
        });
        return;
    }
    println!();
    println!(
        "  {} {}",
        style("Verification URL:").bold(),
        style(&device.verification_uri_complete).cyan()
    );
    println!(
        "  {} {}",
        style("Confirmation code:").bold(),
        style(&device.user_code).yellow()
    );
    println!();
    let _ = webbrowser::open(&device.verification_uri_complete);
    println!("Waiting for you to complete sign-in…");
}

fn print_summary(summary: &SsoSummary) {
    println!();
    println!("{}", style("✓ AWS SSO configured").green().bold());
    println!("  profile:        {}", summary.profile_name);
    println!("  session:        {}", summary.session_name);
    println!(
        "  account:        {}{}",
        summary.account_id,
        summary
            .account_name
            .as_deref()
            .map(|n| format!(" ({})", n))
            .unwrap_or_default()
    );
    println!("  role:           {}", summary.role_name);
    println!("  bedrock region: {}", summary.bedrock_region);
    println!("  session expires: {}", summary.expires_at);
    if !summary.other_accounts.is_empty() {
        println!(
            "  {} other account(s) available: {}",
            summary.other_accounts.len(),
            summary.other_accounts.join(", ")
        );
        println!(
            "  edit ~/.aws/config [profile {}] to switch.",
            DEFAULT_PROFILE_NAME
        );
    }
    if !summary.other_roles.is_empty() {
        println!(
            "  {} other role(s) available in this account: {}",
            summary.other_roles.len(),
            summary.other_roles.join(", ")
        );
    }
    println!();
    println!(
        "Goose is now configured to use Amazon Bedrock via AWS SSO (session '{}').",
        DEFAULT_SESSION_NAME
    );
    println!("Run `goose session` or open the desktop app to start chatting.");
}
