use std::{
    collections::HashMap,
    io,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::{stream, StreamExt};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Wrap},
    Frame, Terminal,
};
use reqwest::{Client, Response};
use serde::Deserialize;
use wakezilla_common::{DeleteMachinePayload, Machine};

const BASE: ratatui::style::Color = ratatui::style::Color::Rgb(30, 30, 46);
const TEXT: ratatui::style::Color = ratatui::style::Color::Rgb(205, 214, 244);
const SUBTEXT: ratatui::style::Color = ratatui::style::Color::Rgb(166, 173, 200);
const SURFACE: ratatui::style::Color = ratatui::style::Color::Rgb(88, 91, 112);
const BLUE: ratatui::style::Color = ratatui::style::Color::Rgb(137, 180, 250);
const GREEN: ratatui::style::Color = ratatui::style::Color::Rgb(166, 227, 161);
const RED: ratatui::style::Color = ratatui::style::Color::Rgb(243, 139, 168);
const YELLOW: ratatui::style::Color = ratatui::style::Color::Rgb(249, 226, 175);
const MAUVE: ratatui::style::Color = ratatui::style::Color::Rgb(203, 166, 247);

pub struct TuiConfig {
    pub api_base_url: String,
}

#[derive(Clone)]
struct ApiClient {
    base_url: String,
    http: Client,
}

impl ApiClient {
    fn new(base_url: String) -> Result<Self> {
        let trimmed = base_url.trim_end_matches('/').to_string();
        if trimmed.is_empty() {
            return Err(anyhow!("TUI API URL cannot be empty"));
        }

        let http = Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .context("failed to create TUI HTTP client")?;

        Ok(Self {
            base_url: trimmed,
            http,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    async fn list_machines(&self) -> Result<Vec<Machine>> {
        let response = self
            .http
            .get(self.url("/api/machines"))
            .send()
            .await
            .context("failed to request /api/machines")?;

        json_response(response)
            .await
            .context("failed to decode /api/machines response")
    }

    async fn is_machine_on(&self, mac: &str) -> Result<bool> {
        #[derive(Deserialize)]
        struct StatusBody {
            is_on: bool,
        }

        let response = self
            .http
            .get(self.url(&format!("/api/machines/{mac}/is-on")))
            .send()
            .await
            .with_context(|| format!("failed to request status for {mac}"))?;

        let status: StatusBody = json_response(response)
            .await
            .with_context(|| format!("failed to decode status for {mac}"))?;
        Ok(status.is_on)
    }

    async fn wake_machine(&self, mac: &str) -> Result<String> {
        self.post_machine_action(mac, "wake").await
    }

    async fn turn_off_machine(&self, mac: &str) -> Result<String> {
        self.post_machine_action(mac, "remote-turn-off").await
    }

    async fn post_machine_action(&self, mac: &str, action: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct MessageBody {
            message: String,
        }

        let response = self
            .http
            .post(self.url(&format!("/api/machines/{mac}/{action}")))
            .send()
            .await
            .with_context(|| format!("failed to request {action} for {mac}"))?;

        let body: MessageBody = json_response(response)
            .await
            .with_context(|| format!("failed to decode {action} response for {mac}"))?;
        Ok(body.message)
    }

    async fn delete_machine(&self, mac: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct StatusBody {
            status: String,
        }

        let response = self
            .http
            .delete(self.url("/api/machines/delete"))
            .json(&DeleteMachinePayload {
                mac: mac.to_string(),
            })
            .send()
            .await
            .with_context(|| format!("failed to delete {mac}"))?;

        let body: StatusBody = json_response(response)
            .await
            .with_context(|| format!("failed to decode delete response for {mac}"))?;
        Ok(body.status)
    }

    async fn statuses_for(&self, machines: &[Machine]) -> HashMap<String, Option<bool>> {
        stream::iter(machines.iter().map(|machine| machine.mac.clone()))
            .map(|mac| {
                let client = self.clone();
                async move {
                    let status = client.is_machine_on(&mac).await.ok();
                    (mac, status)
                }
            })
            .buffer_unordered(8)
            .collect()
            .await
    }
}

async fn json_response<T>(response: Response) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("HTTP {status}: {body}"));
    }

    response.json::<T>().await.context("invalid JSON response")
}

#[derive(Clone, Copy)]
enum MessageLevel {
    Info,
    Success,
    Error,
}

struct StatusMessage {
    text: String,
    level: MessageLevel,
    created_at: Instant,
}

struct App {
    client: ApiClient,
    machines: Vec<Machine>,
    statuses: HashMap<String, Option<bool>>,
    selected: usize,
    loading: bool,
    should_quit: bool,
    confirm_delete: bool,
    message: Option<StatusMessage>,
    last_refresh: Instant,
}

impl App {
    fn new(client: ApiClient) -> Self {
        Self {
            client,
            machines: Vec::new(),
            statuses: HashMap::new(),
            selected: 0,
            loading: true,
            should_quit: false,
            confirm_delete: false,
            message: None,
            last_refresh: Instant::now() - Duration::from_secs(31),
        }
    }

