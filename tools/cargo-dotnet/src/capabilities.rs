//! Validation and generated reporting for `acceptance/capabilities.toml`.
//!
//! The manifest describes support and acceptance journeys, never mutable pass/fail state. When a
//! matrix TSV is supplied, observed status is derived from its `result` column at report time.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write as _;
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};

use crate::cli::{CapabilitiesArgs, CapabilitiesEvidenceScope, CapabilitiesFormat};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    support: Support,
    #[serde(default)]
    blocker: Vec<Blocker>,
    #[serde(default)]
    journey: Vec<Journey>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Support {
    rust_toolchain: String,
    target: String,
    dotnet_cli_profiles: Vec<String>,
    default_dotnet: String,
    presubmit_dotnet: Vec<String>,
    release_dotnet: Vec<String>,
    profiles: Vec<String>,
    host_os: Vec<String>,
    known_limits: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Blocker {
    id: String,
    fixture: String,
    oracle: String,
    current_expected: String,
    closes_when: String,
    status: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Journey {
    id: String,
    outcome: String,
    fixture: String,
    case: String,
    evidence_kind: String,
    #[serde(default)]
    shared_evidence: bool,
    oracle: String,
    comparison: String,
    #[serde(default)]
    completion_marker: String,
    required_artifacts: Vec<String>,
    #[serde(default)]
    presubmit_runtimes: Vec<String>,
    #[serde(default)]
    presubmit_profiles: Vec<String>,
    #[serde(default)]
    release_runtimes: Vec<String>,
    #[serde(default)]
    release_profiles: Vec<String>,
    presubmit: bool,
}

#[derive(Debug, Serialize)]
struct JsonReport<'a> {
    schema: u32,
    source_manifest: String,
    evidence_scope: &'static str,
    support: &'a Support,
    blockers: &'a [Blocker],
    journeys: Vec<JourneyReport<'a>>,
}

#[derive(Debug, Serialize)]
struct JourneyReport<'a> {
    #[serde(flatten)]
    journey: &'a Journey,
    observed: &'static str,
    evidence: EvidenceCoverage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResultRow {
    kind: String,
    dotnet: String,
    profile: String,
    result: String,
    marker: String,
    required: String,
}

#[derive(Clone, Debug, Serialize)]
struct EvidenceCoverage {
    required_cells: usize,
    observed_cells: usize,
    missing_cells: Vec<String>,
}

#[derive(Clone, Debug)]
struct Observation {
    status: &'static str,
    evidence: EvidenceCoverage,
}

pub fn run(args: &CapabilitiesArgs) -> Result<i32> {
    let text = fs::read_to_string(&args.manifest)
        .with_context(|| format!("reading capability manifest {}", args.manifest.display()))?;
    let manifest: Manifest = toml::from_str(&text)
        .with_context(|| format!("parsing capability manifest {}", args.manifest.display()))?;
    validate(&manifest)?;
    let observed = parse_results(&args.results)?;
    validate_result_dimensions(&manifest, &observed)?;
    let report = match args.format {
        CapabilitiesFormat::Markdown => {
            render_markdown(&manifest, &args.manifest, &observed, args.evidence_scope)
        }
        CapabilitiesFormat::Json => serde_json::to_string_pretty(&JsonReport {
            schema: manifest.schema,
            source_manifest: args.manifest.display().to_string(),
            evidence_scope: scope_name(args.evidence_scope),
            support: &manifest.support,
            blockers: &manifest.blocker,
            journeys: manifest
                .journey
                .iter()
                .map(|journey| {
                    let observation = observation_for(journey, &observed, args.evidence_scope);
                    JourneyReport {
                        journey,
                        observed: observation.status,
                        evidence: observation.evidence,
                    }
                })
                .collect(),
        })?,
    };
    if let Some(path) = &args.output {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating report directory {}", parent.display()))?;
        }
        write_atomic(path, format!("{report}\n").as_bytes())?;
        println!("capability report: {}", path.display());
    } else {
        println!("{report}");
    }
    let strict_failed =
        args.strict && strict_evidence_failed(&manifest, &observed, args.evidence_scope);
    if strict_failed {
        eprintln!(
            "capability evidence is incomplete: strict mode requires every presubmit journey to pass every required {} runtime/profile cell",
            scope_name(args.evidence_scope)
        );
        Ok(1)
    } else {
        Ok(0)
    }
}

