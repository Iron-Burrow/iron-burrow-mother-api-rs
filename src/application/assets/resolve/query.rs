use super::constants::MAX_QUERY_LENGTH;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedQuery {
    pub raw: String,
    pub normalized: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryValidationError {
    Missing,
    Empty,
    TooLong,
}

pub fn parse_query(value: Option<&str>) -> Result<NormalizedQuery, QueryValidationError> {
    let value = value.ok_or(QueryValidationError::Missing)?;
    let raw = value.trim();

    if raw.is_empty() {
        return Err(QueryValidationError::Empty);
    }

    if raw.chars().count() > MAX_QUERY_LENGTH {
        return Err(QueryValidationError::TooLong);
    }

    Ok(NormalizedQuery {
        raw: raw.to_string(),
        normalized: normalize_query(raw),
    })
}

pub fn normalize_query(value: &str) -> String {
    let mut cleaned = String::with_capacity(value.len());

    for character in value.trim().to_lowercase().chars() {
        let folded = fold_common_accent(character);

        if folded.is_alphanumeric() || folded.is_whitespace() {
            cleaned.push(folded);
        } else {
            cleaned.push(' ');
        }
    }

    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn fold_common_accent(character: char) -> char {
    match character {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'ē' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'ī' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ō' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'ū' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_case_whitespace_punctuation_and_accents() {
        assert_eq!(normalize_query("  Óro,   de--Ley! "), "oro de ley");
        assert_eq!(normalize_query("USDC   coin   USD"), "usdc coin usd");
    }

    #[test]
    fn validates_required_query() {
        assert_eq!(
            parse_query(None).unwrap_err(),
            QueryValidationError::Missing
        );
        assert_eq!(
            parse_query(Some("")).unwrap_err(),
            QueryValidationError::Empty
        );
        assert_eq!(
            parse_query(Some("   ")).unwrap_err(),
            QueryValidationError::Empty
        );
    }

    #[test]
    fn rejects_overlong_query() {
        let query = "a".repeat(MAX_QUERY_LENGTH + 1);
        assert_eq!(
            parse_query(Some(&query)).unwrap_err(),
            QueryValidationError::TooLong
        );
    }
}
