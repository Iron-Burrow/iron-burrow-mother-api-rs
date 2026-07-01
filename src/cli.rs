use crate::common::rfc3339::parse_rfc3339;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Command {
    Serve,
    Help,
    Db(DbCommand),
    Admin(AdminCommand),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DbCommand {
    Migrate,
    ApplyReference,
    Apply,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AdminCommand {
    ApiKey(AdminApiKeyCommand),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AdminApiKeyCommand {
    Issue(ApiKeyIssueArgs),
    Revoke(ApiKeyRevokeArgs),
    List(ApiKeyListArgs),
    Usage(ApiKeyUsageArgs),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ApiKeyIssueArgs {
    pub(crate) consumer_slug: String,
    pub(crate) display_name: String,
    pub(crate) category: String,
    pub(crate) label: String,
    pub(crate) requests_per_minute: i32,
    pub(crate) requests_per_day: i32,
    pub(crate) expires_at: Option<String>,
    pub(crate) format: OutputFormat,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ApiKeyRevokeArgs {
    pub(crate) key_prefix: String,
    pub(crate) format: OutputFormat,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ApiKeyListArgs {
    pub(crate) consumer_slug: String,
    pub(crate) format: OutputFormat,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ApiKeyUsageArgs {
    pub(crate) consumer_slug: String,
    pub(crate) days: u32,
    pub(crate) format: OutputFormat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputFormat {
    Human,
    Json,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("{message}")]
pub(crate) struct ParseError {
    message: String,
}

impl ParseError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub(crate) const USAGE: &str = "\
Usage:
  mother-api [serve]
  mother-api db migrate
  mother-api db apply-reference
  mother-api db apply
  mother-api admin api-key issue --consumer-slug <slug> --display-name <name> --category <friend|partner|public|internal> --label <label> [--requests-per-minute <n>] [--requests-per-day <n>] [--expires-at <rfc3339>] [--format <human|json>]
  mother-api admin api-key revoke --key-prefix <prefix> [--format <human|json>]
  mother-api admin api-key list --consumer-slug <slug> [--format <human|json>]
  mother-api admin api-key usage --consumer-slug <slug> [--days <n>] [--format <human|json>]
  mother-api help
  mother-api --help";

pub(crate) fn parse_args<I, S>(args: I) -> Result<Command, ParseError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect::<Vec<_>>();
    let parts = args.iter().map(String::as_str).collect::<Vec<_>>();

    match parts.as_slice() {
        [] => Ok(Command::Serve),
        [arg] if is_help_arg(arg) => Ok(Command::Help),
        ["serve"] => Ok(Command::Serve),
        ["serve", ..] => Err(ParseError::new("serve does not accept extra arguments")),
        ["db"] => Err(ParseError::new("db requires a subcommand")),
        ["db", arg] if is_help_arg(arg) => Ok(Command::Help),
        ["db", "migrate"] => Ok(Command::Db(DbCommand::Migrate)),
        ["db", "apply-reference"] => Ok(Command::Db(DbCommand::ApplyReference)),
        ["db", "apply"] => Ok(Command::Db(DbCommand::Apply)),
        ["db", subcommand] => Err(ParseError::new(format!(
            "unknown db subcommand {subcommand:?}"
        ))),
        ["db", subcommand, ..] => Err(ParseError::new(format!(
            "db subcommand {subcommand:?} does not accept extra arguments"
        ))),
        ["admin"] => Err(ParseError::new("admin requires a subcommand")),
        ["admin", arg] if is_help_arg(arg) => Ok(Command::Help),
        ["admin", "api-key"] => Err(ParseError::new("admin api-key requires a subcommand")),
        ["admin", "api-key", arg] if is_help_arg(arg) => Ok(Command::Help),
        ["admin", "api-key", "issue", rest @ ..] => parse_issue(rest).map(admin_api_key),
        ["admin", "api-key", "revoke", rest @ ..] => parse_revoke(rest).map(admin_api_key),
        ["admin", "api-key", "list", rest @ ..] => parse_list(rest).map(admin_api_key),
        ["admin", "api-key", "usage", rest @ ..] => parse_usage(rest).map(admin_api_key),
        ["admin", "api-key", subcommand, ..] => Err(ParseError::new(format!(
            "unknown admin api-key subcommand {subcommand:?}"
        ))),
        ["admin", subcommand, ..] => Err(ParseError::new(format!(
            "unknown admin subcommand {subcommand:?}"
        ))),
        [command, ..] => Err(ParseError::new(format!("unknown command {command:?}"))),
    }
}

fn admin_api_key(command: AdminApiKeyCommand) -> Command {
    Command::Admin(AdminCommand::ApiKey(command))
}

fn parse_issue(args: &[&str]) -> Result<AdminApiKeyCommand, ParseError> {
    let mut consumer_slug = None;
    let mut display_name = None;
    let mut category = None;
    let mut label = None;
    let mut requests_per_minute = 60;
    let mut requests_per_day = 5000;
    let mut expires_at = None;
    let mut format = OutputFormat::Human;
    let mut seen_requests_per_minute = false;
    let mut seen_requests_per_day = false;
    let mut seen_format = false;

    let mut index = 0;
    while index < args.len() {
        let flag = args[index];
        let value = flag_value(args, index)?;
        match flag {
            "--consumer-slug" => set_once(&mut consumer_slug, flag, validate_slug(flag, value)?)?,
            "--display-name" => {
                set_once(&mut display_name, flag, validate_non_empty(flag, value)?)?
            }
            "--category" => set_once(&mut category, flag, validate_category(value)?)?,
            "--label" => set_once(&mut label, flag, validate_non_empty(flag, value)?)?,
            "--requests-per-minute" => {
                set_seen(&mut seen_requests_per_minute, flag)?;
                requests_per_minute = parse_non_negative_i32(flag, value)?
            }
            "--requests-per-day" => {
                set_seen(&mut seen_requests_per_day, flag)?;
                requests_per_day = parse_non_negative_i32(flag, value)?
            }
            "--expires-at" => set_once(&mut expires_at, flag, validate_rfc3339(flag, value)?)?,
            "--format" => {
                set_seen(&mut seen_format, flag)?;
                format = parse_format(value)?
            }
            unknown if unknown.starts_with("--") => {
                return Err(ParseError::new(format!("unknown issue flag {unknown:?}")));
            }
            unexpected => {
                return Err(ParseError::new(format!(
                    "unexpected issue argument {unexpected:?}"
                )));
            }
        }
        index += 2;
    }

    Ok(AdminApiKeyCommand::Issue(ApiKeyIssueArgs {
        consumer_slug: required(consumer_slug, "--consumer-slug")?,
        display_name: required(display_name, "--display-name")?,
        category: required(category, "--category")?,
        label: required(label, "--label")?,
        requests_per_minute,
        requests_per_day,
        expires_at,
        format,
    }))
}

fn parse_revoke(args: &[&str]) -> Result<AdminApiKeyCommand, ParseError> {
    let mut key_prefix = None;
    let mut format = OutputFormat::Human;
    let mut seen_format = false;

    let mut index = 0;
    while index < args.len() {
        let flag = args[index];
        let value = flag_value(args, index)?;
        match flag {
            "--key-prefix" => set_once(&mut key_prefix, flag, validate_key_prefix(value)?)?,
            "--format" => {
                set_seen(&mut seen_format, flag)?;
                format = parse_format(value)?
            }
            unknown if unknown.starts_with("--") => {
                return Err(ParseError::new(format!("unknown revoke flag {unknown:?}")));
            }
            unexpected => {
                return Err(ParseError::new(format!(
                    "unexpected revoke argument {unexpected:?}"
                )));
            }
        }
        index += 2;
    }

    Ok(AdminApiKeyCommand::Revoke(ApiKeyRevokeArgs {
        key_prefix: required(key_prefix, "--key-prefix")?,
        format,
    }))
}

fn parse_list(args: &[&str]) -> Result<AdminApiKeyCommand, ParseError> {
    let mut consumer_slug = None;
    let mut format = OutputFormat::Human;
    let mut seen_format = false;

    let mut index = 0;
    while index < args.len() {
        let flag = args[index];
        let value = flag_value(args, index)?;
        match flag {
            "--consumer-slug" => set_once(&mut consumer_slug, flag, validate_slug(flag, value)?)?,
            "--format" => {
                set_seen(&mut seen_format, flag)?;
                format = parse_format(value)?
            }
            unknown if unknown.starts_with("--") => {
                return Err(ParseError::new(format!("unknown list flag {unknown:?}")));
            }
            unexpected => {
                return Err(ParseError::new(format!(
                    "unexpected list argument {unexpected:?}"
                )));
            }
        }
        index += 2;
    }

    Ok(AdminApiKeyCommand::List(ApiKeyListArgs {
        consumer_slug: required(consumer_slug, "--consumer-slug")?,
        format,
    }))
}

fn parse_usage(args: &[&str]) -> Result<AdminApiKeyCommand, ParseError> {
    let mut consumer_slug = None;
    let mut days = 30;
    let mut format = OutputFormat::Human;
    let mut seen_days = false;
    let mut seen_format = false;

    let mut index = 0;
    while index < args.len() {
        let flag = args[index];
        let value = flag_value(args, index)?;
        match flag {
            "--consumer-slug" => set_once(&mut consumer_slug, flag, validate_slug(flag, value)?)?,
            "--days" => {
                set_seen(&mut seen_days, flag)?;
                days = parse_positive_u32(flag, value)?
            }
            "--format" => {
                set_seen(&mut seen_format, flag)?;
                format = parse_format(value)?
            }
            unknown if unknown.starts_with("--") => {
                return Err(ParseError::new(format!("unknown usage flag {unknown:?}")));
            }
            unexpected => {
                return Err(ParseError::new(format!(
                    "unexpected usage argument {unexpected:?}"
                )));
            }
        }
        index += 2;
    }

    Ok(AdminApiKeyCommand::Usage(ApiKeyUsageArgs {
        consumer_slug: required(consumer_slug, "--consumer-slug")?,
        days,
        format,
    }))
}

fn flag_value<'a>(args: &'a [&str], index: usize) -> Result<&'a str, ParseError> {
    let flag = args[index];
    if !flag.starts_with("--") {
        return Err(ParseError::new(format!("unexpected argument {flag:?}")));
    }

    let Some(value) = args.get(index + 1).copied() else {
        return Err(ParseError::new(format!("{flag} requires a value")));
    };
    if value.starts_with("--") {
        return Err(ParseError::new(format!("{flag} requires a value")));
    }

    Ok(value)
}

fn set_once(target: &mut Option<String>, flag: &str, value: String) -> Result<(), ParseError> {
    if target.is_some() {
        return Err(ParseError::new(format!("duplicate {flag}")));
    }
    *target = Some(value);
    Ok(())
}

fn set_seen(seen: &mut bool, flag: &str) -> Result<(), ParseError> {
    if *seen {
        return Err(ParseError::new(format!("duplicate {flag}")));
    }
    *seen = true;
    Ok(())
}

fn required(value: Option<String>, flag: &str) -> Result<String, ParseError> {
    value.ok_or_else(|| ParseError::new(format!("{flag} is required")))
}

fn validate_slug(flag: &str, value: &str) -> Result<String, ParseError> {
    let trimmed = value.trim();
    if trimmed != value || !is_kebab_slug(trimmed) {
        return Err(ParseError::new(format!(
            "{flag} must be a lowercase kebab-case slug"
        )));
    }
    Ok(trimmed.to_string())
}

fn validate_key_prefix(value: &str) -> Result<String, ParseError> {
    let trimmed = value.trim();
    let Some(random_prefix) = trimmed.strip_prefix("ib_live_") else {
        return Err(ParseError::new(
            "--key-prefix must be a normalized ib_live_ key prefix",
        ));
    };
    if trimmed != value || random_prefix.len() != 16 || !is_lower_hex(random_prefix) {
        return Err(ParseError::new(
            "--key-prefix must be a normalized ib_live_ key prefix",
        ));
    }
    Ok(trimmed.to_string())
}

fn validate_non_empty(flag: &str, value: &str) -> Result<String, ParseError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ParseError::new(format!("{flag} must not be blank")));
    }
    Ok(trimmed.to_string())
}

fn validate_category(value: &str) -> Result<String, ParseError> {
    let trimmed = value.trim();
    if matches!(trimmed, "friend" | "partner" | "public" | "internal") {
        Ok(trimmed.to_string())
    } else {
        Err(ParseError::new(
            "--category must be one of friend, partner, public, or internal",
        ))
    }
}

fn validate_rfc3339(flag: &str, value: &str) -> Result<String, ParseError> {
    let trimmed = value.trim();
    if trimmed != value || parse_rfc3339(trimmed).is_none() {
        return Err(ParseError::new(format!(
            "{flag} must be a valid RFC3339 timestamp"
        )));
    }
    Ok(trimmed.to_string())
}

fn parse_non_negative_i32(flag: &str, value: &str) -> Result<i32, ParseError> {
    let parsed = value
        .parse::<i32>()
        .map_err(|_| ParseError::new(format!("{flag} must be a non-negative integer")))?;
    if parsed < 0 {
        return Err(ParseError::new(format!(
            "{flag} must be a non-negative integer"
        )));
    }
    Ok(parsed)
}

fn parse_positive_u32(flag: &str, value: &str) -> Result<u32, ParseError> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| ParseError::new(format!("{flag} must be a positive integer")))?;
    if parsed == 0 {
        return Err(ParseError::new(format!(
            "{flag} must be a positive integer"
        )));
    }
    Ok(parsed)
}

