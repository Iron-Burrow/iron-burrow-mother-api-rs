use crate::domain::global_assets::GlobalAsset;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetMatch {
    pub asset: GlobalAsset,
    pub confidence: MatchConfidence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatchConfidence {
    SlugExact,
    SymbolExact,
    NameExact,
    AliasExact,
}

impl MatchConfidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SlugExact => "slug_exact",
            Self::SymbolExact => "symbol_exact",
            Self::NameExact => "name_exact",
            Self::AliasExact => "alias_exact",
        }
    }
}

pub(crate) fn confidence_rank(confidence: MatchConfidence) -> u8 {
    match confidence {
        MatchConfidence::SlugExact => 0,
        MatchConfidence::SymbolExact => 1,
        MatchConfidence::NameExact => 2,
        MatchConfidence::AliasExact => 3,
    }
}