fn validate(manifest: &Manifest) -> Result<()> {
    if manifest.schema != 1 {
        bail!(
            "unsupported capability manifest schema {} (expected 1)",
            manifest.schema
        );
    }
    for (label, values) in [
        ("dotnet_cli_profiles", &manifest.support.dotnet_cli_profiles),
        ("presubmit_dotnet", &manifest.support.presubmit_dotnet),
        ("release_dotnet", &manifest.support.release_dotnet),
        ("profiles", &manifest.support.profiles),
        ("host_os", &manifest.support.host_os),
    ] {
        require_unique_nonempty(label, values)?;
    }
    if !manifest
        .support
        .profiles
        .iter()
        .any(|profile| profile == "release")
    {
        bail!("support.profiles must include release for release evidence");
    }
    let cli: BTreeSet<&str> = manifest
        .support
        .dotnet_cli_profiles
        .iter()
        .map(String::as_str)
        .collect();
    if !cli.contains(manifest.support.default_dotnet.as_str()) {
        bail!("default_dotnet must be present in dotnet_cli_profiles");
    }
    for (label, values) in [
        ("presubmit_dotnet", &manifest.support.presubmit_dotnet),
        ("release_dotnet", &manifest.support.release_dotnet),
    ] {
        for value in values {
            if !cli.contains(value.as_str()) {
                bail!("{label} value {value:?} is not in dotnet_cli_profiles");
            }
        }
    }
    let mut blocker_ids = BTreeSet::new();
    for blocker in &manifest.blocker {
        if blocker.id.is_empty() || !blocker_ids.insert(blocker.id.as_str()) {
            bail!("blocker ids must be non-empty and unique");
        }
    }
    let mut ids = BTreeSet::new();
    let mut evidence_keys: BTreeMap<(&str, &str), Vec<&Journey>> = BTreeMap::new();
    for journey in &manifest.journey {
        if !ids.insert(journey.id.as_str()) {
            bail!("duplicate journey id {:?}", journey.id);
        }
        if journey.id.is_empty()
            || journey.outcome.is_empty()
            || journey.fixture.is_empty()
            || journey.case.is_empty()
            || journey.evidence_kind.is_empty()
            || journey.oracle.is_empty()
            || journey.comparison.is_empty()
        {
            bail!("journey {:?} has an empty required field", journey.id);
        }
        evidence_keys
            .entry((journey.case.as_str(), journey.evidence_kind.as_str()))
            .or_default()
            .push(journey);
        for (label, values) in [
            ("presubmit_runtimes", &journey.presubmit_runtimes),
            ("presubmit_profiles", &journey.presubmit_profiles),
            ("release_runtimes", &journey.release_runtimes),
            ("release_profiles", &journey.release_profiles),
        ] {
            require_journey_dimensions(journey, label, values, &manifest.support)?;
        }
    }
    for ((case, kind), journeys) in evidence_keys {
        if journeys.len() > 1 && journeys.iter().any(|journey| !journey.shared_evidence) {
            bail!(
                "journeys sharing evidence ({case:?}, {kind:?}) must all set shared_evidence = true"
            );
        }
    }
    Ok(())
}

fn require_journey_dimensions(
    journey: &Journey,
    label: &str,
    values: &[String],
    support: &Support,
) -> Result<()> {
    if values.is_empty() {
        bail!("journey {:?} has empty {label}", journey.id);
    }
    let allowed: BTreeSet<&str> = match label {
        "presubmit_runtimes" => support
            .presubmit_dotnet
            .iter()
            .map(String::as_str)
            .chain(std::iter::once("independent"))
            .collect(),
        "release_runtimes" => support
            .release_dotnet
            .iter()
            .map(String::as_str)
            .chain(std::iter::once("independent"))
            .collect(),
        _ => support
            .profiles
            .iter()
            .map(String::as_str)
            .chain(std::iter::once("independent"))
            .collect(),
    };
    let mut seen = BTreeSet::new();
    for value in values {
        if !allowed.contains(value.as_str()) || !seen.insert(value) {
            bail!(
                "journey {:?} has invalid or duplicate {label} value {value:?}",
                journey.id
            );
        }
    }
    Ok(())
}

