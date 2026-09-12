# Durability matrix (NFR-10, M9.4)

Each row names the failure, what must hold, and the automated evidence.
Manual rows are marked; they are part of release qualification (M9.5).

| Failure | Must hold | Evidence |
| --- | --- | --- |
| Application crash after a saved decision | The decision is durable; an unsaved Edit draft is offered on reopen, never applied silently; the stale writer lock is taken over | `crates/sbwb-store/tests/durability.rs::crash_keeps_acknowledged_edits_and_recoverable_drafts` |
| Another live process holds the writer lock | The book opens read-only and every write is refused with a clear message | `project::tests::foreign_live_lock_forces_read_only`, `durability::read_only_open_refuses_writes_without_corrupting` |
| Worker process exits or hangs | The page fails with the cause recorded, other pages continue, the run is retryable | `scheduler::tests::timeout_fails_the_page_and_continues` (timeout + grace kill), `worker_died` handling in `sbwb-pipeline/src/scheduler.rs` |
| Processing cancelled or interrupted mid-run | Finished pages stay done, running pages return to queued, a later run resumes without redoing work | `scheduler::tests::cancel_keeps_finished_pages_and_leaves_the_rest_queued`, `sbwb-bench --full` (resume test on the 529-page scan, `docs/benchmarks/`) |
| Grouped correction with a stale match | Nothing is applied until the selection is refreshed; the apply is one transaction | `review::tests::approval_and_group_apply_are_guarded` |
| Cross-page join with a missing fragment | The join rolls back entirely | `durability::cross_page_join_is_atomic` |
| Undo after a later unrelated edit | The undo is refused instead of overwriting the later edit | `review::tests::decide_undo_and_carry_over` (revision guard) |
| Disk full or unwritable destination on save-copy | The copy fails, the live project stays intact and writable | `durability::save_copy_failure_leaves_the_project_intact` |
| Export fails validation or is cancelled | No file is published (the partial is never renamed), earlier exports are untouched | `sbwb-export/tests/export.rs::clean_copy_is_gated_on_approval`; `run()` writes `<name>.docx.partial` and renames only after validation |
| Source PDF altered on disk | Integrity check fails loudly; the embedded copy is used | `project::tests::integrity_mismatch_is_detected` |
| Schema newer than the build | The file is refused without modification | `Project::open` version check (`schema::SCHEMA_VERSION`) |
| Power loss during a write | SQLite WAL with `synchronous=NORMAL`; committed transactions survive, the WAL replays on open | SQLite guarantee; manual pull-the-plug test pending (M9.5) |

Drafts are distinguished from acknowledged data everywhere: they live in
the `drafts` table, are shown with a "Restored an unsaved edit" notice, and
never enter the transcript, the issue index, or an export until saved.
