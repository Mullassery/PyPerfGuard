//! Baseline manager: identification, selection, validation, and persistence.
//!
//! A baseline is a previously recorded [`BenchmarkResult`]. Baselines are
//! *referenced*, not copied, so the underlying result remains part of history.
//! A registry (`baselines.json`) records the selected baseline per benchmark.
//!
//! Selection supports several sources and always reports *why* a baseline was
//! chosen so the user can inspect it. PyPerfGuard never checks out a git
//! revision on the user's behalf; a "revision" baseline means a stored result
//! whose recorded commit matches.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::results::{BenchmarkResult, RevisionInfo, SCHEMA_VERSION};
use crate::store::{atomic_write, ResultStore};

/// A registered baseline pointer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineEntry {
    /// Benchmark id.
    pub benchmark_id: String,
    /// Result file path relative to the store root (or an absolute path
    /// supplied by the user).
    pub file: String,
    /// Timestamp of the baselined result.
    pub started_at: String,
    /// Revision of the baselined result.
    pub revision: RevisionInfo,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct BaselinesFile {
    #[serde(default)]
    schema_version: u32,
    #[serde(default)]
    entries: BTreeMap<String, BaselineEntry>,
}

/// Manages the baseline registry within a store.
#[derive(Debug, Clone)]
pub struct BaselineManager {
    store: ResultStore,
}

/// How a baseline was selected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineSelection {
    /// The baseline result.
    pub result: BenchmarkResult,
    /// Human-readable description of the source (for transparency).
    pub source: String,
    /// File path the baseline was loaded from, when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

impl BaselineManager {
    /// Create a manager over a store.
    pub fn new(store: ResultStore) -> BaselineManager {
        BaselineManager { store }
    }

    /// The underlying store.
    pub fn store(&self) -> &ResultStore {
        &self.store
    }

    fn registry_path(&self) -> std::path::PathBuf {
        self.store.root().join("baselines.json")
    }

    fn load_registry(&self) -> Result<BaselinesFile> {
        let path = self.registry_path();
        if !path.exists() {
            return Ok(BaselinesFile {
                schema_version: SCHEMA_VERSION,
                entries: BTreeMap::new(),
            });
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| Error::io(format!("reading {}", path.display()), e))?;
        serde_json::from_str(&text)
            .map_err(|e| Error::Serialization(format!("parsing {}: {e}", path.display())))
    }

    fn save_registry(&self, reg: &BaselinesFile) -> Result<()> {
        let text =
            serde_json::to_string_pretty(reg).map_err(|e| Error::Serialization(e.to_string()))?;
        atomic_write(&self.registry_path(), text.as_bytes())
    }

    /// Register a stored result as the baseline for its benchmark.
    pub fn register(&self, file: &str, result: &BenchmarkResult) -> Result<()> {
        let mut reg = self.load_registry()?;
        reg.schema_version = SCHEMA_VERSION;
        reg.entries.insert(
            result.benchmark_id.clone(),
            BaselineEntry {
                benchmark_id: result.benchmark_id.clone(),
                file: file.to_string(),
                started_at: result.started_at.clone(),
                revision: result.revision.clone(),
            },
        );
        self.save_registry(&reg)
    }

    /// Register the most recent stored result for a benchmark.
    pub fn register_latest(&self, benchmark_id: &str) -> Result<BaselineEntry> {
        let entry = self
            .store
            .latest(benchmark_id, None)?
            .ok_or_else(|| Error::NotFound(format!("no stored results for '{benchmark_id}'")))?;
        let result = self.store.load_entry(&entry)?;
        self.register(&entry.file, &result)?;
        Ok(BaselineEntry {
            benchmark_id: entry.benchmark_id,
            file: entry.file,
            started_at: entry.started_at,
            revision: entry.revision,
        })
    }

    /// Register an explicit result file.
    pub fn register_file(&self, path: &Path) -> Result<BaselineEntry> {
        let result = self.store.load_path(path)?;
        let file = path.to_string_lossy().to_string();
        self.register(&file, &result)?;
        Ok(BaselineEntry {
            benchmark_id: result.benchmark_id.clone(),
            file,
            started_at: result.started_at.clone(),
            revision: result.revision.clone(),
        })
    }

    /// Get the registered baseline entry for a benchmark.
    pub fn get(&self, benchmark_id: &str) -> Result<Option<BaselineEntry>> {
        Ok(self.load_registry()?.entries.get(benchmark_id).cloned())
    }

    /// List all registered baselines.
    pub fn list(&self) -> Result<Vec<BaselineEntry>> {
        Ok(self.load_registry()?.entries.into_values().collect())
    }

    /// Remove the registered baseline for a benchmark.
    pub fn clear(&self, benchmark_id: &str) -> Result<bool> {
        let mut reg = self.load_registry()?;
        let removed = reg.entries.remove(benchmark_id).is_some();
        self.save_registry(&reg)?;
        Ok(removed)
    }

