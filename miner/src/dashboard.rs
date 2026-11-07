use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};
use serde_json::Value;
use std::{
    collections::VecDeque,
    io::{self, IsTerminal},
    sync::{Arc, Mutex, atomic::Ordering},
    thread,
    time::{Duration, Instant, SystemTime},
};
use zoru_miner::session::Session;

const BG: Color = Color::Rgb(8, 13, 24);
const CYAN: Color = Color::Rgb(60, 210, 230);
const MUTED: Color = Color::Rgb(135, 151, 173);
const GREEN: Color = Color::Rgb(97, 220, 145);
const GOLD: Color = Color::Rgb(239, 198, 101);
const LIMIT: usize = 512;

struct MiningEvent {
    time: String,
    category: String,
    message: String,
}

#[derive(Default)]
pub struct View {
    state: Value,
    inspection: Value,
    inspected: Option<Instant>,
    events: VecDeque<MiningEvent>,
    scroll: usize,
    logs: bool,
}

pub fn enabled(plain: bool, stdin: bool, stdout: bool, term: Option<&str>) -> bool {
    !plain && stdin && stdout && term != Some("dumb")
}

pub fn available(plain: bool) -> bool {
    enabled(
        plain,
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
        std::env::var("TERM").ok().as_deref(),
    )
}

