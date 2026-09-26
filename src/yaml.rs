//! A shallow YAML tokenizer for the workflow-file viewer.
//!
//! It exists so a workflow file can be shown with its keys, scalars, comments
//! and `${{ }}` expressions in different colours. It is not a parser: anything
//! it cannot classify is simply left out, and the viewer paints the gaps in its
//! default colour. Block scalars (`|`, `>`) keep their body out of the token
//! stream entirely, so a shell script inside `run:` is not mis-read as YAML.

use std::ops::Range;

/// What a run of characters in a workflow file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// A mapping key, like `runs-on` in `runs-on: ubuntu-latest`.
    Key,
    /// A scalar value, whether quoted or plain.
    Value,
    /// A number.
    Number,
    /// `true` / `false` / `null` and their relatives.
    Constant,
    /// A `${{ … }}` expression.
    Expression,
    /// A `#` comment.
    Comment,
    /// Structural punctuation: `-`, `:`, `|`, `{`, `}`, `[`, `]`, `,`.
    Punctuation,
    /// An anchor or alias: `&name`, `*name`.
    Anchor,
}

/// Every highlighted run of `source`, as byte ranges paired with their kind.
///
/// Tokens arrive in source order and never overlap. Text between two tokens is
/// plain as far as the viewer is concerned.
pub fn highlight(source: &str) -> Vec<(Range<usize>, TokenKind)> {
    let mut tokens = Vec::new();
    let mut offset = 0;
    let mut block_scalar_indent = None;

    for line in source.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        let indent = indent_of(body);

        // A block scalar's body is more indented than the line that opened it.
        // Blank lines stay inside it, so a wrapped script keeps its body plain.
        if let Some(opener_indent) = block_scalar_indent
            && (body.trim().is_empty() || indent > opener_indent)
        {
            offset += line.len();
            continue;
        }

        block_scalar_indent = if highlight_line(body, offset, &mut tokens) {
            Some(indent)
        } else {
            None
        };
        offset += line.len();
    }

    tokens
}

/// The number of leading spaces and tabs.
fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

/// Tokenize one line. Returns whether the line ends in a block scalar opener,
/// which makes the lines under it the scalar's body.
fn highlight_line(line: &str, base: usize, tokens: &mut Vec<(Range<usize>, TokenKind)>) -> bool {
    let mut cursor = Cursor::new(line);
    let mut opens_block = false;
    cursor.consume_while(|c| c == ' ' || c == '\t');

    while let Some(character) = cursor.peek() {
        let start = cursor.pos;
        let kind = match character {
            '#' => {
                push(tokens, base + start, base + line.len(), TokenKind::Comment);
                return opens_block;
            }
            '"' | '\'' => {
                consume_quoted(&mut cursor, character);
                TokenKind::Value
            }
            '$' if cursor.rest().starts_with("${{") => {
                cursor.pos = match cursor.rest().find("}}") {
                    Some(end) => cursor.pos + end + 2,
                    None => line.len(),
                };
                TokenKind::Expression
            }
            '&' | '*' => {
                cursor.bump();
                cursor.consume_while(|c| !is_delimiter(c));
                TokenKind::Anchor
            }
            '|' | '>' => {
                cursor.bump();
                cursor.consume_while(|c| matches!(c, '-' | '+') || c.is_ascii_digit());
                opens_block = true;
                TokenKind::Punctuation
            }
            '[' | ']' | '{' | '}' | ',' | ':' => {
                cursor.bump();
                TokenKind::Punctuation
            }
            '-' if is_list_marker(line, cursor.pos) => {
                cursor.bump();
                TokenKind::Punctuation
            }
            character if character.is_whitespace() => {
                cursor.bump();
                continue;
            }
            _ => {
                cursor.consume_while(|c| !is_delimiter(c));
                if cursor.pos == start {
                    // A character the delimiter set does not know about, so the
                    // word loop cannot move. Skip it and keep going.
                    cursor.bump();
                    continue;
                }
                let word = &line[start..cursor.pos];
                if is_key(line, cursor.pos) {
                    TokenKind::Key
                } else {
                    classify(word)
                }
            }
        };

        // Only a trailing `|` or `>` opens a block; a later value on the same
        // line takes that back.
        if kind != TokenKind::Punctuation {
            opens_block = false;
        }
        push(tokens, base + start, base + cursor.pos, kind);
    }

    opens_block
}

/// A byte cursor that only ever stops on character boundaries.
struct Cursor<'a> {
    line: &'a str,
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(line: &'a str) -> Self {
        Self { line, pos: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn rest(&self) -> &'a str {
        &self.line[self.pos..]
    }

    fn bump(&mut self) {
        if let Some(character) = self.peek() {
            self.pos += character.len_utf8();
        }
    }

    fn consume_while(&mut self, mut keep: impl FnMut(char) -> bool) {
        while let Some(character) = self.peek() {
            if !keep(character) {
                break;
            }
            self.bump();
        }
    }
}

/// Consume a quoted scalar, including its closing quote when there is one.
fn consume_quoted(cursor: &mut Cursor, quote: char) {
    cursor.bump();
    while let Some(character) = cursor.peek() {
        cursor.bump();
        if character == '\\' && quote == '"' {
            cursor.bump();
        } else if character == quote {
            return;
        }
    }
}

/// Whether the `-` at `pos` is a sequence marker rather than the sign of a
/// number.
fn is_list_marker(line: &str, pos: usize) -> bool {
    match line[pos + 1..].chars().next() {
        None => true,
        Some(character) => character.is_whitespace(),
    }
}