fn require_unique_nonempty(label: &str, values: &[String]) -> Result<()> {
    if values.is_empty() {
        bail!("support.{label} must not be empty");
    }
    let mut seen = BTreeSet::new();
    for value in values {
        if value.is_empty() || !seen.insert(value) {
            bail!("support.{label} contains an empty or duplicate value");
        }
    }
    Ok(())
}

fn parse_results(paths: &[std::path::PathBuf]) -> Result<BTreeMap<String, Vec<ResultRow>>> {
    let mut cells: BTreeMap<(String, String, String, String), ResultRow> = BTreeMap::new();
    for path in paths {
        for (case, row) in parse_result_file(path)? {
            let key = (
                case.clone(),
                row.kind.clone(),
                row.dotnet.clone(),
                row.profile.clone(),
            );
            if let Some(existing) = cells.get(&key) {
                if existing != &row {
                    bail!(
                        "conflicting acceptance results for case {case:?}, kind {:?}, net{}/{}",
                        row.kind,
                        row.dotnet,
                        row.profile
                    );
                }
            } else {
                cells.insert(key, row);
            }
        }
    }
    let mut out: BTreeMap<String, Vec<ResultRow>> = BTreeMap::new();
    for ((case, _, _, _), row) in cells {
        out.entry(case).or_default().push(row);
    }
    Ok(out)
}

fn parse_result_file(path: &Path) -> Result<Vec<(String, ResultRow)>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("reading acceptance results {}", path.display()))?;
    let mut lines = text.lines();
    let header = lines.next().context("acceptance results are empty")?;
    let columns: Vec<&str> = header.split('|').collect();
    let kind_idx = columns
        .iter()
        .position(|column| *column == "kind")
        .context("acceptance results have no kind column")?;
    let case_idx = columns
        .iter()
        .position(|column| *column == "case")
        .context("acceptance results have no case column")?;
    let dotnet_idx = columns.iter().position(|column| *column == "dotnet").context(
        "acceptance results have no dotnet column; regenerate them with the current e2e_matrix.sh",
    )?;
    let profile_idx = columns
        .iter()
        .position(|column| *column == "profile")
        .context("acceptance results have no profile column; regenerate them with e2e_matrix.sh")?;
    let result_idx = columns
        .iter()
        .position(|column| *column == "result")
        .context("acceptance results have no result column; regenerate them with e2e_matrix.sh")?;
    let marker_idx = columns
        .iter()
        .position(|column| *column == "marker")
        .context("acceptance results have no marker column")?;
    let required_idx = columns
        .iter()
        .position(|column| *column == "required")
        .context("acceptance results have no required column")?;
    let mut out = Vec::new();
    for (line_no, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('|').collect();
        if fields.len() != columns.len() {
            bail!("malformed acceptance result at line {}", line_no + 2);
        }
        let result = fields[result_idx];
        if !matches!(result, "PASS" | "FAIL") {
            bail!("invalid result {result:?} at line {}", line_no + 2);
        }
        if !matches!(fields[marker_idx], "yes" | "no")
            || !matches!(fields[required_idx], "yes" | "no")
        {
            bail!("invalid marker contract at line {}", line_no + 2);
        }
        out.push((
            fields[case_idx].to_string(),
            ResultRow {
                kind: fields[kind_idx].to_string(),
                dotnet: fields[dotnet_idx].to_string(),
                profile: fields[profile_idx].to_string(),
                result: result.to_string(),
                marker: fields[marker_idx].to_string(),
                required: fields[required_idx].to_string(),
            },
        ));
    }
    Ok(out)
}

