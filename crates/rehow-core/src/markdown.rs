//! Bounded Markdown parsing into renderer-safe, versioned events.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag};
use serde::Serialize;
use std::fmt;
use ts_rs::TS;

use crate::domain::content::guide::valid_reference_url;

pub const SAFE_MARKDOWN_VERSION: u32 = 1;
pub const MAX_MARKDOWN_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_MARKDOWN_NODES: usize = 100_000;
pub const MAX_MARKDOWN_NESTING: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, TS)]
#[ts(rename_all = "camelCase")]
pub struct SafeMarkdownDocument {
    pub version: u32,
    pub events: Vec<SafeMarkdownEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(tag = "kind", rename_all = "camelCase")]
pub enum SafeMarkdownEvent {
    Start { node: SafeMarkdownContainer },
    End,
    Text { value: String },
    InlineCode { value: String },
    HtmlLiteral { value: String },
    SoftBreak,
    HardBreak,
    Rule,
    TaskMarker { checked: bool },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(tag = "kind", rename_all = "camelCase")]
pub enum SafeMarkdownContainer {
    Paragraph,
    Heading {
        level: u8,
    },
    BlockQuote,
    CodeBlock {
        language: Option<String>,
    },
    List {
        start: Option<u32>,
    },
    Item,
    Emphasis,
    Strong,
    Strikethrough,
    Link {
        destination: Option<String>,
        title: Option<String>,
    },
    Image {
        title: Option<String>,
    },
    Table {
        alignments: Vec<SafeMarkdownAlignment>,
    },
    TableHead,
    TableRow,
    TableCell,
    HtmlBlock,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum SafeMarkdownAlignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafeMarkdownError {
    InputLimit,
    NodeLimit,
    NestingLimit,
}

impl fmt::Display for SafeMarkdownError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InputLimit => "Guide Markdown exceeds the safe input limit",
            Self::NodeLimit => "Guide Markdown exceeds the safe node limit",
            Self::NestingLimit => "Guide Markdown is nested too deeply",
        })
    }
}

impl std::error::Error for SafeMarkdownError {}

pub fn parse_safe_markdown(source: &str) -> Result<SafeMarkdownDocument, SafeMarkdownError> {
    if source.len() > MAX_MARKDOWN_BYTES {
        return Err(SafeMarkdownError::InputLimit);
    }
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(source, options);
    let mut events = Vec::new();
    let mut nodes = 0usize;
    let mut depth = 0usize;

    for event in parser {
        let safe_event = match event {
            Event::Start(tag) => {
                count_node(&mut nodes)?;
                depth = depth
                    .checked_add(1)
                    .filter(|depth| *depth <= MAX_MARKDOWN_NESTING)
                    .ok_or(SafeMarkdownError::NestingLimit)?;
                SafeMarkdownEvent::Start {
                    node: safe_container(tag),
                }
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                SafeMarkdownEvent::End
            }
            Event::Text(value) => leaf(
                &mut nodes,
                SafeMarkdownEvent::Text {
                    value: value.into_string(),
                },
            )?,
            Event::Code(value) => leaf(
                &mut nodes,
                SafeMarkdownEvent::InlineCode {
                    value: value.into_string(),
                },
            )?,
            Event::Html(value) | Event::InlineHtml(value) => leaf(
                &mut nodes,
                SafeMarkdownEvent::HtmlLiteral {
                    value: value.into_string(),
                },
            )?,
            Event::SoftBreak => leaf(&mut nodes, SafeMarkdownEvent::SoftBreak)?,
            Event::HardBreak => leaf(&mut nodes, SafeMarkdownEvent::HardBreak)?,
            Event::Rule => leaf(&mut nodes, SafeMarkdownEvent::Rule)?,
            Event::TaskListMarker(checked) => {
                leaf(&mut nodes, SafeMarkdownEvent::TaskMarker { checked })?
            }
            Event::InlineMath(value)
            | Event::DisplayMath(value)
            | Event::FootnoteReference(value) => leaf(
                &mut nodes,
                SafeMarkdownEvent::Text {
                    value: value.into_string(),
                },
            )?,
        };
        events.push(safe_event);
    }

    Ok(SafeMarkdownDocument {
        version: SAFE_MARKDOWN_VERSION,
        events,
    })
}

fn count_node(nodes: &mut usize) -> Result<(), SafeMarkdownError> {
    *nodes = nodes.checked_add(1).ok_or(SafeMarkdownError::NodeLimit)?;
    if *nodes > MAX_MARKDOWN_NODES {
        return Err(SafeMarkdownError::NodeLimit);
    }
    Ok(())
}

fn leaf(
    nodes: &mut usize,
    event: SafeMarkdownEvent,
) -> Result<SafeMarkdownEvent, SafeMarkdownError> {
    count_node(nodes)?;
    Ok(event)
}

