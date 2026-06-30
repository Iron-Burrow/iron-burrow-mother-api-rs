#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Command {
    Serve,
    Help,
    Db(DbCommand),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DbCommand {
    Migrate,
    ApplyReference,
    Apply,
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
        [command, ..] => Err(ParseError::new(format!("unknown command {command:?}"))),
    }
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
}
