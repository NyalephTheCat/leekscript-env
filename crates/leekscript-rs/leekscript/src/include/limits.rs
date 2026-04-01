//! Caps for transitive `include` loading (distinct source files).

/// Bounds applied while discovering **distinct** source files in an include graph.
///
/// Typical check before inserting a new canonical path: abort if
/// `visited.len() > max_distinct_files` (matches the reference compiler’s main-block include pass).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IncludeLimits {
    /// Threshold for `visited.len() > max_distinct_files` before accepting another distinct file.
    pub max_distinct_files: usize,
}

impl IncludeLimits {
    /// No practical cap (`usize::MAX`, so the length check never triggers).
    pub const UNLIMITED: Self = Self {
        max_distinct_files: usize::MAX,
    };

    /// Same threshold as the reference LeekScript compiler (`mIncludedFirstPass.size() > 500`).
    pub const REFERENCE: Self = Self {
        max_distinct_files: 500,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_distinct() {
        assert_ne!(IncludeLimits::UNLIMITED, IncludeLimits::REFERENCE);
        assert!(
            IncludeLimits::UNLIMITED.max_distinct_files
                > IncludeLimits::REFERENCE.max_distinct_files
        );
    }
}
