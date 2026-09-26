//! A new workflow, as the console's "new workflow" form fills it in.
//!
//! The draft is pure: it turns the form's fields into the YAML a repository
//! gets, and it refuses anything that would not be a valid workflow file. The
//! text it produces is meant to be read and refined in the editor afterwards,
//! not to model everything a workflow can express.

use crate::yaml::is_number;

/// Where new workflow files live in a repository.
const WORKFLOW_DIRECTORY: &str = ".github/workflows";

/// The runner a draft starts from when the form leaves it unset.
const DEFAULT_RUNNER: &str = "ubuntu-latest";

/// The action every generated job starts with.
const CHECKOUT_STEP: &str = "actions/checkout@v4";

/// The runners the "operating system" field offers.
pub const RUNNERS: [&str; 5] = [
    "ubuntu-latest",
    "ubuntu-24.04",
    "ubuntu-22.04",
    "windows-latest",
    "macos-latest",
];

/// Why a draft cannot become a workflow file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftProblem {
    /// The file name is empty, or GitHub would not accept it as a file in
    /// `.github/workflows/`.
    FileName,
    /// A job identifier is missing, or is not a plain YAML key.
    JobId,
    /// The draft has no jobs, so it would not run anything.
    NoJobs,
}

/// One job in the draft.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobDraft {
    /// The job's identifier, which is also its key under `jobs:`.
    pub id: String,
    /// The job's display name; an empty one leaves the key to speak for itself.
    pub name: String,
    /// One shell command for the job. An empty command leaves the job with
    /// nothing but the checkout step.
    pub command: String,
}

/// A workflow the console is about to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowDraft {
    /// The file's name inside `.github/workflows/`, with or without its
    /// extension.
    pub file_name: String,
    /// The workflow's `name:`. An empty one falls back to the file's stem.
    pub name: String,
    /// The runner every job runs on.
    pub runs_on: String,
    /// The container image every job runs in, when the workflow uses one.
    pub container: Option<String>,
    pub jobs: Vec<JobDraft>,
}

impl Default for WorkflowDraft {
    fn default() -> Self {
        Self {
            file_name: String::new(),
            name: String::new(),
            runs_on: DEFAULT_RUNNER.to_owned(),
            container: None,
            jobs: vec![JobDraft::default()],
        }
    }
}

impl WorkflowDraft {
    /// The workflow file's repository path, e.g. `.github/workflows/ci.yml`.
    pub fn path(&self) -> Result<String, DraftProblem> {
        let name = self.file_name.trim();
        let name = name
            .strip_suffix(".yaml")
            .or_else(|| name.strip_suffix(".yml"))
            .unwrap_or(name);

        let usable = !name.is_empty()
            && name
                .chars()
                .any(|character| character.is_ascii_alphanumeric())
            && name.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
            });
        if !usable {
            return Err(DraftProblem::FileName);
        }

        Ok(format!("{WORKFLOW_DIRECTORY}/{name}.yml"))
    }

    /// The workflow's `name:`, or the file's stem when the form left it empty.
    pub fn workflow_name(&self) -> String {
        let name = self.name.trim();
        if !name.is_empty() {
            return name.to_owned();
        }

        let file = self.file_name.trim();
        file.strip_suffix(".yaml")
            .or_else(|| file.strip_suffix(".yml"))
            .unwrap_or(file)
            .to_owned()
    }

    /// The container image to run in, when the form asked for one.
    pub fn container_image(&self) -> Option<&str> {
        self.container
            .as_deref()
            .map(str::trim)
            .filter(|image| !image.is_empty())
    }

    fn runner(&self) -> &str {
        let runner = self.runs_on.trim();
        if runner.is_empty() {
            DEFAULT_RUNNER
        } else {
            runner
        }
    }

    /// The workflow file's text. A draft that is not a workflow yet comes back
    /// as the reason instead of as text.
    pub fn to_yaml(&self) -> Result<String, DraftProblem> {
        self.path()?;
        if self.jobs.is_empty() {
            return Err(DraftProblem::NoJobs);
        }
        for job in &self.jobs {
            if !is_job_id(job.id.trim()) {
                return Err(DraftProblem::JobId);
            }
        }

        let mut yaml = String::new();
        yaml.push_str(&format!("name: {}\n\n", scalar(&self.workflow_name())));
        // A workflow the console made is meant to be run from the console.
        yaml.push_str("on:\n  workflow_dispatch:\n\n");
        yaml.push_str("jobs:\n");

        for job in &self.jobs {
            yaml.push_str(&format!("  {}:\n", job.id.trim()));
            if !job.name.trim().is_empty() {
                yaml.push_str(&format!("    name: {}\n", scalar(job.name.trim())));
            }
            yaml.push_str(&format!("    runs-on: {}\n", scalar(self.runner())));
            if let Some(image) = self.container_image() {
                yaml.push_str(&format!("    container: {}\n", scalar(image)));
            }
            yaml.push_str("    steps:\n");
            yaml.push_str(&format!("      - uses: {}\n", CHECKOUT_STEP));
            if !job.command.trim().is_empty() {
                yaml.push_str(&format!("      - run: {}\n", scalar(job.command.trim())));
            }
        }

        Ok(yaml)
    }
}

