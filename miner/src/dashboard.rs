use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Sparkline, Table, Wrap},
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

const BG: Color = Color::Rgb(10, 15, 25);
const SURFACE: Color = Color::Rgb(16, 23, 36);
const EDGE: Color = Color::Rgb(41, 57, 78);
const INK: Color = Color::Rgb(228, 236, 247);
const RED: Color = Color::Rgb(255, 113, 133);
const CYAN: Color = Color::Rgb(60, 210, 230);
const MUTED: Color = Color::Rgb(135, 151, 173);
const GREEN: Color = Color::Rgb(97, 220, 145);
const GOLD: Color = Color::Rgb(239, 198, 101);
const LIMIT: usize = 512;

struct MiningEvent {
    time: String,
    category: String,
    message: String,
    repeats: u64,
}

#[derive(Default)]
pub struct View {
    state: Value,
    inspection: Value,
    inspected: Option<Instant>,
    events: VecDeque<MiningEvent>,
    scroll: usize,
    logs: bool,
    rates: VecDeque<u64>,
    sampled: Option<Instant>,
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
            "state" | "mine-result" => {
                self.state = value["data"].clone();
                if self
                    .sampled
                    .is_none_or(|t| t.elapsed() >= Duration::from_secs(1))
                    && let Some(rate) = self.state["hashesPerSecond"].as_f64()
                {
                    self.rates.push_back(rate.max(0.0) as u64);
                    if self.rates.len() > 60 {
                        self.rates.pop_front();
                    }
                    self.sampled = Some(Instant::now());
                }
            }
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
        // These transitions already have live indicators in the header and queue.
        if matches!(category, "HASHING" | "QUEUED")
            || (category == "WARNING" && message == "waiting-beacon-or-recovery")
        {
            return;
        }
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
        if matches!(category, "WARNING" | "ERROR")
            && let Some(event) = self
                .events
                .iter_mut()
                .rev()
                .find(|e| e.category == category && e.message == message)
        {
            event.repeats = event.repeats.saturating_add(1);
            return;
        }
        // Exact consecutive copies of other events need only one row.
        if self
            .events
            .back()
            .is_some_and(|e| e.category == category && e.message == message)
        {
            return;
        }
        if self.scroll > 0 && (self.logs || category != "FOUND") {
            self.scroll = (self.scroll + 1).min(LIMIT - 1);
        }
        self.events.push_back(MiningEvent {
            time: chrono::DateTime::<chrono::Utc>::from(SystemTime::now())
                .format("%H:%M:%S")
                .to_string(),
            category: category.to_owned(),
            message,
            repeats: 1,
        });
        if self.events.len() > LIMIT {
            self.events.pop_front();
        }
    }
    fn visible_event_count(&self) -> usize {
        self.events
            .iter()
            .filter(|e| self.logs || e.category != "FOUND")
            .count()
    }
    fn key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Up => {
                self.scroll = (self.scroll + 1).min(self.visible_event_count().saturating_sub(1))
            }
            KeyCode::Down => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::PageUp => {
                self.scroll = (self.scroll + 10).min(self.visible_event_count().saturating_sub(1))
            }
            KeyCode::PageDown => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::Char('l' | 'L') => {
                self.logs = !self.logs;
                self.scroll = 0;
            }
            KeyCode::Char('r' | 'R') => self.scroll = 0,
            _ => {}
        }
    }
    fn draw(&self, frame: &mut Frame<'_>, ascii: bool) {
        let area = frame.area();
        frame.render_widget(
            Block::default().style(Style::default().bg(BG).fg(INK)),
            area,
        );
        let wide = area.width >= 100;
        let compact = area.height < 28;
        let header_height = if compact { 4 } else { 5 };
        let metrics_height = if compact {
            4
        } else if wide {
            8
        } else {
            11
        };
        let details_height = if compact {
            5
        } else if wide {
            11
        } else {
            7
        };
        let available = area
            .height
            .saturating_sub(header_height + metrics_height + details_height + 1);
        let activity_height = if self.logs {
            available
        } else {
            available.min(14)
        };
        let rows = Layout::vertical([
            Constraint::Length(header_height),
            Constraint::Length(metrics_height),
            Constraint::Length(details_height),
            Constraint::Length(activity_height),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);
        let s = &self.state;
        let p = &self.inspection;
        let (phase, color) = match text(&s["status"]) {
            "mining" => ("HASHING", GREEN),
            "paused" | "paused-resource" => ("PAUSED", GOLD),
            "rpc-degraded" => ("RPC DEGRADED", RED),
            "stopped" => ("STOPPED", MUTED),
            "stopping" => ("STOPPING", GOLD),
            _ => ("WAITING FOR WORK", GOLD),
        };
        let (rpc, rpc_color) = if s["rpcHealthy"] == false {
            ("RPC DEGRADED", RED)
        } else if s["rpcHealthy"] == true || self.inspected.is_some() {
            ("RPC ONLINE", GREEN)
        } else {
            ("RPC CONNECTING", MUTED)
        };
        let elapsed = s["elapsedSeconds"].as_f64().unwrap_or(0.0).max(0.0) as u64;
        let uptime = format!(
            "{:02}:{:02}:{:02}",
            elapsed / 3600,
            elapsed / 60 % 60,
            elapsed % 60
        );
        let network = if s["deploymentMode"] == "local" {
            "ISOLATED"
        } else {
            "MAINNET"
        };
        let mut header = vec![
            Line::from(vec![
                Span::styled(" ZORU MINER ", Style::default().fg(CYAN).bold()),
                Span::styled(
                    format!(" {network} / Chain {}", display(&s["chainId"])),
                    Style::default().fg(MUTED),
                ),
            ]),
            Line::from(vec![
                Span::raw(" "),
                badge(phase, color),
                Span::raw("  "),
                badge(rpc, rpc_color),
            ]),
        ];
        if !compact {
            header.push(Line::from(vec![Span::styled(
                format!(" {}", short(text(&s["address"]), ascii)),
                Style::default().fg(INK),
            )]));
        }
        frame.render_widget(
            Paragraph::new(header).block(panel("", ascii).border_style(Style::default().fg(CYAN))),
            rows[0],
        );
        let unclaimed = if p["myRewardsHistoryComplete"] == true {
            amount(&p["myUnclaimedBaseUnits"], 8)
        } else {
            "Syncing".into()
        };
        let chart_height = if compact { 0 } else { 5 };
        let card_area = Rect {
            height: rows[1].height.saturating_sub(chart_height),
            ..rows[1]
        };
        let strips = Layout::vertical(if wide {
            vec![Constraint::Percentage(100)]
        } else {
            vec![Constraint::Percentage(50); 2]
        })
        .split(card_area);
        let count = if wide { 4 } else { 2 };
        let mut cards = Vec::new();
        for strip in strips.iter() {
            cards.extend(
                Layout::horizontal(vec![Constraint::Ratio(1, count); count as usize])
                    .split(*strip)
                    .iter()
                    .copied(),
            );
        }
        for (i, (label, value, color)) in [
            (" VEX / GAS ", amount(&p["nativeBalanceWei"], 18), INK),
            (
                " ZORU / WALLET ",
                amount(&p["tokenBalanceBaseUnits"], 8),
                GOLD,
            ),
            (" UNCLAIMED / ZORU ", unclaimed, GOLD),
            (
                if compact {
                    " HASHRATE "
                } else {
                    " LOCAL HASHES "
                },
                if compact {
                    hashrate(s["hashesPerSecond"].as_f64().unwrap_or(0.0))
                } else {
                    display(&s["hashes"])
                },
                CYAN,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            if compact && !wide {
                frame.render_widget(
                    Paragraph::new(vec![
                        Line::styled(label, Style::default().fg(MUTED)),
                        Line::styled(format!(" {value}"), Style::default().fg(color).bold()),
                    ])
                    .style(Style::default().bg(SURFACE)),
                    cards[i],
                );
                continue;
            }
            let block = panel(label, ascii);
            let inner = block.inner(cards[i]);
            frame.render_widget(block, cards[i]);
            frame.render_widget(
                Paragraph::new(vec![Line::styled(
                    format!(" {value}"),
                    Style::default().fg(color).bold(),
                )]),
                inner,
            );
        }
        if !compact && rows[1].height >= 5 {
            let chart = Rect::new(
                rows[1].x,
                rows[1].bottom().saturating_sub(5),
                rows[1].width,
                5,
            );
            let block = panel(" HASHRATE / HISTORY ", ascii);
            let inner = block.inner(chart);
            frame.render_widget(block, chart);
            let data: Vec<u64> = self
                .rates
                .iter()
                .skip(
                    self.rates
                        .len()
                        .saturating_sub(inner.width.saturating_sub(2) as usize),
                )
                .copied()
                .collect();
            let peak = data.iter().copied().max().unwrap_or(0);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        format!(
                            " {}",
                            hashrate(s["hashesPerSecond"].as_f64().unwrap_or(0.0))
                        ),
                        Style::default().fg(CYAN).bold(),
                    ),
                    Span::styled(
                        format!("   PEAK {}", hashrate(peak as f64)),
                        Style::default().fg(MUTED),
                    ),
                ])),
                Rect::new(inner.x, inner.y, inner.width, 1),
            );
            let plot = Rect::new(
                inner.right().saturating_sub(1 + data.len() as u16),
                inner.y + 1,
                data.len() as u16,
                inner.height.saturating_sub(1),
            );
            let mut graph = Sparkline::default()
                .data(&data)
                .max(peak.max(1))
                .style(Style::default().fg(CYAN));
            if ascii {
                graph = graph.bar_set(ratatui::symbols::bar::Set {
                    full: "#",
                    seven_eighths: "#",
                    three_quarters: "*",
                    five_eighths: "*",
                    half: "+",
                    three_eighths: "=",
                    one_quarter: ":",
                    one_eighth: ".",
                    empty: " ",
                });
            }
            frame.render_widget(graph, plot);
        }
        let remaining = p["epochEndsAt"]
            .as_u64()
            .zip(
                p["blockTimestamp"]
                    .as_str()
                    .and_then(|v| v.parse::<u64>().ok()),
            )
            .map(|(end, time)| {
                format!(
                    "{}s",
                    end.saturating_sub(time + self.inspected.map_or(0, |t| t.elapsed().as_secs()))
                )
            })
            .unwrap_or("N/A".into());
        if wide {
            let cols = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[2]);
            details(
                frame,
                cols[0],
                " NETWORK / EPOCH ",
                vec![
                    ("Epoch", display(&p["currentEpoch"]), CYAN),
                    (
                        "Target",
                        p["target"]
                            .as_str()
                            .and_then(|v| zoru_miner::abi::quantity(v).ok())
                            .map(|v| short(&format!("{v:#066x}"), ascii))
                            .unwrap_or("N/A".into()),
                        MUTED,
                    ),
                    ("Epoch proofs", display(&p["currentEpochProofs"]), INK),
                    (
                        "Epoch budget",
                        format!("{} ZORU", amount(&p["currentEpochBudgetBaseUnits"], 8)),
                        GOLD,
                    ),
                    ("Time remaining", remaining, INK),
                    ("Generation", display(&p["generation"]), INK),
                    ("Seed", short(text(&p["seed"]), ascii), MUTED),
                    ("Total proofs", display(&p["totalAcceptedProofs"]), INK),
                    (
                        "Snapshot age",
                        self.inspected
                            .map_or("N/A".into(), |t| format!("{}s", t.elapsed().as_secs())),
                        MUTED,
                    ),
                ],
                ascii,
            );
            details(
                frame,
                cols[1],
                " SESSION / ENGINE ",
                vec![
                    ("Accepted", display(&s["accepted"]), GREEN),
                    (
                        "Rejected",
                        display(&s["rejected"]),
                        if s["rejected"].as_u64().unwrap_or(0) > 0 {
                            RED
                        } else {
                            MUTED
                        },
                    ),
                    (
                        "Workers / CPU",
                        format!(
                            "{} / {} cores",
                            display(&s["workers"]),
                            display(&s["cpuBudgetCores"])
                        ),
                        INK,
                    ),
                    (
                        "Candidate queue",
                        format!("{} / 64", display(&s["candidateQueueDepth"])),
                        CYAN,
                    ),
                    ("Pending TX", display(&s["pendingTransactions"]), GOLD),
                    ("Uptime", uptime.clone(), INK),
                    (
                        "TX pipeline",
                        display(&s["transactionPipeline"]["status"]),
                        INK,
                    ),
                    (
                        "Auto-claim",
                        format!(
                            "{} / {}",
                            display(&s["autoClaimStatus"]),
                            display(&s["autoClaimAfter"])
                        ),
                        MUTED,
                    ),
                    (
                        "Stale / overflow",
                        format!(
                            "{} / {}",
                            display(&s["staleBeforeBroadcast"]),
                            display(&s["candidateQueueOverflow"])
                        ),
                        MUTED,
                    ),
                ],
                ascii,
            );
        } else {
            let block = panel(" SESSION / NETWORK ", ascii);
            let inner = block.inner(rows[2]);
            frame.render_widget(block, rows[2]);
            let cols = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(inner);
            render_table(
                frame,
                cols[0],
                vec![
                    ("Accepted", display(&s["accepted"]), GREEN),
                    (
                        "Rejected",
                        display(&s["rejected"]),
                        if s["rejected"].as_u64().unwrap_or(0) > 0 {
                            RED
                        } else {
                            MUTED
                        },
                    ),
                    ("Workers", display(&s["workers"]), INK),
                    ("CPU cores", display(&s["cpuBudgetCores"]), INK),
                    ("Pending TX", display(&s["pendingTransactions"]), GOLD),
                ],
                11,
            );
            render_table(
                frame,
                cols[1],
                vec![
                    ("Epoch", display(&p["currentEpoch"]), CYAN),
                    ("Ends in", remaining, INK),
                    (
                        "Queue",
                        format!("{}/64", display(&s["candidateQueueDepth"])),
                        CYAN,
                    ),
                    ("Uptime", uptime, INK),
                    ("Auto-claim", display(&s["autoClaimAfter"]), GOLD),
                ],
                11,
            );
        }
        self.draw_events(frame, rows[3], ascii);
        let controls = if s["status"] == "stopping" {
            Line::styled(
                " Stopping safely; retaining signing journals",
                Style::default().fg(GOLD),
            )
        } else {
            Line::from(vec![
                badge("Q", CYAN),
                Span::styled(" Quit  ", Style::default().fg(MUTED)),
                badge(if ascii { "UD" } else { "↑↓" }, MUTED),
                Span::styled(" Scroll  ", Style::default().fg(MUTED)),
                badge("L", MUTED),
                Span::styled(" Detail  ", Style::default().fg(MUTED)),
                badge("R", MUTED),
                Span::styled(" Live  / UTC", Style::default().fg(MUTED)),
            ])
        };
        frame.render_widget(Paragraph::new(controls), rows[5]);
    }
    fn draw_events(&self, frame: &mut Frame<'_>, area: Rect, ascii: bool) {
        let visible = area.height.saturating_sub(2) as usize;
        let events: Vec<_> = self
            .events
            .iter()
            .filter(|e| self.logs || e.category != "FOUND")
            .collect();
        let end = events.len().saturating_sub(self.scroll);
        let mut lines: Vec<Line<'_>> = events
            .iter()
            .take(end)
            .skip(end.saturating_sub(visible))
            .map(|e| {
                let message = match e.category.as_str() {
                    "FOUND" => e
                        .message
                        .strip_prefix("Valid local candidate · nonce ")
                        .map(|nonce| format!("Nonce {}", short(nonce, ascii))),
                    "EPOCH" => e
                        .message
                        .strip_prefix("Work activated · epoch ")
                        .map(str::to_owned),
                    "ACCEPTED" => e
                        .message
                        .strip_prefix("Proof confirmed in block ")
                        .map(|block| format!("Block {block}")),
                    _ => None,
                }
                .unwrap_or_else(|| e.message.clone());
                let color = match e.category.as_str() {
                    "HASHING" | "FOUND" | "ACCEPTED" => GREEN,
                    "CLAIM" => GOLD,
                    "WARNING" => GOLD,
                    "ERROR" | "REJECTED" => RED,
                    _ => CYAN,
                };
                Line::from(vec![
                    Span::styled(
                        format!(
                            " {} ",
                            if area.width < 65 {
                                &e.time[3..]
                            } else {
                                &e.time
                            }
                        ),
                        Style::default().fg(MUTED),
                    ),
                    badge(&format!("{:<8}", e.category), color),
                    Span::styled(
                        if e.repeats > 1 {
                            format!(" x{} ", e.repeats)
                        } else {
                            " ".into()
                        },
                        Style::default().fg(GOLD),
                    ),
                    Span::raw(if ascii {
                        message.replace('·', "|").replace('…', "...")
                    } else {
                        message
                    }),
                ])
            })
            .collect();
        if lines.is_empty() {
            lines.push(Line::styled(
                " Waiting for mining events...",
                Style::default().fg(MUTED),
            ));
        }
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        let overflow = paragraph
            .line_count(area.width.saturating_sub(2))
            .saturating_sub(visible)
            .min(u16::MAX as usize) as u16;
        frame.render_widget(
            paragraph.scroll((overflow, 0)).block(
                panel(
                    if self.logs {
                        " ACTIVITY / DETAIL "
                    } else {
                        " TRANSACTIONS / NETWORK "
                    },
                    ascii,
                )
                .border_style(Style::default().fg(if self.logs { CYAN } else { EDGE }))
                .title_bottom(Line::styled(
                    if self.scroll == 0 {
                        " LIVE ".to_owned()
                    } else {
                        format!(" {} events back / R live ", self.scroll)
                    },
                    Style::default().fg(if self.scroll == 0 { GREEN } else { GOLD }),
                )),
            ),
            area,
        );
    }
}