fn validate_result_dimensions(
    manifest: &Manifest,
    observed: &BTreeMap<String, Vec<ResultRow>>,
) -> Result<()> {
    for (case, rows) in observed {
        let journeys: Vec<&Journey> = manifest
            .journey
            .iter()
            .filter(|journey| journey.case == *case)
            .collect();
        for row in rows {
            if journeys.is_empty() {
                if matches!(
                    row.kind.as_str(),
                    "native_diff" | "managed_selfcheck" | "managed_host"
                ) {
                    // The compatibility matrix intentionally contains many probes beyond the
                    // product-journey manifest. They remain valid evidence, but do not affect a
                    // capability status until a journey explicitly references them.
                    continue;
                }
                bail!("acceptance results contain unknown scripted case {case:?}");
            }
            let matching: Vec<&Journey> = journeys
                .iter()
                .copied()
                .filter(|journey| journey.evidence_kind == row.kind)
                .collect();
            if matching.is_empty() {
                bail!(
                    "acceptance result for case {case:?} uses undeclared evidence kind {:?}",
                    row.kind
                );
            }
            let matrix_kind = matches!(
                row.kind.as_str(),
                "native_diff" | "managed_selfcheck" | "managed_host"
            );
            let dimension_matches = if matrix_kind {
                manifest.support.dotnet_cli_profiles.contains(&row.dotnet)
                    && manifest.support.profiles.contains(&row.profile)
            } else {
                matching.iter().any(|journey| {
                    let runtimes = journey
                        .presubmit_runtimes
                        .iter()
                        .chain(&journey.release_runtimes);
                    let profiles = journey
                        .presubmit_profiles
                        .iter()
                        .chain(&journey.release_profiles);
                    runtimes.clone().any(|value| value == &row.dotnet)
                        && profiles.clone().any(|value| value == &row.profile)
                })
            };
            if !dimension_matches {
                bail!(
                    "acceptance result for case {case:?}, kind {:?} uses undeclared cell net{}/{}",
                    row.kind,
                    row.dotnet,
                    row.profile
                );
            }
            if row.result == "PASS"
                && matching
                    .iter()
                    .any(|journey| !journey.completion_marker.is_empty())
                && (row.marker != "yes" || row.required != "yes")
            {
                bail!(
                    "passing acceptance result for case {case:?}, kind {:?} did not prove its required completion marker",
                    row.kind
                );
            }
        }
    }
    Ok(())
}

fn observation_for(
    journey: &Journey,
    observed: &BTreeMap<String, Vec<ResultRow>>,
    scope: CapabilitiesEvidenceScope,
) -> Observation {
    let matching_rows: Vec<&ResultRow> = observed
        .get(&journey.case)
        .into_iter()
        .flatten()
        .filter(|row| row.kind == journey.evidence_kind)
        .collect();
    let rows = matching_rows.as_slice();
    if !journey.presubmit {
        return Observation {
            status: if rows.is_empty() {
                "NOT RUN"
            } else if rows.iter().all(|row| row.result == "PASS") {
                "PASS"
            } else {
                "FAIL"
            },
            evidence: EvidenceCoverage {
                required_cells: 0,
                observed_cells: rows
                    .iter()
                    .map(|row| (row.dotnet.as_str(), row.profile.as_str()))
                    .collect::<BTreeSet<_>>()
                    .len(),
                missing_cells: Vec::new(),
            },
        };
    }

    let (required_runtimes, required_profiles) = match scope {
        CapabilitiesEvidenceScope::Presubmit => {
            (&journey.presubmit_runtimes, &journey.presubmit_profiles)
        }
        CapabilitiesEvidenceScope::Release => {
            (&journey.release_runtimes, &journey.release_profiles)
        }
    };
    let required_cells: Vec<(String, String)> = required_runtimes
        .iter()
        .flat_map(|dotnet| {
            required_profiles
                .iter()
                .map(move |profile| (dotnet.clone(), profile.clone()))
        })
        .collect();
    let required: BTreeSet<(String, String)> = required_cells.iter().cloned().collect();
    let present: BTreeSet<(String, String)> = rows
        .iter()
        .map(|row| (row.dotnet.clone(), row.profile.clone()))
        .collect();
    let missing: Vec<String> = required_cells
        .iter()
        .filter(|cell| !present.contains(*cell))
        .map(|(dotnet, profile)| format!("net{dotnet}/{profile}"))
        .collect();
    let status = if rows.is_empty() {
        "NOT RUN"
    } else if rows.iter().any(|row| row.result == "FAIL") {
        "FAIL"
    } else if missing.is_empty() {
        "PASS"
    } else {
        "PARTIAL"
    };
    Observation {
        status,
        evidence: EvidenceCoverage {
            required_cells: required.len(),
            observed_cells: required.intersection(&present).count(),
            missing_cells: missing,
        },
    }
}