    fn selected_machine(&self) -> Option<&Machine> {
        self.machines.get(self.selected)
    }

    fn next(&mut self) {
        if !self.machines.is_empty() {
            self.selected = (self.selected + 1) % self.machines.len();
        }
    }

    fn previous(&mut self) {
        if !self.machines.is_empty() {
            self.selected = if self.selected == 0 {
                self.machines.len() - 1
            } else {
                self.selected - 1
            };
        }
    }

    fn set_message(&mut self, text: impl Into<String>, level: MessageLevel) {
        self.message = Some(StatusMessage {
            text: text.into(),
            level,
            created_at: Instant::now(),
        });
    }

    fn current_message(&self) -> Option<&StatusMessage> {
        self.message
            .as_ref()
            .filter(|message| message.created_at.elapsed() < Duration::from_secs(5))
    }

    async fn refresh(&mut self) {
        self.loading = true;
        match self.client.list_machines().await {
            Ok(machines) => {
                self.machines = machines;
                if self.selected >= self.machines.len() {
                    self.selected = self.machines.len().saturating_sub(1);
                }
                self.statuses = self.client.statuses_for(&self.machines).await;
                self.loading = false;
                self.last_refresh = Instant::now();
                self.set_message("Loaded machines", MessageLevel::Success);
            }
            Err(error) => {
                self.loading = false;
                self.set_message(format!("API error: {error}"), MessageLevel::Error);
            }
        }
    }

    async fn refresh_if_due(&mut self) {
        if self.last_refresh.elapsed() >= Duration::from_secs(30) {
            self.refresh().await;
        }
    }

    async fn wake_selected(&mut self) {
        let Some(mac) = self.selected_machine().map(|machine| machine.mac.clone()) else {
            self.set_message("No machine selected", MessageLevel::Info);
            return;
        };

        match self.client.wake_machine(&mac).await {
            Ok(message) => self.set_message(message, MessageLevel::Success),
            Err(error) => self.set_message(format!("Wake failed: {error}"), MessageLevel::Error),
        }
    }

    async fn turn_off_selected(&mut self) {
        let Some(mac) = self.selected_machine().map(|machine| machine.mac.clone()) else {
            self.set_message("No machine selected", MessageLevel::Info);
            return;
        };

        match self.client.turn_off_machine(&mac).await {
            Ok(message) => self.set_message(message, MessageLevel::Success),
            Err(error) => {
                self.set_message(format!("Turn off failed: {error}"), MessageLevel::Error)
            }
        }
    }

    async fn delete_selected(&mut self) {
        let Some(mac) = self.selected_machine().map(|machine| machine.mac.clone()) else {
            self.set_message("No machine selected", MessageLevel::Info);
            return;
        };

        match self.client.delete_machine(&mac).await {
            Ok(message) => {
                self.set_message(message, MessageLevel::Success);
                self.refresh().await;
            }
            Err(error) => self.set_message(format!("Delete failed: {error}"), MessageLevel::Error),
        }
    }
}

struct TerminalRestore;

impl Drop for TerminalRestore {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

pub async fn run(config: TuiConfig) -> Result<()> {
    let client = ApiClient::new(config.api_base_url)?;
    let mut app = App::new(client);

    enable_raw_mode().context("failed to enable terminal raw mode")?;
    execute!(io::stdout(), EnterAlternateScreen).context("failed to enter alternate screen")?;
    let _restore = TerminalRestore;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).context("failed to create terminal backend")?;
    terminal.clear().context("failed to clear terminal")?;

    app.refresh().await;

    while !app.should_quit {
        terminal
            .draw(|frame| render(frame, &app))
            .context("failed to draw TUI frame")?;

        if event::poll(Duration::from_millis(100)).context("failed to poll terminal events")? {
            if let Event::Key(key) = event::read().context("failed to read terminal event")? {
                handle_key(&mut app, key).await;
            }
        }

        app.refresh_if_due().await;
    }

    terminal.show_cursor().context("failed to show cursor")?;
    Ok(())
}

async fn handle_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.should_quit = true;
        return;
    }

    if app.confirm_delete {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                app.confirm_delete = false;
                app.delete_selected().await;
            }
            _ => {
                app.confirm_delete = false;
                app.set_message("Delete cancelled", MessageLevel::Info);
            }
        }
        return;
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('j') | KeyCode::Down => app.next(),
        KeyCode::Char('k') | KeyCode::Up => app.previous(),
        KeyCode::Char('r') => app.refresh().await,
        KeyCode::Char('w') => app.wake_selected().await,
        KeyCode::Char('t') => app.turn_off_selected().await,
        KeyCode::Char('d') if app.selected_machine().is_some() => {
            app.confirm_delete = true;
        }
        _ => {}
    }
}

fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_header(frame, chunks[0], app);
    render_body(frame, chunks[1], app);
    render_footer(frame, chunks[2], app);

    if app.confirm_delete {
        render_delete_overlay(frame, frame.area());
    }
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let status = if app.loading { "loading" } else { "ready" };
    let title = Line::from(vec![
        Span::styled(
            " Wakezilla TUI ",
            Style::default().fg(MAUVE).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {status} "), Style::default().fg(SUBTEXT)),
    ]);

    let header = Paragraph::new(title).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(SURFACE)),
    );
    frame.render_widget(header, area);
}

