# Benchmarks

Reports in this folder are produced by `sbwb-bench` (M9.1, M9.2). Every
report records host, OS, CPU, memory, build profile, git revision, model
and binary hashes, the input hash, the cache state, and failures, so a
number can never be quoted without its conditions (NFR-02).

```bash
cargo build -p sbwb-bench
./target/debug/sbwb-bench --pages 30          # throughput 1 vs 4 workers, interaction, export
./target/debug/sbwb-bench --full --no-throughput   # 529-page scan with resource sampling and resume
```

Page navigation is measured in the running app, from asking for a page
to its scan image being in the DOM (M2.6):

```bash
scripts/nav-bench.sh path/to/book.sbwb     # Git Bash; Vite dev server running, debug app built
```

Use a release build (`cargo build --release -p sbwb-bench`) for numbers
that represent the shipped binary; debug builds are reported as such.

Targets (from `docs/requirements.md`):

| Measure | Target |
| --- | --- |
| Four workers vs one on the CPU OCR fixture | at least 2× throughput (NFR-07) |
| Aggregate resident memory during processing | at most 6 GB (NFR-04) |
| Project summary p95 | at most 3 s (NFR-03) |
| Next-issue query p95 | at most 300 ms (NFR-03) |
| Uncached preview p95 | at most 2 s (NFR-03) |
| Cached page navigation p95 | at most 250 ms (NFR-03) |
| 250,000-word export with validation | at most 60 s (NFR-05) |
| Resume after interruption | no page recognised twice, every page done (A-14) |
