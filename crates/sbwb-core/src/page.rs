use serde::{Deserialize, Serialize};
use std::fmt;

/// Zero-based physical page index into the source PDF.
///
/// Displayed to users as `index + 1` ("Page 48"); printed page labels are a
/// separate concept (UX-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PageIndex(pub u32);

impl PageIndex {
    pub fn number(self) -> u32 {
        self.0 + 1
    }
    pub fn from_number(n: u32) -> Option<Self> {
        n.checked_sub(1).map(PageIndex)
    }
}

impl fmt::Display for PageIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "p.{}", self.number())
    }
}

/// Which physical pages are in the processing scope (PRJ-03).
///
/// Ranges are inclusive page numbers (1-based) as the user sees them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PageScope {
    pub ranges: Vec<(u32, u32)>,
}

impl PageScope {
    pub fn first_n(n: u32, total: u32) -> Self {
        let end = n.min(total);
        if end == 0 {
            return Self::default();
        }
        Self {
            ranges: vec![(1, end)],
        }
    }
    pub fn all(total: u32) -> Self {
        Self::first_n(total, total)
    }
    pub fn from_ranges(mut ranges: Vec<(u32, u32)>, total: u32) -> Self {
        ranges.retain(|(a, b)| *a >= 1 && a <= b && *a <= total);
        for r in &mut ranges {
            r.1 = r.1.min(total);
        }
        ranges.sort_unstable();
        // merge overlapping / adjacent
        let mut merged: Vec<(u32, u32)> = Vec::new();
        for (a, b) in ranges {
            if let Some(last) = merged.last_mut() {
                if a <= last.1 + 1 {
                    last.1 = last.1.max(b);
                    continue;
                }
            }
            merged.push((a, b));
        }
        Self { ranges: merged }
    }
    pub fn contains(&self, p: PageIndex) -> bool {
        let n = p.number();
        self.ranges.iter().any(|(a, b)| n >= *a && n <= *b)
    }
    pub fn len(&self) -> u32 {
        self.ranges.iter().map(|(a, b)| b - a + 1).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn pages(&self) -> impl Iterator<Item = PageIndex> + '_ {
        self.ranges
            .iter()
            .flat_map(|(a, b)| (*a..=*b).map(|n| PageIndex(n - 1)))
    }
    /// Human label such as "1–50" or "1–50, 60–70".
    pub fn label(&self) -> String {
        self.ranges
            .iter()
            .map(|(a, b)| {
                if a == b {
                    a.to_string()
                } else {
                    format!("{a}–{b}")
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
    /// Extend to include another scope (A-02).
    pub fn union(&self, other: &PageScope, total: u32) -> PageScope {
        let mut r = self.ranges.clone();
        r.extend(other.ranges.iter().copied());
        PageScope::from_ranges(r, total)
    }
}

/// Processing status of a page in the scope (PRJ-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageStatus {
    /// In the source but outside the processing scope.
    Unprocessed,
    /// Explicitly excluded by the user.
    Excluded,
    Queued,
    Running,
    Failed,
    Done,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_merges_and_counts() {
        let s = PageScope::from_ranges(vec![(10, 20), (1, 5), (6, 8), (19, 25), (600, 700)], 529);
        assert_eq!(s.ranges, vec![(1, 8), (10, 25)]);
        assert_eq!(s.len(), 24);
        assert!(s.contains(PageIndex(0)));
        assert!(!s.contains(PageIndex(8)));
        assert_eq!(s.label(), "1–8, 10–25");
    }

    #[test]
    fn extend_trial() {
        let trial = PageScope::first_n(50, 529);
        let more = trial.union(&PageScope::from_ranges(vec![(51, 110)], 529), 529);
        assert_eq!(more.ranges, vec![(1, 110)]);
    }
}