impl View {
    pub fn receive(&mut self, value: Value) {
        match value["event"].as_str().unwrap_or_default() {
            "state" | "mine-result" => self.state = value["data"].clone(),
            "inspection" => {
                self.inspection = value["data"].clone();
                self.inspected = Some(Instant::now());
            }
            "mining-event" => self.push(text(&value["category"]), text(&value["message"])),
            "transaction" => {
                let data = &value["data"];
                if data["status"] == "tx-submission" {
                    self.push(
                        "SUBMIT",
                        &format!(
                            "{} · {}",
                            text(&data["label"]),
                            short(text(&data["hash"]), false)
                        ),
                    );
                }
            }
            "error" => self.push("ERROR", text(&value["error"])),
            "warning" => self.push("WARNING", text(&value["warning"])),
            _ => {}
        }
    }
    fn push(&mut self, category: &str, message: &str) {
        let message: String = message
            .chars()
            .filter(|c| !c.is_control())
            .take(350)
            .collect();
        let message = message
            .split_whitespace()
            .map(|word| {
                if word.starts_with("0x") && word.len() == 66 {
                    short(word, false)
                } else {
                    word.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        // An unavailable RPC may repeat the same error on every work refresh.
        if self
            .events
            .back()
            .is_some_and(|e| e.category == category && e.message == message)
        {
            return;
        }
        if self.scroll > 0 {
            self.scroll = (self.scroll + 1).min(LIMIT - 1);
        }
        self.events.push_back(MiningEvent {
            time: chrono::DateTime::<chrono::Utc>::from(SystemTime::now())
                .format("%H:%M:%S")
                .to_string(),
            category: category.to_owned(),
            message,
        });
        if self.events.len() > LIMIT {
            self.events.pop_front();
        }
    }
    fn key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Up => self.scroll = (self.scroll + 1).min(self.events.len().saturating_sub(1)),
            KeyCode::Down => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::PageUp => {
                self.scroll = (self.scroll + 10).min(self.events.len().saturating_sub(1))
            }
            KeyCode::PageDown => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::Char('l' | 'L') => self.logs = !self.logs,
            KeyCode::Char('r' | 'R') => self.scroll = 0,
            _ => {}
        }
    }
    fn draw(&self, frame: &mut Frame<'_>, ascii: bool) {
        let area = frame.area();
        frame.render_widget(
            Block::default().style(Style::default().bg(BG).fg(Color::White)),
            area,
        );
        let compact = area.height < 24 || area.width < 80;
        let rows = Layout::vertical(if compact {
            vec![
                Constraint::Length(4),
                Constraint::Length(3),
                Constraint::Length(4),
                Constraint::Min(2),
                Constraint::Length(1),
            ]
        } else {
            vec![
                Constraint::Length(4),
                Constraint::Length(4),
                Constraint::Length(11),
                Constraint::Min(3),
                Constraint::Length(1),
            ]
        })
        .split(area);
        let state = &self.state;
        let snapshot = &self.inspection;
        let phase = match text(&state["status"]) {
            "mining" => "HASHING",
            "paused" | "paused-resource" => "PAUSED",
            "rpc-degraded" => "RPC DEGRADED",
            "stopped" => "STOPPED",
            "stopping" => "STOPPING",
            _ => "WORK REFRESH",
        };
        let rpc = if state["rpcHealthy"] == false {
            "DEGRADED"
        } else if self.inspected.is_some() {
            "HEALTHY"
        } else {
            "CONNECTING"
        };
        let network = match state["deploymentMode"].as_str() {
            Some("local") => "ISOLATED EVM",
            Some("mainnet") => "VEXANIUM EVM / MAINNET",
            _ => "VEXANIUM EVM",
        };
        let header = vec![
            Line::from(vec![
                Span::styled(
                    " ZORU MINER",
                    Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("   {network}   Chain {}", text(&state["chainId"]))),
            ]),
            Line::from(vec![
                Span::styled(
                    format!(" {phase}"),
                    Style::default().fg(if phase == "HASHING" {
                        GREEN
                    } else {
                        Color::Yellow
                    }),
                ),
                Span::styled(
                    format!(
                        "   RPC {rpc}   Wallet {}",
                        short(text(&state["address"]), ascii)
                    ),
                    Style::default().fg(MUTED),
                ),
            ]),
        ];
        frame.render_widget(Paragraph::new(header).block(panel("", ascii)), rows[0]);
        let unclaimed = if snapshot["myRewardsHistoryComplete"] == true {
            amount(&snapshot["myUnclaimedBaseUnits"], 8)
        } else {
            "N/A (syncing)".into()
        };
        let metrics = if compact { 2 } else { 4 };
        let cells = Layout::default()
            .direction(if compact {
                Direction::Vertical
            } else {
                Direction::Horizontal
            })
            .constraints(vec![Constraint::Ratio(1, metrics); metrics as usize])
            .split(rows[1]);
        if compact {
            frame.render_widget(
                Paragraph::new(format!(
                    " VEX {}   ZORU {}",
                    amount(&snapshot["nativeBalanceWei"], 18),
                    amount(&snapshot["tokenBalanceBaseUnits"], 8)
                ))
                .style(Style::default().fg(GOLD)),
                cells[0],
            );
            frame.render_widget(
                Paragraph::new(format!(
                    " Unclaimed {unclaimed}   {}",
                    hashrate(state["hashesPerSecond"].as_f64().unwrap_or(0.0))
                ))
                .style(Style::default().fg(GREEN)),
                cells[1],
            );
        } else {
            for (index, (label, value, color)) in [
                (
                    " VEX BALANCE",
                    format!("{} VEX", amount(&snapshot["nativeBalanceWei"], 18)),
                    Color::White,
                ),
                (
                    " ZORU BALANCE",
                    format!("{} ZORU", amount(&snapshot["tokenBalanceBaseUnits"], 8)),
                    GOLD,
                ),
                (" UNCLAIMED", format!("{unclaimed} ZORU"), GOLD),
                (
                    " HASHRATE",
                    hashrate(state["hashesPerSecond"].as_f64().unwrap_or(0.0)),
                    GREEN,
                ),
            ]
            .into_iter()
            .enumerate()
            {
                frame.render_widget(
                    Paragraph::new(vec![
                        Line::styled(label, Style::default().fg(MUTED)),
                        Line::styled(
                            format!(" {value}"),
                            Style::default().fg(color).add_modifier(Modifier::BOLD),
                        ),
                    ])
                    .block(panel("", ascii)),
                    cells[index],
                );
            }
        }
        let tx = &state["transactionPipeline"];
        let uptime = state["elapsedSeconds"].as_f64().unwrap_or(0.0).max(0.0) as u64;
        if compact {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(format!(
                        " Epoch {}  Workers {}  CPU {}",
                        display(&snapshot["currentEpoch"]),
                        display(&state["workers"]),
                        display(&state["cpuBudgetCores"])
                    )),
                    Line::from(format!(
                        " Accepted {}  Rejected {}  Queue {}",
                        display(&state["accepted"]),
                        display(&state["rejected"]),
                        display(&state["candidateQueueDepth"])
                    )),
                ])
                .block(panel(" ENGINE / CONTRACT", ascii)),
                rows[2],
            );
        } else {
            let columns =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(rows[2]);
            let remaining = snapshot["epochEndsAt"]
                .as_u64()
                .zip(
                    snapshot["blockTimestamp"]
                        .as_str()
                        .and_then(|v| v.parse::<u64>().ok()),
                )
                .map(|(end, time)| {
                    format!(
                        "{}s",
                        end.saturating_sub(
                            time + self.inspected.map_or(0, |t| t.elapsed().as_secs())
                        )
                    )
                })
                .unwrap_or_else(|| "N/A".into());
            let global = vec![
                row("Epoch", display(&snapshot["currentEpoch"])),
                row(
                    "Target",
                    snapshot["target"]
                        .as_str()
                        .and_then(|v| zoru_miner::abi::quantity(v).ok())
                        .map(|v| short(&format!("{v:#066x}"), ascii))
                        .unwrap_or_else(|| "N/A".into()),
                ),
                row("Epoch proofs", display(&snapshot["currentEpochProofs"])),
                row(
                    "Epoch budget",
                    format!(
                        "{} ZORU",
                        amount(&snapshot["currentEpochBudgetBaseUnits"], 8)
                    ),
                ),
                row("Epoch ends", remaining),
                row("Generation", display(&snapshot["generation"])),
                row("Seed", short(text(&snapshot["seed"]), ascii)),
                row("Total proofs", display(&snapshot["totalAcceptedProofs"])),
                row(
                    "Snapshot age",
                    format!("{}s", self.inspected.map_or(0, |t| t.elapsed().as_secs())),
                ),
            ];
            let engine = vec![
                row("Status", phase.into()),
                row(
                    "Workers / CPU",
                    format!(
                        "{} / {} cores",
                        display(&state["workers"]),
                        display(&state["cpuBudgetCores"])
                    ),
                ),
                row(
                    "Accepted / rejected",
                    format!(
                        "{} / {}",
                        display(&state["accepted"]),
                        display(&state["rejected"])
                    ),
                ),
                row(
                    "Candidate queue",
                    format!("{} / 64", display(&state["candidateQueueDepth"])),
                ),
                row("Pending TX", display(&state["pendingTransactions"])),
                row("TX pipeline", display(&tx["status"])),
                row("Auto-claim", display(&state["autoClaimStatus"])),
                row(
                    "Stale / overflow",
                    format!(
                        "{} / {}",
                        display(&state["staleBeforeBroadcast"]),
                        display(&state["candidateQueueOverflow"])
                    ),
                ),
                row(
                    "Uptime",
                    format!(
                        "{:02}:{:02}:{:02}",
                        uptime / 3600,
                        uptime / 60 % 60,
                        uptime % 60
                    ),
                ),
            ];
            frame.render_widget(
                Paragraph::new(global).block(panel(" GLOBAL CONTRACT", ascii)),
                columns[0],
            );
            frame.render_widget(
                Paragraph::new(engine).block(panel(" MINING ENGINE", ascii)),
                columns[1],
            );
        }
        self.draw_events(frame, rows[3], ascii);
        let controls = if self.state["status"] == "stopping" {
            " Stopping safely; signed journals are retained"
        } else if area.width < 65 {
            " Q Quit  Arrows/Pg  L Logs  R Refresh"
        } else {
            " Q Quit  Up/Down Scroll  PgUp/PgDn  L Logs  R Refresh view | UTC"
        };
        frame.render_widget(
            Paragraph::new(controls).style(Style::default().fg(MUTED)),
            rows[4],
        );
    }
    fn draw_events(&self, frame: &mut Frame<'_>, area: Rect, ascii: bool) {
        let visible = area.height.saturating_sub(2) as usize;
        let end = self.events.len().saturating_sub(self.scroll);
        let lines: Vec<Line<'_>> = self
            .events
            .iter()
            .take(end)
            .skip(end.saturating_sub(visible))
            .map(|e| {
                let color = match e.category.as_str() {
                    "HASHING" | "FOUND" | "ACCEPTED" => GREEN,
                    "CLAIM" => GOLD,
                    "WARNING" => Color::Yellow,
                    "ERROR" | "REJECTED" => Color::Red,
                    _ => CYAN,
                };
                Line::from(vec![
                    Span::styled(format!(" {}  ", e.time), Style::default().fg(MUTED)),
                    Span::styled(format!("{:<9} ", e.category), Style::default().fg(color)),
                    Span::raw(if ascii {
                        e.message.replace('·', "|").replace('…', "...")
                    } else {
                        e.message.clone()
                    }),
                ])
            })
            .collect();
        frame.render_widget(
            Paragraph::new(lines).block(
                panel(
                    if self.logs {
                        " LIVE MINING EVENTS / SCROLL"
                    } else {
                        " LIVE MINING EVENTS"
                    },
                    ascii,
                )
                .border_style(Style::default().fg(if self.logs {
                    CYAN
                } else {
                    MUTED
                })),
            ),
            area,
        );
    }
}

