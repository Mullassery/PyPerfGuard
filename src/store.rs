//! Results store: versioned local persistence and history retrieval.
//!
//! Layout under the configured output directory:
//!
//! ```text
//! .pyperfguard/
//!   index.json
//!   results/<benchmark-id>/<timestamp>.json
//!   baselines.json
//! ```
//!
//! The index is a convenience cache. If it is missing or corrupt it can be
//! rebuilt from the result files, so a corrupted index never loses data.
//! Writes are atomic (temp file + rename) so an interrupted write cannot
//! produce a half-written artifact.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::git;
use crate::results::{BenchmarkResult, BenchmarkStatus, RevisionInfo, SCHEMA_VERSION};

/// One entry in the history index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexEntry {
    /// Benchmark id.
    pub benchmark_id: String,
    /// Path to the result file, relative to the store root.
    pub file: String,
    /// Run start timestamp.
    pub started_at: String,
    /// Run finish timestamp.
    pub finished_at: String,
    /// Overall status.
    pub status: BenchmarkStatus,
    /// Revision metadata.
    pub revision: RevisionInfo,
    /// Schema version of the referenced file.
    pub schema_version: u32,
    /// Tool version that produced it.
    pub tool_version: String,
}

impl IndexEntry {
    fn from_result(file: String, result: &BenchmarkResult) -> IndexEntry {
        IndexEntry {
            benchmark_id: result.benchmark_id.clone(),
            file,
            started_at: result.started_at.clone(),
            finished_at: result.finished_at.clone(),
            status: result.status,
            revision: result.revision.clone(),
            schema_version: result.schema_version,
            tool_version: result.tool.version.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct IndexFile {
    schema_version: u32,
    entries: Vec<IndexEntry>,
}

/// A local-first store of benchmark results.
#[derive(Debug, Clone)]
pub struct ResultStore {
    root: PathBuf,
}

fn sanitize_component(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

impl ResultStore {
    /// Open (and lazily create) a store rooted at `root`.
    pub fn open(root: impl Into<PathBuf>) -> Result<ResultStore> {
        let store = ResultStore { root: root.into() };
        store.ensure_dirs()?;
        Ok(store)
    }

    /// The store root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn results_dir(&self) -> PathBuf {
        self.root.join("results")
    }

    fn index_path(&self) -> PathBuf {
        self.root.join("index.json")
    }

    fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(self.results_dir())
            .map_err(|e| Error::io("creating results directory", e))?;
        Ok(())
    }

    fn load_index(&self) -> Result<IndexFile> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(IndexFile {
                schema_version: SCHEMA_VERSION,
                entries: Vec::new(),
            });
        }
        let text = fs::read_to_string(&path)
            .map_err(|e| Error::io(format!("reading index {}", path.display()), e))?;
        match serde_json::from_str::<IndexFile>(&text) {
            Ok(idx) => Ok(idx),
            Err(_) => {
                // Corrupt index: rebuild from the result files.
                self.rebuild_index()
            }
        }
    }

    fn write_index(&self, index: &IndexFile) -> Result<()> {
        let text =
            serde_json::to_string_pretty(index).map_err(|e| Error::Serialization(e.to_string()))?;
        atomic_write(&self.index_path(), text.as_bytes())
    }

    /// Rebuild the index by scanning every result file. Returns the rebuilt
    /// index.
    fn rebuild_index(&self) -> Result<IndexFile> {
        let mut entries = Vec::new();
        let results_dir = self.results_dir();
        if results_dir.is_dir() {
            for bench_dir in read_dir_sorted(&results_dir)? {
                if !bench_dir.is_dir() {
                    continue;
                }
                for file in read_dir_sorted(&bench_dir)? {
                    if file.extension().and_then(|e| e.to_str()) != Some("json") {
                        continue;
                    }
                    match read_result_file(&file) {
                        Ok(result) => {
                            let rel = file
                                .strip_prefix(&self.root)
                                .unwrap_or(&file)
                                .to_string_lossy()
                                .to_string();
                            entries.push(IndexEntry::from_result(rel, &result));
                        }
                        Err(e) => {
                            // Skip unreadable artifact but keep going.
                            eprintln!(
                                "warning: skipping unreadable result {}: {e}",
                                file.display()
                            );
                        }
                    }
                }
            }
        }
        entries.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        let index = IndexFile {
            schema_version: SCHEMA_VERSION,
            entries,
        };
        self.write_index(&index)?;
        Ok(index)
    }