fn safe_container(tag: Tag<'_>) -> SafeMarkdownContainer {
    match tag {
        Tag::Paragraph => SafeMarkdownContainer::Paragraph,
        Tag::Heading { level, .. } => SafeMarkdownContainer::Heading {
            level: match level {
                HeadingLevel::H1 => 1,
                HeadingLevel::H2 => 2,
                HeadingLevel::H3 => 3,
                HeadingLevel::H4 => 4,
                HeadingLevel::H5 => 5,
                HeadingLevel::H6 => 6,
            },
        },
        Tag::BlockQuote(_) => SafeMarkdownContainer::BlockQuote,
        Tag::CodeBlock(kind) => SafeMarkdownContainer::CodeBlock {
            language: match kind {
                CodeBlockKind::Indented => None,
                CodeBlockKind::Fenced(value) => value
                    .split_ascii_whitespace()
                    .next()
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
            },
        },
        Tag::List(start) => SafeMarkdownContainer::List {
            start: start.and_then(|value| u32::try_from(value).ok()),
        },
        Tag::Item => SafeMarkdownContainer::Item,
        Tag::Emphasis => SafeMarkdownContainer::Emphasis,
        Tag::Strong => SafeMarkdownContainer::Strong,
        Tag::Strikethrough => SafeMarkdownContainer::Strikethrough,
        Tag::Link {
            dest_url, title, ..
        } => {
            let destination = dest_url.into_string();
            SafeMarkdownContainer::Link {
                destination: valid_reference_url(&destination).then_some(destination),
                title: nonempty(title.into_string()),
            }
        }
        Tag::Image { title, .. } => SafeMarkdownContainer::Image {
            title: nonempty(title.into_string()),
        },
        Tag::Table(alignments) => SafeMarkdownContainer::Table {
            alignments: alignments.into_iter().map(safe_alignment).collect(),
        },
        Tag::TableHead => SafeMarkdownContainer::TableHead,
        Tag::TableRow => SafeMarkdownContainer::TableRow,
        Tag::TableCell => SafeMarkdownContainer::TableCell,
        Tag::HtmlBlock => SafeMarkdownContainer::HtmlBlock,
        Tag::FootnoteDefinition(_)
        | Tag::DefinitionList
        | Tag::DefinitionListTitle
        | Tag::DefinitionListDefinition
        | Tag::Superscript
        | Tag::Subscript
        | Tag::MetadataBlock(_) => SafeMarkdownContainer::Paragraph,
    }
}

fn nonempty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn safe_alignment(alignment: Alignment) -> SafeMarkdownAlignment {
    match alignment {
        Alignment::None => SafeMarkdownAlignment::None,
        Alignment::Left => SafeMarkdownAlignment::Left,
        Alignment::Center => SafeMarkdownAlignment::Center,
        Alignment::Right => SafeMarkdownAlignment::Right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_supported_commonmark_and_gfm_nodes() {
        let document = parse_safe_markdown(
            "## Steps\n\n- [x] **done**\n\n```sh\nprintf ok\n```\n\n| A | B |\n|:-|:-:|\n| 1 | 2 |",
        )
        .unwrap();
        assert_eq!(document.version, 1);
        assert!(document.events.iter().any(|event| matches!(
            event,
            SafeMarkdownEvent::Start {
                node: SafeMarkdownContainer::Heading { level: 2 }
            }
        )));
        assert!(
            document
                .events
                .iter()
                .any(|event| matches!(event, SafeMarkdownEvent::TaskMarker { checked: true }))
        );
        assert!(document.events.iter().any(|event| matches!(event,
            SafeMarkdownEvent::Start { node: SafeMarkdownContainer::CodeBlock { language: Some(language) } } if language == "sh")));
        assert!(document.events.iter().any(|event| matches!(
            event,
            SafeMarkdownEvent::Start {
                node: SafeMarkdownContainer::Table { .. }
            }
        )));
    }

    #[test]
    fn preserves_html_as_inert_literal_events() {
        let document =
            parse_safe_markdown("<script>alert('no')</script>\n\nhello <img src=x onerror=x>")
                .unwrap();
        let literals = document
            .events
            .iter()
            .filter_map(|event| match event {
                SafeMarkdownEvent::HtmlLiteral { value } => Some(value.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert!(literals.contains("<script>"));
        assert!(literals.contains("<img"));
    }

    #[test]
    fn only_retains_canonical_allowlisted_link_destinations() {
        let document = parse_safe_markdown(
            "[web](https://example.com/path) [mail](mailto:user@example.com) [script](javascript:alert(1)) [file](file:///etc/passwd) [relative](../secret)",
        ).unwrap();
        let destinations = document
            .events
            .iter()
            .filter_map(|event| match event {
                SafeMarkdownEvent::Start {
                    node: SafeMarkdownContainer::Link { destination, .. },
                } => Some(destination.as_deref()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            destinations,
            vec![
                Some("https://example.com/path"),
                Some("mailto:user@example.com"),
                None,
                None,
                None
            ]
        );
    }

    #[test]
    fn never_exposes_image_sources() {
        let document = parse_safe_markdown("![secret](https://tracker.example/pixel.png)").unwrap();
        assert!(document.events.iter().any(|event| matches!(
            event,
            SafeMarkdownEvent::Start {
                node: SafeMarkdownContainer::Image { .. }
            }
        )));
        assert!(
            !serde_json::to_string(&document)
                .unwrap()
                .contains("tracker.example")
        );
    }

    #[test]
    fn rejects_one_node_over_the_limit() {
        let exact = "x\n\n".repeat(MAX_MARKDOWN_NODES / 2);
        assert!(parse_safe_markdown(&exact).is_ok());
        let source = "x\n\n".repeat((MAX_MARKDOWN_NODES / 2) + 1);
        assert_eq!(
            parse_safe_markdown(&source),
            Err(SafeMarkdownError::NodeLimit)
        );
    }

    #[test]
    fn rejects_input_before_parsing_over_the_byte_limit() {
        let source = "x".repeat(MAX_MARKDOWN_BYTES + 1);
        assert_eq!(
            parse_safe_markdown(&source),
            Err(SafeMarkdownError::InputLimit)
        );
    }

    #[test]
    fn rejects_excessive_nesting() {
        let source = format!("{}x", "> ".repeat(MAX_MARKDOWN_NESTING + 1));
        assert_eq!(
            parse_safe_markdown(&source),
            Err(SafeMarkdownError::NestingLimit)
        );
    }
}