/// Whether the word ending at `pos` is a key, i.e. a `:` follows it and starts a
/// block mapping entry or a flow pair.
fn is_key(line: &str, pos: usize) -> bool {
    let Some(rest) = line[pos..].strip_prefix(':') else {
        return false;
    };
    match rest.chars().next() {
        None => true,
        Some(character) => character.is_whitespace() || matches!(character, ',' | '}' | ']'),
    }
}

fn is_delimiter(character: char) -> bool {
    character.is_whitespace() || matches!(character, ',' | ':' | '[' | ']' | '{' | '}' | '#')
}

fn classify(word: &str) -> TokenKind {
    if matches!(
        word.to_ascii_lowercase().as_str(),
        "true" | "false" | "null" | "~" | "yes" | "no" | "on" | "off"
    ) {
        return TokenKind::Constant;
    }
    if is_number(word) {
        TokenKind::Number
    } else {
        TokenKind::Value
    }
}

fn is_number(word: &str) -> bool {
    !word.is_empty()
        && word.chars().any(|character| character.is_ascii_digit())
        && word.chars().all(|character| {
            character.is_ascii_digit() || matches!(character, '-' | '+' | '.' | 'e' | 'E' | '_')
        })
}

fn push(tokens: &mut Vec<(Range<usize>, TokenKind)>, start: usize, end: usize, kind: TokenKind) {
    if start < end {
        tokens.push((start..end, kind));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text of every token of `kind`, in source order.
    fn texts(source: &str, kind: TokenKind) -> Vec<&str> {
        highlight(source)
            .into_iter()
            .filter(|(_, token_kind)| *token_kind == kind)
            .map(|(range, _)| &source[range])
            .collect()
    }

    #[test]
    fn keys_and_values_are_separated() {
        let source = "name: build\nruns-on: ubuntu-latest\n";

        assert_eq!(texts(source, TokenKind::Key), ["name", "runs-on"]);
        assert_eq!(texts(source, TokenKind::Value), ["build", "ubuntu-latest"]);
    }

    #[test]
    fn a_colon_without_a_space_is_not_a_key() {
        let source = "url: https://example.com\n";

        assert_eq!(texts(source, TokenKind::Key), ["url"]);
    }

    #[test]
    fn comments_run_to_the_end_of_the_line() {
        let source = "# a comment\nname: build # trailing\n";

        assert_eq!(
            texts(source, TokenKind::Comment),
            ["# a comment", "# trailing"]
        );
    }

    #[test]
    fn a_quoted_value_may_contain_a_colon_and_a_hash() {
        let source = "run: \"echo hi: there # not a comment\"\n";

        assert_eq!(
            texts(source, TokenKind::Value),
            ["\"echo hi: there # not a comment\""]
        );
        assert!(texts(source, TokenKind::Comment).is_empty());
    }

    #[test]
    fn expressions_are_a_single_token() {
        let source = "if: ${{ github.event_name == 'push' }}\n";

        assert_eq!(
            texts(source, TokenKind::Expression),
            ["${{ github.event_name == 'push' }}"]
        );
        assert_eq!(texts(source, TokenKind::Key), ["if"]);
    }

    #[test]
    fn numbers_and_constants_are_recognised() {
        let source = "retries: 3\ncontinue: true\ntimeout: 12.5\n";

        assert_eq!(texts(source, TokenKind::Number), ["3", "12.5"]);
        assert_eq!(texts(source, TokenKind::Constant), ["true"]);
    }

    #[test]
    fn a_plain_number_negatives_stay_a_number() {
        let source = "offset: -1\n";

        assert_eq!(texts(source, TokenKind::Number), ["-1"]);
    }

    #[test]
    fn list_markers_are_punctuation() {
        let source = "steps:\n  - uses: actions/checkout@v4\n";

        assert_eq!(texts(source, TokenKind::Punctuation), [":", "-", ":"]);
        assert_eq!(texts(source, TokenKind::Key), ["steps", "uses"]);
    }

    #[test]
    fn anchors_and_aliases_are_their_own_kind() {
        let source = "value: &anchor\ncopy: *anchor\n";

        assert_eq!(texts(source, TokenKind::Anchor), ["&anchor", "*anchor"]);
    }

    #[test]
    fn a_block_scalar_body_is_left_plain() {
        let source = "run: |\n  echo \"a: b\"\n  npm run build\nname: after\n";

        // The script body keeps no tokens at all, so it stays in the default
        // colour instead of being read as mappings.
        assert_eq!(texts(source, TokenKind::Key), ["run", "name"]);
        assert_eq!(texts(source, TokenKind::Value), ["after"]);
    }

    #[test]
    fn a_block_scalar_ends_at_the_next_line_with_the_same_indent() {
        let source = "jobs:\n  build:\n    run: >-\n      echo hi\n  other:\n    steps: []\n";

        assert_eq!(
            texts(source, TokenKind::Key),
            ["jobs", "build", "run", "other", "steps"]
        );
    }

    #[test]
    fn tokens_are_ordered_and_within_the_source() {
        let source = "on:\n  push:\n    branches: [main]\n";
        let mut previous_end = 0;

        for (range, _) in highlight(source) {
            assert!(
                range.start >= previous_end,
                "{range:?} overlaps its neighbour"
            );
            assert!(source.is_char_boundary(range.start));
            assert!(source.is_char_boundary(range.end));
            previous_end = range.end;
        }
    }

    #[test]
    fn non_ascii_text_does_not_break_the_ranges() {
        let source = "name: 构建\nnote: \"中文：值\"\n";

        assert_eq!(texts(source, TokenKind::Key), ["name", "note"]);
        assert_eq!(texts(source, TokenKind::Value), ["构建", "\"中文：值\""]);
    }
}