    /// Select a baseline for `benchmark_id` using a selector.
    ///
    /// Selector forms:
    /// * `None` -> the registered baseline, if any.
    /// * `"none"` -> explicitly no baseline.
    /// * `"latest"` -> the most recent stored result (excluding `exclude`).
    /// * a path to a `.json` result file -> that artifact.
    /// * anything else -> treated as a commit SHA prefix.
    pub fn select(
        &self,
        benchmark_id: &str,
        selector: Option<&str>,
        exclude: Option<&str>,
    ) -> Result<Option<BaselineSelection>> {
        let selector = match selector {
            Some(s) => s,
            None => {
                // Fall back to the registered baseline.
                return match self.get(benchmark_id)? {
                    Some(entry) => {
                        let result = self.load_baseline_entry(&entry)?;
                        Ok(Some(BaselineSelection {
                            result,
                            source: format!("registered baseline ({})", entry.started_at),
                            file: Some(entry.file),
                        }))
                    }
                    None => Ok(None),
                };
            }
        };

        if selector == "none" {
            return Ok(None);
        }
        if selector == "latest" {
            return match self.store.latest(benchmark_id, exclude)? {
                Some(entry) => {
                    let result = self.store.load_entry(&entry)?;
                    Ok(Some(BaselineSelection {
                        result,
                        source: format!("latest stored result ({})", entry.started_at),
                        file: Some(entry.file),
                    }))
                }
                None => Ok(None),
            };
        }

        let as_path = Path::new(selector);
        if as_path.extension().and_then(|e| e.to_str()) == Some("json") || as_path.exists() {
            if !as_path.exists() {
                return Err(Error::NotFound(format!(
                    "baseline file '{selector}' does not exist"
                )));
            }
            let result = self.store.load_path(as_path)?;
            return Ok(Some(BaselineSelection {
                result,
                source: format!("artifact '{selector}'"),
                file: Some(selector.to_string()),
            }));
        }

        // Treat as a commit SHA prefix.
        match self.store.find_by_revision(benchmark_id, selector)? {
            Some(entry) => {
                let result = self.store.load_entry(&entry)?;
                Ok(Some(BaselineSelection {
                    result,
                    source: format!("revision '{selector}' -> {}", entry.started_at),
                    file: Some(entry.file),
                }))
            }
            None => Err(Error::NotFound(format!(
                "no stored result for '{benchmark_id}' at revision '{selector}'"
            ))),
        }
    }

    fn load_baseline_entry(&self, entry: &BaselineEntry) -> Result<BenchmarkResult> {
        let path = self.store.root().join(&entry.file);
        if path.exists() {
            self.store.load_path(&path)
        } else {
            // The file may be an absolute path outside the store.
            self.store.load_path(Path::new(&entry.file))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::results::{EnvironmentInfo, Observation, ObservationStatus, Phase};

    fn result(id: &str, started: &str, commit: &str) -> BenchmarkResult {
        let obs = Observation {
            index: 0,
            phase: Phase::Measured,
            status: ObservationStatus::Ok,
            success: true,
            exit_code: Some(0),
            signal: None,
            wall_time_ms: Some(1.0),
            cpu_time_ms: Some(0.5),
            peak_rss_bytes: Some(10),
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
                commit: Some(commit.into()),
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

    fn manager() -> (tempfile::TempDir, BaselineManager) {
        let tmp = tempfile::tempdir().unwrap();
        let store = ResultStore::open(tmp.path()).unwrap();
        (tmp, BaselineManager::new(store))
    }

    #[test]
    fn register_and_get() {
        let (_tmp, mgr) = manager();
        mgr.store()
            .save(&result("b", "2026-01-01T00:00:00Z", "abc"))
            .unwrap();
        mgr.register_latest("b").unwrap();
        assert!(mgr.get("b").unwrap().is_some());
    }

    #[test]
    fn select_latest() {
        let (_tmp, mgr) = manager();
        mgr.store()
            .save(&result("b", "2026-01-01T00:00:00Z", "abc"))
            .unwrap();
        mgr.store()
            .save(&result("b", "2026-01-02T00:00:00Z", "def"))
            .unwrap();
        let sel = mgr.select("b", Some("latest"), None).unwrap().unwrap();
        assert!(sel.source.contains("latest"));
        assert_eq!(sel.result.revision.commit.as_deref(), Some("def"));
    }

    #[test]
    fn select_by_revision() {
        let (_tmp, mgr) = manager();
        mgr.store()
            .save(&result("b", "2026-01-01T00:00:00Z", "abcdef01"))
            .unwrap();
        let sel = mgr.select("b", Some("abcdef"), None).unwrap().unwrap();
        assert!(sel.source.contains("revision"));
    }

    #[test]
    fn select_none_returns_none() {
        let (_tmp, mgr) = manager();
        assert!(mgr.select("b", Some("none"), None).unwrap().is_none());
    }

    #[test]
    fn select_missing_revision_errors() {
        let (_tmp, mgr) = manager();
        assert!(mgr.select("b", Some("deadbeef"), None).is_err());
    }

    #[test]
    fn clear_removes_entry() {
        let (_tmp, mgr) = manager();
        mgr.store()
            .save(&result("b", "2026-01-01T00:00:00Z", "abc"))
            .unwrap();
        mgr.register_latest("b").unwrap();
        assert!(mgr.clear("b").unwrap());
        assert!(mgr.get("b").unwrap().is_none());
    }
}
