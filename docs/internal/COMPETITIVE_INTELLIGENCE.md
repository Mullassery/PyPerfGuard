# Competitive intelligence in roadmap and upgrade work

**Policy date:** 2026-10-19

Competitive findings are an input to product decisions, not a one-off report.
Every pending roadmap item, new feature, and routine dependency upgrade should
check the repository's [`AUDIT_FINDINGS.md`](AUDIT_FINDINGS.md) before work
starts, and record the decision in the PR or changelog.

## Required check before building

1. **Read `AUDIT_FINDINGS.md`.** Find the relevant competitor row and the
   ranked next actions. If the finding is stale, update it with a dated source.
2. **Name the competitor and the user problem.** A feature is not justified by
   "competitor has it". State the workflow it unlocks, who needs it, and how
   often. Mark unverified demand as a hypothesis.
3. **Prefer reuse over new code.** Check whether an existing Mullassery repo
   already provides the capability (see `BUNDLES.md` in the profile repo and
   the bundle role file in this repo, when present). Typical reuse:
   - crawling/fetching/robots/SSRF → **MudFish**
   - architecture maps / PR diffs → **CodeAtlas**
   - data-quality contracts / validation → **StatGuardian**
   - token counting / cost tables → **PyTokenCalc**
   - dependency graph / advisories / SBOM → **PyDeepMesh**
   - segmentation / CLV / churn → **ClusterAudienceKit**
   - activation / lineage / PII → **PyReverseETL**
   - fleet state sync → **SyncFleet** (only after conflict resolution)
4. **Choose one disposition:** build, integrate, defer, or reject. Write it
   down. A rejection with a reason is a successful outcome.
5. **For routine upgrades**, check whether the upgrade closes a known
   competitor gap or breaks a differentiated capability. Run the tests that
   cover the affected path; do not merge a green build that silently changes a
   user-visible contract.

## PR / changelog checklist

- [ ] Competitor and source URL/date recorded (or "no direct competitor").
- [ ] User impact and frequency stated; unverified demand labelled.
- [ ] Reuse check done against sibling repos/bundles.
- [ ] Failure mode and rollback considered (especially auth, PII, cost).
- [ ] Docs/roadmap status updated honestly.
- [ ] Tests or a manual verification command included.

## Review cadence

- Refresh the competitor row when a release changes the capability.
- Re-check the full `AUDIT_FINDINGS.md` at least quarterly and before a major
  release.
- If a finding is wrong or stale, correct it in the same PR that relies on it.

## Non-goals

- Copying features for parity.
- Adding a dependency just because a competitor uses it.
- Treating star counts or download counts as proof of user need.

## OSS gap analysis — 2026-10-24

**What matters most here:** shared baselines and harness adapters. Local-only history cannot enforce a team-wide regression policy; add server-side baselines by branch/testbed and importers for pytest-benchmark, Criterion, JMH, and hyperfine.

Scope: PyPerfGuard 0.2.0 (Rust CLI, Apache-2.0) against OSS benchmark runners and continuous-benchmarking tools, judged on measurement, statistics, baselines, and CI enforcement. "No" means not found in the reviewed public docs, not proof of absence. CodSpeed was excluded because it is a proprietary service, not an OSS GitHub project; pytest-benchmark and asv are treated as harnesses PyPerfGuard could ingest, not as end-to-end guards.

### Comparable OSS projects

| Project | GitHub | License | Last activity | Why it is the comparison |
|---|---|---|---|---|
| hyperfine | https://github.com/sharkdp/hyperfine | MIT OR Apache-2.0 | v2.0.0 released 2026-10-07; last commit 2026-10-12 | CLI benchmark runner for arbitrary commands, with statistics, outlier detection, warmups, and CSV/JSON/Markdown export. The closest direct alternative for "measure this command". |
| Criterion.rs | https://github.com/criterion-rs/criterion.rs | MIT OR Apache-2.0 | v0.8.2 released 2026-02-04; last commit 2026-10-02 | Statistics-driven Rust microbenchmarking with local baselines and change detection. The reference for statistical rigor and baseline comparison in Rust. |
| Bencher | https://github.com/bencherdev/bencher | Apache-2.0/MIT (directories named `plus` use the Bencher Plus License) | v0.6.13 released 2026-09-28 (Runner Canary 2026-10-22); last commit 2026-10-23 | Continuous benchmarking platform with stored results, branch/testbed tracking, thresholds, alerts, and self-hosting. The closest alternative for shared baseline history and CI gating. |

### Capability comparison

