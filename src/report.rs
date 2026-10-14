//! Reporting engine: terminal, JSON, Markdown, CSV, and SVG outputs.
//!
//! Output is deterministic for equivalent input (timestamps are the only
//! inherently variable field). Missing measurements are rendered as `n/a` and
//! are never plotted or tabulated as zero.

use crate::compare::Comparison;
use crate::metric::Metric;
use crate::policy::{Outcome, PolicyReport};
use crate::results::BenchmarkResult;
use crate::stats::Summary;

/// Format a metric value with an appropriate unit.
pub fn fmt_metric(metric: Metric, value: f64) -> String {
    match metric {
        Metric::PeakRss => fmt_bytes(value),
        Metric::CpuUtilization => format!("{value:.1}%"),
        Metric::WallTime | Metric::CpuTime => {
            if value.abs() >= 1000.0 {
                format!("{:.2}s", value / 1000.0)
            } else {
                format!("{value:.2}ms")
            }
        }
    }
}

/// Format a byte count using binary units.
pub fn fmt_bytes(bytes: f64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    if bytes >= GIB {
        format!("{:.2} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

fn fmt_pct(pct: Option<f64>) -> String {
    match pct {
        Some(p) => format!("{p:+.2}%"),
        None => "n/a".to_string(),
    }
}

fn fmt_change_abs(metric: Metric, abs: Option<f64>) -> String {
    match abs {
        Some(v) => format!(
            "{}{}",
            if v >= 0.0 { "+" } else { "" },
            fmt_metric(metric, v)
        ),
        None => "n/a".to_string(),
    }
}

/// Render a completed benchmark result for the terminal.
pub fn terminal_run(result: &BenchmarkResult) -> String {
    let mut out = String::new();
    out.push_str(&format!("benchmark: {}\n", result.benchmark_id));
    if !result.description.is_empty() {
        out.push_str(&format!("  {}\n", result.description));
    }
    out.push_str(&format!("  command:   {}\n", result.command.join(" ")));
    out.push_str(&format!(
        "  revision:  {}\n",
        result
            .revision
            .commit
            .as_deref()
            .map(crate::git::short_sha)
            .unwrap_or_else(|| "(no git)".to_string())
    ));
    out.push_str(&format!("  status:    {:?}\n", result.status));
    out.push_str(&format!(
        "  samples:   {} successful / {} failed (measured)\n",
        result.summary.as_ref().map(|s| s.successful).unwrap_or(0),
        result.summary.as_ref().map(|s| s.failed).unwrap_or(0)
    ));
    out.push_str(&format!("  started:   {}\n", result.started_at));

    match &result.summary {
        Some(s) => {
            out.push_str(
                "\n  metric            n   median        mean          min           max\n",
            );
            for m in &result.requested_metrics {
                if let Some(ms) = s.metrics.get(m.name()) {
                    let sm = &ms.summary;
                    out.push_str(&format!(
                        "  {:<16} {:>3}   {:<12} {:<12} {:<12} {:<12}\n",
                        m.name(),
                        sm.n,
                        fmt_metric(*m, sm.median),
                        fmt_metric(*m, sm.mean),
                        fmt_metric(*m, sm.min),
                        fmt_metric(*m, sm.max),
                    ));
                }
            }
        }
        None => {
            out.push_str("\n  no successful measurements\n");
        }
    }

    if !result.errors.is_empty() {
        out.push_str("\n  errors:\n");
        for e in &result.errors {
            out.push_str(&format!("    - {e}\n"));
        }
    }
    out
}

/// Render a comparison for the terminal.
pub fn terminal_comparison(cmp: &Comparison) -> String {
    let mut out = String::new();
    out.push_str(&format!("comparison for '{}':\n", cmp.benchmark_id));
    out.push_str(&format!(
        "  baseline: {} @ {}\n",
        cmp.baseline_revision
            .commit
            .as_deref()
            .map(crate::git::short_sha)
            .unwrap_or_else(|| "(no git)".into()),
        cmp.baseline_started_at
    ));
    out.push_str(&format!(
        "  current:  {} @ {}\n",
        cmp.current_revision
            .commit
            .as_deref()
            .map(crate::git::short_sha)
            .unwrap_or_else(|| "(no git)".into()),
        cmp.current_started_at
    ));
    if !cmp.compatible {
        out.push_str("  WARNING: results are not compatible for comparison\n");
    }
    out.push_str(
        "\n  metric            baseline      current       change        pct       significant\n",
    );
    for mc in &cmp.metrics {
        let base = mc
            .baseline
            .as_ref()
            .map(|s| fmt_metric(mc.metric, s.median))
            .unwrap_or_else(|| "n/a".into());
        let cur = mc
            .current
            .as_ref()
            .map(|s| fmt_metric(mc.metric, s.median))
            .unwrap_or_else(|| "n/a".into());
        let sig = match mc.significantly_different(0.05) {
            Some(true) => "yes",
            Some(false) => "no",
            None => "n/a",
        };
        out.push_str(&format!(
            "  {:<16} {:<13} {:<13} {:<13} {:<8} {}\n",
            mc.metric.name(),
            base,
            cur,
            fmt_change_abs(mc.metric, mc.abs_change),
            fmt_pct(mc.pct_change),
            sig,
        ));
    }
    if !cmp.caveats.is_empty() {
        out.push_str("\n  caveats:\n");
        for c in &cmp.caveats {
            out.push_str(&format!("    - {c}\n"));
        }
    }
    out
}

/// Whether a delta represents an improvement, regression, or neutral change.
fn direction(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Regression => "regression",
        Outcome::Warning => "warning",
        Outcome::Info => "info",
        Outcome::Inconclusive => "inconclusive",
        Outcome::Error => "error",
        Outcome::Pass => "pass",
    }
}

/// Render a policy report for the terminal.
pub fn terminal_policy(report: &PolicyReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("policy check: {}\n", report.summary));
    if let Some(src) = &report.baseline_source {
        out.push_str(&format!("  baseline source: {src}\n"));
    }
    out.push('\n');
    for o in &report.metric_outcomes {
        let delta = match o.pct_change {
            Some(p) => format!("{p:+.2}%"),
            None => fmt_change_abs(o.metric, o.abs_change),
        };
        out.push_str(&format!(
            "  [{}] {:<16} {}\n",
            direction(o.outcome),
            o.metric.name(),
            delta
        ));
        if !o.note.is_empty() {
            out.push_str(&format!("        {}\n", o.note));
        }
    }
    out.push_str(&format!(
        "\n  overall: {} (exit code {})\n",
        report.overall.label(),
        report.exit_code()
    ));
    out
}

fn outcome_badge(o: Outcome) -> &'static str {
    match o {
        Outcome::Pass => "PASS",
        Outcome::Info => "INFO",
        Outcome::Inconclusive => "INCONCLUSIVE",
        Outcome::Warning => "WARN",
        Outcome::Regression => "REGRESSION",
        Outcome::Error => "ERROR",
    }
}

