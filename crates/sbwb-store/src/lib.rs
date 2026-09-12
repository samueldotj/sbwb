//! The `.sbwb` project file (PRJ-01, PRJ-02, PRJ-03, D-09).
//!
//! One SQLite database holds everything the project needs to be portable:
//! the verified source PDF as a blob, page table and scope, run records
//! with raw OCR evidence, history, and export log. A rebuildable cache
//! directory next to the file (`<name>.sbwb.cache/`) holds the extracted
//! source PDF for the renderer and page renders; it is never required to
//! open or export a project.

pub mod lock;
pub mod project;
pub mod schema;

pub use lock::{LockState, WriterLock};
pub use project::{
    OpenMode, PageCounts, PageLayout, PageOcr, PageRow, Project, ProjectMeta, ProjectSummary,
    RunRecord, SourceInfo,
};
pub use schema::SCHEMA_VERSION;

/// File extension for project files.
pub const PROJECT_EXTENSION: &str = "sbwb";

/// Cache directory beside a project file.
pub fn cache_dir_for(project_path: &std::path::Path) -> std::path::PathBuf {
    let mut name = project_path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".cache");
    project_path.with_file_name(name)
}
