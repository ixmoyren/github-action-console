//! The release model: what a repository can release, where each channel points,
//! and how far a release has got — all without touching GitHub.
//!
//! The manifest reader is a deliberately small YAML subset: block mappings,
//! block sequences and scalars. Flow collections, anchors and multi-line
//! scalars are refused with a message naming the line, rather than half-read.

use std::collections::BTreeMap;
use std::fmt;

use crate::github::{WorkflowRun, any_running};

/// Where a repository's release manifest lives.
pub const MANIFEST_PATH: &str = ".github/release-console.yml";

/// The channels every release target moves along, in board order.
pub const CHANNELS: [&str; 3] = ["lts", "latest", "dogfood"];

/// Why a manifest could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestProblem {
    /// The manifest declares no release targets at all.
    NoTargets,
    /// A target is missing a field, or the field is empty.
    MissingField { target: String, field: &'static str },
    /// A target names a packaging config the manifest does not define.
    UnknownPackaging { target: String, packaging: String },
    /// A packaging config has no workflow to dispatch.
    MissingWorkflow { packaging: String },
    /// A name is empty or carries characters the manifest does not allow.
    BadName { line: usize, name: String },
    /// A workflow value is empty.
    BadWorkflow { name: String },
    /// A line of the manifest uses YAML this reader does not accept.
    Unsupported { line: usize, text: String },
}

impl fmt::Display for ManifestProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTargets => write!(formatter, "发布清单里没有发布目标。"),
            Self::MissingField { target, field } => {
                write!(formatter, "发布目标「{target}」缺少 {field}。")
            }
            Self::UnknownPackaging { target, packaging } => write!(
                formatter,
                "发布目标「{target}」引用了清单里没有的打包配置「{packaging}」。"
            ),
            Self::MissingWorkflow { packaging } => {
                write!(
                    formatter,
                    "打包配置「{packaging}」没有写要触发的 workflow。"
                )
            }
            Self::BadName { line, name } => {
                write!(
                    formatter,
                    "第 {line} 行的名字「{name}」不合法：只能用字母、数字、点和减号。"
                )
            }
            Self::BadWorkflow { name } => {
                write!(formatter, "打包配置「{name}」的 workflow 是空的。")
            }
            Self::Unsupported { line, text } => write!(
                formatter,
                "第 {line} 行「{text}」用了这份清单不支持的写法（只支持块状映射、块状列表与普通标量）。"
            ),
        }
    }
}

/// The three platforms a release target can build for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Macos,
    Windows,
    Linux,
}

impl Platform {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "macos" => Some(Self::Macos),
            "windows" => Some(Self::Windows),
            "linux" => Some(Self::Linux),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Macos => "macos",
            Self::Windows => "windows",
            Self::Linux => "linux",
        }
    }
}

/// One release target: platform × architecture × distribution, built by one
/// packaging config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseTarget {
    pub name: String,
    pub platform: Platform,
    pub arch: String,
    pub distribution: String,
    /// The packaging config's name.
    pub packaging: String,
    /// Steps this target cannot really run here, e.g. signing.
    pub simulated: Vec<String>,
}

impl ReleaseTarget {
    /// Whether any part of this target's release is simulated.
    pub fn is_simulated(&self) -> bool {
        !self.simulated.is_empty()
    }
}

/// A reusable recipe: the workflow to dispatch and the inputs it takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackagingConfig {
    pub name: String,
    pub workflow: String,
    pub inputs: BTreeMap<String, String>,
}

impl PackagingConfig {
    pub fn input(&self, key: &str) -> Option<&str> {
        self.inputs.get(key).map(String::as_str)
    }
}

/// A repository's release manifest: its targets and its recipes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReleaseManifest {
    targets: Vec<ReleaseTarget>,
    packaging: Vec<PackagingConfig>,
}

impl ReleaseManifest {
    pub fn targets(&self) -> &[ReleaseTarget] {
        &self.targets
    }

    pub fn packaging(&self) -> &[PackagingConfig] {
        &self.packaging
    }

    pub fn target(&self, name: &str) -> Option<&ReleaseTarget> {
        self.targets.iter().find(|target| target.name == name)
    }

    pub fn packaging_config(&self, name: &str) -> Option<&PackagingConfig> {
        self.packaging.iter().find(|config| config.name == name)
    }