    /// Persist a result and update the index. Returns the path written.
    pub fn save(&self, result: &BenchmarkResult) -> Result<PathBuf> {
        result.validate()?;
        let bench_dir = self
            .results_dir()
            .join(sanitize_component(&result.benchmark_id));
        fs::create_dir_all(&bench_dir).map_err(|e| Error::io("creating benchmark directory", e))?;
        let filename = format!(
            "{}-{}.json",
            crate::time_util::now_compact_unique(),
            git::short_sha(result.revision.commit.as_deref().unwrap_or("nogit"))
        );
        let path = bench_dir.join(&filename);
        let json = serde_json::to_string_pretty(result)
            .map_err(|e| Error::Serialization(e.to_string()))?;
        atomic_write(&path, json.as_bytes())?;

        let mut index = self.load_index()?;
        let rel = path
            .strip_prefix(&self.root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        index.entries.retain(|e| e.file != rel);
        index.entries.push(IndexEntry::from_result(rel, result));
        index
            .entries
            .sort_by(|a, b| b.started_at.cmp(&a.started_at));
        self.write_index(&index)?;
        Ok(path)
    }

    /// List history entries, optionally filtered to one benchmark, newest
    /// first.
    pub fn list(&self, benchmark_id: Option<&str>) -> Result<Vec<IndexEntry>> {
        let index = self.load_index()?;
        let mut entries: Vec<IndexEntry> = index
            .entries
            .into_iter()
            .filter(|e| benchmark_id.map(|b| e.benchmark_id == b).unwrap_or(true))
            .collect();
        entries.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(entries)
    }

    /// Load a result by its index entry.
    pub fn load_entry(&self, entry: &IndexEntry) -> Result<BenchmarkResult> {
        let path = self.root.join(&entry.file);
        let result = read_result_file(&path)?;
        result.validate()?;
        Ok(result)
    }

    /// Load a result from an arbitrary path, validating it.
    pub fn load_path(&self, path: &Path) -> Result<BenchmarkResult> {
        let result = read_result_file(path)?;
        result.validate()?;
        Ok(result)
    }

    /// The most recent entry for a benchmark, optionally excluding a file.
    pub fn latest(
        &self,
        benchmark_id: &str,
        exclude_file: Option<&str>,
    ) -> Result<Option<IndexEntry>> {
        let entries = self.list(Some(benchmark_id))?;
        Ok(entries
            .into_iter()
            .find(|e| Some(e.file.as_str()) != exclude_file))
    }

    /// Find the most recent entry for a benchmark whose commit starts with
    /// `sha_prefix`.
    pub fn find_by_revision(
        &self,
        benchmark_id: &str,
        sha_prefix: &str,
    ) -> Result<Option<IndexEntry>> {
        let entries = self.list(Some(benchmark_id))?;
        Ok(entries.into_iter().find(|e| {
            e.revision
                .commit
                .as_deref()
                .map(|c| c.starts_with(sha_prefix))
                .unwrap_or(false)
        }))
    }

    /// Enforce retention: keep at most `retain` results per benchmark,
    /// deleting the oldest. `retain == 0` means unlimited.
    pub fn enforce_retention(&self, retain: usize) -> Result<usize> {
        if retain == 0 {
            return Ok(0);
        }
        let mut index = self.load_index()?;
        let mut by_bench: std::collections::BTreeMap<String, Vec<IndexEntry>> =
            std::collections::BTreeMap::new();
        for e in &index.entries {
            by_bench
                .entry(e.benchmark_id.clone())
                .or_default()
                .push(e.clone());
        }
        let mut to_remove: Vec<String> = Vec::new();
        for (_, mut entries) in by_bench {
            entries.sort_by(|a, b| b.started_at.cmp(&a.started_at));
            for e in entries.into_iter().skip(retain) {
                to_remove.push(e.file);
            }
        }
        for rel in &to_remove {
            let path = self.root.join(rel);
            if path.exists() {
                let _ = fs::remove_file(&path);
            }
        }
        index.entries.retain(|e| !to_remove.contains(&e.file));
        self.write_index(&index)?;
        Ok(to_remove.len())
    }
}

fn read_result_file(path: &Path) -> Result<BenchmarkResult> {
    let text = fs::read_to_string(path)
        .map_err(|e| Error::io(format!("reading result {}", path.display()), e))?;
    // Accept either a single BenchmarkResult or a Vec<BenchmarkResult> (e.g. from `run --output`).
    // If it's a vector, prefer the last element (most recent) when multiple exist.
    if let Ok(vec) = serde_json::from_str::<Vec<BenchmarkResult>>(&text) {
        return vec
            .into_iter()
            .next_back()
            .ok_or_else(|| {
                Error::Serialization(format!("empty result array in {}", path.display()))
            })
            .and_then(|r| r.validate().map(|_| r));
    }
    serde_json::from_str::<BenchmarkResult>(&text)
        .map_err(|e| Error::Serialization(format!("parsing {}: {e}", path.display())))
}

fn read_dir_sorted(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| Error::io(format!("reading dir {}", dir.display()), e))?
        .filter_map(|r| r.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    Ok(entries)
}

/// Write `data` to `path` atomically using a temporary file and rename.
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| Error::io("creating directory", e))?;
    let tmp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("result"),
        std::process::id()
    ));
    {
        let mut f = fs::File::create(&tmp)
            .map_err(|e| Error::io(format!("creating temp file {}", tmp.display()), e))?;
        f.write_all(data)
            .map_err(|e| Error::io("writing temp file", e))?;
        f.sync_all()
            .map_err(|e| Error::io("syncing temp file", e))?;
    }
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        Error::io(
            format!("renaming {} -> {}", tmp.display(), path.display()),
            e,
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::results::{EnvironmentInfo, Observation, ObservationStatus, Phase};

    fn result(id: &str, started: &str, commit: Option<&str>, wall: f64) -> BenchmarkResult {
        let obs = Observation {
            index: 0,
            phase: Phase::Measured,
            status: ObservationStatus::Ok,
            success: true,
            exit_code: Some(0),
            signal: None,
            wall_time_ms: Some(wall),
            cpu_time_ms: Some(wall * 0.5),
            peak_rss_bytes: Some(1024),
            cpu_utilization_pct: Some(50.0),
            stdout: None,
            stderr: None,
            error: None,
        };
        BenchmarkResult::build(
            id.into(),
            String::new(),
            vec!["echo".into()],
            None,
            vec![crate::metric::Metric::WallTime],
            RevisionInfo {
                commit: commit.map(|s| s.to_string()),
                branch: None,
                dirty: None,
            },
            EnvironmentInfo::current(),
            started.into(),
            started.into(),
            0,
            1,
            vec![obs],
            vec![],
        )
    }

    #[test]
    fn save_and_list_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ResultStore::open(tmp.path()).unwrap();
        store
            .save(&result("bench", "2026-01-01T00:00:00Z", Some("abc"), 1.0))
            .unwrap();
        store
            .save(&result("bench", "2026-01-02T00:00:00Z", Some("def"), 2.0))
            .unwrap();
        let list = store.list(Some("bench")).unwrap();
        assert_eq!(list.len(), 2);
        // Newest first.
        assert_eq!(list[0].started_at, "2026-01-02T00:00:00Z");
    }

    #[test]
    fn latest_excludes_current() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ResultStore::open(tmp.path()).unwrap();
        store
            .save(&result("b", "2026-01-01T00:00:00Z", Some("a"), 1.0))
            .unwrap();
        let first = store.list(Some("b")).unwrap().remove(0);
        assert!(store.latest("b", Some(&first.file)).unwrap().is_none());
    }

    #[test]
    fn find_by_revision_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ResultStore::open(tmp.path()).unwrap();
        store
            .save(&result(
                "b",
                "2026-01-01T00:00:00Z",
                Some("abcdef0123"),
                1.0,
            ))
            .unwrap();
        assert!(store.find_by_revision("b", "abcdef").unwrap().is_some());
        assert!(store.find_by_revision("b", "zzz").unwrap().is_none());
    }

    #[test]
    fn retention_removes_oldest() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ResultStore::open(tmp.path()).unwrap();
        for i in 0..5 {
            store
                .save(&result(
                    "b",
                    &format!("2026-01-0{}T00:00:00Z", i + 1),
                    Some("a"),
                    i as f64,
                ))
                .unwrap();
        }
        let removed = store.enforce_retention(2).unwrap();
        assert_eq!(removed, 3);
        assert_eq!(store.list(Some("b")).unwrap().len(), 2);
    }

    #[test]
    fn corrupt_index_is_rebuilt() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ResultStore::open(tmp.path()).unwrap();
        store
            .save(&result("b", "2026-01-01T00:00:00Z", Some("a"), 1.0))
            .unwrap();
        fs::write(store.index_path(), b"{ this is not json").unwrap();
        let list = store.list(Some("b")).unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn load_rejects_future_schema() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ResultStore::open(tmp.path()).unwrap();
        let mut r = result("b", "2026-01-01T00:00:00Z", Some("a"), 1.0);
        r.schema_version = SCHEMA_VERSION + 5;
        let path = tmp.path().join("weird.json");
        fs::write(&path, serde_json::to_string(&r).unwrap()).unwrap();
        assert!(store.load_path(&path).is_err());
    }
}