/// Render a Markdown comparison table.
pub fn markdown_comparison(cmp: &Comparison) -> String {
    let mut out = String::new();
    out.push_str(&format!("## Comparison: `{}`\n\n", cmp.benchmark_id));
    out.push_str(&format!(
        "- Baseline: `{}` ({})\n",
        cmp.baseline_revision
            .commit
            .as_deref()
            .map(crate::git::short_sha)
            .unwrap_or_else(|| "no-git".into()),
        cmp.baseline_started_at
    ));
    out.push_str(&format!(
        "- Current: `{}` ({})\n\n",
        cmp.current_revision
            .commit
            .as_deref()
            .map(crate::git::short_sha)
            .unwrap_or_else(|| "no-git".into()),
        cmp.current_started_at
    ));

    out.push_str("| Metric | Baseline | Current | Change | % | Significant |\n");
    out.push_str("| --- | ---: | ---: | ---: | ---: | :---: |\n");
    for mc in &cmp.metrics {
        let base = mc
            .baseline
            .as_ref()
            .map(|s| fmt_metric(mc.metric, s.median))
            .unwrap_or_else(|| "n/a".into());
        let cur = mc
            .current
            .as_ref()
            .map(|s| fmt_metric(mc.metric, s.median))
            .unwrap_or_else(|| "n/a".into());
        let sig = match mc.significantly_different(0.05) {
            Some(true) => "yes",
            Some(false) => "no",
            None => "n/a",
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            mc.metric.name(),
            base,
            cur,
            fmt_change_abs(mc.metric, mc.abs_change),
            fmt_pct(mc.pct_change),
            sig
        ));
    }
    if !cmp.caveats.is_empty() {
        out.push_str("\n**Caveats**\n\n");
        for c in &cmp.caveats {
            out.push_str(&format!("- {c}\n"));
        }
    }
    out
}

