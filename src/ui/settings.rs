//! The Settings section: every preference, editable in place.
//!
//! One row per [`SettingRow`], with the value bracketed by `‹ ›` arrows —
//! the affordance says which keys move it without spending a line on
//! instructions. Changes land immediately, so the interface behind this
//! panel is the preview.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::app::{App, SettingRow};

/// Columns reserved for the label, so every value starts on the same column.
const LABEL_WIDTH: usize = 26;

/// Draws the Settings panel inside `block`.
pub fn draw(f: &mut Frame, app: &App, area: Rect, block: Block) {
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let theme = app.theme();
    let cursor = app.ui.settings_cursor.min(SettingRow::ALL.len() - 1);

    let mut lines: Vec<Line> = vec![Line::from("")];
    for (index, row) in SettingRow::ALL.iter().enumerate() {
        let selected = index == cursor;
        let label = format!(
            "  {:<width$}",
            row.label(),
            width = LABEL_WIDTH.min(inner.width as usize)
        );
        let value = app.setting_value(*row);

        let label_style = if selected {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.subtext)
        };
        // Only the row under the cursor shows its arrows: nine rows of
        // arrows would read as nine things demanding attention at once.
        let (open, close) = if selected {
            ("‹ ", " ›")
        } else {
            ("  ", "  ")
        };
        let arrow_style = Style::default().fg(if selected { theme.accent } else { theme.border });
        let value_style = if selected {
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        };

        lines.push(Line::from(vec![
            Span::styled(
                if selected { "▍" } else { " " },
                Style::default().fg(theme.accent),
            ),
            Span::styled(label, label_style),
            Span::styled(open, arrow_style),
            Span::styled(value, value_style),
            Span::styled(close, arrow_style),
        ]));
    }

    // The hint for the selected row, at the foot of the panel: a settings
    // screen that only lists values leaves the user guessing what changing
    // one will cost them.
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("  {}", SettingRow::ALL[cursor].hint()),
        Style::default().fg(theme.muted),
    )));

    f.render_widget(Paragraph::new(lines).alignment(Alignment::Left), inner);

    if inner.height as usize > SettingRow::ALL.len() + 5 {
        let footer = Line::from(vec![
            Span::styled("↑↓", Style::default().fg(theme.subtext)),
            Span::styled(" escolher  ", Style::default().fg(theme.muted)),
            Span::styled("←→", Style::default().fg(theme.subtext)),
            Span::styled(" mudar  ", Style::default().fg(theme.muted)),
            Span::styled("salvo automaticamente", Style::default().fg(theme.border)),
        ]);
        f.render_widget(
            Paragraph::new(footer).alignment(Alignment::Center),
            Rect {
                y: inner.y + inner.height - 1,
                height: 1,
                ..inner
            },
        );
    }
}
