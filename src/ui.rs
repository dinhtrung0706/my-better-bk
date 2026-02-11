use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, LineGauge, Paragraph, Widget, Wrap},
};

use crate::app::{App, Screen};
use crate::event::{CheckOutcome, SplashStep};

impl Widget for &App {
    /// Renders the user interface widgets.
    ///
    // This is where you add new widgets.
    // See the following resources:
    // - https://docs.rs/ratatui/latest/ratatui/widgets/index.html
    // - https://github.com/ratatui/ratatui/tree/master/examples
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self.screen {
            Screen::Splash => render_splash(self, area, buf),
            Screen::Auth => render_auth(self, area, buf),
            Screen::Main => render_main(self, area, buf),
        }
    }
}

fn render_splash(app: &App, area: Rect, buf: &mut Buffer) {
    let block = Block::bordered()
        .title("MyBetterBK - Starting")
        .title_alignment(Alignment::Center)
        .border_type(BorderType::Rounded)
        .fg(Color::LightCyan);

    let inner = block.inner(area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(inner);

    block.render(area, buf);

    let title = Paragraph::new("Initializing checks...")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    title.render(layout[0], buf);

    let status_lines = vec![
        status_line(app, SplashStep::Auth, "Auth"),
        status_line(app, SplashStep::Strategies, "Strategies"),
        status_line(app, SplashStep::Version, "Version"),
    ];
    let status = Paragraph::new(status_lines)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });
    status.render(layout[1], buf);

    let (progress, total) = splash_progress(app);
    let gauge = LineGauge::default()
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title("Loading"),
        )
        .line_set(ratatui::symbols::line::THICK)
        .filled_style(Style::default().fg(Color::Green))
        .label(format!("{progress}/{total}"))
        .ratio(progress as f64 / total as f64);
    gauge.render(layout[3], buf);

    let hint = Paragraph::new("Please wait while checks complete. Press q to quit.")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    hint.render(layout[4], buf);
}

fn render_auth(app: &App, area: Rect, buf: &mut Buffer) {
    let block = Block::bordered()
        .title("Authentication Required")
        .title_alignment(Alignment::Center)
        .border_type(BorderType::Rounded)
        .fg(Color::LightCyan);
    let inner = block.inner(area);
    block.render(area, buf);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(2),
            Constraint::Length(1)
        ])
        .split(inner);

    Paragraph::new("Please authenticate to continue.")
        .alignment(Alignment::Center)
        .render(layout[0], buf);

    let active_field_style = Style::default()
        .fg(Color::White)
        // .bg(Color::LightCyan)
        .add_modifier(Modifier::BOLD);
    let inactive_field_style = Style::default().fg(Color::Yellow);

    let username_style = if matches!(app.auth_field, crate::app::AuthField::Username) {
        active_field_style
    } else {
        inactive_field_style
    };
    let password_style = if matches!(app.auth_field, crate::app::AuthField::Password) {
        active_field_style
    } else {
        inactive_field_style
    };

    let username_prefix = if matches!(app.auth_field, crate::app::AuthField::Username) {
        "> "
    } else {
        "  "
    };
    let password_prefix = if matches!(app.auth_field, crate::app::AuthField::Password) {
        "> "
    } else {
        "  "
    };

    let username = Paragraph::new(format!(
        "{username_prefix}Username: {}",
        app.auth_username
    ))
    .style(username_style);
    username.render(layout[1], buf);

    let password = Paragraph::new(format!(
        "{password_prefix}Password: {}",
        "*".repeat(app.auth_password.len())
    ))
    .style(password_style);
    password.render(layout[2], buf);

    let message = app
        .auth_message
        .clone()
        .unwrap_or_else(|| "Press Enter to login".to_string());
    Paragraph::new(message)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan))
        .render(layout[3], buf);

    let hint_navigate = "Use Tab to switch fields. Press `Esc` or `q` to quit.";
    Paragraph::new(hint_navigate)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray))
        .render(layout[4], buf);
}

fn render_main(_app: &App, area: Rect, buf: &mut Buffer) {
     let block = Block::bordered()
         .title("MyBetterBK")
         .title_alignment(Alignment::Center)
         .border_type(BorderType::Rounded)
         .fg(Color::LightCyan);
     let inner = block.inner(area);
     block.render(area, buf);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(3)])
        .split(inner);

    let banner = r#" __    __     __  __        ______     ______     ______   ______   ______     ______        ______     __  __    
/\ "-./  \   /\ \_\ \      /\  == \   /\  ___\   /\__  _\ /\__  _\ /\  ___\   /\  == \      /\  == \   /\ \/ /    
\ \ \-./\ \  \ \____ \     \ \  __<   \ \  __\   \/_/\ \/ \/_/\ \/ \ \  __\   \ \  __<      \ \  __<   \ \  _"-.  
 \ \_\ \ \_\  \/\_____\     \ \_____\  \ \_____\    \ \_\    \ \_\  \ \_____\  \ \_\ \_\     \ \_____\  \ \_\ \_\ 
  \/_/  \/_/   \/_____/      \/_____/   \/_____/     \/_/     \/_/   \/_____/   \/_/ /_/      \/_____/   \/_/\/_/ 
                                                                                                                  
"#;

    let banner_paragraph = Paragraph::new(banner)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan));
    banner_paragraph.render(layout[0], buf);

    let text = "This is a tui template.\nPress `Esc` or `q` to stop running.";
    let paragraph = Paragraph::new(text)
        .block(Block::default())
        .fg(Color::Cyan)
        .bg(Color::Black)
        .centered();
    paragraph.render(layout[1], buf);
}

fn status_line(app: &App, step: SplashStep, label: &str) -> Line<'static> {
    let (symbol, color, message) = match app.splash_results.get(&step) {
        Some(CheckOutcome::Success) => ("✅", Color::Green, "OK".to_string()),
        Some(CheckOutcome::Warning(msg)) => ("⚠️", Color::Yellow, msg.clone()),
        Some(CheckOutcome::Failure(msg)) => ("❌", Color::Red, msg.clone()),
        None => ("⏳", Color::DarkGray, "Pending".to_string()),
    };

    Line::from(vec![
        Span::styled(format!("{symbol} {label}: "), Style::default().fg(color)),
        Span::styled(message, Style::default().fg(Color::White)),
    ])
}

fn splash_progress(app: &App) -> (usize, usize) {
    let total = 3;
    let completed = app.splash_results.len().min(total);
    (completed, total)
}






