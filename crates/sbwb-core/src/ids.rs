use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! id_type {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
            pub fn parse(s: &str) -> Option<Self> {
                Uuid::parse_str(s).ok().map(Self)
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
        impl From<$name> for String {
            fn from(v: $name) -> String {
                v.0.to_string()
            }
        }
    };
}

id_type!(ProjectId, "Identity of a project file.");
id_type!(RegionId, "A layout region on a page.");
id_type!(LineId, "A physical text line inside a region.");
id_type!(WordId, "A recognized word (raw OCR evidence).");
id_type!(SpanId, "A span of effective text with source anchors.");
id_type!(IssueId, "A review issue.");
id_type!(ProposalId, "A proposed change to a span.");
id_type!(DecisionId, "A recorded human decision.");
id_type!(JobId, "A processing job.");
id_type!(HistoryId, "A history entry.");
id_type!(ExportId, "An export run.");
id_type!(RunId, "One execution of an engine or algorithm.");