/// Render a Markdown policy report suitable for a GitHub Actions job summary.
pub fn markdown_policy(report: &PolicyReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("## PyPerfGuard: `{}`\n\n", report.benchmark_id));
    out.push_str(&format!(
        "**Overall: {}** (exit code {})\n\n",
        outcome_badge(report.overall),
        report.exit_code()
    ));
    if let Some(src) = &report.baseline_source {
        out.push_str(&format!("Baseline: {src}\n\n"));
    }
    out.push_str("| Metric | Baseline | Current | Change | % | Threshold | Outcome |\n");
    out.push_str("| --- | ---: | ---: | ---: | ---: | ---: | :--- |\n");
    for o in &report.metric_outcomes {
        let base = o
            .baseline_median
            .map(|v| fmt_metric(o.metric, v))
            .unwrap_or_else(|| "n/a".into());
        let cur = o
            .current_median
            .map(|v| fmt_metric(o.metric, v))
            .unwrap_or_else(|| "n/a".into());
        let threshold = match (o.threshold_pct, o.threshold_abs) {
            (Some(p), Some(a)) => format!("{} / +{}", fmt_pct(Some(p)), fmt_metric(o.metric, a)),
            (Some(p), None) => fmt_pct(Some(p)),
            (None, Some(a)) => format!("+{}", fmt_metric(o.metric, a)),
            (None, None) => "-".into(),
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            o.metric.name(),
            base,
            cur,
            fmt_change_abs(o.metric, o.abs_change),
            fmt_pct(o.pct_change),
            threshold,
            outcome_badge(o.outcome)
        ));
    }
    out.push('\n');
    for o in &report.metric_outcomes {
        if !o.note.is_empty() && o.outcome != Outcome::Pass {
            out.push_str(&format!("> {}: {}\n", o.metric.name(), o.note));
        }
    }
    out
}

/// Render a single benchmark result as Markdown.
pub fn markdown_run(result: &BenchmarkResult) -> String {
    let mut out = String::new();
    out.push_str(&format!("## Benchmark: `{}`\n\n", result.benchmark_id));
    if !result.description.is_empty() {
        out.push_str(&format!("{}\n\n", result.description));
    }
    out.push_str(&format!("- Command: `{}`\n", result.command.join(" ")));
    out.push_str(&format!(
        "- Revision: `{}`\n",
        result
            .revision
            .commit
            .as_deref()
            .map(crate::git::short_sha)
            .unwrap_or_else(|| "no-git".into())
    ));
    out.push_str(&format!("- Status: `{:?}`\n", result.status));
    out.push_str(&format!("- Started: {}\n\n", result.started_at));
    if let Some(s) = &result.summary {
        out.push_str("| Metric | n | Median | Mean | Min | Max |\n");
        out.push_str("| --- | ---: | ---: | ---: | ---: | ---: |\n");
        for m in &result.requested_metrics {
            if let Some(ms) = s.metrics.get(m.name()) {
                let sm = &ms.summary;
                out.push_str(&format!(
                    "| {} | {} | {} | {} | {} | {} |\n",
                    m.name(),
                    sm.n,
                    fmt_metric(*m, sm.median),
                    fmt_metric(*m, sm.mean),
                    fmt_metric(*m, sm.min),
                    fmt_metric(*m, sm.max)
                ));
            }
        }
    } else {
        out.push_str("_No successful measurements._\n");
    }
    out
}

