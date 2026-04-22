use crate::doc::{Board, Status};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

pub struct Peer {
    pub name: String,
    pub online: bool,
}

pub struct App {
    pub doc:        crate::doc::Doc,
    pub operator:   String,
    pub topic:      String,
    pub peers:      Vec<Peer>,
    pub input:      String,
    pub log:        Vec<String>,
    pub comms_log:  Vec<CommsEntry>,
    pub show_help:         bool,
    pub copy_flash:        Option<std::time::Instant>,
    pub mention_bell:      Option<std::time::Instant>,
    pub joined_announced:  bool,
}

pub struct CommsEntry {
    pub timestamp: String,
    pub kind:      CommsKind,
}

pub enum CommsKind {
    Message { author: String, text: String },
    System(String),
}

impl App {
    pub fn new(operator: String, topic: String) -> Self {
        Self {
            doc: crate::doc::Doc::new(),
            operator,
            topic,
            peers: vec![],
            input: String::new(),
            log: vec![],
            comms_log: vec![],
            show_help: false,
            copy_flash: None,
            mention_bell: None,
            joined_announced: false,
        }
    }

    pub fn push_log(&mut self, msg: impl Into<String>) {
        self.log.push(msg.into());
        if self.log.len() > 50 {
            self.log.remove(0);
        }
    }

}


pub fn render(frame: &mut Frame, app: &App) {
    let board = app.doc.read();

    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(8),
        Constraint::Length(3),
    ])
    .split(frame.area());

    render_statusbar(frame, app, chunks[0]);
    render_main(frame, &board, app, chunks[1]);
    render_log(frame, app, chunks[2]);
    render_input(frame, app, chunks[3]);

    if app.show_help {
        render_help_modal(frame);
    }
}

pub fn is_copy_button_clicked(app: &App, col: u16, row: u16) -> bool {
    if row != 0 {
        return false;
    }
    let online_count = app.peers.iter().filter(|p| p.online).count() + 1; // +1 for self
    let peers_len = online_count.to_string().len() + 7; // "X online"
    let start_col = 11 + app.operator.len() + 11 + peers_len + 10 + app.topic.len() + 3;
    let end_col = start_col + 6; // "[Copy]" is 6 chars

    (col as usize) >= start_col && (col as usize) < end_col
}

fn render_statusbar(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let online_count = app.peers.iter().filter(|p| p.online).count() + 1; // +1 for self
    let peers = format!("{online_count} online");
    
    let text = Line::from(vec![
        Span::raw(" OPERATOR: "),
        Span::styled(&app.operator, Style::default().fg(Color::Yellow)),
        Span::raw("   ONLINE: "),
        Span::styled(peers, Style::default().fg(Color::Green)),
        Span::raw("   TOPIC: "),
        Span::styled(&app.topic, Style::default().fg(Color::Cyan)),
        Span::raw("   "),
        {
            let flashing = app.copy_flash
                .map(|t| t.elapsed() < std::time::Duration::from_millis(300))
                .unwrap_or(false);
            if flashing {
                Span::styled("[Copied!]", Style::default().fg(Color::Black).bg(Color::Green))
            } else {
                Span::styled("[Copy]", Style::default().fg(Color::Black).bg(Color::White))
            }
        },
    ]);
    
    frame.render_widget(
        Paragraph::new(text).style(Style::default().bg(Color::DarkGray).fg(Color::White)),
        area,
    );
}

