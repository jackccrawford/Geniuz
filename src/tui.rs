//! Terminal UI for geniuz — `geniuz tui`.
//!
//! Single-pane navigator over local memories. Refuses to launch from a
//! non-interactive caller (no TTY) so an agent that accidentally invokes
//! `geniuz tui` returns an error instead of locking up.

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io::{self, IsTerminal};

use geniuz::db::{DatabaseManager, SignalEntry};

/// Public entry point invoked by the `geniuz tui` subcommand.
pub fn run() -> std::result::Result<(), String> {
    if !io::stdin().is_terminal() {
        return Err("geniuz tui requires an interactive terminal (stdin must be a TTY)".to_string());
    }
    run_inner().map_err(|e| format!("{e}"))
}

fn run_inner() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    result
}

enum Mode {
    List,
    Search,
    Detail,
    Help,
    Compose,
}

#[derive(Clone, Copy, PartialEq)]
enum ComposeFocus {
    Gist,
    Content,
}

struct App {
    db: DatabaseManager,
    memories: Vec<SignalEntry>,
    list_state: ListState,
    input: String,
    status: String,
    hint: String,
    mode: Mode,
    detail: Option<(SignalEntry, Option<String>)>,
    should_quit: bool,
    total: usize,
    compose_gist: String,
    compose_content: String,
    compose_focus: ComposeFocus,
    sort_desc: bool,
}

