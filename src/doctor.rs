//! Structured, plain-text diagnostic reports.

use std::fmt::Write;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Ok,
    Warning,
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub section: &'static str,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub hint: Option<String>,
}

impl Check {
    pub fn ok(section: &'static str, title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(section, Severity::Ok, title, detail, None)
    }

    pub fn warning(
        section: &'static str,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::new(section, Severity::Warning, title, detail, None)
    }

    pub fn failure(
        section: &'static str,
        title: impl Into<String>,
        detail: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self::new(section, Severity::Failure, title, detail, Some(hint.into()))
    }

    fn new(
        section: &'static str,
        severity: Severity,
        title: impl Into<String>,
        detail: impl Into<String>,
        hint: Option<String>,
    ) -> Self {
        Self {
            section,
            severity,
            title: title.into(),
            detail: detail.into(),
            hint,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub checks: Vec<Check>,
}

impl Report {
    pub fn new(checks: Vec<Check>) -> Self {
        Self { checks }
    }

    pub fn exit_code(&self) -> i32 {
        i32::from(
            self.checks
                .iter()
                .any(|check| check.severity == Severity::Failure),
        )
    }

    pub fn render(&self) -> String {
        let mut output = String::new();
        let mut previous_section = None;
        let mut passed = 0;
        let mut warnings = 0;
        let mut failed = 0;

        for check in &self.checks {
            if previous_section != Some(check.section) {
                if previous_section.is_some() {
                    output.push('\n');
                }
                writeln!(output, "{}", check.section).expect("writing to a String cannot fail");
                previous_section = Some(check.section);
            }

            let label = match check.severity {
                Severity::Ok => {
                    passed += 1;
                    "ok"
                }
                Severity::Warning => {
                    warnings += 1;
                    "warn"
                }
                Severity::Failure => {
                    failed += 1;
                    "fail"
                }
            };
            writeln!(output, "  [{label}] {}: {}", check.title, check.detail)
                .expect("writing to a String cannot fail");

            if let Some(hint) = &check.hint {
                writeln!(output, "    Hint: {hint}").expect("writing to a String cannot fail");
            }
        }

        if !self.checks.is_empty() {
            output.push('\n');
        }
        writeln!(
            output,
            "Summary: {passed} passed, {warnings} warning{}, {failed} failed",
            if warnings == 1 { "" } else { "s" }
        )
        .expect("writing to a String cannot fail");
        output
    }
}

pub fn sanitize_detail(detail: &str, home: Option<&Path>) -> String {
    let with_home_redacted = home
        .and_then(Path::to_str)
        .filter(|home| !home.is_empty())
        .map_or_else(|| detail.to_owned(), |home| detail.replace(home, "$HOME"));

    with_home_redacted
        .split(['\r', '\n'])
        .filter(|line| !line.is_empty())
        .map(sanitize_line)
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Copy)]
enum CredentialMarker {
    Sapisid,
    Authorization,
    Cookie,
}

impl CredentialMarker {
    const ALL: [(Self, &'static str); 3] = [
        (Self::Sapisid, "SAPISID="),
        (Self::Authorization, "Authorization:"),
        (Self::Cookie, "Cookie:"),
    ];
}

fn sanitize_line(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut remaining = line;

    while let Some((offset, marker, marker_text)) = next_marker(remaining) {
        output.push_str(&remaining[..offset + marker_text.len()]);
        remaining = &remaining[offset + marker_text.len()..];

        match marker {
            CredentialMarker::Sapisid => {
                let value_end = remaining
                    .find(|character: char| character == ';' || character.is_whitespace())
                    .unwrap_or(remaining.len());
                output.push_str("[redacted]");
                remaining = &remaining[value_end..];
            }
            CredentialMarker::Authorization => {
                let value_start = remaining.len() - remaining.trim_start().len();
                output.push(' ');
                output.push_str("[redacted]");
                let value = &remaining[value_start..];
                if let Some(home_start) = home_path_start(value) {
                    output.push(' ');
                    remaining = &value[home_start..];
                } else {
                    remaining = "";
                }
            }
            CredentialMarker::Cookie => {
                output.push(' ');
                output.push_str("[redacted]");
                remaining = "";
            }
        }
    }

    output.push_str(remaining);
    output
}

fn home_path_start(value: &str) -> Option<usize> {
    value.match_indices("$HOME").find_map(|(offset, _)| {
        let follows_whitespace = value[..offset]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace);
        let suffix = &value[offset + "$HOME".len()..];
        let starts_path = suffix.is_empty() || suffix.starts_with('/');
        (follows_whitespace && starts_path).then_some(offset)
    })
}

fn next_marker(input: &str) -> Option<(usize, CredentialMarker, &'static str)> {
    CredentialMarker::ALL
        .iter()
        .filter_map(|(kind, marker)| {
            find_ascii_case_insensitive(input, marker).map(|offset| (offset, *kind, *marker))
        })
        .min_by_key(|(offset, _, _)| *offset)
}

fn find_ascii_case_insensitive(input: &str, needle: &str) -> Option<usize> {
    input
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn warnings_do_not_fail_but_required_failures_do() {
        let warnings = Report::new(vec![Check::warning("Runtime", "deno", "optional")]);
        assert_eq!(warnings.exit_code(), 0);

        let failed = Report::new(vec![Check::failure(
            "Runtime",
            "ffmpeg",
            "missing",
            "install ffmpeg",
        )]);
        assert_eq!(failed.exit_code(), 1);
    }

    #[test]
    fn rendering_redacts_credentials_and_home_paths() {
        let source =
            "SAPISID=synthetic-secret; Authorization: SAPISIDHASH 1_synthetic-hash /home/alice/profile";
        let rendered = sanitize_detail(source, Some(Path::new("/home/alice")));

        assert!(!rendered.contains("synthetic-secret"));
        assert!(!rendered.contains("synthetic-hash"));
        assert!(!rendered.contains("/home/alice"));
        assert!(rendered.contains("$HOME"));
    }

    #[test]
    fn report_groups_sections_and_prints_summary() {
        let report = Report::new(vec![
            Check::ok("Runtime", "yt-dlp", "2026.07.04"),
            Check::warning("Runtime", "deno", "not found"),
        ]);

        let text = report.render();

        assert!(text.contains("[ok] yt-dlp"));
        assert_eq!(text.matches("Runtime\n").count(), 1);
        assert!(text.contains("Summary: 1 passed, 1 warning, 0 failed"));
    }

    #[test]
    fn sanitization_is_case_insensitive_and_collapses_newlines() {
        let source = concat!(
            "sApIsId=synthetic-cookie-value; safe\r\n",
            "AUTHORIZATION: SAPISIDHASH 1_synthetic-header-hash /home/alice/profile\n",
            "cOoKiE: PREF=synthetic-cookie-header; HSID=synthetic-second-cookie\r",
            "done"
        );

        let sanitized = sanitize_detail(source, Some(Path::new("/home/alice")));

        assert!(!sanitized.contains("synthetic-cookie-value"));
        assert!(!sanitized.contains("synthetic-header-hash"));
        assert!(!sanitized.contains("synthetic-cookie-header"));
        assert!(!sanitized.contains("synthetic-second-cookie"));
        assert!(!sanitized.contains("/home/alice"));
        assert!(sanitized.contains("$HOME/profile"));
        assert!(!sanitized.contains(['\r', '\n']));
        assert!(sanitized.contains("safe"));
        assert!(sanitized.ends_with("done"));
    }

    #[test]
    fn authorization_redacts_every_header_token() {
        let source =
            "Authorization: Digest synthetic-first-token synthetic-second-token\nsafe detail";

        let sanitized = sanitize_detail(source, None);

        assert!(!sanitized.contains("synthetic-first-token"));
        assert!(!sanitized.contains("synthetic-second-token"));
        assert!(sanitized.ends_with("safe detail"));
    }

    #[test]
    fn report_indents_failure_hints() {
        let report = Report::new(vec![Check::failure(
            "Runtime",
            "ffmpeg",
            "missing",
            "install ffmpeg",
        )]);

        assert!(report.render().contains("\n    Hint: install ffmpeg\n"));
    }
}
