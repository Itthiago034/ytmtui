//! Pure account-confirmation modal rendering.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::provider::SignInPreview;
use crate::theme::ThemeColors;

/// Draws the prepared-account picker over the completed normal interface.
pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let Some((preview, selected)) = app.sign_in_preview() else {
        return;
    };
    let width = area.width.saturating_sub(4).min(64);
    let account_rows = u16::try_from(preview.accounts.len()).unwrap_or(u16::MAX);
    let height = account_rows
        .saturating_add(8)
        .min(area.height.saturating_sub(2));
    if width < 20 || height < 6 {
        return;
    }
    let popup = centered_rect(width, height, area);
    f.render_widget(Clear, popup);
    render_account_list(f, popup, preview, selected, app.theme());
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(area.width.saturating_sub(width) / 2),
        y: area
            .y
            .saturating_add(area.height.saturating_sub(height) / 2),
        width,
        height,
    }
}

fn render_account_list(
    f: &mut Frame,
    popup: Rect,
    preview: &SignInPreview,
    selected: usize,
    theme: ThemeColors,
) {
    let block = Block::default()
        .title(" Connect an account ")
        .title_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    if inner.width == 0 || inner.height < 4 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let browser = title_case(&preview.method);
    let mut header = vec![Line::from(vec![
        Span::styled(
            browser,
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" browser session", Style::default().fg(theme.subtext)),
    ])];
    header.push(Line::from(Span::styled(
        preview
            .profile_label
            .as_deref()
            .map(|profile| format!("Profile: {profile}"))
            .unwrap_or_else(|| "Default browser profile".to_string()),
        Style::default().fg(theme.muted),
    )));
    f.render_widget(Paragraph::new(header), rows[0]);

    let items: Vec<ListItem> = preview
        .accounts
        .iter()
        .map(|account| {
            let is_current = preview.current_account_name.as_deref() == Some(account.name.as_str());
            let mut spans = vec![Span::styled(
                account.name.clone(),
                Style::default().fg(theme.text),
            )];
            if let Some(handle) = &account.handle {
                spans.push(Span::styled(
                    format!("  {handle}"),
                    Style::default().fg(theme.subtext),
                ));
            }
            if is_current {
                spans.push(Span::styled(
                    "  current",
                    Style::default()
                        .fg(theme.secondary)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let list = List::new(items).highlight_symbol("› ").highlight_style(
        Style::default()
            .fg(theme.accent_fg)
            .bg(theme.accent)
            .add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default().with_selected(Some(selected));
    f.render_stateful_widget(list, rows[1], &mut state);

    let footer = Line::from(vec![
        Span::styled("↑/↓", Style::default().fg(theme.secondary)),
        Span::styled(" select  ", Style::default().fg(theme.muted)),
        Span::styled("Enter", Style::default().fg(theme.secondary)),
        Span::styled(" confirm  ", Style::default().fg(theme.muted)),
        Span::styled("Esc", Style::default().fg(theme.secondary)),
        Span::styled(" cancel", Style::default().fg(theme.muted)),
    ]);
    f.render_widget(Paragraph::new(footer), rows[2]);
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_uppercase().chain(chars).collect()
}
