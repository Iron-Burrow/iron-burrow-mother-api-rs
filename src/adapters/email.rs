use serde::Serialize;

#[derive(Clone, Debug, thiserror::Error)]
pub(crate) enum EmailError {
    #[error("email delivery is not configured")]
    NotConfigured,
    #[error("email delivery failed")]
    Delivery,
}

/// The production adapter intentionally exposes no provider response body: it
/// can contain recipient-specific diagnostics and must never reach a browser.
pub(crate) async fn send_resend_magic_link(
    api_key: Option<&str>,
    from: Option<&str>,
    recipient: &str,
    link: &str,
) -> Result<(), EmailError> {
    let (Some(api_key), Some(from)) = (api_key, from) else {
        return Err(EmailError::NotConfigured);
    };
    #[derive(Serialize)]
    struct Payload<'a> {
        from: &'a str,
        to: [&'a str; 1],
        subject: &'a str,
        html: String,
    }
    let payload = Payload {
        from,
        to: [recipient],
        subject: "Your Iron Burrow sign-in link",
        html: format!("<p>Use this one-time link to continue to Iron Burrow:</p><p><a href=\"{link}\">Continue</a></p><p>This link expires in 15 minutes.</p>"),
    };
    let response = reqwest::Client::new()
        .post("https://api.resend.com/emails")
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await
        .map_err(|_| EmailError::Delivery)?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(EmailError::Delivery)
    }
}
