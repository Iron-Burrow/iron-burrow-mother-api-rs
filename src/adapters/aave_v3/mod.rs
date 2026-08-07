use fastnum::{decimal::Context, D512};
use num_bigint::BigUint;
use serde_json::{json, Value};

use crate::{adapters::bigwig::BigwigClient, domain::defi::RealizedYieldProtocol};

const SELECTOR: &str = "d15e0053";
const SECONDS_PER_YEAR: u64 = 31_536_000;

#[derive(Clone, Debug)]
pub(crate) struct Request {
    pub(crate) asset_slug: String,
    pub(crate) from_block: u64,
    pub(crate) to_block: u64,
    pub(crate) include_annualized_apy_estimate: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Result {
    pub(crate) asset_symbol: String,
    pub(crate) underlying_asset_address: String,
    pub(crate) from_index: String,
    pub(crate) to_index: String,
    pub(crate) realized_yield: String,
    pub(crate) from_timestamp: Option<u64>,
    pub(crate) to_timestamp: Option<u64>,
    pub(crate) annualized_apy_estimate: Option<String>,
    pub(crate) warnings: Vec<String>,
}

/// One verified Aave reserve normalized-income observation. It is the
/// reusable primitive behind both the realized-yield study and portfolio
/// simulations; callers supply the canonical block evidence they resolved.
#[derive(Clone, Debug)]
pub(crate) struct IncomeIndexObservation {
    pub(crate) asset_symbol: String,
    pub(crate) underlying_asset_address: String,
    pub(crate) income_index: String,
}

#[derive(Clone, Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("unsupported asset")]
    UnsupportedAsset,
    #[error("invalid block range")]
    InvalidBlockRange,
    #[error("block not final")]
    BlockNotFinal,
    #[error("archive provider failed")]
    Provider,
    #[error("invalid Aave response")]
    InvalidResponse,
    #[error("invalid income index")]
    InvalidIncomeIndex,
    #[error("calculation failed")]
    Calculation,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AaveV3RealizedYieldAdapter;

impl AaveV3RealizedYieldAdapter {
    pub(crate) async fn income_index_at(
        &self,
        protocol: &RealizedYieldProtocol,
        asset_slug: &str,
        block_number: u64,
        bigwig: &BigwigClient,
    ) -> std::result::Result<IncomeIndexObservation, Error> {
        if block_number == 0 {
            return Err(Error::InvalidBlockRange);
        }
        let reserve = protocol
            .reserves
            .iter()
            .find(|reserve| reserve.asset_slug == asset_slug)
            .ok_or(Error::UnsupportedAsset)?;
        let calldata = encode_get_reserve_normalized_income(&reserve.underlying_asset_address)?;
        let raw = bigwig
            .archive_rpc(
                "eth_call",
                json!([{"to": protocol.pool_address, "data": calldata}, format!("0x{block_number:x}")]),
            )
            .await
            .map_err(|_| Error::Provider)?;
        let income_index = decode_uint256(raw)?;
        if income_index == BigUint::from(0u8) {
            return Err(Error::InvalidIncomeIndex);
        }
        Ok(IncomeIndexObservation {
            asset_symbol: reserve.asset_symbol.clone(),
            underlying_asset_address: reserve.underlying_asset_address.clone(),
            income_index: income_index.to_string(),
        })
    }

    pub(crate) async fn resolve(
        &self,
        protocol: &RealizedYieldProtocol,
        request: Request,
        bigwig: &BigwigClient,
        min_confirmations: u64,
    ) -> std::result::Result<Result, Error> {
        if request.from_block == 0
            || request.to_block == 0
            || request.from_block >= request.to_block
        {
            return Err(Error::InvalidBlockRange);
        }
        let head = parse_hex_u64(
            bigwig
                .archive_rpc("eth_blockNumber", json!([]))
                .await
                .map_err(|_| Error::Provider)?,
        )?;
        if request.to_block > head.saturating_sub(min_confirmations) {
            return Err(Error::BlockNotFinal);
        }
        let (from, to) = tokio::try_join!(
            self.income_index_at(protocol, &request.asset_slug, request.from_block, bigwig),
            self.income_index_at(protocol, &request.asset_slug, request.to_block, bigwig),
        )?;
        let from_index = BigUint::parse_bytes(from.income_index.as_bytes(), 10)
            .ok_or(Error::InvalidIncomeIndex)?;
        let to_index = BigUint::parse_bytes(to.income_index.as_bytes(), 10)
            .ok_or(Error::InvalidIncomeIndex)?;
        let mut warnings = Vec::new();
        if to_index < from_index {
            warnings.push("decreasing_income_index".to_string());
        }
        let realized_yield = ratio_minus_one(&to_index, &from_index)?;
        let (from_timestamp, to_timestamp, annualized_apy_estimate) =
            if request.include_annualized_apy_estimate {
                let from_block = bigwig.archive_rpc(
                    "eth_getBlockByNumber",
                    json!([format!("0x{:x}", request.from_block), false]),
                );
                let to_block = bigwig.archive_rpc(
                    "eth_getBlockByNumber",
                    json!([format!("0x{:x}", request.to_block), false]),
                );
                let (from_block, to_block) = tokio::join!(from_block, to_block);
                match (
                    from_block
                        .ok()
                        .and_then(|value| block_timestamp(value).ok()),
                    to_block.ok().and_then(|value| block_timestamp(value).ok()),
                ) {
                    (Some(from_timestamp), Some(to_timestamp)) if to_timestamp > from_timestamp => {
                        let apy =
                            annualized_apy(&to_index, &from_index, to_timestamp - from_timestamp)?;
                        (Some(from_timestamp), Some(to_timestamp), Some(apy))
                    }
                    _ => {
                        warnings.push("timestamp_lookup_failed".to_string());
                        (None, None, None)
                    }
                }
            } else {
                (None, None, None)
            };
        Ok(Result {
            asset_symbol: from.asset_symbol,
            underlying_asset_address: from.underlying_asset_address,
            from_index: from_index.to_string(),
            to_index: to_index.to_string(),
            realized_yield,
            from_timestamp,
            to_timestamp,
            annualized_apy_estimate,
            warnings,
        })
    }
}

pub(crate) fn encode_get_reserve_normalized_income(
    address: &str,
) -> std::result::Result<String, Error> {
    let address = address.strip_prefix("0x").unwrap_or(address);
    if address.len() != 40 || !address.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::InvalidResponse);
    }
    Ok(format!("0x{SELECTOR}{:0>64}", address.to_ascii_lowercase()))
}