fn parse_format(value: &str) -> Result<OutputFormat, ParseError> {
    match value {
        "human" => Ok(OutputFormat::Human),
        "json" => Ok(OutputFormat::Json),
        _ => Err(ParseError::new("--format must be human or json")),
    }
}

fn is_kebab_slug(value: &str) -> bool {
    let mut previous_hyphen = false;
    if value.is_empty() || value.starts_with('-') || value.ends_with('-') {
        return false;
    }

    for byte in value.bytes() {
        match byte {
            b'a'..=b'z' | b'0'..=b'9' => previous_hyphen = false,
            b'-' if !previous_hyphen => previous_hyphen = true,
            _ => return false,
        }
    }

    true
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_help_arg(arg: &str) -> bool {
    matches!(arg, "-h" | "--help" | "help")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_args_defaults_to_serve() {
        assert_eq!(parse_args([] as [&str; 0]).unwrap(), Command::Serve);
    }

    #[test]
    fn parses_serve() {
        assert_eq!(parse_args(["serve"]).unwrap(), Command::Serve);
    }

    #[test]
    fn parses_db_commands() {
        assert_eq!(
            parse_args(["db", "migrate"]).unwrap(),
            Command::Db(DbCommand::Migrate)
        );
        assert_eq!(
            parse_args(["db", "apply-reference"]).unwrap(),
            Command::Db(DbCommand::ApplyReference)
        );
        assert_eq!(
            parse_args(["db", "apply"]).unwrap(),
            Command::Db(DbCommand::Apply)
        );
    }

    #[test]
    fn parses_help() {
        assert_eq!(parse_args(["--help"]).unwrap(), Command::Help);
        assert_eq!(parse_args(["db", "--help"]).unwrap(), Command::Help);
        assert_eq!(parse_args(["admin", "--help"]).unwrap(), Command::Help);
        assert_eq!(
            parse_args(["admin", "api-key", "--help"]).unwrap(),
            Command::Help
        );
    }

    #[test]
    fn parses_api_key_issue_command_with_defaults() {
        assert_eq!(
            parse_args([
                "admin",
                "api-key",
                "issue",
                "--consumer-slug",
                "first-customer",
                "--display-name",
                "First Customer",
                "--category",
                "partner",
                "--label",
                "beta key"
            ])
            .unwrap(),
            Command::Admin(AdminCommand::ApiKey(AdminApiKeyCommand::Issue(
                ApiKeyIssueArgs {
                    consumer_slug: "first-customer".to_string(),
                    display_name: "First Customer".to_string(),
                    category: "partner".to_string(),
                    label: "beta key".to_string(),
                    requests_per_minute: 60,
                    requests_per_day: 5000,
                    expires_at: None,
                    format: OutputFormat::Human,
                }
            )))
        );
    }

    #[test]
    fn parses_api_key_issue_command_with_options() {
        assert_eq!(
            parse_args([
                "admin",
                "api-key",
                "issue",
                "--consumer-slug",
                "first-customer",
                "--display-name",
                "First Customer",
                "--category",
                "partner",
                "--label",
                "beta key",
                "--requests-per-minute",
                "7",
                "--requests-per-day",
                "8",
                "--expires-at",
                "2026-09-30T00:00:00Z",
                "--format",
                "json"
            ])
            .unwrap(),
            Command::Admin(AdminCommand::ApiKey(AdminApiKeyCommand::Issue(
                ApiKeyIssueArgs {
                    consumer_slug: "first-customer".to_string(),
                    display_name: "First Customer".to_string(),
                    category: "partner".to_string(),
                    label: "beta key".to_string(),
                    requests_per_minute: 7,
                    requests_per_day: 8,
                    expires_at: Some("2026-09-30T00:00:00Z".to_string()),
                    format: OutputFormat::Json,
                }
            )))
        );
    }

    #[test]
    fn parses_revoke_list_and_usage_commands() {
        assert_eq!(
            parse_args([
                "admin",
                "api-key",
                "revoke",
                "--key-prefix",
                "ib_live_0123456789abcdef",
                "--format",
                "json"
            ])
            .unwrap(),
            Command::Admin(AdminCommand::ApiKey(AdminApiKeyCommand::Revoke(
                ApiKeyRevokeArgs {
                    key_prefix: "ib_live_0123456789abcdef".to_string(),
                    format: OutputFormat::Json,
                }
            )))
        );
        assert_eq!(
            parse_args([
                "admin",
                "api-key",
                "list",
                "--consumer-slug",
                "first-customer"
            ])
            .unwrap(),
            Command::Admin(AdminCommand::ApiKey(AdminApiKeyCommand::List(
                ApiKeyListArgs {
                    consumer_slug: "first-customer".to_string(),
                    format: OutputFormat::Human,
                }
            )))
        );
        assert_eq!(
            parse_args([
                "admin",
                "api-key",
                "usage",
                "--consumer-slug",
                "first-customer",
                "--days",
                "12"
            ])
            .unwrap(),
            Command::Admin(AdminCommand::ApiKey(AdminApiKeyCommand::Usage(
                ApiKeyUsageArgs {
                    consumer_slug: "first-customer".to_string(),
                    days: 12,
                    format: OutputFormat::Human,
                }
            )))
        );
    }

    #[test]
    fn rejects_unknown_commands() {
        assert_eq!(
            parse_args(["start"]).unwrap_err(),
            ParseError::new("unknown command \"start\"")
        );
        assert_eq!(
            parse_args(["db", "seed"]).unwrap_err(),
            ParseError::new("unknown db subcommand \"seed\"")
        );
        assert_eq!(
            parse_args(["admin", "api-key", "rotate"]).unwrap_err(),
            ParseError::new("unknown admin api-key subcommand \"rotate\"")
        );
    }

    #[test]
    fn rejects_incomplete_or_extra_db_usage() {
        assert_eq!(
            parse_args(["db"]).unwrap_err(),
            ParseError::new("db requires a subcommand")
        );
        assert_eq!(
            parse_args(["db", "migrate", "now"]).unwrap_err(),
            ParseError::new("db subcommand \"migrate\" does not accept extra arguments")
        );
    }

    #[test]
    fn rejects_missing_required_api_key_flags() {
        assert_eq!(
            parse_args(["admin", "api-key", "issue"]).unwrap_err(),
            ParseError::new("--consumer-slug is required")
        );
        assert_eq!(
            parse_args(["admin", "api-key", "revoke"]).unwrap_err(),
            ParseError::new("--key-prefix is required")
        );
        assert_eq!(
            parse_args(["admin", "api-key", "list"]).unwrap_err(),
            ParseError::new("--consumer-slug is required")
        );
        assert_eq!(
            parse_args(["admin", "api-key", "usage"]).unwrap_err(),
            ParseError::new("--consumer-slug is required")
        );
    }

    #[test]
    fn rejects_bad_admin_flag_values() {
        for args in [
            vec![
                "admin",
                "api-key",
                "issue",
                "--consumer-slug",
                "BadSlug",
                "--display-name",
                "Name",
                "--category",
                "partner",
                "--label",
                "label",
            ],
            vec![
                "admin",
                "api-key",
                "issue",
                "--consumer-slug",
                "ok",
                "--display-name",
                " ",
                "--category",
                "partner",
                "--label",
                "label",
            ],
            vec![
                "admin",
                "api-key",
                "issue",
                "--consumer-slug",
                "ok",
                "--display-name",
                "Name",
                "--category",
                "customer",
                "--label",
                "label",
            ],
            vec![
                "admin",
                "api-key",
                "issue",
                "--consumer-slug",
                "ok",
                "--display-name",
                "Name",
                "--category",
                "partner",
                "--label",
                "label",
                "--requests-per-day",
                "-1",
            ],
            vec![
                "admin",
                "api-key",
                "issue",
                "--consumer-slug",
                "ok",
                "--display-name",
                "Name",
                "--category",
                "partner",
                "--label",
                "label",
                "--expires-at",
                "tomorrow",
            ],
            vec![
                "admin",
                "api-key",
                "usage",
                "--consumer-slug",
                "ok",
                "--days",
                "0",
            ],
            vec![
                "admin",
                "api-key",
                "list",
                "--consumer-slug",
                "ok",
                "--format",
                "yaml",
            ],
        ] {
            assert!(parse_args(args).is_err());
        }
    }

    #[test]
    fn rejects_duplicate_unknown_and_missing_value_flags() {
        assert_eq!(
            parse_args([
                "admin",
                "api-key",
                "list",
                "--consumer-slug",
                "ok",
                "--consumer-slug",
                "again"
            ])
            .unwrap_err(),
            ParseError::new("duplicate --consumer-slug")
        );
        assert_eq!(
            parse_args([
                "admin",
                "api-key",
                "usage",
                "--consumer-slug",
                "ok",
                "--days",
                "3",
                "--days",
                "4"
            ])
            .unwrap_err(),
            ParseError::new("duplicate --days")
        );
        assert_eq!(
            parse_args([
                "admin",
                "api-key",
                "list",
                "--consumer-slug",
                "ok",
                "--extra",
                "x"
            ])
            .unwrap_err(),
            ParseError::new("unknown list flag \"--extra\"")
        );
        assert_eq!(
            parse_args(["admin", "api-key", "list", "--consumer-slug"]).unwrap_err(),
            ParseError::new("--consumer-slug requires a value")
        );
    }
}