fn panel(title: &str, ascii: bool) -> Block<'_> {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(SURFACE).fg(INK))
        .border_style(Style::default().fg(EDGE))
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
fn badge(label: &str, color: Color) -> Span<'static> {
    Span::styled(
        format!(" {label} "),
        Style::default().fg(BG).bg(color).bold(),
    )
}
fn render_table(
    frame: &mut Frame<'_>,
    area: Rect,
    values: Vec<(&str, String, Color)>,
    label_width: u16,
) {
    let rows = values.into_iter().map(|(label, value, color)| {
        Row::new(vec![
            Cell::from(format!(" {label}")).style(Style::default().fg(MUTED)),
            Cell::from(value).style(Style::default().fg(color).bold()),
        ])
    });
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(label_width.min(area.width / 2 + 1)),
                Constraint::Min(1),
            ],
        )
        .column_spacing(1),
        area,
    );
}
fn details(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    values: Vec<(&str, String, Color)>,
    ascii: bool,
) {
    let block = panel(title, ascii);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    render_table(frame, inner, values, 18);
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
        view.logs = true;
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
                assert!(line.contains("Chain 6736"));
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

    #[test]
    fn phone_and_desktop_keep_key_metrics_visible_and_colored() {
        let mut view = View::default();
        view.receive(json!({"event":"state","data":{
            "status":"mining", "chainId":6736, "rpcHealthy":true,
            "deploymentMode":"mainnet", "address":"0x1234567890123456789012345678901234567890",
            "hashesPerSecond":517493.0, "accepted":27, "rejected":0,
            "workers":1, "cpuBudgetCores":0.5, "candidateQueueDepth":4,
            "pendingTransactions":1, "elapsedSeconds":3601, "autoClaimAfter":50
        }}));
        view.receive(json!({"event":"inspection","data":{
            "nativeBalanceWei":"499856800000000000000", "tokenBalanceBaseUnits":"1250000000",
            "myRewardsHistoryComplete":true, "myUnclaimedBaseUnits":"250000000",
            "currentEpoch":"13921"
        }}));
        view.push("ACCEPTED", "Proof confirmed in block 11957934");
        for (width, height) in [(66, 32), (66, 63), (42, 32), (80, 24), (120, 35)] {
            for ascii in [false, true] {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal.draw(|f| view.draw(f, ascii)).unwrap();
                let buffer = terminal.backend().buffer();
                let screen = (0..height)
                    .map(|y| {
                        (0..width)
                            .map(|x| buffer[(x, y)].symbol())
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                for expected in [
                    "Chain 6736",
                    "HASHING",
                    "RPC ONLINE",
                    "499.8568",
                    "12.5000",
                    "2.5000",
                    "517.49 kH/s",
                    "Accepted",
                    "13921",
                ] {
                    assert!(
                        screen.contains(expected),
                        "{width}x{height} missing {expected}:\n{screen}"
                    );
                }
                assert!(!screen.contains("VEX available"));
                assert!(!screen.contains("ZORU balance"));
                if height >= 28 {
                    assert_eq!(
                        screen.matches("01:00:01").count(),
                        1,
                        "Uptime must appear once: {screen}"
                    );
                }
                assert!(buffer.content().iter().any(|c| c.bg == GREEN && c.fg == BG));
                assert!(buffer.content().iter().any(|c| c.fg == CYAN));
                if ascii {
                    assert!(screen.is_ascii(), "ASCII mode contains Unicode: {screen}");
                }
                if !ascii && width == 66 && height >= 32 {
                    for y in [13, 14] {
                        assert!(
                            (1..65).any(|x| buffer[(x, y)].symbol() == "█"),
                            "Graph needs two visible rows"
                        );
                    }
                    if height == 63 {
                        assert_eq!(buffer[(0, 50)].symbol(), " ");
                    }
                    let graph: String = (1..65).map(|x| buffer[(x, 13)].symbol()).collect();
                    assert!(
                        !graph.chars().any(|c| c.is_ascii_alphabetic()),
                        "Graph must not retain caption text: {graph}"
                    );
                }
            }
        }
    }

    #[test]
    fn rate_history_is_bounded_and_ignores_missing_measurements() {
        let mut view = View::default();
        for rate in 0..100 {
            view.sampled = None;
            view.receive(json!({"event":"state","data":{"hashesPerSecond":rate}}));
        }
        assert_eq!(view.rates.len(), 60);
        assert_eq!(view.rates.front(), Some(&40));
        view.sampled = None;
        view.receive(json!({"event":"state","data":{}}));
        assert_eq!(view.rates.len(), 60);
    }

    #[test]
    fn activity_coalesces_repeated_warnings_but_keeps_distinct_receipts() {
        let mut view = View::default();
        view.push("HASHING", "mining");
        view.push("QUEUED", "Candidate awaiting the transaction writer");
        view.push("WARNING", "waiting-beacon-or-recovery");
        assert!(view.events.is_empty());
        view.push("WARNING", "Submission response: RPC timeout");
        view.push("ACCEPTED", "Proof confirmed in block 100");
        view.push("WARNING", "Submission response: RPC timeout");
        view.push("ACCEPTED", "Proof confirmed in block 101");
        assert_eq!(view.events.len(), 3);
        assert_eq!(view.events[0].repeats, 2);
        assert_eq!(view.events[1].repeats, 1);
        assert_eq!(view.events[2].message, "Proof confirmed in block 101");
    }

    #[test]
    fn wrapped_rpc_detail_and_latest_receipt_remain_visible() {
        let mut view = View::default();
        view.push("FOUND", "Valid local candidate · nonce 0x1234");
        view.push("WARNING", "Submission response: eth_sendRawTransaction: account vex.evm has insufficient ram; needs 5995936 bytes has 5995719 bytes");
        view.push("ACCEPTED", "Proof confirmed in block 11957934");
        assert_eq!(view.visible_event_count(), 2);
        let mut terminal = Terminal::new(TestBackend::new(66, 8)).unwrap();
        terminal
            .draw(|f| view.draw_events(f, f.area(), true))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let screen = (0..8)
            .map(|y| (0..66).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            screen.contains("5995719"),
            "RPC reason was clipped: {screen}"
        );
        assert!(screen.contains("Block 11957934"));
        assert!(!screen.contains("Nonce"));
        view.key(KeyCode::Char('l'));
        assert_eq!(view.visible_event_count(), 3);
        assert_eq!(view.scroll, 0);
    }
}
