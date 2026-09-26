//! The workflow-file viewer: a `gpui-kit` code editor over the YAML.
//!
//! The editor supplies the frame — line numbers, scrolling, selection, copy.
//! Its syntax highlighting comes from this application rather than a grammar:
//! no tree-sitter language set is compiled in, so [`crate::yaml`] answers
//! through the editor's [`InputHighlighter`] seam and its colours come from the
//! theme's syntax palette (the resolver the editor hands over).

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::component::input::{
    EditorState, FoldRange, HighlightStyleResolver, InputEdit, InputHighlighter,
    InputHighlighterFactory, Rope,
};
// Imported by name rather than through `gpui_kit::*`, which would shadow the
// standard `#[test]` attribute in the tests below.
use gpui_kit::{Context, HighlightStyle, SharedString, Window};

use crate::yaml::{self, TokenKind};

/// The language name a workflow file's editor reports.
const YAML_LANGUAGE: &str = "yaml";

/// Build the editor state a workflow file is shown in.
pub(super) fn yaml_editor_state(window: &mut Window, cx: &mut Context<EditorState>) -> EditorState {
    let mut state = EditorState::new(window, cx).language(YAML_LANGUAGE);
    state.set_highlighter_factory(yaml_highlighter_factory(), cx);
    state
}

fn yaml_highlighter_factory() -> InputHighlighterFactory {
    Rc::new(|language: &str| {
        language
            .eq_ignore_ascii_case(YAML_LANGUAGE)
            .then(|| Box::new(YamlHighlighter::default()) as Box<dyn InputHighlighter>)
    })
}

/// The console's YAML tokenizer, wearing the editor's highlighter trait.
#[derive(Default)]
struct YamlHighlighter {
    tokens: Vec<(Range<usize>, TokenKind)>,
}

impl InputHighlighter for YamlHighlighter {
    fn language(&self) -> SharedString {
        YAML_LANGUAGE.into()
    }

    fn update(
        &mut self,
        _edit: Option<InputEdit>,
        text: &Rope,
        _folding: bool,
        _window: &mut Window,
        _cx: &mut Context<EditorState>,
    ) {
        self.tokens = yaml::highlight(&text.to_string());
    }

    /// The editor asks for style runs that cover `range` in full, so the gaps
    /// between tokens are reported as unstyled text.
    fn styles(
        &self,
        range: &Range<usize>,
        resolver: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        style_runs(&self.tokens, range, &|name| resolver.style(name))
    }

    /// The console shows a file, it does not offer to fold it.
    fn fold_ranges(&self, _text: &Rope) -> Vec<FoldRange> {
        Vec::new()
    }
}

/// Turn tokens into the ordered, gapless style runs the editor asks for: every
/// byte of `range` ends up in exactly one run, and text outside a token keeps
/// the editor's own style.
fn style_runs(
    tokens: &[(Range<usize>, TokenKind)],
    range: &Range<usize>,
    style_of: &dyn Fn(&str) -> Option<HighlightStyle>,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut runs = Vec::new();
    let mut cursor = range.start;

    for (token, kind) in tokens {
        let start = token.start.max(range.start);
        let end = token.end.min(range.end);
        if start >= end || end <= cursor {
            continue;
        }

        if start > cursor {
            runs.push((cursor..start, HighlightStyle::default()));
        }
        runs.push((start..end, style_of(syntax_name(*kind)).unwrap_or_default()));
        cursor = end;
    }

    if cursor < range.end {
        runs.push((cursor..range.end, HighlightStyle::default()));
    }
    runs
}

/// The syntax token each kind borrows its colour from.
fn syntax_name(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Key => "property",
        TokenKind::Value => "string",
        TokenKind::Number => "number",
        TokenKind::Constant => "boolean",
        TokenKind::Expression => "preproc",
        TokenKind::Comment => "comment",
        TokenKind::Punctuation => "punctuation.delimiter",
        TokenKind::Anchor => "label",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::FontWeight;

    /// A resolver that styles one token name, so the tests can tell a styled
    /// run from an unstyled one without depending on the theme's colours.
    struct StubResolver;

    const STUB: StubResolver = StubResolver;

    impl HighlightStyleResolver for StubResolver {
        fn style(&self, name: &str) -> Option<HighlightStyle> {
            (name == "string").then(|| HighlightStyle {
                font_weight: Some(FontWeight::BOLD),
                ..Default::default()
            })
        }
    }

    /// The text each styled run covers.
    fn styled<'a>(source: &'a str, runs: &[(Range<usize>, HighlightStyle)]) -> Vec<&'a str> {
        runs.iter()
            .filter(|(_, style)| style.font_weight.is_some())
            .map(|(range, _)| &source[range.clone()])
            .collect()
    }

    #[test]
    fn the_factory_claims_only_the_editors_language() {
        let factory = yaml_highlighter_factory();

        assert!(factory(YAML_LANGUAGE).is_some());
        assert!(factory("python").is_none());
    }

    #[test]
    fn style_runs_cover_the_range_and_style_the_values() {
        let source = "name: build\nother: ci\n";
        let tokens = yaml::highlight(source);

        let runs = style_runs(&tokens, &(0..source.len()), &|name| STUB.style(name));

        let mut cursor = 0;
        for (range, _) in &runs {
            assert_eq!(range.start, cursor, "{range:?} does not continue the run");
            cursor = range.end;
        }
        assert_eq!(cursor, source.len(), "the runs leave text uncovered");
        assert_eq!(styled(source, &runs), ["build", "ci"]);
    }

    #[test]
    fn style_runs_clip_to_the_requested_range() {
        let source = "name: build\nother: ci\n";
        let tokens = yaml::highlight(source);
        let range = 0..6;

        let runs = style_runs(&tokens, &range, &|name| STUB.style(name));

        let mut cursor = range.start;
        for (run, _) in &runs {
            assert_eq!(run.start, cursor);
            assert!(run.end <= range.end);
            cursor = run.end;
        }
        assert_eq!(cursor, range.end);
        // The first six bytes hold only the key and its colon.
        assert!(styled(source, &runs).is_empty());
    }

    #[test]
    fn a_range_without_tokens_is_one_unstyled_run() {
        let range = 3..9;

        let runs = style_runs(&[], &range, &|_| None);

        assert_eq!(runs, vec![(range, HighlightStyle::default())]);
    }
}