/// Render a comparison as CSV.
pub fn comparison_csv(cmp: &Comparison) -> String {
    let mut out = String::new();
    out.push_str("metric,unit,baseline_median,current_median,abs_change,pct_change,significant\n");
    for mc in &cmp.metrics {
        let base = mc
            .baseline
            .as_ref()
            .map(|s| format!("{:.6}", s.median))
            .unwrap_or_default();
        let cur = mc
            .current
            .as_ref()
            .map(|s| format!("{:.6}", s.median))
            .unwrap_or_default();
        let abs = mc.abs_change.map(|v| format!("{v:.6}")).unwrap_or_default();
        let pct = mc.pct_change.map(|v| format!("{v:.6}")).unwrap_or_default();
        let sig = match mc.significantly_different(0.05) {
            Some(true) => "true",
            Some(false) => "false",
            None => "",
        };
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            mc.metric.name(),
            mc.unit,
            base,
            cur,
            abs,
            pct,
            sig
        ));
    }
    out
}

/// Render a full Markdown summary for one or more policy reports.
pub fn markdown_summary(reports: &[PolicyReport]) -> String {
    let mut out = String::new();
    out.push_str("# PyPerfGuard report\n\n");
    let worst = reports
        .iter()
        .map(|r| r.overall)
        .max_by_key(|o| o.rank())
        .unwrap_or(Outcome::Pass);
    out.push_str(&format!("**Overall result: {}**\n\n", outcome_badge(worst)));
    for (i, r) in reports.iter().enumerate() {
        if i > 0 {
            out.push_str("\n---\n\n");
        }
        out.push_str(&markdown_policy(r));
    }
    out
}

/// A point in a metric's history.
#[derive(Debug, Clone)]
pub struct TrendPoint {
    /// Timestamp of the run.
    pub started_at: String,
    /// Commit SHA, when available.
    pub commit: Option<String>,
    /// Number of successful samples.
    pub n: usize,
    /// Summary statistics.
    pub summary: Summary,
}

/// Render a CSV export of a metric's history.
pub fn csv_trend(metric: Metric, points: &[TrendPoint]) -> String {
    let mut out = String::new();
    out.push_str("metric,timestamp,commit,n,median,mean,min,max,std_dev\n");
    for p in points {
        let commit = p.commit.as_deref().unwrap_or("");
        let std = p
            .summary
            .std_dev
            .map(|v| format!("{v:.6}"))
            .unwrap_or_default();
        out.push_str(&format!(
            "{},{},{},{},{:.6},{:.6},{:.6},{:.6},{}\n",
            metric.name(),
            p.started_at,
            commit,
            p.n,
            p.summary.median,
            p.summary.mean,
            p.summary.min,
            p.summary.max,
            std
        ));
    }
    out
}

/// Render a sparkline of the metric medians.
pub fn sparkline(values: &[f64]) -> String {
    const TICKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    if values.is_empty() {
        return String::new();
    }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = max - min;
    values
        .iter()
        .map(|v| {
            if span <= f64::EPSILON {
                TICKS[3]
            } else {
                let idx = (((v - min) / span) * (TICKS.len() - 1) as f64).round() as usize;
                TICKS[idx.min(TICKS.len() - 1)]
            }
        })
        .collect()
}

/// Render a simple SVG line chart of a metric's history.
///
/// The y-axis is labeled with the true minimum and maximum so the scale cannot
/// mislead. With fewer than two points a note is emitted instead of a chart.
pub fn svg_trend(metric: Metric, points: &[TrendPoint], title: &str) -> String {
    if points.len() < 2 {
        return format!(
            "<!-- not enough history for a trend chart (need >= 2 points, have {}) -->\n",
            points.len()
        );
    }
    let width = 640.0;
    let height = 240.0;
    let pad_l = 70.0;
    let pad_r = 20.0;
    let pad_t = 30.0;
    let pad_b = 40.0;
    let plot_w = width - pad_l - pad_r;
    let plot_h = height - pad_t - pad_b;

    let values: Vec<f64> = points.iter().map(|p| p.summary.median).collect();
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = if (max - min).abs() < f64::EPSILON {
        1.0
    } else {
        max - min
    };

    let n = points.len();
    let x = |i: usize| pad_l + plot_w * (i as f64) / ((n - 1) as f64);
    let y = |v: f64| pad_t + plot_h * (1.0 - (v - min) / span);

    let mut out = String::new();
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         viewBox=\"0 0 {width} {height}\" role=\"img\" aria-label=\"{title}\">\n"
    ));
    out.push_str(&format!(
        "<rect width=\"{width}\" height=\"{height}\" fill=\"white\" stroke=\"#ccc\"/>\n"
    ));
    out.push_str(&format!(
        "<text x=\"{pad_l}\" y=\"18\" font-family=\"sans-serif\" font-size=\"14\" \
         fill=\"#222\">{title} (median {})</text>\n",
        metric.name()
    ));
    // Axes ticks.
    out.push_str(&format!(
        "<text x=\"4\" y=\"{:.1}\" font-family=\"sans-serif\" font-size=\"11\" fill=\"#555\">{}</text>\n",
        y(max),
        fmt_metric(metric, max)
    ));
    out.push_str(&format!(
        "<text x=\"4\" y=\"{:.1}\" font-family=\"sans-serif\" font-size=\"11\" fill=\"#555\">{}</text>\n",
        y(min),
        fmt_metric(metric, min)
    ));
    canvg_line(&mut out, points, x, y, metric);

    out.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"sans-serif\" font-size=\"11\" fill=\"#555\">{}</text>\n",
        pad_l,
        height - 10.0,
        points.first().map(|p| p.started_at.as_str()).unwrap_or("")
    ));
    out.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" font-family=\"sans-serif\" \
         font-size=\"11\" fill=\"#555\">{}</text>\n",
        width - pad_r,
        height - 10.0,
        points.last().map(|p| p.started_at.as_str()).unwrap_or("")
    ));
    out.push_str("</svg>\n");
    out
}