fn decode_uint256(value: Value) -> std::result::Result<BigUint, Error> {
    let value = value.as_str().ok_or(Error::InvalidResponse)?;
    let hex = value.strip_prefix("0x").ok_or(Error::InvalidResponse)?;
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::InvalidResponse);
    }
    BigUint::parse_bytes(hex.as_bytes(), 16).ok_or(Error::InvalidResponse)
}

fn parse_hex_u64(value: Value) -> std::result::Result<u64, Error> {
    u64::from_str_radix(
        value
            .as_str()
            .and_then(|value| value.strip_prefix("0x"))
            .ok_or(Error::InvalidResponse)?,
        16,
    )
    .map_err(|_| Error::InvalidResponse)
}

fn block_timestamp(value: Value) -> std::result::Result<u64, Error> {
    parse_hex_u64(
        value
            .get("timestamp")
            .cloned()
            .ok_or(Error::InvalidResponse)?,
    )
}

fn decimal(value: &BigUint) -> std::result::Result<D512, Error> {
    D512::from_str(&value.to_string(), Context::default()).map_err(|_| Error::Calculation)
}

fn ratio_minus_one(to: &BigUint, from: &BigUint) -> std::result::Result<String, Error> {
    let one = D512::from_str("1", Context::default()).map_err(|_| Error::Calculation)?;
    Ok(canonical_decimal(decimal(to)? / decimal(from)? - one))
}

fn annualized_apy(
    to: &BigUint,
    from: &BigUint,
    elapsed_seconds: u64,
) -> std::result::Result<String, Error> {
    let one = D512::from_str("1", Context::default()).map_err(|_| Error::Calculation)?;
    let seconds = D512::from_str(&elapsed_seconds.to_string(), Context::default())
        .map_err(|_| Error::Calculation)?;
    let year = D512::from_str(&SECONDS_PER_YEAR.to_string(), Context::default())
        .map_err(|_| Error::Calculation)?;
    Ok(canonical_decimal(
        (decimal(to)? / decimal(from)?).pow(year / seconds) - one,
    ))
}

fn canonical_decimal(value: D512) -> String {
    let value = value.to_string();
    if let Some((coefficient, exponent)) = value.split_once(['E', 'e']) {
        return expand_exponent(coefficient, exponent).unwrap_or(value);
    }
    value
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn expand_exponent(coefficient: &str, exponent: &str) -> Option<String> {
    let exponent = exponent.parse::<i32>().ok()?;
    let negative = coefficient.strip_prefix('-').is_some();
    let digits = coefficient.trim_start_matches('-').replace('.', "");
    let decimal_index = coefficient
        .trim_start_matches('-')
        .find('.')
        .unwrap_or(coefficient.trim_start_matches('-').len()) as i32;
    let index = decimal_index + exponent;
    let body = if index <= 0 {
        format!("0.{}{}", "0".repeat((-index) as usize), digits)
    } else if index as usize >= digits.len() {
        format!("{}{}", digits, "0".repeat(index as usize - digits.len()))
    } else {
        format!(
            "{}.{}",
            &digits[..index as usize],
            &digits[index as usize..]
        )
    };
    Some(
        format!("{}{}", if negative { "-" } else { "" }, body)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        annualized_apy, decode_uint256, encode_get_reserve_normalized_income, ratio_minus_one,
        Error,
    };
    use num_bigint::BigUint;

    #[test]
    fn reserve_income_call_uses_the_verified_aave_selector_and_word_encoding() {
        assert_eq!(
            encode_get_reserve_normalized_income("0xA0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")
                .unwrap(),
            "0xd15e0053000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        );
    }

    #[test]
    fn uint256_decoder_requires_exactly_one_abi_word() {
        assert_eq!(
            decode_uint256(json!(
                "0x000000000000000000000000000000000000000000000000000000000000002a"
            ))
            .unwrap(),
            BigUint::from(42u8)
        );
        assert!(matches!(
            decode_uint256(json!("0x2a")),
            Err(Error::InvalidResponse)
        ));
        assert!(matches!(
            decode_uint256(json!(42)),
            Err(Error::InvalidResponse)
        ));
    }

    #[test]
    fn math_is_deterministic_non_scientific_and_handles_decreases() {
        assert_eq!(
            ratio_minus_one(&BigUint::from(1_020u64), &BigUint::from(1_000u64)).unwrap(),
            "0.02"
        );
        assert_eq!(
            ratio_minus_one(&BigUint::from(990u64), &BigUint::from(1_000u64)).unwrap(),
            "-0.01"
        );
        let annualized =
            annualized_apy(&BigUint::from(1_001u64), &BigUint::from(1_000u64), 86_400).unwrap();
        assert!(!annualized.contains(['E', 'e']));
    }
}
