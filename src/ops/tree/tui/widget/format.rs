use std::iter::Peekable;

use anyhow::{Result, bail};
use ratatui::{style::Style, text::Span};

use crate::core::{Dependency, PackageFormatRequirements};

#[derive(Debug)]
enum Chunk {
    Text(String),
    Package,
    License,
    Repository,
    Features,
    Library,
}

/// Parsed Cargo-style package display format.
#[derive(Debug)]
pub struct TreeFormat {
    chunks: Vec<Chunk>,
    requirements: PackageFormatRequirements,
}

impl TreeFormat {
    pub fn parse(format: &str) -> Result<Self> {
        let mut chars = format.chars().peekable();
        let mut chunks = Vec::new();
        let mut text = String::new();

        while let Some(ch) = chars.next() {
            match ch {
                '{' if chars.peek() == Some(&'{') => {
                    chars.next();
                    text.push('{');
                }
                '{' => {
                    push_text(&mut chunks, &mut text);
                    let name = parse_name(&mut chars);
                    if chars.next() != Some('}') {
                        bail!("tree format `{format}` not valid: expected '}}'");
                    }
                    let chunk = match name.as_str() {
                        "p" | "package" => Chunk::Package,
                        "l" | "license" => Chunk::License,
                        "r" | "repository" => Chunk::Repository,
                        "f" | "features" => Chunk::Features,
                        "lib" => Chunk::Library,
                        _ => {
                            bail!("tree format `{format}` not valid: unsupported pattern `{name}`")
                        }
                    };
                    chunks.push(chunk);
                }
                '}' if chars.peek() == Some(&'}') => {
                    chars.next();
                    text.push('}');
                }
                '}' => bail!("tree format `{format}` not valid: unexpected '}}'"),
                _ => text.push(ch),
            }
        }
        push_text(&mut chunks, &mut text);

        let requirements = PackageFormatRequirements {
            license: chunks.iter().any(|chunk| matches!(chunk, Chunk::License)),
            repository: chunks
                .iter()
                .any(|chunk| matches!(chunk, Chunk::Repository)),
            features: chunks.iter().any(|chunk| matches!(chunk, Chunk::Features)),
            library: chunks.iter().any(|chunk| matches!(chunk, Chunk::Library)),
            source: chunks.iter().any(|chunk| matches!(chunk, Chunk::Package)),
        };

        Ok(Self {
            chunks,
            requirements,
        })
    }

    pub fn requirements(&self) -> PackageFormatRequirements {
        self.requirements
    }

    pub fn append_spans<'a>(
        &'a self,
        dependency: &'a Dependency,
        spans: &mut Vec<Span<'a>>,
        name_style: Style,
        version_style: Style,
        suffix_style: Style,
    ) {
        for chunk in &self.chunks {
            match chunk {
                Chunk::Text(text) => {
                    spans.push(Span::styled(text.as_str(), name_style));
                }
                Chunk::Package => {
                    spans.push(Span::styled(dependency.name.as_str(), name_style));
                    spans.push(Span::styled(" v", version_style));
                    spans.push(Span::styled(dependency.version.as_str(), version_style));
                    if dependency.is_proc_macro {
                        spans.push(Span::styled(" (proc-macro)", suffix_style));
                    }
                    if let Some(source) = dependency
                        .format_metadata
                        .as_ref()
                        .and_then(|metadata| metadata.source.as_deref())
                    {
                        spans.push(Span::styled(" (", suffix_style));
                        spans.push(Span::styled(source, suffix_style));
                        spans.push(Span::styled(")", suffix_style));
                    }
                }
                Chunk::License => push_value(
                    spans,
                    dependency
                        .format_metadata
                        .as_ref()
                        .and_then(|metadata| metadata.license.as_deref()),
                    name_style,
                ),
                Chunk::Repository => push_value(
                    spans,
                    dependency
                        .format_metadata
                        .as_ref()
                        .and_then(|metadata| metadata.repository.as_deref()),
                    name_style,
                ),
                Chunk::Features => push_value(
                    spans,
                    dependency
                        .format_metadata
                        .as_ref()
                        .and_then(|metadata| metadata.features.as_deref()),
                    name_style,
                ),
                Chunk::Library => push_value(
                    spans,
                    dependency
                        .format_metadata
                        .as_ref()
                        .and_then(|metadata| metadata.library.as_deref()),
                    name_style,
                ),
            }
        }
    }
}

