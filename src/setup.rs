//! Interactive `setup` wizard: configure and install a Wakezilla system service.

use anyhow::{Context, Result};
use clap::Parser;

use crate::config::{self, Config};
use crate::service::{self, Mode};

/// CLI arguments for the `setup` subcommand.
#[derive(Parser, Debug, Default)]
#[command()]
pub struct SetupArgs {
    /// Pre-select the mode ("proxy" or "client"); skips the TUI prompt if combined with --port.
    #[arg(long, help_heading = "Setup Options")]
    pub mode: Option<String>,

    /// Pre-select the port; skips the TUI prompt if combined with --mode.
    #[arg(long, help_heading = "Setup Options")]
    pub port: Option<u16>,
}

/// Build a Config with the chosen port placed in the correct field for `mode`.
pub fn build_config(mode: Mode, port: u16) -> Config {
    let mut cfg = Config::default();
    match mode {
        Mode::Proxy => cfg.server.proxy_port = port,
        Mode::Client => cfg.server.client_port = port,
    }
    cfg
}

/// Write the config file, install the service, and validate it.
/// Returns the config path on success.
pub fn apply(mode: Mode, port: u16) -> Result<std::path::PathBuf> {
    let exe = std::env::current_exe().context("failed to resolve current executable path")?;
    let exe = exe.to_string_lossy().to_string();

    let cfg = build_config(mode, port);
    let path = config::config_path();
    cfg.save_to(&path)
        .with_context(|| format!("failed to write config to {}", path.display()))?;

    service::install(mode, &exe).context("failed to install system service")?;
    service::validate(port, 10).context("service installed but did not become reachable")?;

    Ok(path)
}

use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};

/// Wizard entry point. Requires elevation. Falls back to the TUI when the mode/port
/// are not both provided on the command line.
pub fn run(args: SetupArgs) -> Result<()> {
    if !service::is_elevated() {
        eprintln!(
            "wakezilla setup must run with administrator privileges.\n\
             Re-run with: sudo wakezilla setup   (Linux/macOS)  or  an elevated shell (Windows)."
        );
        std::process::exit(1);
    }

    // Headless path: both flags provided.
    if let (Some(mode_str), Some(port)) = (&args.mode, args.port) {
        let mode = Mode::from_str_opt(mode_str)
            .with_context(|| format!("invalid --mode '{mode_str}' (use 'proxy' or 'client')"))?;
        let path = apply(mode, port)?;
        println!(
            "Configured {} on port {port}. Config written to {}.",
            mode.subcommand(),
            path.display()
        );
        return Ok(());
    }

    let (mode, port) = run_wizard(args)?;
    let path = apply(mode, port)?;
    println!(
        "Configured {} on port {port}. Config written to {}.",
        mode.subcommand(),
        path.display()
    );
    Ok(())
}

#[derive(PartialEq)]
enum Step {
    ModeSelect,
    PortInput,
    Confirm,
}

struct Wizard {
    step: Step,
    mode: Mode,
    port_input: String,
    error: Option<String>,
}

impl Wizard {
    fn new(args: &SetupArgs) -> Self {
        let mode = args
            .mode
            .as_deref()
            .and_then(Mode::from_str_opt)
            .unwrap_or(Mode::Proxy);
        Wizard {
            step: Step::ModeSelect,
            mode,
            port_input: args
                .port
                .map(|p| p.to_string())
                .unwrap_or_else(|| mode.default_port().to_string()),
            error: None,
        }
    }
}

/// Run the interactive wizard, returning the chosen (mode, port).
fn run_wizard(args: SetupArgs) -> Result<(Mode, u16)> {
    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("failed to enter alt screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to create terminal")?;

    let result = wizard_loop(&mut terminal, args);

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    result
}

fn wizard_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    args: SetupArgs,
) -> Result<(Mode, u16)> {
    let mut w = Wizard::new(&args);

    loop {
        terminal.draw(|f| draw_wizard(f, &w))?;

        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        if let Event::Key(key) = event::read()? {
            // Ctrl-C / Esc aborts.
            if key.code == KeyCode::Esc
                || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
            {
                return Err(anyhow::anyhow!("setup cancelled"));
            }

            match w.step {
                Step::ModeSelect => match key.code {
                    KeyCode::Left | KeyCode::Right | KeyCode::Char('h') | KeyCode::Char('l') => {
                        w.mode = match w.mode {
                            Mode::Proxy => Mode::Client,
                            Mode::Client => Mode::Proxy,
                        };
                        w.port_input = w.mode.default_port().to_string();
                    }
                    KeyCode::Enter => w.step = Step::PortInput,
                    _ => {}
                },
                Step::PortInput => match key.code {
                    KeyCode::Char(c) if c.is_ascii_digit() && w.port_input.len() < 5 => {
                        w.port_input.push(c);
                    }
                    KeyCode::Backspace => {
                        w.port_input.pop();
                    }
                    KeyCode::Enter => match w.port_input.parse::<u16>() {
                        Ok(p) if p > 0 => {
                            w.error = None;
                            w.step = Step::Confirm;
                        }
                        _ => w.error = Some("Enter a valid port (1-65535)".to_string()),
                    },
                    _ => {}
                },
                Step::Confirm => match key.code {
                    KeyCode::Enter => {
                        let port: u16 = w.port_input.parse().unwrap_or(w.mode.default_port());
                        return Ok((w.mode, port));
                    }
                    KeyCode::Char('b') => w.step = Step::PortInput,
                    _ => {}
                },
            }
        }
    }
}

fn draw_wizard(f: &mut Frame, w: &Wizard) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(f.area());

    let title = Paragraph::new("Wakezilla Setup")
        .style(Style::default().add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    let body = match w.step {
        Step::ModeSelect => {
            let proxy = mode_span("Proxy server", w.mode == Mode::Proxy);
            let client = mode_span("Client server", w.mode == Mode::Client);
            vec![
                Line::from(
                    "What do you want to configure? (Left/Right to switch, Enter to confirm)",
                ),
                Line::from(""),
                Line::from(vec![proxy, Span::raw("   "), client]),
            ]
        }
        Step::PortInput => vec![
            Line::from(format!(
                "Port for {} (Enter to confirm):",
                w.mode.subcommand()
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!("> {}", w.port_input),
                Style::default().add_modifier(Modifier::BOLD),
            )),
        ],
        Step::Confirm => vec![
            Line::from("Confirm configuration (Enter to apply, 'b' to go back):"),
            Line::from(""),
            Line::from(format!("  Mode: {}", w.mode.subcommand())),
            Line::from(format!("  Port: {}", w.port_input)),
            Line::from(format!("  Config: {}", config::config_path().display())),
        ],
    };

    let mut lines = body;
    if let Some(err) = &w.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            err.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )));
    }

    let para = Paragraph::new(lines).block(Block::default().borders(Borders::ALL));
    f.render_widget(para, chunks[1]);

    let footer =
        Paragraph::new("Esc / Ctrl-C: cancel").block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[2]);
}

fn mode_span(label: &str, selected: bool) -> Span<'static> {
    let text = if selected {
        format!("[ {label} ]")
    } else {
        format!("  {label}  ")
    };
    let style = if selected {
        Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        Style::default()
    };
    Span::styled(text, style)
}