fn render_main(frame: &mut Frame, board: &Board, app: &App, area: ratatui::layout::Rect) {
    let cols = Layout::horizontal([
        Constraint::Percentage(55),
        Constraint::Percentage(25),
        Constraint::Percentage(20),
    ])
    .split(area);

    // Objectives
    let items: Vec<ListItem> = board
        .objectives
        .iter()
        .enumerate()
        .map(|(i, obj)| {
            let (badge_fg, badge_bg, task_style) = match obj.status {
                Status::Active   => (Color::Black, Color::Yellow,  Style::default().add_modifier(Modifier::BOLD)),
                Status::Done     => (Color::Black, Color::Green,   Style::default().add_modifier(Modifier::DIM)),
                Status::Abort    => (Color::White, Color::Red,     Style::default().add_modifier(Modifier::DIM)),
                Status::Pending  => (Color::Black, Color::DarkGray,Style::default()),
            };
            let line = Line::from(vec![
                Span::styled(
                    format!(" [{:>2}] ", i + 1),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!(" {} ", obj.status.as_str()),
                    Style::default().fg(badge_fg).bg(badge_bg).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("{:<30}", obj.task),
                    task_style,
                ),
                Span::styled(
                    format!(" @ {}", obj.assignee),
                    Style::default().fg(Color::Cyan),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    frame.render_widget(
        List::new(items).block(Block::default().title(" OBJECTIVES ").borders(Borders::ALL)),
        cols[0],
    );

    // Operators (col 1)
    let mut operator_items: Vec<ListItem> = vec![ListItem::new(Line::from(vec![
        Span::styled(" ● ", Style::default().fg(Color::Green)),
        Span::styled(
            app.operator.clone(),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" (you)", Style::default().fg(Color::DarkGray)),
    ]))];

    for peer in &app.peers {
        let (bullet, style) = if peer.online {
            ("● ", Style::default().fg(Color::Cyan))
        } else {
            ("○ ", Style::default().fg(Color::DarkGray))
        };
        operator_items.push(ListItem::new(Line::from(vec![
            Span::styled(format!(" {bullet}"), style),
            Span::styled(peer.name.clone(), style),
        ])));
    }

    frame.render_widget(
        List::new(operator_items)
            .block(Block::default().title(" OPERATORS ").borders(Borders::ALL)),
        cols[1],
    );

    // Comms (col 2)
    let known_operators: Vec<&str> = std::iter::once(app.operator.as_str())
        .chain(app.peers.iter().map(|p| p.name.as_str()))
        .collect();
    let inner_height = cols[2].height.saturating_sub(2) as usize;
    let comms_items: Vec<ListItem> = app
        .comms_log
        .iter()
        .rev()
        .take(inner_height)
        .rev()
        .map(|entry| match &entry.kind {
            CommsKind::Message { author, text } => {
                let is_me = *author == app.operator;
                let author_style = if is_me {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                };
                let mut spans = vec![
                    Span::styled(format!(" [{}] ", entry.timestamp), Style::default().fg(Color::DarkGray)),
                    Span::styled(format!("{author}: "), author_style),
                ];
                spans.extend(render_message_text(text, &known_operators));
                ListItem::new(Line::from(spans))
            }
            CommsKind::System(text) => ListItem::new(Line::from(vec![
                Span::styled(format!(" [{}] ", entry.timestamp), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("-- {text} --"), Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)),
            ])),
        })
        .collect();

    let bell_active = app.mention_bell
        .map(|t| {
            let ms = t.elapsed().as_millis();
            ms < 200 || (ms >= 350 && ms < 600)
        })
        .unwrap_or(false);
    let comms_border_style = if bell_active {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    frame.render_widget(
        List::new(comms_items).block(
            Block::default().title(" COMMS ").borders(Borders::ALL).border_style(comms_border_style)
        ),
        cols[2],
    );
}


fn render_message_text<'a>(text: &'a str, known_operators: &[&str]) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    let mut remaining = text;
    while !remaining.is_empty() {
        if let Some(at_pos) = remaining.find('@') {
            if at_pos > 0 {
                spans.push(Span::raw(remaining[..at_pos].to_string()));
            }
            let after_at = &remaining[at_pos + 1..];
            let token_end = after_at.find(|c: char| c.is_whitespace()).unwrap_or(after_at.len());
            let name = &after_at[..token_end];
            let is_known = known_operators.iter().any(|op| {
                op.to_lowercase().starts_with(&name.to_lowercase()) && !name.is_empty()
            });
            if is_known {
                spans.push(Span::styled(
                    format!("@{name}"),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::raw(format!("@{name}")));
            }
            remaining = &remaining[at_pos + 1 + token_end..];
        } else {
            spans.push(Span::raw(remaining.to_string()));
            break;
        }
    }
    spans
}

fn render_log(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let lines: Vec<Line> = app
        .log
        .iter()
        .rev()
        .take(inner_height)
        .rev()
        .map(|s| {
            Line::from(Span::styled(
                s.clone(),
                Style::default().fg(Color::DarkGray),
            ))
        })
        .collect();

    frame.render_widget(
        Paragraph::new(lines).block(Block::default().title(" LOG ").borders(Borders::ALL)),
        area,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vert = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vert[1])[1]
}

fn render_help_modal(frame: &mut Frame) {
    let area = centered_rect(60, 60, frame.area());
    frame.render_widget(Clear, area);

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  add    ", Style::default().fg(Color::Yellow)),
            Span::raw("\"<task>\""),
        ]),
        Line::from(Span::styled(
            "    create unassigned objective",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  assign ", Style::default().fg(Color::Yellow)),
            Span::raw("<n> <operator>"),
        ]),
        Line::from(Span::styled(
            "    assign objective to an operator",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  status ", Style::default().fg(Color::Yellow)),
            Span::raw("<n> active|done|abort|pending"),
        ]),
        Line::from(Span::styled(
            "    update objective status",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  take   ", Style::default().fg(Color::Yellow)),
            Span::raw("<n>"),
        ]),
        Line::from(Span::styled(
            "    reassign objective to yourself",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  del    ", Style::default().fg(Color::Yellow)),
            Span::raw("<n>"),
        ]),
        Line::from(Span::styled(
            "    delete objective",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  msg    ", Style::default().fg(Color::Cyan)),
            Span::raw("<text>"),
        ]),
        Line::from(Span::styled(
            "    send a comms message  (Tab to switch boxes)",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  q / quit",
            Style::default().fg(Color::Yellow),
        )]),
        Line::from(Span::styled(
            "    exit",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Press Esc or Enter to close",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )),
    ];

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" HELP ")
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::Black)),
        ),
        area,
    );
}