| Capability | Ours | hyperfine | Criterion.rs | Bencher | Gap |
|---|---|---|---|---|---|
| Runs arbitrary commands/scripts/binaries | Yes | Yes | No | Partial | Ours and hyperfine are language-agnostic; Criterion is a Rust harness, Bencher delegates measurement. |
| Wall-clock measurement | Yes | Yes | Yes | Partial | Parity with hyperfine/Criterion; Bencher relies on adapters. |
| CPU time and peak RSS | Yes | Yes | No | Partial | Ours is ahead of Criterion; hyperfine 2.0 now measures CPU time, RSS, cycles, and instructions. |
| Statistical significance test | Yes | Partial | Yes | Partial | We use Mann-Whitney U; Criterion uses bootstrap change detection; hyperfine has no significance test. |
| Confidence intervals/effect size | No | No | Yes | Partial | Weaker; Criterion reports confidence intervals, ours reports p-values and point statistics. |
| Local baseline history | Yes | No | Yes | Yes | Parity with Criterion and Bencher; hyperfine has no persistent baseline. |
| Git/branch-aware baseline selection | Yes | No | Partial | Yes | Ours is ahead of Criterion; Bencher adds branch/start-point and testbed dimensions. |
| Policy thresholds with block/warn severity | Yes | No | No | Yes | Ours is ahead of hyperfine/Criterion; Bencher has thresholds and alerts. |
| Distinguishes over-budget from statistically real | Yes | No | Partial | Partial | Ours is ahead: regression/warning/inconclusive is a first-class outcome. |
| CI report formats | Yes | Yes | Partial | Yes | Parity; ours also emits SVG and Markdown job summaries. |
| Shared/remote baseline storage | Partial | No | No | Yes | Weaker; only `baseline export/import` artifacts, and the receiver must already have result files. |
| Harness/adapter ingestion | No | No | No | Yes | Absent; Bencher ingests pytest-benchmark, Criterion, JMH, hyperfine, and others. |
| Hardware counters | No | Yes | Partial | Partial | Weaker; hyperfine exposes cycles, instructions, cache references, and branch misses. |
| Prebuilt/package distribution | Partial | Yes | Yes | Yes | Weaker; `dist/` has a macOS arm64 wheel and sdist, but crates.io and prebuilt binaries are blocked. |

### Gaps that matter (ranked)

1. **No shared baseline service.** Evidence: README lists only local history; CHANGELOG says imported baselines require the underlying result files to exist on the receiver; AUDIT_FINDINGS names "No shared baseline storage" as a verified gap. Bencher stores results server-side by branch, testbed, benchmark, and measure. Hurts ephemeral CI runners and teams comparing across machines. Effort: M.
2. **No harness adapter ecosystem.** Evidence: PyPerfGuard runs commands but has no importer for pytest-benchmark, Criterion, JMH, or hyperfine output; Bencher ships adapters for all of these. Hurts teams with existing benchmarks that would have to re-run everything through the CLI. Effort: M.
3. **Distribution is still blocked.** Evidence: QUEUE_STATUS and AUDIT_FINDINGS say crates.io/prebuilt binaries are blocked on a registry token; `dist/` contains only `pyperfguard-0.2.0-py3-none-macosx_11_0_arm64.whl` and an sdist. hyperfine, Criterion, and Bencher install from public registries. Hurts Linux CI and non-macOS users. Effort: S-M.
4. **Statistical estimation is narrower than Criterion's.** Evidence: our statistics are median/mean/quantiles/CV/IQR/Mann-Whitney (CHANGELOG); Criterion advertises statistical change detection and reports confidence intervals. A p-value without an interval or effect size makes small regressions harder to act on. Hurts teams tuning near a threshold. Effort: S-M.
5. **No runner isolation or hardware-level visibility.** Evidence: README records environment metadata but says it cannot quiet a noisy machine; hyperfine 2.0 exposes cycles/instructions/cache counters and cache-clearing; Bencher sells bare-metal runners at <2% variance. Hurts shared CI where the noise floor exceeds the policy threshold. Effort: L.

### Where we are differentiated

- Explicit separation of "over budget" and "statistically real": pass, inconclusive, warning, regression, and error are distinct outcomes with documented exit codes.
- Git-aware baselines with commit, branch, environment, and tool version recorded, plus commit-SHA and artifact selectors.
- One process measures wall time, CPU time, peak RSS, and derived CPU utilization via `wait4(2)` across languages, without a Python runtime or a language-specific harness.
- Honest measurement semantics: missing values are `n/a`/null, failed executions are not valid observations, and outliers are reported but never silently removed.
- Machine-readable artifacts with atomic writes, a rebuildable history index, redaction, and validation of imported results.
- No hosted service is required; the GitHub Actions flow works from artifacts and a local CI script reproduces the same exit behavior.

### Disposition

- **Build:** a content-addressed remote baseline backend (S3/object store or GitHub artifacts) with branch/testbed keys; importers for pytest-benchmark/Criterion/hyperfine JSON; confidence intervals and effect sizes.
- **Integrate/reuse:** emit Bencher-compatible JSON or write to a self-hosted Bencher API instead of inventing a second history service; keep hyperfine-style hardware counters as an optional adapter rather than a core dependency.
- **Defer/reject:** flamegraph/profiling stays deferred (already rejected for now); no hosted SaaS, no replacement benchmark harnesses, and no silent outlier deletion.

### Sources

- https://github.com/sharkdp/hyperfine (accessed 2026-10-24)
- https://github.com/criterion-rs/criterion.rs (accessed 2026-10-24; supersedes the older bheisler/criterion.rs repo, whose README redirects here)
- https://github.com/bencherdev/bencher (accessed 2026-10-24)
- Repo files inspected: README.md, AUDIT_FINDINGS.md, COMPETITIVE_INTELLIGENCE.md, Cargo.toml, pyproject.toml, CHANGELOG.md, QUEUE_STATUS.md, IMPLEMENTATION_PLAN.md, dist/
