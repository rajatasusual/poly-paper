use crate::{
    analyst::loader::load_sessions,
    analyst::metrics::compute_metrics,
    analyst::models::Session,
    analyst::watcher::watch_logs,
};
use anyhow::Result;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::*,
    widgets::*,
};
use std::sync::mpsc::{channel, Receiver};
pub struct App {
    pub sessions: Vec<Session>,
    pub filtered_sessions: Vec<usize>,
    pub selected: usize,
    pub should_quit: bool,
    pub filter_mode: bool,
    pub filter_input: String,
    pub logs_path: String,
    pub reload_rx: Receiver<()>,
}
impl App {
    pub fn new(logs_path: &str) -> Result<Self> {
        let sessions = load_sessions(logs_path)?;
        let filtered_sessions =
            (0..sessions.len()).collect::<Vec<_>>();
        let (reload_tx, reload_rx) = channel();
        let _ = watch_logs(logs_path, reload_tx);
        Ok(Self {
            sessions,
            filtered_sessions,
            selected: 0,
            should_quit: false,
            filter_mode: false,
            filter_input: String::new(),
            logs_path: logs_path.to_string(),
            reload_rx,
        })
    }
    pub fn tick(&mut self) {
        if self.reload_rx.try_recv().is_ok() {
            let _ = self.reload();
        }
    }
    pub fn reload(&mut self) -> Result<()> {
        self.sessions = load_sessions(&self.logs_path)?;
        self.apply_filter();
        Ok(())
    }
    pub fn apply_filter(&mut self) {
        let query = self.filter_input.to_lowercase();
        self.filtered_sessions = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| {
                if query.is_empty() {
                    return true;
                }
                session.slug.to_lowercase().contains(&query)
                    || session
                        .question
                        .to_lowercase()
                        .contains(&query)
            })
            .map(|(idx, _)| idx)
            .collect();
        if self.filtered_sessions.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.filtered_sessions.len() {
            self.selected = self.filtered_sessions.len() - 1;
        }
    }
    pub fn next(&mut self) {
        if self.selected + 1 < self.filtered_sessions.len() {
            self.selected += 1;
        }
    }
    pub fn previous(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }
    fn selected_session(&self) -> Option<&Session> {
        self.filtered_sessions
            .get(self.selected)
            .and_then(|idx| self.sessions.get(*idx))
    }
    pub fn render(&self, frame: &mut Frame) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(frame.area());
        let main = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(75),
            ])
            .split(layout[0]);
        self.render_session_list(frame, main[0]);
        self.render_dashboard(frame, main[1]);
        self.render_footer(frame, layout[1]);
    }
    fn render_session_list(
        &self,
        frame: &mut Frame,
        area: Rect,
    ) {
        let items: Vec<ListItem> = self
            .filtered_sessions
            .iter()
            .map(|idx| {
                let session = &self.sessions[*idx];
                let pnl = session
                    .realized_pnl
                    .parse::<f64>()
                    .unwrap_or(0.0);
                ListItem::new(format!(
                    "{}\nPnL: {:.4}\n{}",
                    session.slug,
                    pnl,
                    session.question
                ))
            })
            .collect();
        let title = if self.filter_mode {
            format!("Sessions | FILTER: {}", self.filter_input)
        } else {
            format!("Sessions [{}]", self.filtered_sessions.len())
        };
        let list = List::new(items)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White),
            )
            .highlight_symbol("▶ ");
        let mut state = ListState::default();
        state.select(Some(self.selected));
        frame.render_stateful_widget(list, area, &mut state);
    }
    fn render_dashboard(
        &self,
        frame: &mut Frame,
        area: Rect,
    ) {
        let Some(session) = self.selected_session() else {
            return;
        };
        let metrics = compute_metrics(session);
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Min(10),
            ])
            .split(area);
        self.render_summary(frame, layout[0], session, &metrics);
        self.render_capital_chart(frame, layout[1], session);
        self.render_edge_chart(frame, layout[2], session);
        self.render_execution_table(frame, layout[3], session);
    }
    fn render_summary(
        &self,
        frame: &mut Frame,
        area: Rect,
        session: &Session,
        metrics: &crate::analyst::metrics::SessionMetrics,
    ) {
        let text = format!(
            "Question: {}\nPnL: {:.4} | Execs: {} | Duration: {:.2}s\nPeak Deployment: {:.2}\nEfficiency: {:.4}\nCash: {}",
            session.question,
            metrics.pnl,
            metrics.execution_count,
            metrics.duration_secs,
            metrics.peak_deployment,
            metrics.efficiency,
            session.cash,
        );
        let widget = Paragraph::new(text)
            .block(
                Block::default()
                    .title("Session Analytics")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(widget, area);
    }
    fn render_capital_chart(
        &self,
        frame: &mut Frame,
        area: Rect,
        session: &Session,
    ) {
        let data: Vec<u64> = session
            .executions
            .iter()
            .map(|exec| {
                exec
                    .pending_settlement_payout_after
                    .parse::<f64>()
                    .unwrap_or(100.0)
                    .round() as u64
            })
            .collect();
        let sparkline = Sparkline::default()
            .block(
                Block::default()
                    .title("Capital Deployment Timeline")
                    .borders(Borders::ALL),
            )
            .style(Style::default().fg(Color::Green))
            .data(&data);
        frame.render_widget(sparkline, area);
    }
    fn render_edge_chart(
        &self,
        frame: &mut Frame,
        area: Rect,
        session: &Session,
    ) {
        let data: Vec<u64> = session
            .executions
            .iter()
            .map(|exec| {
                let edge = exec
                    .guaranteed_profit
                    .parse::<f64>()
                    .unwrap_or(0.0);
                (edge * 1000.0) as u64
            })
            .collect();
        let sparkline = Sparkline::default()
            .block(
                Block::default()
                    .title("Edge Quality Distribution")
                    .borders(Borders::ALL),
            )
            .data(&data)
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(sparkline, area);
    }
    fn render_execution_table(
        &self,
        frame: &mut Frame,
        area: Rect,
        session: &Session,
    ) {
        let rows: Vec<Row> = session
            .executions
            .iter()
            .map(|exec| {
                let edge = exec
                    .guaranteed_profit
                    .parse::<f64>()
                    .unwrap_or(0.0);
                let color = if edge > 0.25 {
                    Color::Green
                } else if edge > 0.10 {
                    Color::Yellow
                } else {
                    Color::Red
                };
                Row::new(vec![
                    exec.strategy.clone(),
                    exec.size.clone(),
                    exec.package_price.clone(),
                    format!("{:.4}", edge),
                ])
                .style(Style::default().fg(color))
            })
            .collect();
        let table = Table::new(
            rows,
            [
                Constraint::Percentage(40),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
            ],
        )
        .header(
            Row::new(vec![
                "Strategy",
                "Size",
                "Package",
                "Edge",
            ])
            .style(Style::default().fg(Color::Cyan)),
        )
        .block(
            Block::default()
                .title("Execution Flow")
                .borders(Borders::ALL),
        )
        .column_spacing(1);
        frame.render_widget(table, area);
    }
    fn render_footer(
        &self,
        frame: &mut Frame,
        area: Rect,
    ) {
        let shortcuts = vec![Line::from(
            "↑↓ Navigate  / Filter  ESC Exit Filter  r Reload  q Quit"
        )];
        let footer = Paragraph::new(shortcuts)
            .block(
                Block::default()
                    .title("Keyboard Shortcuts")
                    .borders(Borders::ALL),
            )
            .style(Style::default().fg(Color::Gray));
        frame.render_widget(footer, area);
    }
}