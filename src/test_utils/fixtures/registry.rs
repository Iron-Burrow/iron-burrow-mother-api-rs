use std::sync::Arc;

use crate::domain::canonical_registry::CanonicalRegistry;

pub(crate) fn embedded_canonical_registry() -> Arc<CanonicalRegistry> {
    Arc::new(
        CanonicalRegistry::from_embedded_catalog()
            .expect("embedded catalog should construct the canonical registry"),
    )
}
