use crate::domain::assets::global_assets::GlobalAsset;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AssetMatch {
    pub(crate) asset: GlobalAsset,
    pub(crate) confidence: ExactMatchConfidence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExactMatchConfidence {
    Slug,
    Symbol,
    Name,
    Alias,
}

impl ExactMatchConfidence {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Slug => "slug_exact",
            Self::Symbol => "symbol_exact",
            Self::Name => "name_exact",
            Self::Alias => "alias_exact",
        }
    }
}

pub(crate) fn confidence_rank(confidence: ExactMatchConfidence) -> u8 {
    match confidence {
        ExactMatchConfidence::Slug => 0,
        ExactMatchConfidence::Symbol => 1,
        ExactMatchConfidence::Name => 2,
        ExactMatchConfidence::Alias => 3,
    }
}
