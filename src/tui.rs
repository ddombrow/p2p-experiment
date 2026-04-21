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
    pub doc: crate::doc::Doc,
    pub operator: String,
    pub peers: Vec<Peer>,
    pub input: String,
    pub log: Vec<String>,
    pub show_help: bool,
}

impl App {
    pub fn new(operator: String) -> Self {
        Self {
            doc: crate::doc::Doc::new(),
            operator,
            peers: vec![],
            input: String::new(),
            log: vec![],
            show_help: false,
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

    let outer = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(5),
        Constraint::Length(3),
    ])
    .split(frame.area());

    render_statusbar(frame, app, outer[0]);
    render_main(frame, &board, app, outer[1]);
    render_log(frame, app, outer[2]);
    render_input(frame, app, outer[3]);

    if app.show_help {
        render_help_modal(frame);
    }
}

fn render_statusbar(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let online_count = app.peers.iter().filter(|p| p.online).count();
    let peers = if online_count == 0 {
        "no peers".to_string()
    } else {
        format!("{online_count} online")
    };
    let text = format!(" OPERATOR: {}   ONLINE: {}", app.operator, peers);
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
            let (status_color, status_mod) = match obj.status {
                Status::Active => (Color::Yellow, Modifier::BOLD),
                Status::Done => (Color::Green, Modifier::DIM),
                Status::Abort => (Color::Red, Modifier::DIM),
                Status::Pending => (Color::Gray, Modifier::empty()),
            };
            let line = Line::from(vec![
                Span::styled(
                    format!(" [{:>2}] ", i + 1),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("[{}] ", obj.status.as_str()),
                    Style::default().fg(status_color).add_modifier(status_mod),
                ),
                Span::raw(format!("{:<30}", obj.task)),
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

    // Notes
    let note_items: Vec<ListItem> = board
        .notes
        .iter()
        .map(|note| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {}: ", note.author),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(note.text.clone()),
            ]))
        })
        .collect();

    frame.render_widget(
        List::new(note_items).block(Block::default().title(" NOTES ").borders(Borders::ALL)),
        cols[1],
    );

    // Operators
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
        cols[2],
    );
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
            Span::styled("  note   ", Style::default().fg(Color::Yellow)),
            Span::raw("<text>"),
        ]),
        Line::from(Span::styled(
            "    append to mission notes",
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
    let help = " assign \"<task>\" <op>  |  status <n> active|done|abort  |  take <n>  |  del <n>  |  note <text>  |  q";
    let content = format!("> {}", app.input);
    let para = Paragraph::new(vec![
        Line::from(content),
        Line::from(Span::styled(help, Style::default().fg(Color::DarkGray))),
    ])
    .block(Block::default().title(" COMMAND ").borders(Borders::ALL));
    frame.render_widget(para, area);
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

    if let Some(rest) = input.strip_prefix("note ") {
        return Command::Note {
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
    Note { text: String },
    Clear,
    Help,
    Quit,
    Unknown(String),
}