fn strict_evidence_failed(
    manifest: &Manifest,
    observed: &BTreeMap<String, Vec<ResultRow>>,
    scope: CapabilitiesEvidenceScope,
) -> bool {
    manifest.journey.iter().any(|journey| {
        journey.presubmit && observation_for(journey, observed, scope).status != "PASS"
    })
}

fn scope_name(scope: CapabilitiesEvidenceScope) -> &'static str {
    match scope {
        CapabilitiesEvidenceScope::Presubmit => "presubmit",
        CapabilitiesEvidenceScope::Release => "release",
    }
}

fn render_markdown(
    manifest: &Manifest,
    path: &Path,
    observed: &BTreeMap<String, Vec<ResultRow>>,
    scope: CapabilitiesEvidenceScope,
) -> String {
    let support = &manifest.support;
    let mut out = format!(
        "# Capability report\n\nGenerated from `{}`. Support and journey definitions come from the manifest; observed PASS/FAIL/PARTIAL values, when present, come only from the supplied acceptance result TSV files. A presubmit journey reaches PASS only when every runtime/profile cell explicitly required by that journey for the selected `{}` evidence scope is present and passing.\n\n## Supported surface\n\n| Field | Value |\n|---|---|\n| Rust toolchain | `{}` |\n| Target | `{}` |\n| CLI runtime profiles | {} |\n| Default runtime | .NET {} |\n| Presubmit runtimes | {} |\n| Release runtime | {} |\n| Build profiles | {} |\n| Host OS | {} |\n\n## Journeys\n\n| Journey | Outcome | Fixture | Evidence kind | Oracle | Presubmit | Observed | Coverage | Missing |\n|---|---|---|---|---|---:|---|---:|---|\n",
        path.display(),
        scope_name(scope),
        support.rust_toolchain,
        support.target,
        support.dotnet_cli_profiles.join(", "),
        support.default_dotnet,
        support.presubmit_dotnet.join(", "),
        support.release_dotnet.join(", "),
        support.profiles.join(", "),
        support.host_os.join(", "),
    );
    for journey in &manifest.journey {
        let observation = observation_for(journey, observed, scope);
        let coverage = if journey.presubmit {
            format!(
                "{}/{}",
                observation.evidence.observed_cells, observation.evidence.required_cells
            )
        } else {
            "n/a".to_string()
        };
        let missing = if observation.evidence.missing_cells.is_empty() {
            "—".to_string()
        } else {
            observation.evidence.missing_cells.join(", ")
        };
        out.push_str(&format!(
            "| {} | {} | `{}` | `{}` | `{}` | {} | {} | {} | {} |\n",
            escape(&journey.id),
            escape(&journey.outcome),
            escape(&journey.fixture),
            escape(&journey.evidence_kind),
            escape(&journey.oracle),
            if journey.presubmit { "yes" } else { "no" },
            observation.status,
            coverage,
            missing,
        ));
    }
    out.push_str("\n## Known limits\n\n");
    for limit in &support.known_limits {
        out.push_str(&format!("- {}\n", limit));
    }
    out
}

fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("creating temporary report in {}", parent.display()))?;
    temp.write_all(contents)
        .with_context(|| format!("writing temporary capability report for {}", path.display()))?;
    temp.as_file()
        .sync_all()
        .with_context(|| format!("syncing temporary capability report for {}", path.display()))?;
    temp.persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("atomically replacing capability report {}", path.display()))?;
    Ok(())
}