impl App {
    fn new() -> Result<Self> {
        let db_path = geniuz::data_dir().join("memory.db");
        let db_path_str = db_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("memory.db path is not valid UTF-8"))?;
        let db = DatabaseManager::new(db_path_str).map_err(|e| anyhow::anyhow!(e))?;
        let memories = db.recent(24).map_err(|e| anyhow::anyhow!(e))?;
        let total = db.count().map_err(|e| anyhow::anyhow!(e))?;
        let mut list_state = ListState::default();
        if !memories.is_empty() {
            list_state.select(Some(0));
        }
        Ok(Self {
            db,
            memories,
            list_state,
            input: String::new(),
            status: format!("{} memories", total),
            hint: "/recent  /search <q>  /find <q>  /remember  /detail  /random  /reorder  /status  /help  /quit".into(),
            mode: Mode::List,
            detail: None,
            should_quit: false,
            total,
            compose_gist: String::new(),
            compose_content: String::new(),
            compose_focus: ComposeFocus::Gist,
            sort_desc: true,
        })
    }

    fn handle_submit(&mut self) -> Result<()> {
        let input = self.input.trim().to_string();
        self.input.clear();
        if input.is_empty() {
            return Ok(());
        }
        if let Some(stripped) = input.strip_prefix('/') {
            self.handle_command(stripped)?;
        } else {
            self.status = "(no LLM backend yet — try /help)".into();
        }
        Ok(())
    }

    fn handle_command(&mut self, line: &str) -> Result<()> {
        let mut parts = line.splitn(2, ' ');
        let cmd = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("").trim();
        match cmd {
            "quit" | "q" | "exit" => self.should_quit = true,
            "recent" => {
                let limit: usize = arg.parse().unwrap_or(24);
                self.memories = self.db.recent(limit).map_err(|e| anyhow::anyhow!(e))?;
                self.sort_desc = true;
                self.mode = Mode::List;
                self.detail = None;
                self.list_state
                    .select(if self.memories.is_empty() { None } else { Some(0) });
                self.status = format!("recent · showing {}", self.memories.len());
            }
            "search" => {
                if arg.is_empty() {
                    self.status = "usage: /search <query>".into();
                } else {
                    self.memories = self
                        .db
                        .keyword_search(arg, 24)
                        .map_err(|e| anyhow::anyhow!(e))?;
                    self.mode = Mode::Search;
                    self.detail = None;
                    self.list_state
                        .select(if self.memories.is_empty() { None } else { Some(0) });
                    self.status = format!("search: {} · {} matches", arg, self.memories.len());
                }
            }
            "find" | "similar" | "semantic" => {
                if arg.is_empty() {
                    self.status = "usage: /find <query>".into();
                } else {
                    self.memories = self
                        .db
                        .semantic_search(arg, 24)
                        .map_err(|e| anyhow::anyhow!(e))?;
                    self.mode = Mode::Search;
                    self.detail = None;
                    self.list_state
                        .select(if self.memories.is_empty() { None } else { Some(0) });
                    self.status = format!("find: {} · {} matches", arg, self.memories.len());
                }
            }
            "detail" => {
                let key = if arg.is_empty() {
                    self.list_state
                        .selected()
                        .and_then(|i| self.memories.get(i))
                        .map(|m| m.memory_uuid.clone())
                } else {
                    Some(arg.to_string())
                };
                if let Some(uuid) = key {
                    let entry = self
                        .db
                        .get_by_uuid_prefix(&uuid)
                        .map_err(|e| anyhow::anyhow!(e))?;
                    if let Some(e) = entry {
                        let content = self
                            .db
                            .get_full_content(&e.memory_uuid)
                            .map_err(|err| anyhow::anyhow!(err))?;
                        self.status = format!("detail · {}", &e.memory_uuid[..8]);
                        self.detail = Some((e, content));
                        self.mode = Mode::Detail;
                    } else {
                        self.status = format!("no match for {}", uuid);
                    }
                } else {
                    self.status = "select a row first or pass a uuid".into();
                }
            }
            "random" => {
                if let Some(e) = self.db.random().map_err(|err| anyhow::anyhow!(err))? {
                    let content = self
                        .db
                        .get_full_content(&e.memory_uuid)
                        .map_err(|err| anyhow::anyhow!(err))?;
                    self.status = format!("random · {}", &e.memory_uuid[..8]);
                    self.detail = Some((e, content));
                    self.mode = Mode::Detail;
                }
            }
            "list" | "back" => {
                self.mode = Mode::List;
                self.detail = None;
                self.status = format!("{} memories", self.total);
            }
            "status" => {
                let count = self.db.count().map_err(|e| anyhow::anyhow!(e))?;
                self.total = count;
                self.status = format!(
                    "{} memories · db: {}",
                    count,
                    geniuz::data_dir().join("memory.db").display()
                );
            }
            "help" | "?" => {
                self.mode = Mode::Help;
                self.detail = None;
                self.status = "help · Esc to return".into();
            }
            "remember" | "r" => {
                self.handle_remember(arg)?;
            }
            "reorder" | "sort" | "reverse" => {
                self.memories.reverse();
                self.list_state
                    .select(if self.memories.is_empty() { None } else { Some(0) });
                self.sort_desc = !self.sort_desc;
                self.status = if self.sort_desc {
                    format!("sort: newest first · {} shown", self.memories.len())
                } else {
                    format!("sort: oldest first · {} shown", self.memories.len())
                };
            }
            _ => {
                self.status = format!("unknown command: /{} · /help for list", cmd);
            }
        }
        Ok(())
    }

    fn handle_remember(&mut self, gist_arg: &str) -> Result<()> {
        let preset = gist_arg.trim();
        self.compose_gist = preset.to_string();
        self.compose_content.clear();
        self.compose_focus = if preset.is_empty() {
            ComposeFocus::Gist
        } else {
            ComposeFocus::Content
        };
        self.mode = Mode::Compose;
        self.detail = None;
        self.status = "compose · Tab to switch field · Ctrl-S submit · Esc cancel".into();
        Ok(())
    }

    fn compose_submit(&mut self) -> Result<()> {
        let content = self.compose_content.trim().to_string();
        let gist = self.compose_gist.trim().to_string();
        if content.is_empty() {
            self.status = "remember cancelled (empty content)".into();
            self.compose_cancel_state();
            return Ok(());
        }
        let gist_opt = if gist.is_empty() { None } else { Some(gist.as_str()) };
        let new_uuid = self
            .db
            .signal(&content, gist_opt, None, None)
            .map_err(|e| anyhow::anyhow!(e))?;
        self.memories = self.db.recent(24).map_err(|e| anyhow::anyhow!(e))?;
        self.total = self.db.count().map_err(|e| anyhow::anyhow!(e))?;
        self.list_state
            .select(if self.memories.is_empty() { None } else { Some(0) });
        self.compose_cancel_state();
        self.status = format!("remembered · {} · {} memories", new_uuid, self.total);
        Ok(())
    }

    fn compose_cancel(&mut self) {
        self.compose_cancel_state();
        self.status = "remember cancelled".into();
    }

    fn compose_cancel_state(&mut self) {
        self.compose_gist.clear();
        self.compose_content.clear();
        self.compose_focus = ComposeFocus::Gist;
        self.mode = Mode::List;
    }

    fn compose_toggle_focus(&mut self) {
        self.compose_focus = match self.compose_focus {
            ComposeFocus::Gist => ComposeFocus::Content,
            ComposeFocus::Content => ComposeFocus::Gist,
        };
    }

    fn compose_push_char(&mut self, c: char) {
        match self.compose_focus {
            ComposeFocus::Gist => self.compose_gist.push(c),
            ComposeFocus::Content => self.compose_content.push(c),
        }
    }

    fn compose_pop_char(&mut self) {
        match self.compose_focus {
            ComposeFocus::Gist => {
                self.compose_gist.pop();
            }
            ComposeFocus::Content => {
                self.compose_content.pop();
            }
        }
    }

    fn compose_push_newline(&mut self) {
        match self.compose_focus {
            ComposeFocus::Gist => self.compose_focus = ComposeFocus::Content,
            ComposeFocus::Content => self.compose_content.push('\n'),
        }
    }

    fn move_up(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i > 0 {
                self.list_state.select(Some(i - 1));
            }
        }
    }

    fn move_down(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i + 1 < self.memories.len() {
                self.list_state.select(Some(i + 1));
            }
        }
    }
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>) -> Result<()> {
    let mut app = App::new()?;
    while !app.should_quit {
        terminal.draw(|f| ui(f, &mut app))?;
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                app.should_quit = true;
                continue;
            }

            if matches!(app.mode, Mode::Compose) {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Char('s') | KeyCode::Char('d') if ctrl => app.compose_submit()?,
                    KeyCode::Esc => app.compose_cancel(),
                    KeyCode::Tab | KeyCode::BackTab => app.compose_toggle_focus(),
                    KeyCode::Enter => app.compose_push_newline(),
                    KeyCode::Char(c) => app.compose_push_char(c),
                    KeyCode::Backspace => app.compose_pop_char(),
                    _ => {}
                }
                continue;
            }

            match key.code {
                KeyCode::Enter => {
                    if app.input.is_empty() {
                        if matches!(app.mode, Mode::List | Mode::Search) {
                            app.handle_command("detail")?;
                        } else if matches!(app.mode, Mode::Detail | Mode::Help) {
                            app.handle_command("back")?;
                        }
                    } else {
                        app.handle_submit()?;
                    }
                }
                KeyCode::Char(c) => app.input.push(c),
                KeyCode::Backspace => {
                    app.input.pop();
                }
                KeyCode::Esc => {
                    if app.input.is_empty() && matches!(app.mode, Mode::Detail | Mode::Help) {
                        app.handle_command("back")?;
                    } else {
                        app.input.clear();
                    }
                }
                KeyCode::Up => app.move_up(),
                KeyCode::Down => app.move_down(),
                _ => {}
            }
        }
    }
    Ok(())
}

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(f.area());

    render_status(f, app, chunks[0]);
    match app.mode {
        Mode::Detail => render_detail(f, app, chunks[1]),
        Mode::Help => render_help(f, chunks[1]),
        Mode::Compose => render_compose(f, app, chunks[1]),
        _ => render_list(f, app, chunks[1]),
    }
    render_input(f, app, chunks[2]);
    render_hint(f, app, chunks[3]);
}

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let line = Line::from(vec![
        Span::styled(
            " geniuz ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::raw(app.status.clone()),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn render_list(f: &mut Frame, app: &mut App, area: Rect) {
    let total = app.memories.len();
    if total == 0 {
        let msg = match app.mode {
            Mode::Search => vec![
                Line::raw(""),
                Line::from(vec![Span::styled(
                    "  No matches.",
                    Style::default().fg(Color::DarkGray),
                )]),
                Line::raw(""),
                Line::from(vec![Span::styled(
                    "  Try a different query, or /recent to see everything.",
                    Style::default().fg(Color::DarkGray),
                )]),
            ],
            _ => vec![
                Line::raw(""),
                Line::from(vec![Span::styled(
                    "  No memories yet.",
                    Style::default().fg(Color::DarkGray),
                )]),
                Line::raw(""),
                Line::from(vec![
                    Span::styled("  Try ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        "/remember",
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" to add one.", Style::default().fg(Color::DarkGray)),
                ]),
            ],
        };
        let title = match app.mode {
            Mode::List => " Recent ",
            Mode::Search => " Search ",
            _ => " Memories ",
        };
        let para = Paragraph::new(msg)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false });
        f.render_widget(para, area);
        return;
    }
    let sort_desc = app.sort_desc;
    let items: Vec<ListItem> = app
        .memories
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let n = if sort_desc { total - i } else { i + 1 };
            let (category, body) = split_gist(&m.gist);
            let when = m.created_at.get(..16).unwrap_or(&m.created_at);
            let header = Line::from(vec![
                Span::styled(format!("#{:<4}", n), Style::default().fg(Color::DarkGray)),
                Span::raw(" · "),
                Span::raw(when.to_string()),
                Span::raw(" · "),
                Span::styled(
                    category.to_string(),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
            ]);
            let body_line = Line::from(vec![
                Span::raw("    "),
                Span::styled(truncate(body, 240), Style::default().fg(Color::Gray)),
            ]);
            ListItem::new(vec![header, body_line, Line::raw("")])
        })
        .collect();

    let title = match app.mode {
        Mode::List => " Recent ",
        Mode::Search => " Search ",
        _ => " Memories ",
    };

    let list = List::new(items)
        .block(Block::default().title(title).borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );
    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn render_detail(f: &mut Frame, app: &App, area: Rect) {
    let Some((entry, content)) = &app.detail else {
        return;
    };
    let (category, gist_body) = split_gist(&entry.gist);
    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled(
                category.to_string(),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled(
                entry.created_at.clone(),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::raw(""),
        Line::from(vec![Span::styled(
            gist_body.to_string(),
            Style::default().fg(Color::White),
        )]),
        Line::raw(""),
        Line::from(vec![Span::styled(
            format!("uuid: {}", entry.memory_uuid),
            Style::default().fg(Color::DarkGray),
        )]),
        Line::raw(""),
        Line::raw(""),
    ];
    if let Some(c) = content {
        for chunk in c.split('\n') {
            lines.push(Line::raw(chunk.to_string()));
        }
    }
    let para = Paragraph::new(lines)
        .block(Block::default().title(" Detail ").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    f.render_widget(para, area);
}

fn render_input(f: &mut Frame, app: &App, area: Rect) {
    let prompt_style = Style::default().fg(Color::Yellow);
    let (prompt, body) = if matches!(app.mode, Mode::Compose) {
        ("⧉ ", String::from("(composing — type below; Ctrl-S submit; Esc cancel)"))
    } else {
        ("› ", app.input.clone())
    };
    let line = Line::from(vec![
        Span::styled(prompt, prompt_style),
        Span::raw(body),
        if matches!(app.mode, Mode::Compose) {
            Span::raw("")
        } else {
            Span::styled(
                "▌",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::SLOW_BLINK),
            )
        },
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn render_hint(f: &mut Frame, app: &App, area: Rect) {
    let text = if matches!(app.mode, Mode::Compose) {
        " Tab = switch field · Enter = newline (content) · Ctrl-S = submit · Esc = cancel".to_string()
    } else {
        format!(" {}", app.hint)
    };
    let line = Line::from(vec![Span::styled(
        text,
        Style::default().fg(Color::DarkGray),
    )]);
    f.render_widget(Paragraph::new(line), area);
}

fn render_help(f: &mut Frame, area: Rect) {
    let header = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let cmd = Style::default().fg(Color::Yellow);
    let dim = Style::default().fg(Color::DarkGray);
    let lines: Vec<Line> = vec![
        Line::from(vec![Span::styled("geniuz tui — your memories, your terminal", header)]),
        Line::raw(""),
        Line::from(vec![Span::styled("COMMANDS", header)]),
        Line::from(vec![Span::styled("  /recent [n]   ", cmd), Span::raw("last n memories (default 24)")]),
        Line::from(vec![Span::styled("  /search <q>   ", cmd), Span::raw("keyword search across gists and content")]),
        Line::from(vec![Span::styled("  /find <q>     ", cmd), Span::raw("find by meaning (semantic search)")]),
        Line::from(vec![Span::styled("  /remember     ", cmd), Span::raw("author a new memory (compose pane)")]),
        Line::from(vec![Span::styled("  /detail [id]  ", cmd), Span::raw("open detail; no arg uses selected row")]),
        Line::from(vec![Span::styled("  /random       ", cmd), Span::raw("open a random memory")]),
        Line::from(vec![Span::styled("  /reorder      ", cmd), Span::raw("toggle list order (newest / oldest first)")]),
        Line::from(vec![Span::styled("  /status       ", cmd), Span::raw("refresh memory count and show db path")]),
        Line::from(vec![Span::styled("  /help         ", cmd), Span::raw("this screen")]),
        Line::from(vec![Span::styled("  /quit         ", cmd), Span::raw("exit")]),
        Line::raw(""),
        Line::from(vec![Span::styled("KEYS", header)]),
        Line::from(vec![Span::styled("  ↑ / ↓         ", cmd), Span::raw("navigate list")]),
        Line::from(vec![Span::styled("  Enter         ", cmd), Span::raw("open detail (empty input) · submit command (with text)")]),
        Line::from(vec![Span::styled("  Esc           ", cmd), Span::raw("clear input · back from detail/help")]),
        Line::from(vec![Span::styled("  Ctrl-C        ", cmd), Span::raw("exit")]),
        Line::raw(""),
        Line::from(vec![Span::styled("Memories live at ~/.geniuz/memory.db. Free and local.", dim)]),
    ];
    let para = Paragraph::new(lines)
        .block(Block::default().title(" Help ").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    f.render_widget(para, area);
}

fn render_compose(f: &mut Frame, app: &App, area: Rect) {
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let gist_focused = app.compose_focus == ComposeFocus::Gist;
    let content_focused = !gist_focused;

    let cursor = "▌";
    let dim = Style::default().fg(Color::DarkGray);
    let active_border = Style::default().fg(Color::Yellow);
    let inactive_border = Style::default().fg(Color::DarkGray);

    let gist_text = if app.compose_gist.is_empty() && !gist_focused {
        Line::from(vec![Span::styled(
            "(short shelf-label, e.g. \"fix: auth token order\" — used for retrieval)",
            dim,
        )])
    } else if gist_focused {
        Line::from(vec![
            Span::raw(app.compose_gist.clone()),
            Span::styled(cursor, active_border),
        ])
    } else {
        Line::from(Span::raw(app.compose_gist.clone()))
    };
    let gist_title = if gist_focused { " Gist ◂ " } else { " Gist " };
    let gist_para = Paragraph::new(vec![gist_text])
        .block(
            Block::default()
                .title(gist_title)
                .borders(Borders::ALL)
                .border_style(if gist_focused { active_border } else { inactive_border }),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(gist_para, split[0]);

    let content_lines: Vec<Line> = if app.compose_content.is_empty() && !content_focused {
        vec![Line::from(vec![Span::styled(
            "(content — Enter inserts a newline)",
            dim,
        )])]
    } else {
        let body = if content_focused {
            format!("{}{}", app.compose_content, cursor)
        } else {
            app.compose_content.clone()
        };
        body.split('\n').map(|l| Line::raw(l.to_string())).collect()
    };
    let content_title = if content_focused { " Content ◂ " } else { " Content " };
    let content_para = Paragraph::new(content_lines)
        .block(
            Block::default()
                .title(content_title)
                .borders(Borders::ALL)
                .border_style(if content_focused { active_border } else { inactive_border }),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(content_para, split[1]);
}

fn split_gist(gist: &str) -> (&str, &str) {
    // First separator wins: |, ;, or :
    let candidates = [
        gist.find('|'),
        gist.find(';'),
        gist.find(':'),
    ];
    let earliest = candidates.into_iter().flatten().min();
    match earliest {
        Some(i) => (gist[..i].trim(), gist[i + 1..].trim()),
        None => ("", gist.trim()),
    }
}

fn truncate(s: &str, n: usize) -> String {
    let count = s.chars().count();
    if count <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}