    /// The target's recipe: what the trigger form needs to dispatch it.
    pub fn recipe_for(&self, target: &str) -> Option<(&ReleaseTarget, &PackagingConfig)> {
        let target = self.target(target)?;
        let config = self.packaging_config(&target.packaging)?;
        Some((target, config))
    }
}

/// Read a manifest out of its YAML text.
pub fn parse_manifest(source: &str) -> Result<ReleaseManifest, ManifestProblem> {
    let mut targets: Vec<PartialTarget> = Vec::new();
    let mut configs: Vec<PartialConfig> = Vec::new();
    let mut section = Section::None;
    let mut child_indent: Option<usize> = None;
    let mut list_key = false;

    for (index, raw) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = strip_comment(raw);
        if line.trim().is_empty() {
            continue;
        }
        if line.contains('\t') {
            return Err(ManifestProblem::Unsupported {
                line: line_number,
                text: raw.trim().to_owned(),
            });
        }

        let indent = line.len() - line.trim_start().len();
        let text = line.trim();
        if let Some(item) = text.strip_prefix("- ") {
            // A sequence item belongs to the last key that opened a block list.
            if !list_key {
                return Err(ManifestProblem::Unsupported {
                    line: line_number,
                    text: text.to_owned(),
                });
            }
            match section {
                Section::Targets => {
                    if let Some(target) = targets.last_mut() {
                        target.simulated.push(scalar(item, line_number)?);
                    }
                }
                _ => {
                    return Err(ManifestProblem::Unsupported {
                        line: line_number,
                        text: text.to_owned(),
                    });
                }
            }
            continue;
        }
        list_key = false;

        let Some((key, value)) = text.split_once(':') else {
            return Err(ManifestProblem::Unsupported {
                line: line_number,
                text: text.to_owned(),
            });
        };
        let key = key.trim();
        let value = value.trim();

        if indent == 0 {
            if value.is_empty() {
                section = match key {
                    "targets" => Section::Targets,
                    "packaging" => Section::Packaging,
                    // `version:` and anything else at the top level is not ours.
                    _ => Section::None,
                };
                child_indent = None;
                continue;
            }
            // A scalar top-level key (a version number, say) is fine; a flow
            // collection is not part of the subset this reader accepts.
            if value.starts_with('[') || value.starts_with('{') {
                return Err(ManifestProblem::Unsupported {
                    line: line_number,
                    text: text.to_owned(),
                });
            }
            continue;
        }

        match section {
            Section::Targets => {
                let child = *child_indent.get_or_insert(indent);
                if indent == child {
                    if !value.is_empty() {
                        return Err(ManifestProblem::Unsupported {
                            line: line_number,
                            text: text.to_owned(),
                        });
                    }
                    targets.push(PartialTarget::new(key.to_owned(), line_number)?);
                    continue;
                }
                let Some(target) = targets.last_mut() else {
                    return Err(ManifestProblem::Unsupported {
                        line: line_number,
                        text: text.to_owned(),
                    });
                };
                match key {
                    "simulated" if value.is_empty() => list_key = true,
                    "platform" => target.platform = Platform::parse(&scalar(value, line_number)?),
                    "arch" => target.arch = Some(scalar(value, line_number)?),
                    "distribution" => target.distribution = Some(scalar(value, line_number)?),
                    "packaging" => target.packaging = Some(scalar(value, line_number)?),
                    _ => {}
                }
            }
            Section::Packaging => {
                let child = *child_indent.get_or_insert(indent);
                if indent == child {
                    if !value.is_empty() {
                        return Err(ManifestProblem::Unsupported {
                            line: line_number,
                            text: text.to_owned(),
                        });
                    }
                    configs.push(PartialConfig::new(key.to_owned(), line_number)?);
                    continue;
                }
                let Some(config) = configs.last_mut() else {
                    return Err(ManifestProblem::Unsupported {
                        line: line_number,
                        text: text.to_owned(),
                    });
                };
                match key {
                    "workflow" => config.workflow = Some(scalar(value, line_number)?),
                    "inputs" if value.is_empty() => {}
                    _ => {
                        config
                            .inputs
                            .insert(scalar(key, line_number)?, scalar(value, line_number)?);
                    }
                }
            }
            Section::None => {}
        }
    }
    finish(targets, configs)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    Targets,
    Packaging,
}