fn canvg_line(
    out: &mut String,
    points: &[TrendPoint],
    x: impl Fn(usize) -> f64,
    y: impl Fn(f64) -> f64,
    metric: Metric,
) {
    let mut path = String::from("M");
    for (i, p) in points.iter().enumerate() {
        if i > 0 {
            path.push('L');
        }
        path.push_str(&format!("{:.2},{:.2} ", x(i), y(p.summary.median)));
    }
    out.push_str(&format!(
        "<path d=\"{}\" fill=\"none\" stroke=\"#c0392b\" stroke-width=\"2\"/>\n",
        path.trim_end()
    ));
    for (i, p) in points.iter().enumerate() {
        out.push_str(&format!(
            "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"3\" fill=\"#c0392b\"><title>{}</title></circle>\n",
            x(i),
            y(p.summary.median),
            fmt_metric(metric, p.summary.median)
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::summarize;

    fn trend_point(t: &str, values: &[f64]) -> TrendPoint {
        TrendPoint {
            started_at: t.into(),
            commit: Some("abc".into()),
            n: values.len(),
            summary: summarize(values).unwrap(),
        }
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(fmt_bytes(512.0), "512 B");
        assert_eq!(fmt_bytes(2048.0), "2.00 KiB");
        assert_eq!(fmt_bytes(2.0 * 1024.0 * 1024.0), "2.00 MiB");
    }

    #[test]
    fn formats_time() {
        assert_eq!(fmt_metric(Metric::WallTime, 20.0), "20.00ms");
        assert_eq!(fmt_metric(Metric::WallTime, 2500.0), "2.50s");
    }

    #[test]
    fn sparkline_handles_flat_and_varying() {
        assert_eq!(sparkline(&[]), "");
        assert_eq!(sparkline(&[5.0, 5.0, 5.0]).chars().count(), 3);
        let s = sparkline(&[1.0, 5.0, 10.0]);
        assert_eq!(s.chars().count(), 3);
    }

    #[test]
    fn svg_needs_two_points() {
        let s = svg_trend(Metric::WallTime, &[trend_point("t", &[1.0])], "x");
        assert!(s.contains("not enough history"));
    }

    #[test]
    fn svg_contains_line_for_two_points() {
        let pts = vec![
            trend_point("t1", &[1.0, 2.0]),
            trend_point("t2", &[3.0, 4.0]),
        ];
        let s = svg_trend(Metric::WallTime, &pts, "Trend");
        assert!(s.contains("<svg"));
        assert!(s.contains("<path"));
        assert!(s.contains("</svg>"));
    }

    #[test]
    fn csv_has_header_and_rows() {
        let pts = vec![trend_point("t1", &[1.0, 2.0, 3.0])];
        let csv = csv_trend(Metric::WallTime, &pts);
        assert!(csv.starts_with("metric,timestamp,commit,n,median,mean,min,max,std_dev\n"));
        assert_eq!(csv.lines().count(), 2);
    }
}