fn parse_name(chars: &mut Peekable<impl Iterator<Item = char>>) -> String {
    let mut name = String::new();
    if chars.peek().is_some_and(|ch| ch.is_alphabetic()) {
        name.push(chars.next().expect("peeked character exists"));
        while chars.peek().is_some_and(|ch| ch.is_alphanumeric()) {
            name.push(chars.next().expect("peeked character exists"));
        }
    }
    name
}

fn push_text(chunks: &mut Vec<Chunk>, text: &mut String) {
    if !text.is_empty() {
        chunks.push(Chunk::Text(std::mem::take(text)));
    }
}

fn push_value<'a>(spans: &mut Vec<Span<'a>>, value: Option<&'a str>, style: Style) {
    if let Some(value) = value {
        spans.push(Span::styled(value, style));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ratatui::style::Style;

    use crate::core::{Dependency, PackageFormatMetadata};

    use super::TreeFormat;

    fn dependency(format_metadata: Option<PackageFormatMetadata>) -> Dependency {
        Dependency {
            name: "demo".to_owned(),
            version: "1.2.3".to_owned(),
            package_id: None,
            manifest_dir: None,
            is_proc_macro: false,
            format_metadata: format_metadata.map(Arc::new),
            children: Vec::new(),
        }
    }

    fn render(format: &TreeFormat, dependency: &Dependency) -> String {
        let mut spans = Vec::new();
        format.append_spans(
            dependency,
            &mut spans,
            Style::default(),
            Style::default(),
            Style::default(),
        );
        spans.iter().fold(String::new(), |mut output, span| {
            output.push_str(span.content.as_ref());
            output
        })
    }

    #[test]
    fn aliases_request_their_cargo_metadata() {
        let format =
            TreeFormat::parse("{p} {package} {l} {license} {r} {repository} {f} {features} {lib}")
                .unwrap();

        let requirements = format.requirements();
        assert!(requirements.source);
        assert!(requirements.license);
        assert!(requirements.repository);
        assert!(requirements.features);
        assert!(requirements.library);

        let license_only = TreeFormat::parse("{l}").unwrap().requirements();
        assert!(license_only.license);
        assert!(!license_only.source);
        assert!(!license_only.repository);
        assert!(!license_only.features);
        assert!(!license_only.library);
    }

    #[test]
    fn formats_package_and_metadata_values() {
        let format = TreeFormat::parse("{p}|{l}|{r}|{f}|{lib}").unwrap();
        let mut dependency = dependency(Some(PackageFormatMetadata {
            license: Some("MIT".to_owned()),
            repository: Some("https://example.com/demo".to_owned()),
            features: Some("default,serde".to_owned()),
            library: Some("demo_lib".to_owned()),
            source: Some("git+https://example.com/demo".to_owned()),
        }));
        dependency.is_proc_macro = true;

        assert_eq!(
            render(&format, &dependency),
            "demo v1.2.3 (proc-macro) (git+https://example.com/demo)|MIT|https://example.com/demo|default,serde|demo_lib"
        );
    }

    #[test]
    fn escaped_braces_are_literal_text() {
        let format = TreeFormat::parse("{{literal}} {p} }}").unwrap();
        let dependency = dependency(None);

        assert_eq!(render(&format, &dependency), "{literal} demo v1.2.3 }");
    }

    #[test]
    fn absent_metadata_expands_to_empty_text() {
        let format = TreeFormat::parse("{l}|{r}|{f}|{lib}|{p}").unwrap();
        let dependency = dependency(None);

        assert_eq!(render(&format, &dependency), "||||demo v1.2.3");
    }

    #[test]
    fn rejects_unsupported_or_malformed_patterns() {
        for input in ["{unknown}", "{p", "}"] {
            assert!(TreeFormat::parse(input).is_err(), "accepted {input:?}");
        }
    }
}