/// Whether `id` can be a job key: a leading letter or underscore, then letters,
/// digits, dashes and underscores.
fn is_job_id(id: &str) -> bool {
    let mut characters = id.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && characters
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

/// Write `text` as YAML, quoting it when a plain scalar would be read as
/// something else — a number, a boolean, a nested key, an empty string.
fn scalar(text: &str) -> String {
    let plain = !text.is_empty()
        && text.trim() == text
        && !text.chars().any(|character| {
            matches!(
                character,
                ':' | '#'
                    | '{'
                    | '}'
                    | '['
                    | ']'
                    | ','
                    | '&'
                    | '*'
                    | '?'
                    | '|'
                    | '>'
                    | '!'
                    | '%'
                    | '@'
                    | '`'
                    | '"'
                    | '\''
                    | '\n'
            )
        })
        && !matches!(
            text.to_ascii_lowercase().as_str(),
            "true" | "false" | "null" | "~" | "yes" | "no" | "on" | "off"
        )
        && !is_number(text)
        && !text
            .chars()
            .next()
            .is_some_and(|first| matches!(first, '-' | '?' | ':'));

    if plain {
        text.to_owned()
    } else {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(file_name: &str) -> WorkflowDraft {
        WorkflowDraft {
            file_name: file_name.to_owned(),
            name: "CI".to_owned(),
            runs_on: "ubuntu-latest".to_owned(),
            container: None,
            jobs: vec![JobDraft {
                id: "build".to_owned(),
                name: "Build".to_owned(),
                command: "npm test".to_owned(),
            }],
        }
    }

    #[test]
    fn the_file_name_becomes_a_workflow_path() {
        assert_eq!(draft("ci").path().unwrap(), ".github/workflows/ci.yml");
        assert_eq!(draft("ci.yml").path().unwrap(), ".github/workflows/ci.yml");
        assert_eq!(
            draft("release-notes.yaml").path().unwrap(),
            ".github/workflows/release-notes.yml"
        );
    }

    #[test]
    fn a_file_name_that_is_not_a_file_is_refused() {
        for name in ["", "   ", "a/b", "a b", ".", "..", "ci\\x"] {
            assert_eq!(
                draft(name).path(),
                Err(DraftProblem::FileName),
                "{name:?} should not be a workflow file"
            );
        }
    }

    #[test]
    fn an_empty_workflow_name_falls_back_to_the_file_name() {
        let mut draft = draft("nightly.yml");
        draft.name = "   ".to_owned();

        assert_eq!(draft.workflow_name(), "nightly");
    }

    #[test]
    fn a_job_id_must_be_a_plain_key() {
        for id in ["", "1build", "build it", "build.it", "-build"] {
            let mut draft = draft("ci");
            draft.jobs[0].id = id.to_owned();
            assert_eq!(draft.to_yaml(), Err(DraftProblem::JobId), "{id:?}");
        }

        for id in ["build", "_build", "build-1", "Build_2"] {
            let mut draft = draft("ci");
            draft.jobs[0].id = id.to_owned();
            assert!(draft.to_yaml().is_ok(), "{id:?} should be a job key");
        }
    }

    #[test]
    fn a_draft_without_jobs_is_refused() {
        let mut draft = draft("ci");
        draft.jobs.clear();

        assert_eq!(draft.to_yaml(), Err(DraftProblem::NoJobs));
    }

    #[test]
    fn the_yaml_carries_the_runner_job_and_steps() {
        let yaml = draft("ci").to_yaml().unwrap();

        assert_eq!(
            yaml,
            "name: CI\n\
             \n\
             on:\n  workflow_dispatch:\n\
             \n\
             jobs:\n  build:\n    name: Build\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - run: npm test\n"
        );
    }

    #[test]
    fn a_container_is_written_only_when_the_form_asked_for_one() {
        let mut draft = draft("ci");
        draft.container = Some("  node:20-bullseye  ".to_owned());
        let yaml = draft.to_yaml().unwrap();
        // The colon makes the image a quoted scalar, which YAML needs.
        assert!(
            yaml.contains("    container: \"node:20-bullseye\"\n"),
            "{yaml}"
        );

        draft.container = Some("   ".to_owned());
        assert!(!draft.to_yaml().unwrap().contains("container:"));
    }

    #[test]
    fn every_job_gets_its_own_runner_and_command() {
        let mut draft = draft("ci");
        draft.jobs.push(JobDraft {
            id: "package".to_owned(),
            name: String::new(),
            command: String::new(),
        });

        let yaml = draft.to_yaml().unwrap();

        assert_eq!(yaml.matches("runs-on: ubuntu-latest").count(), 2);
        assert!(yaml.contains("  package:\n    runs-on: ubuntu-latest\n"));
        // The second job has no name and nothing to run.
        assert!(!yaml.contains("    name: package"));
        assert_eq!(yaml.matches("- uses: actions/checkout@v4").count(), 2);
        assert_eq!(yaml.matches("- run:").count(), 1);
    }

    #[test]
    fn text_that_is_not_a_plain_scalar_is_quoted() {
        assert_eq!(scalar("Build"), "Build");
        assert_eq!(scalar("ubuntu-latest"), "ubuntu-latest");
        assert_eq!(scalar(""), "\"\"");
        assert_eq!(scalar("release: nightly"), "\"release: nightly\"");
        assert_eq!(scalar("#1"), "\"#1\"");
        assert_eq!(scalar("42"), "\"42\"");
        assert_eq!(scalar("true"), "\"true\"");
        assert_eq!(scalar("- build"), "\"- build\"");
        assert_eq!(scalar("say \"hi\""), "\"say \\\"hi\\\"\"");
        // A backslash is plain in YAML; it only needs escaping inside the
        // quotes something else forced.
        assert_eq!(scalar("back\\slash"), "back\\slash");
        assert_eq!(scalar("a\"b\\"), "\"a\\\"b\\\\\"");
    }

    #[test]
    fn the_yaml_ends_with_a_newline() {
        let yaml = draft("ci").to_yaml().unwrap();

        assert!(yaml.ends_with('\n'));
        assert!(!yaml.ends_with("\n\n"));
    }
}