fn panel(title: &str, ascii: bool) -> Block<'_> {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(MUTED))
        .title(Span::styled(title, Style::default().fg(CYAN)));
    if ascii {
        block.border_set(ratatui::symbols::border::Set {
            top_left: "+",
            top_right: "+",
            bottom_left: "+",
            bottom_right: "+",
            vertical_left: "|",
            vertical_right: "|",
            horizontal_top: "-",
            horizontal_bottom: "-",
        })
    } else {
        block
    }
}
fn row(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {label:<18}"), Style::default().fg(MUTED)),
        Span::raw(value),
    ])
}
fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("N/A")
}
fn display(value: &Value) -> String {
    if value.is_null() {
        "N/A".into()
    } else if let Some(s) = value.as_str() {
        s.into()
    } else {
        value.to_string()
    }
}
fn short(value: &str, ascii: bool) -> String {
    let chars: Vec<_> = value.chars().collect();
    if chars.len() <= 22 {
        return value.into();
    }
    format!(
        "{}{}{}",
        chars[..12].iter().collect::<String>(),
        if ascii { "..." } else { "…" },
        chars[chars.len() - 6..].iter().collect::<String>()
    )
}
fn amount(value: &Value, decimals: usize) -> String {
    let Some(value) = value
        .as_str()
        .filter(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
    else {
        return "N/A".into();
    };
    let padded = format!("{value:0>width$}", width = decimals + 1);
    let (whole, fraction) = padded.split_at(padded.len() - decimals);
    let grouped: String = whole
        .chars()
        .enumerate()
        .flat_map(|(i, c)| {
            if i > 0 && (whole.len() - i).is_multiple_of(3) {
                vec![',', c]
            } else {
                vec![c]
            }
        })
        .collect();
    format!("{grouped}.{}", &fraction[..4.min(decimals)])
}
fn hashrate(rate: f64) -> String {
    if rate >= 1e6 {
        format!("{:.2} MH/s", rate / 1e6)
    } else if rate >= 1e3 {
        format!("{:.2} kH/s", rate / 1e3)
    } else {
        format!("{rate:.0} H/s")
    }
}

pub fn run(
    session: &Session,
    mining: &thread::JoinHandle<Result<Value>>,
    view: Arc<Mutex<View>>,
    ascii: bool,
) -> Result<()> {
    let mut terminal = ratatui::try_init()?;
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            ratatui::restore();
        }
    }
    let _restore = Restore;
    // try_init installs Ratatui's panic restoration hook. The guard covers
    // normal return and rendering/input errors.
    (|| -> Result<()> {
        while !mining.is_finished() {
            terminal.draw(|frame| {
                let mut view = view.lock().unwrap();
                if session.stop.load(Ordering::Relaxed) {
                    view.state["status"] = Value::String("stopping".into());
                }
                view.draw(frame, ascii);
            })?;
            if event::poll(Duration::from_millis(200))?
                && let Event::Key(key) = event::read()?
                && key.kind != KeyEventKind::Release
            {
                if matches!(key.code, KeyCode::Char('q' | 'Q'))
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL))
                {
                    session.stop.store(true, Ordering::SeqCst);
                    view.lock().unwrap().state["status"] = Value::String("stopping".into());
                } else {
                    view.lock().unwrap().key(key.code);
                }
            }
        }
        Ok(())
    })()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    use serde_json::json;

    #[test]
    fn output_detection_requires_two_ttys_and_respects_plain_and_dumb() {
        assert!(enabled(false, true, true, Some("xterm")));
        for (plain, input, output, term) in [
            (true, true, true, None),
            (false, false, true, None),
            (false, true, false, None),
            (false, true, true, Some("dumb")),
        ] {
            assert!(!enabled(plain, input, output, term));
        }
    }
    #[test]
    fn resize_keeps_header_fixed_and_event_memory_bounded() {
        let mut view = View::default();
        view.receive(json!({"event":"state","data":{"status":"mining","chainId":6736}}));
        for i in 0..2000 {
            view.push("FOUND", &format!("candidate {i}"));
        }
        assert_eq!(view.events.len(), LIMIT);
        let mut terminal = Terminal::new(TestBackend::new(110, 35)).unwrap();
        for (w, h) in [
            (110, 35),
            (80, 27),
            (80, 24),
            (42, 16),
            (10, 4),
            (1, 1),
            (120, 40),
        ] {
            terminal.backend_mut().resize(w, h);
            terminal.resize(Rect::new(0, 0, w, h)).unwrap();
            terminal.draw(|f| view.draw(f, true)).unwrap();
            view.key(KeyCode::PageUp);
            terminal.draw(|f| view.draw(f, false)).unwrap();
            if w > 40 && h > 16 {
                let line: String = (0..w)
                    .map(|x| terminal.backend().buffer()[(x, 1)].symbol())
                    .collect();
                assert!(line.contains("ZORU MINER"));
            }
        }
        assert!(view.scroll > 0);
        view.key(KeyCode::Char('r'));
        assert_eq!(view.scroll, 0);
    }
    #[test]
    fn amounts_keep_precision_and_missing_data_is_unavailable() {
        assert_eq!(amount(&json!("125000000000"), 8), "1,250.0000");
        assert_eq!(amount(&json!("12450000000000000000"), 18), "12.4500");
        assert_eq!(amount(&json!("1"), 8), "0.0000");
        assert_eq!(amount(&Value::Null, 8), "N/A");
    }
}