fn escape(value: &str) -> String {
    value.replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn support() -> Support {
        Support {
            rust_toolchain: "nightly-test".into(),
            target: "test-target".into(),
            dotnet_cli_profiles: vec!["10".into()],
            default_dotnet: "10".into(),
            presubmit_dotnet: vec!["10".into()],
            release_dotnet: vec!["10".into()],
            profiles: vec!["debug".into(), "release".into()],
            host_os: vec!["linux".into()],
            known_limits: Vec::new(),
        }
    }

    fn journey() -> Journey {
        Journey {
            id: "j".into(),
            outcome: "o".into(),
            fixture: "f".into(),
            case: "case".into(),
            evidence_kind: "managed_selfcheck".into(),
            shared_evidence: false,
            oracle: "managed_selfcheck".into(),
            comparison: "marker".into(),
            completion_marker: String::new(),
            required_artifacts: Vec::new(),
            presubmit_runtimes: vec!["10".into()],
            presubmit_profiles: vec!["debug".into(), "release".into()],
            release_runtimes: vec!["10".into()],
            release_profiles: vec!["release".into()],
            presubmit: true,
        }
    }

    fn row(dotnet: &str, profile: &str, result: &str) -> ResultRow {
        ResultRow {
            kind: "managed_selfcheck".into(),
            dotnet: dotnet.into(),
            profile: profile.into(),
            result: result.into(),
            marker: "yes".into(),
            required: "yes".into(),
        }
    }

    #[test]
    fn presubmit_requires_every_runtime_profile_cell() {
        let journey_cfg = journey();
        let empty = observation_for(
            &journey_cfg,
            &BTreeMap::new(),
            CapabilitiesEvidenceScope::Presubmit,
        );
        assert_eq!(empty.status, "NOT RUN");
        assert_eq!(empty.evidence.observed_cells, 0);
        assert_eq!(empty.evidence.required_cells, 2);

        let mut rows = BTreeMap::new();
        rows.insert("case".into(), vec![row("10", "release", "PASS")]);
        let partial = observation_for(&journey_cfg, &rows, CapabilitiesEvidenceScope::Presubmit);
        assert_eq!(partial.status, "PARTIAL");
        assert_eq!(partial.evidence.observed_cells, 1);
        assert_eq!(partial.evidence.required_cells, 2);

        rows.insert(
            "case".into(),
            vec![
                row("10", "debug", "PASS"),
                row("10", "release", "PASS"),
                row("10", "release", "PASS"),
            ],
        );
        assert_eq!(
            observation_for(&journey_cfg, &rows, CapabilitiesEvidenceScope::Presubmit).status,
            "PASS"
        );

        let complete_manifest = Manifest {
            schema: 1,
            support: support(),
            blocker: Vec::new(),
            journey: vec![journey()],
        };
        assert!(!strict_evidence_failed(
            &complete_manifest,
            &rows,
            CapabilitiesEvidenceScope::Presubmit
        ));

        let release = observation_for(&journey_cfg, &rows, CapabilitiesEvidenceScope::Release);
        assert_eq!(release.status, "PASS");
        assert_eq!(release.evidence.required_cells, 1);

        rows.get_mut("case")
            .unwrap()
            .push(row("10", "debug", "FAIL"));
        assert_eq!(
            observation_for(&journey_cfg, &rows, CapabilitiesEvidenceScope::Presubmit).status,
            "FAIL"
        );
        assert!(strict_evidence_failed(
            &complete_manifest,
            &rows,
            CapabilitiesEvidenceScope::Presubmit
        ));
    }

    #[test]
    fn result_parser_requires_runtime_and_profile_dimensions() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("matrix.tsv");
        fs::write(
            &path,
            "kind|profile|case|marker|required|result\nmanaged|release|case|yes|yes|PASS\n",
        )
        .unwrap();
        let error = parse_results(std::slice::from_ref(&path))
            .unwrap_err()
            .to_string();
        assert!(error.contains("no dotnet column"));

        fs::write(
            &path,
            "kind|dotnet|profile|case|marker|required|result\nmanaged_selfcheck|10|release|case|yes|yes|PASS\n",
        )
        .unwrap();
        let parsed = parse_results(std::slice::from_ref(&path)).unwrap();
        assert_eq!(parsed["case"], vec![row("10", "release", "PASS")]);
    }

    #[test]
    fn result_parser_merges_files_and_rejects_conflicting_cells() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first.tsv");
        let second = temp.path().join("second.tsv");
        let header = "kind|dotnet|profile|case|marker|required|result\n";
        fs::write(
            &first,
            format!("{header}managed_selfcheck|10|release|case|yes|yes|PASS\n"),
        )
        .unwrap();
        fs::write(
            &second,
            format!("{header}managed_selfcheck|10|release|case|yes|yes|PASS\n"),
        )
        .unwrap();
        let parsed = parse_results(&[first.clone(), second.clone()]).unwrap();
        assert_eq!(parsed["case"], vec![row("10", "release", "PASS")]);

        fs::write(
            &second,
            format!("{header}managed_selfcheck|10|release|case|yes|yes|FAIL\n"),
        )
        .unwrap();
        let error = parse_results(&[first, second]).unwrap_err().to_string();
        assert!(error.contains("conflicting acceptance results"));
    }

    #[test]
    fn validation_rejects_unknown_evidence_and_unproven_markers() {
        let manifest = Manifest {
            schema: 1,
            support: support(),
            blocker: Vec::new(),
            journey: vec![Journey {
                completion_marker: "== done ==".into(),
                ..journey()
            }],
        };
        let mut observed = BTreeMap::new();
        let mut bad_kind = row("10", "release", "PASS");
        bad_kind.kind = "other".into();
        observed.insert("case".into(), vec![bad_kind]);
        assert!(
            validate_result_dimensions(&manifest, &observed)
                .unwrap_err()
                .to_string()
                .contains("undeclared evidence kind")
        );

        let mut missing_marker = row("10", "release", "PASS");
        missing_marker.marker = "no".into();
        observed.insert("case".into(), vec![missing_marker]);
        assert!(
            validate_result_dimensions(&manifest, &observed)
                .unwrap_err()
                .to_string()
                .contains("did not prove its required completion marker")
        );
    }

    #[test]
    fn report_write_is_atomic_and_replaceable() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("report.md");
        write_atomic(&path, b"first\n").unwrap();
        write_atomic(&path, b"second\n").unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "second\n");
    }

    #[test]
    fn checked_in_manifest_has_a_satisfiable_explicit_release_contract() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../acceptance/capabilities.toml");
        let manifest: Manifest = toml::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        validate(&manifest).unwrap();

        let mut observed: BTreeMap<String, Vec<ResultRow>> = BTreeMap::new();
        let mut unique = BTreeSet::new();
        for journey in manifest.journey.iter().filter(|journey| journey.presubmit) {
            for runtime in &journey.release_runtimes {
                for profile in &journey.release_profiles {
                    let key = (
                        journey.case.clone(),
                        journey.evidence_kind.clone(),
                        runtime.clone(),
                        profile.clone(),
                    );
                    if unique.insert(key) {
                        observed
                            .entry(journey.case.clone())
                            .or_default()
                            .push(ResultRow {
                                kind: journey.evidence_kind.clone(),
                                dotnet: runtime.clone(),
                                profile: profile.clone(),
                                result: "PASS".into(),
                                marker: "yes".into(),
                                required: "yes".into(),
                            });
                    }
                }
            }
        }
        validate_result_dimensions(&manifest, &observed).unwrap();
        assert!(!strict_evidence_failed(
            &manifest,
            &observed,
            CapabilitiesEvidenceScope::Release
        ));

        observed.remove("install_bundle_acceptance");
        assert!(strict_evidence_failed(
            &manifest,
            &observed,
            CapabilitiesEvidenceScope::Release
        ));
    }
}