#[derive(Debug, Default)]
struct PartialTarget {
    name: String,
    platform: Option<Platform>,
    arch: Option<String>,
    distribution: Option<String>,
    packaging: Option<String>,
    simulated: Vec<String>,
}

impl PartialTarget {
    fn new(name: String, line: usize) -> Result<Self, ManifestProblem> {
        if !is_name(&name) {
            return Err(ManifestProblem::BadName { line, name });
        }
        Ok(Self {
            name,
            ..Self::default()
        })
    }
}

#[derive(Debug, Default)]
struct PartialConfig {
    name: String,
    workflow: Option<String>,
    inputs: BTreeMap<String, String>,
}

impl PartialConfig {
    fn new(name: String, line: usize) -> Result<Self, ManifestProblem> {
        if !is_name(&name) {
            return Err(ManifestProblem::BadName { line, name });
        }
        Ok(Self {
            name,
            ..Self::default()
        })
    }
}

/// Turn the partial reads into a manifest, or say what is missing.
fn finish(
    pending_targets: Vec<PartialTarget>,
    pending_configs: Vec<PartialConfig>,
) -> Result<ReleaseManifest, ManifestProblem> {
    if pending_targets.is_empty() {
        return Err(ManifestProblem::NoTargets);
    }

    let mut packaging = Vec::with_capacity(pending_configs.len());
    for config in pending_configs {
        let workflow = config.workflow.filter(|value| !value.is_empty()).ok_or(
            ManifestProblem::MissingWorkflow {
                packaging: config.name.clone(),
            },
        )?;
        packaging.push(PackagingConfig {
            name: config.name,
            workflow,
            inputs: config.inputs,
        });
    }

    let mut targets = Vec::with_capacity(pending_targets.len());
    for target in pending_targets {
        let name = target.name;
        let platform = target
            .platform
            .ok_or_else(|| missing(&name, "platform（macos / windows / linux）"))?;
        let arch = filled(target.arch, &name, "arch")?;
        let distribution = filled(target.distribution, &name, "distribution")?;
        let packaging_name = filled(target.packaging, &name, "packaging")?;
        if !packaging.iter().any(|config| config.name == packaging_name) {
            return Err(ManifestProblem::UnknownPackaging {
                target: name,
                packaging: packaging_name,
            });
        }
        targets.push(ReleaseTarget {
            name,
            platform,
            arch,
            distribution,
            packaging: packaging_name,
            simulated: target.simulated,
        });
    }

    Ok(ReleaseManifest { targets, packaging })
}

fn missing(target: &str, field: &'static str) -> ManifestProblem {
    ManifestProblem::MissingField {
        target: target.to_owned(),
        field,
    }
}

/// A field a target must fill in.
fn filled(
    value: Option<String>,
    target: &str,
    field: &'static str,
) -> Result<String, ManifestProblem> {
    value
        .filter(|value| !value.is_empty())
        .ok_or_else(|| missing(target, field))
}

fn strip_comment(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut quote: Option<char> = None;
    for character in line.chars() {
        match quote {
            Some(open) => {
                out.push(character);
                if character == open {
                    quote = None;
                }
            }
            None if character == '"' || character == '\'' => {
                quote = Some(character);
                out.push(character);
            }
            None if character == '#' => break,
            None => out.push(character),
        }
    }
    out
}

/// A scalar as written: quotes come off, the text stays as it is. Flow
/// collections are not part of the subset this reader accepts.
fn scalar(value: &str, line: usize) -> Result<String, ManifestProblem> {
    let value = value.trim();
    if value.starts_with('[') || value.starts_with('{') {
        return Err(ManifestProblem::Unsupported {
            line,
            text: value.to_owned(),
        });
    }
    let unquoted = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        });
    Ok(unquoted.unwrap_or(value).to_owned())
}

fn is_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
        })
}

/// One channel pointer: where a channel points for one release target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelPointer {
    pub repository: String,
    pub target: String,
    pub channel: String,
    pub version: String,
    pub updated_at: String,
}

/// A build the console itself asked for. GitHub's dispatch API answers without
/// a run id, so the console records what it asked for, and binds the run once it
/// appears in the run list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildDispatch {
    pub id: i64,
    pub repository: String,
    pub target: String,
    pub version: String,
    pub config: String,
    pub dispatched_at: String,
    /// The run this dispatch turned into, once the console has seen it.
    pub run_id: Option<u64>,
}

