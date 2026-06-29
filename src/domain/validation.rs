pub(crate) fn is_asset_slug(asset_slug: &str) -> bool {
    if asset_slug.is_empty()
        || asset_slug.starts_with('-')
        || asset_slug.ends_with('-')
        || asset_slug.contains("--")
    {
        return false;
    }

    asset_slug.bytes().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == b'-'
    })
}

pub(crate) fn is_evm_address(address: &str) -> bool {
    address.len() == 42
        && address.starts_with("0x")
        && address.as_bytes()[2..]
            .iter()
            .all(|character| character.is_ascii_hexdigit())
}
