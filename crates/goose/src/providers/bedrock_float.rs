//! Float fork additions for the Bedrock provider.
//!
//! Kept in a separate file so upstream `bedrock.rs` merges cleanly. The only
//! things `bedrock.rs` touches that belong to us are:
//! - the `sdk_config: Option<aws_config::SdkConfig>` field on `BedrockProvider`
//!   (Rust structs can't span files)
//! - one call site inside `fetch_supported_models` that delegates here
//! - constant tweaks (default model / known-model ordering) that must live
//!   next to their upstream declarations
//!
//! Everything else — the AWS control-plane enumeration, the picker pin list,
//! the pin-and-sort logic — is owned here. See `docs/float-fork/PATCHES.md`.

use std::collections::HashSet;

use aws_config::SdkConfig;

/// Models pinned to the top of the model picker, in order. Anything present
/// here AND surfaced by the AWS SSO identity's `ListInferenceProfiles` /
/// `ListFoundationModels` response is floated to the front; the rest follow
/// sorted alphabetically.
pub const BEDROCK_PREFERRED_MODELS: &[&str] = &[
    "global.anthropic.claude-sonnet-5",
    "us.anthropic.claude-opus-4-8",
];

/// Enumerate models the current identity can actually see: cross-region
/// inference profiles (what Claude / recent models are invoked as) plus
/// on-demand foundation models. Returns an empty vec if credentials aren't
/// available; callers fall back to the static list.
pub(crate) async fn fetch_supported_models_via_aws(
    sdk_config: Option<&SdkConfig>,
) -> Result<Vec<String>, anyhow::Error> {
    let Some(sdk_config) = sdk_config else {
        return Ok(Vec::new());
    };
    let client = aws_sdk_bedrock::Client::new(sdk_config);
    let mut ids: Vec<String> = Vec::new();

    // Cross-region inference profiles — the IDs that actually work at invoke
    // time for the majority of chat models. Failures here should not kill
    // the enumeration; some roles have foundation model access without
    // profile-listing permissions.
    match client.list_inference_profiles().send().await {
        Ok(resp) => {
            for p in resp.inference_profile_summaries() {
                ids.push(p.inference_profile_id.clone());
            }
        }
        Err(e) => tracing::debug!("list_inference_profiles failed: {}", e),
    }

    match client.list_foundation_models().send().await {
        Ok(resp) => {
            for m in resp.model_summaries() {
                let supports_on_demand = m
                    .inference_types_supported()
                    .iter()
                    .any(|t| t.as_str() == "ON_DEMAND");
                if supports_on_demand {
                    ids.push(m.model_id.clone());
                }
            }
        }
        Err(e) => tracing::debug!("list_foundation_models failed: {}", e),
    }

    ids.sort();
    ids.dedup();

    Ok(pin_preferred(ids))
}

/// Float preferred models (sonnet-5, opus-4-8) to the top of the picker, in
/// the order they appear in `BEDROCK_PREFERRED_MODELS`. Anything the caller
/// doesn't have access to is silently dropped from the pinned slice — no
/// phantom entries in the picker.
fn pin_preferred(ids: Vec<String>) -> Vec<String> {
    let preferred: HashSet<&str> = BEDROCK_PREFERRED_MODELS.iter().copied().collect();
    let (mut pinned, rest): (Vec<String>, Vec<String>) = ids
        .into_iter()
        .partition(|id| preferred.contains(id.as_str()));
    pinned.sort_by_key(|id| {
        BEDROCK_PREFERRED_MODELS
            .iter()
            .position(|p| *p == id)
            .unwrap_or(usize::MAX)
    });
    pinned.extend(rest);
    pinned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_preferred_puts_pinned_models_first_in_order() {
        // Caller (`fetch_supported_models_via_aws`) sorts+dedups before pinning,
        // so `pin_preferred` only needs to hoist pinned ids while preserving the
        // input order of everything else.
        let ids = vec![
            "amazon.nova".to_string(),
            "meta.llama-3".to_string(),
            "us.anthropic.claude-opus-4-8".to_string(),
            "global.anthropic.claude-sonnet-5".to_string(),
        ];
        assert_eq!(
            pin_preferred(ids),
            vec![
                "global.anthropic.claude-sonnet-5",
                "us.anthropic.claude-opus-4-8",
                "amazon.nova",
                "meta.llama-3",
            ]
        );
    }

    #[test]
    fn pin_preferred_drops_pinned_ids_that_are_absent() {
        let ids = vec!["amazon.nova".to_string(), "meta.llama-3".to_string()];
        assert_eq!(
            pin_preferred(ids),
            vec!["amazon.nova".to_string(), "meta.llama-3".to_string()]
        );
    }

    #[tokio::test]
    async fn fetch_returns_empty_without_sdk_config() {
        assert_eq!(
            fetch_supported_models_via_aws(None).await.unwrap(),
            Vec::<String>::new()
        );
    }
}