impl BuildDispatch {
    /// The newest dispatch for a target, if the console ever asked for one.
    pub fn latest_for<'a>(dispatches: &'a [BuildDispatch], target: &str) -> Option<&'a Self> {
        dispatches
            .iter()
            .filter(|dispatch| dispatch.target == target)
            .max_by(|left, right| left.dispatched_at.cmp(&right.dispatched_at))
    }
}

/// How far one release target has got on one release version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseState {
    /// Nothing has been built for it yet.
    Todo,
    /// A run is queued or running.
    Building,
    /// The run finished well.
    Built,
    /// The build is real, but signing / notarization / store work is simulated
    /// here and has not happened.
    AwaitingSigning,
    /// Built and installable, waiting for the explicit publish action.
    ReadyToPublish,
    /// The channel pointer says this version is published for this target.
    Published,
    /// The last run failed.
    Failed,
    /// The last run was cancelled.
    Cancelled,
}

impl ReleaseState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Todo => "待构建",
            Self::Building => "构建中",
            Self::Built => "构建成功",
            Self::AwaitingSigning => "待签名公证",
            Self::ReadyToPublish => "待发布",
            Self::Published => "已发布",
            Self::Failed => "失败",
            Self::Cancelled => "已取消",
        }
    }
}

/// Derive a cell's state from the facts the console holds: the target, whether
/// a channel points at this version, and the newest run the console knows of.
///
/// The pointer is what makes "已发布" true — a green build never does.
pub fn release_state(
    target: &ReleaseTarget,
    published: bool,
    latest: Option<&WorkflowRun>,
) -> ReleaseState {
    if published {
        return ReleaseState::Published;
    }
    let Some(run) = latest else {
        return ReleaseState::Todo;
    };
    if any_running(std::slice::from_ref(run)) {
        return ReleaseState::Building;
    }
    match run.conclusion.as_deref() {
        Some("failure") | Some("timed_out") | Some("action_required") => ReleaseState::Failed,
        Some("cancelled") | Some("skipped") => ReleaseState::Cancelled,
        Some("success") if target.is_simulated() => ReleaseState::AwaitingSigning,
        Some("success") => ReleaseState::ReadyToPublish,
        // Finished without a conclusion the console knows: treat it as built.
        Some(_) => ReleaseState::Built,
        None => ReleaseState::Building,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "\
# 发布清单
version: 1

targets:
  web-arm:
    platform: macos
    arch: arm64
    distribution: github-releases
    packaging: macos-dmg
  mas:
    platform: macos
    arch: universal
    distribution: mac-app-store
    packaging: mas-pkg
    simulated:
      - signing
      - notarization

packaging:
  macos-dmg:
    workflow: release-target.yml
    inputs:
      package: dmg
  mas-pkg:
    workflow: release-target.yml
    inputs:
      package: pkg
";

    fn manifest() -> ReleaseManifest {
        parse_manifest(MANIFEST).expect("the sample manifest parses")
    }

    #[test]
    fn a_manifest_reads_its_targets_and_recipes() {
        let manifest = manifest();

        assert_eq!(
            manifest
                .targets()
                .iter()
                .map(|target| target.name.as_str())
                .collect::<Vec<_>>(),
            ["web-arm", "mas"]
        );
        let arm = manifest.target("web-arm").unwrap();
        assert_eq!(arm.platform, Platform::Macos);
        assert_eq!(arm.arch, "arm64");
        assert_eq!(arm.distribution, "github-releases");
        assert_eq!(arm.packaging, "macos-dmg");
        assert!(!arm.is_simulated());

        let mas = manifest.target("mas").unwrap();
        assert_eq!(mas.simulated, ["signing", "notarization"]);
        assert!(mas.is_simulated());

        let recipe = manifest.recipe_for("web-arm").unwrap().1;
        assert_eq!(recipe.workflow, "release-target.yml");
        assert_eq!(recipe.input("package"), Some("dmg"));
    }

    #[test]
    fn a_manifest_without_targets_is_refused() {
        assert_eq!(
            parse_manifest("packaging:\n  macos-dmg:\n    workflow: w.yml\n"),
            Err(ManifestProblem::NoTargets)
        );
    }

    #[test]
    fn a_target_must_name_a_config_the_manifest_defines() {
        let source = "\
targets:
  web-arm:
    platform: macos
    arch: arm64
    distribution: github-releases
    packaging: missing
";
        assert_eq!(
            parse_manifest(source),
            Err(ManifestProblem::UnknownPackaging {
                target: "web-arm".to_owned(),
                packaging: "missing".to_owned(),
            })
        );
    }

    #[test]
    fn a_target_must_say_what_it_is() {
        let source = "\
targets:
  web-arm:
    arch: arm64
    distribution: github-releases
    packaging: macos-dmg
packaging:
  macos-dmg:
    workflow: release-target.yml
";
        assert_eq!(
            parse_manifest(source),
            Err(ManifestProblem::MissingField {
                target: "web-arm".to_owned(),
                field: "platform（macos / windows / linux）",
            })
        );
    }

    #[test]
    fn a_config_needs_a_workflow() {
        let source = "\
targets:
  web-arm:
    platform: macos
    arch: arm64
    distribution: github-releases
    packaging: macos-dmg
packaging:
  macos-dmg:
    inputs:
      package: dmg
";
        assert_eq!(
            parse_manifest(source),
            Err(ManifestProblem::MissingWorkflow {
                packaging: "macos-dmg".to_owned(),
            })
        );
    }

    #[test]
    fn a_name_that_is_not_a_name_is_refused_at_its_line() {
        let source = "targets:\n  web arm:\n    platform: macos\n";

        assert_eq!(
            parse_manifest(source),
            Err(ManifestProblem::BadName {
                line: 2,
                name: "web arm".to_owned(),
            })
        );
    }

    #[test]
    fn yaml_this_reader_does_not_accept_says_which_line() {
        let source = "targets: [web-arm]\n";

        assert_eq!(
            parse_manifest(source),
            Err(ManifestProblem::Unsupported {
                line: 1,
                text: "targets: [web-arm]".to_owned(),
            })
        );
    }

    #[test]
    fn comments_and_quotes_are_handled() {
        let source = "\
targets:
  web-arm:                      # 官网 arm 包
    platform: \"macos\"           # 引号包起来也认
    arch: arm64
    distribution: github-releases
    packaging: macos-dmg
packaging:
  macos-dmg:
    workflow: release-target.yml
    inputs:
      package: dmg
";
        let manifest = parse_manifest(source).unwrap();
        assert_eq!(
            manifest.target("web-arm").unwrap().platform,
            Platform::Macos
        );
    }

    fn run(id: u64, status: crate::github::RunStatus, conclusion: Option<&str>) -> WorkflowRun {
        WorkflowRun {
            id,
            workflow_id: 1,
            name: "release-target".to_owned(),
            status,
            conclusion: conclusion.map(str::to_owned),
            branch: Some("main".to_owned()),
            event: "workflow_dispatch".to_owned(),
            actor: Some("octocat".to_owned()),
            created_at: Some("2026-09-22T10:00:00Z".to_owned()),
            html_url: None,
        }
    }

    #[test]
    fn a_pointer_is_what_makes_a_release_published() {
        use crate::github::RunStatus;
        let manifest = manifest();
        let arm = manifest.target("web-arm").unwrap();
        let mas = manifest.target("mas").unwrap();
        let green = run(1, RunStatus::Completed, Some("success"));
        let red = run(2, RunStatus::Completed, Some("failure"));
        let moving = run(3, RunStatus::InProgress, None);

        // A green build is not a release.
        assert_eq!(
            release_state(arm, false, Some(&green)),
            ReleaseState::ReadyToPublish
        );
        // The pointer is.
        assert_eq!(
            release_state(arm, true, Some(&green)),
            ReleaseState::Published
        );
        assert_eq!(release_state(arm, false, Some(&red)), ReleaseState::Failed);
        assert_eq!(
            release_state(arm, false, Some(&moving)),
            ReleaseState::Building
        );
        assert_eq!(release_state(arm, false, None), ReleaseState::Todo);

        // A target with simulated steps stops at "待签名公证", never "已发布".
        assert_eq!(
            release_state(mas, false, Some(&green)),
            ReleaseState::AwaitingSigning
        );
        assert_eq!(
            release_state(mas, true, Some(&green)),
            ReleaseState::Published
        );
    }
}