fn render_body(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    render_machine_table(frame, chunks[0], app);
    render_machine_detail(frame, chunks[1], app);
}

fn render_machine_table(frame: &mut Frame, area: Rect, app: &App) {
    let rows = app.machines.iter().map(|machine| {
        let status = match app.statuses.get(&machine.mac).copied().flatten() {
            Some(true) => Span::styled(
                "ON ",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            ),
            Some(false) => Span::styled("OFF", Style::default().fg(RED)),
            None => Span::styled(" ? ", Style::default().fg(YELLOW)),
        };

        Row::new(vec![
            Cell::from(status),
            Cell::from(machine.name.clone()),
            Cell::from(machine.ip.clone()),
            Cell::from(machine.mac.clone()),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Percentage(30),
            Constraint::Percentage(25),
            Constraint::Percentage(40),
        ],
    )
    .header(
        Row::new(vec!["State", "Name", "IP", "MAC"])
            .style(Style::default().fg(BLUE).add_modifier(Modifier::BOLD)),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(SURFACE))
            .title(" Machines ")
            .title_style(Style::default().fg(BLUE)),
    )
    .row_highlight_style(
        Style::default()
            .bg(BASE)
            .fg(TEXT)
            .add_modifier(Modifier::BOLD),
    );

    let mut table_state = TableState::default();
    if !app.machines.is_empty() {
        table_state.select(Some(app.selected));
    }

    frame.render_stateful_widget(table, area, &mut table_state);
}

fn render_machine_detail(frame: &mut Frame, area: Rect, app: &App) {
    let Some(machine) = app.selected_machine() else {
        let empty =
            Paragraph::new("No machines loaded. Start `wakezilla proxy-server`, then press r.")
                .style(Style::default().fg(SUBTEXT))
                .wrap(Wrap { trim: false })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(SURFACE))
                        .title(" Detail "),
                );
        frame.render_widget(empty, area);
        return;
    };

    let forwards = if machine.port_forwards.is_empty() {
        vec![Line::from(Span::styled(
            "No port forwards",
            Style::default().fg(SUBTEXT),
        ))]
    } else {
        machine
            .port_forwards
            .iter()
            .map(|forward| {
                let name = forward.name.as_deref().unwrap_or("unnamed");
                Line::from(vec![
                    Span::styled(name.to_string(), Style::default().fg(TEXT)),
                    Span::raw("  "),
                    Span::styled(
                        format!("{} → {}", forward.local_port, forward.target_port),
                        Style::default().fg(SUBTEXT),
                    ),
                ])
            })
            .collect()
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Name: ", label_style()),
            Span::raw(&machine.name),
        ]),
        Line::from(vec![
            Span::styled("MAC:  ", label_style()),
            Span::raw(&machine.mac),
        ]),
        Line::from(vec![
            Span::styled("IP:   ", label_style()),
            Span::raw(&machine.ip),
        ]),
        Line::from(vec![
            Span::styled("Can turn off: ", label_style()),
            Span::raw(if machine.can_be_turned_off {
                "yes"
            } else {
                "no"
            }),
        ]),
        Line::from(vec![
            Span::styled("Turn off port: ", label_style()),
            Span::raw(
                machine
                    .turn_off_port
                    .map(|port| port.to_string())
                    .unwrap_or_else(|| "not configured".to_string()),
            ),
        ]),
        Line::from(vec![
            Span::styled("Inactivity: ", label_style()),
            Span::raw(format!("{} min", machine.inactivity_period)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Port forwards",
            Style::default().fg(MAUVE).add_modifier(Modifier::BOLD),
        )),
    ];
    lines.extend(forwards);

    let detail = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(SURFACE))
            .title(" Detail ")
            .title_style(Style::default().fg(MAUVE)),
    );

    frame.render_widget(detail, area);
}

fn label_style() -> Style {
    Style::default().fg(BLUE).add_modifier(Modifier::BOLD)
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let line = if let Some(message) = app.current_message() {
        let color = match message.level {
            MessageLevel::Info => BLUE,
            MessageLevel::Success => GREEN,
            MessageLevel::Error => RED,
        };
        Line::from(Span::styled(
            message.text.clone(),
            Style::default().fg(color),
        ))
    } else {
        Line::from(Span::styled(
            "j/k move │ r refresh │ w wake │ t turn off │ d delete │ q quit",
            Style::default().fg(SUBTEXT),
        ))
    };

    frame.render_widget(Paragraph::new(line), area);
}

fn render_delete_overlay(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(48, 7, area);
    frame.render_widget(ratatui::widgets::Clear, popup);

    let content = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "Delete selected machine?",
            Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                " y ",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::raw("delete  "),
            Span::styled(
                " any other key ",
                Style::default().fg(RED).add_modifier(Modifier::BOLD),
            ),
            Span::raw("cancel"),
        ]),
    ])
    .alignment(ratatui::layout::Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(YELLOW))
            .title(" Confirm "),
    );

    frame.render_widget(content, popup);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