fn render_input(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let help = " add \"<task>\"  assign <n> <op>  status <n> active|done|abort  take <n>  del <n>  msg <text>  help";
    let content = format!("> {}", app.input);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(content),
            Line::from(Span::styled(help, Style::default().fg(Color::DarkGray))),
        ])
        .block(Block::default().title(" COMMAND ").borders(Borders::ALL)),
        area,
    );
}

pub fn parse_command(input: &str) -> Command {
    let input = input.trim();
    if input == "q" || input == "quit" {
        return Command::Quit;
    }

    if input == "help" || input == "?" {
        return Command::Help;
    }

    if input == "clear" {
        return Command::Clear;
    }

    if let Some(rest) = input.strip_prefix("add ") {
        // add "task text"
        if rest.starts_with('"')
            && let Some(end) = rest[1..].find('"')
        {
            let task = rest[1..end + 1].to_string();
            return Command::Add { task };
        }
    }

    if let Some(rest) = input.strip_prefix("assign ") {
        // assign <n> <operator>
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        if parts.len() == 2
            && let Ok(n) = parts[0].parse::<usize>()
        {
            return Command::Assign {
                index: n.saturating_sub(1),
                assignee: parts[1].trim().to_string(),
            };
        }
    }

    if let Some(rest) = input.strip_prefix("status ") {
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        if parts.len() == 2
            && let Ok(n) = parts[0].parse::<usize>()
        {
            return Command::Status {
                index: n.saturating_sub(1),
                status: parts[1].to_uppercase(),
            };
        }
    }

    if let Some(rest) = input.strip_prefix("take ")
        && let Ok(n) = rest.trim().parse::<usize>()
    {
        return Command::Take {
            index: n.saturating_sub(1),
        };
    }

    if let Some(rest) = input.strip_prefix("del ")
        && let Ok(n) = rest.trim().parse::<usize>()
    {
        return Command::Delete {
            index: n.saturating_sub(1),
        };
    }

    if let Some(rest) = input.strip_prefix("msg ") {
        return Command::Msg {
            text: rest.to_string(),
        };
    }

    Command::Unknown(input.to_string())
}

pub enum Command {
    Add { task: String },
    Assign { index: usize, assignee: String },
    Status { index: usize, status: String },
    Take { index: usize },
    Delete { index: usize },
    Msg { text: String },
    Clear,
    Help,
    Quit,
    Unknown(String),
}
