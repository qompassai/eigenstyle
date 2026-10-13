//! eigenstyle-tui — ratatui terminal UI over eigenstyle-core.
//!
//! Screens: Corpus (browse, include/exclude), Add (file path or paste,
//! explicit genre + authorship), Fingerprint (headline + per-feature
//! table, user corrections, recompute with dry-run preview before
//! save), Check (draft file → verdict + top features + integrity),
//! Profile (readable profile viewer).
//!
//! The app state machine (`App::handle_key`) is terminal-independent
//! and unit-tested; `eigenstyle-tui --snapshot <screen>` renders one frame
//! to stdout without a TTY for headless smoke checks.

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use eigenstyle_core::corpus::{Authorship, NewSample};
use eigenstyle_core::fingerprint::FingerprintDoc;
use eigenstyle_core::{Store, check_draft};

/// Screens in tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Corpus browser.
    Corpus,
    /// Add-writing form.
    Add,
    /// Fingerprint table + corrections + recompute.
    Fingerprint,
    /// Draft checker.
    Check,
    /// Readable profile viewer.
    Profile,
}

impl Screen {
    fn all() -> [Screen; 5] {
        [
            Screen::Corpus,
            Screen::Add,
            Screen::Fingerprint,
            Screen::Check,
            Screen::Profile,
        ]
    }

    fn title(&self) -> &'static str {
        match self {
            Screen::Corpus => "Corpus",
            Screen::Add => "Add writing",
            Screen::Fingerprint => "Fingerprint",
            Screen::Check => "Check draft",
            Screen::Profile => "Profile",
        }
    }

    fn parse(name: &str) -> Option<Screen> {
        match name {
            "corpus" => Some(Screen::Corpus),
            "add" => Some(Screen::Add),
            "fingerprint" => Some(Screen::Fingerprint),
            "check" => Some(Screen::Check),
            "profile" => Some(Screen::Profile),
            _ => None,
        }
    }
}

/// Add-form field order.
const ADD_FIELDS: [&str; 6] = [
    "Title",
    "Genre",
    "Authorship",
    "Source",
    "File path",
    "Text (paste)",
];

/// The whole TUI state. Rendering is a pure function of this state.
pub struct App {
    store: Store,
    screen: Screen,
    corpus_selected: usize,
    add_values: [String; 6],
    add_authorship: Authorship,
    add_focus: usize,
    feature_selected: usize,
    editing_feature: Option<String>,
    edit_buffer: String,
    check_path: String,
    check_source_path: String,
    check_focus: usize,
    check_result: Option<eigenstyle_core::checker::CheckReport>,
    profile_scroll: u16,
    pending_recompute: Option<(FingerprintDoc, String)>,
    message: String,
    should_quit: bool,
}

impl App {
    /// Open (or seed) the store at `home` and build the app.
    pub fn new(home: PathBuf) -> Result<App, String> {
        let store = Store::open(&home).map_err(|e| e.to_string())?;
        Ok(App {
            store,
            screen: Screen::Corpus,
            corpus_selected: 0,
            add_values: Default::default(),
            add_authorship: Authorship::Sole,
            add_focus: 0,
            feature_selected: 0,
            editing_feature: None,
            edit_buffer: String::new(),
            check_path: String::new(),
            check_source_path: String::new(),
            check_focus: 0,
            check_result: None,
            profile_scroll: 0,
            pending_recompute: None,
            message: "Ready. Keys: 1-5 switch screens, q quits.".to_string(),
            should_quit: false,
        })
    }

    /// Handle one key press. Returns nothing; inspect state after.
    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.editing_feature.is_some() {
            self.handle_edit_key(key);
            return;
        }
        match self.screen {
            Screen::Add => self.handle_add_key(key),
            _ => self.handle_normal_key(key),
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char(c @ '1'..='5') => {
                let idx = usize::try_from(c as u32 - '1' as u32).unwrap_or(0);
                self.screen = Screen::all()[idx];
            }
            KeyCode::Tab => {
                let idx = Screen::all()
                    .iter()
                    .position(|s| *s == self.screen)
                    .unwrap_or(0);
                self.screen = Screen::all()[(idx + 1) % 5];
            }
            _ => match self.screen {
                Screen::Corpus => self.handle_corpus_key(key),
                Screen::Fingerprint => self.handle_fingerprint_key(key),
                Screen::Check => self.handle_check_key(key),
                Screen::Profile => match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        self.profile_scroll = self.profile_scroll.saturating_add(1);
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        self.profile_scroll = self.profile_scroll.saturating_sub(1);
                    }
                    _ => {}
                },
                Screen::Add => {}
            },
        }
    }

    fn handle_corpus_key(&mut self, key: KeyEvent) {
        let count = self.store.corpus.samples().len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if count > 0 {
                    self.corpus_selected = (self.corpus_selected + 1).min(count - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.corpus_selected = self.corpus_selected.saturating_sub(1);
            }
            KeyCode::Char(' ') => {
                if let Some(sample) = self.store.corpus.samples().get(self.corpus_selected) {
                    let (id, included) = (sample.id, !sample.included);
                    match self.store.corpus.set_included(id, included) {
                        Ok(()) => {
                            let _ = self.store.save_corpus();
                            self.message = format!(
                                "Sample {id} {} the fingerprint.",
                                if included {
                                    "included in"
                                } else {
                                    "excluded from"
                                }
                            );
                        }
                        Err(e) => self.message = format!("Error: {e}"),
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_fingerprint_key(&mut self, key: KeyEvent) {
        let count = self.store.fingerprint.features.len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if count > 0 {
                    self.feature_selected = (self.feature_selected + 1).min(count - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.feature_selected = self.feature_selected.saturating_sub(1);
            }
            KeyCode::Char('r') => match self.store.recompute(true) {
                Ok((doc, deltas, old, new)) => {
                    let summary = format!(
                        "Preview: {} pieces, {} features change, bands {:.3}/{:.3} -> {:.3}/{:.3}. Press R to save, Esc to discard.",
                        doc.corpus_pieces.len(),
                        deltas.len(),
                        old.close_max,
                        old.drifting_max,
                        new.close_max,
                        new.drifting_max
                    );
                    self.pending_recompute = Some((doc, summary.clone()));
                    self.message = summary;
                }
                Err(e) => self.message = format!("Recompute preview failed: {e}"),
            },
            KeyCode::Char('R') => {
                if self.pending_recompute.take().is_some() {
                    match self.store.recompute(false) {
                        Ok((_, deltas, _, _)) => {
                            self.message = format!(
                                "Fingerprint recomputed and saved ({} features changed).",
                                deltas.len()
                            );
                        }
                        Err(e) => self.message = format!("Recompute failed: {e}"),
                    }
                } else {
                    self.message =
                        "Nothing to save — press r for a dry-run preview first.".to_string();
                }
            }
            KeyCode::Esc => {
                if self.pending_recompute.take().is_some() {
                    self.message = "Recompute preview discarded.".to_string();
                }
            }
            KeyCode::Char('e') => {
                if let Some(feature) = self.store.fingerprint.features.get(self.feature_selected) {
                    self.editing_feature = Some(feature.clone());
                    self.message =
                        format!("Type a new mean for {feature}, Enter applies, Esc cancels.");
                }
            }
            KeyCode::Char('x') => {
                self.store.fingerprint.clear_corrections();
                let _ = self.store.save_fingerprint();
                self.message =
                    "All user corrections cleared; computed values restored.".to_string();
            }
            _ => {}
        }
    }

    fn handle_edit_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.editing_feature = None;
                self.message = "Correction cancelled.".to_string();
            }
            KeyCode::Enter => {
                if let Some(feature) = self.editing_feature.take() {
                    let value = self.edit_buffer.trim().parse::<f64>();
                    self.edit_buffer.clear();
                    match value {
                        Ok(v) => {
                            match self
                                .store
                                .fingerprint
                                .set_correction(&feature, Some(v), None)
                            {
                                Ok(()) => {
                                    let _ = self.store.save_fingerprint();
                                    self.message = format!(
                                        "{feature} mean corrected to {v} (user-corrected; survives recompute)."
                                    );
                                }
                                Err(e) => self.message = format!("Correction rejected: {e}"),
                            }
                        }
                        Err(_) => self.message = "Correction rejected: not a number.".to_string(),
                    }
                }
            }
            KeyCode::Backspace => {
                self.edit_buffer.pop();
            }
            KeyCode::Char(c) if self.edit_buffer.chars().count() < 24 => {
                self.edit_buffer.push(c);
            }
            _ => {}
        }
    }

    fn handle_check_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => {
                self.check_focus = (self.check_focus + 1) % 2;
            }
            KeyCode::Backspace => {
                if self.check_focus == 0 {
                    self.check_path.pop();
                } else {
                    self.check_source_path.pop();
                }
            }
            KeyCode::Enter => self.run_check(),
            KeyCode::Char(c) => {
                let target = if self.check_focus == 0 {
                    &mut self.check_path
                } else {
                    &mut self.check_source_path
                };
                if target.chars().count() < 4096 {
                    target.push(c);
                }
            }
            _ => {}
        }
    }

    fn run_check(&mut self) {
        let draft = match std::fs::read_to_string(self.check_path.trim()) {
            Ok(text) => text,
            Err(e) => {
                self.message = format!("Cannot read draft: {e}");
                return;
            }
        };
        let source = if self.check_source_path.trim().is_empty() {
            None
        } else {
            match std::fs::read_to_string(self.check_source_path.trim()) {
                Ok(text) => Some(text),
                Err(e) => {
                    self.message = format!("Cannot read source: {e}");
                    return;
                }
            }
        };
        match check_draft(
            &self.store.fingerprint,
            &self.store.avoid_list,
            &draft,
            source.as_deref(),
        ) {
            Ok(report) => {
                self.message = format!(
                    "Verdict: {} ({} words{})",
                    report.verdict.as_str(),
                    report.words,
                    report
                        .delta
                        .map(|d| format!(", delta {d:.3}"))
                        .unwrap_or_default()
                );
                self.check_result = Some(report);
            }
            Err(e) => self.message = format!("Check failed: {e}"),
        }
    }

    fn handle_add_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            self.submit_add();
            return;
        }
        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::Corpus;
            }
            KeyCode::Tab => {
                self.add_focus = (self.add_focus + 1) % ADD_FIELDS.len();
            }
            KeyCode::BackTab => {
                self.add_focus = (self.add_focus + ADD_FIELDS.len() - 1) % ADD_FIELDS.len();
            }
            KeyCode::Backspace => {
                if self.add_focus != 2 {
                    self.add_values[self.add_focus].pop();
                }
            }
            KeyCode::Enter => {
                if self.add_focus == 4 {
                    // File path field: load the file into the paste buffer.
                    match std::fs::read_to_string(self.add_values[4].trim()) {
                        Ok(text) => {
                            self.add_values[5] = text;
                            self.message =
                                "File loaded into the text buffer. Ctrl+S saves.".to_string();
                        }
                        Err(e) => self.message = format!("Cannot read file: {e}"),
                    }
                } else if self.add_focus == 5 {
                    self.add_values[5].push('\n');
                } else {
                    self.add_focus = (self.add_focus + 1) % ADD_FIELDS.len();
                }
            }
            KeyCode::Char(c) => {
                if self.add_focus == 2 {
                    if matches!(c, ' ' | 'h' | 'l') {
                        self.add_authorship = match self.add_authorship {
                            Authorship::Sole => Authorship::Blended,
                            Authorship::Blended => Authorship::Sole,
                        };
                    }
                } else {
                    let field = &mut self.add_values[self.add_focus];
                    if field.len() < 300_000 {
                        field.push(c);
                    }
                }
            }
            _ => {}
        }
    }

    fn submit_add(&mut self) {
        let text = if !self.add_values[5].trim().is_empty() {
            self.add_values[5].clone()
        } else if !self.add_values[4].trim().is_empty() {
            match std::fs::read_to_string(self.add_values[4].trim()) {
                Ok(text) => text,
                Err(e) => {
                    self.message = format!("Cannot read file: {e}");
                    return;
                }
            }
        } else {
            self.message = "Nothing to add: paste text or give a file path.".to_string();
            return;
        };
        let new = NewSample {
            title: self.add_values[0].clone(),
            genre: self.add_values[1].clone(),
            authorship: self.add_authorship,
            source: self.add_values[3].clone(),
            text,
        };
        match self.store.corpus.add_sample(new) {
            Ok(sample) => {
                let _ = self.store.save_corpus();
                self.add_values = Default::default();
                self.corpus_selected = self.store.corpus.samples().len().saturating_sub(1);
                self.screen = Screen::Corpus;
                self.message = format!(
                    "Added sample {} ({} words). Fingerprint NOT recomputed — see the Fingerprint screen.",
                    sample.id,
                    sample.word_count()
                );
            }
            Err(e) => self.message = format!("Add rejected: {e}"),
        }
    }

    /// Render the current screen into a frame.
    pub fn render(&self, frame: &mut ratatui::Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(2),
            ])
            .split(frame.area());
        let tabs: Vec<Span> = Screen::all()
            .iter()
            .enumerate()
            .flat_map(|(i, s)| {
                let label = format!(" {}. {} ", i + 1, s.title());
                if *s == self.screen {
                    vec![Span::styled(
                        label,
                        Style::default().add_modifier(Modifier::REVERSED),
                    )]
                } else {
                    vec![Span::raw(label)]
                }
            })
            .collect();
        let header = Paragraph::new(Line::from(tabs)).block(
            Block::default().borders(Borders::ALL).title(format!(
                " eigenstyle — {} samples, {} included words ",
                self.store.corpus.samples().len(),
                self.store.corpus.included_word_count()
            )),
        );
        frame.render_widget(header, chunks[0]);
        match self.screen {
            Screen::Corpus => self.render_corpus(frame, chunks[1]),
            Screen::Add => self.render_add(frame, chunks[1]),
            Screen::Fingerprint => self.render_fingerprint(frame, chunks[1]),
            Screen::Check => self.render_check(frame, chunks[1]),
            Screen::Profile => self.render_profile(frame, chunks[1]),
        }
        let footer_text = if let Some(feature) = &self.editing_feature {
            format!(
                "Correcting {feature} mean: {}_ (Enter applies, Esc cancels)",
                self.edit_buffer
            )
        } else {
            self.message.clone()
        };
        frame.render_widget(
            Paragraph::new(footer_text).wrap(Wrap { trim: false }),
            chunks[2],
        );
    }

    fn render_corpus(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let items: Vec<ListItem> = self
            .store
            .corpus
            .samples()
            .iter()
            .map(|s| {
                let mark = if s.included { "[x]" } else { "[ ]" };
                ListItem::new(format!(
                    "{mark} #{} {:>5}w  {:<12} {:<7} {}",
                    s.id,
                    s.word_count(),
                    s.genre,
                    s.authorship.as_str(),
                    s.title
                ))
            })
            .collect();
        let mut state = ListState::default();
        state.select(Some(self.corpus_selected));
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Corpus — j/k move, Space include/exclude "),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        frame.render_stateful_widget(list, area, &mut state);
    }

    fn render_add(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let mut lines: Vec<Line> = Vec::new();
        for (i, name) in ADD_FIELDS.iter().enumerate() {
            let value = if i == 2 {
                self.add_authorship.as_str().to_string()
            } else if i == 5 {
                let v = &self.add_values[5];
                format!(
                    "{} ({} chars)",
                    v.lines().next().unwrap_or(""),
                    v.chars().count()
                )
            } else {
                self.add_values[i].clone()
            };
            let marker = if i == self.add_focus { ">" } else { " " };
            lines.push(Line::from(format!("{marker} {name}: {value}")));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(
            "Tab moves between fields; space toggles authorship; Enter on File path loads it; Ctrl+S saves; Esc back.",
        ));
        frame.render_widget(
            Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Add writing "),
                )
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_fingerprint(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let doc = &self.store.fingerprint;
        let effective = doc.effective_reference();
        let mut lines: Vec<Line> = vec![Line::from(format!(
            "Bands: CLOSE <= {:.3}, DRIFTING <= {:.3}   Pieces: {}   r = preview recompute, R = save preview, e = correct mean, x = clear corrections",
            doc.bands.close_max,
            doc.bands.drifting_max,
            doc.corpus_pieces.len()
        ))];
        for (i, f) in doc.features.iter().enumerate() {
            let stat = effective.get(f).copied();
            let marker = if i == self.feature_selected { ">" } else { " " };
            let corrected = if doc.is_corrected(f) {
                " (user-corrected)"
            } else {
                ""
            };
            lines.push(Line::from(format!(
                "{marker} {:<28} mean {:>9.3}  sd {:>8.3}{corrected}",
                f,
                stat.map(|s| s.mean).unwrap_or(0.0),
                stat.map(|s| s.sd).unwrap_or(0.0)
            )));
        }
        if let Some((_, summary)) = &self.pending_recompute {
            lines.push(Line::from(""));
            lines.push(Line::from(summary.clone()));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Fingerprint "),
                )
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_check(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let mut lines: Vec<Line> = vec![
            Line::from(format!(
                "{} Draft file: {}",
                if self.check_focus == 0 { ">" } else { " " },
                self.check_path
            )),
            Line::from(format!(
                "{} Source file (optional, enables dropped-number/URL accounting): {}",
                if self.check_focus == 1 { ">" } else { " " },
                self.check_source_path
            )),
            Line::from("Tab switches field, Enter runs the check."),
            Line::from(""),
        ];
        if let Some(report) = &self.check_result {
            lines.push(Line::from(format!(
                "VERDICT: {} — {} words{}",
                report.verdict.as_str(),
                report.words,
                report
                    .delta
                    .map(|d| format!(
                        ", delta {d:.3} (CLOSE <= {:.3}, DRIFTING <= {:.3})",
                        report.close_max, report.drifting_max
                    ))
                    .unwrap_or_default()
            )));
            for dev in &report.top_features {
                lines.push(Line::from(format!(
                    "  {:<28} draft {:>9.2} vs mean {:>9.2} (z {:.2})",
                    dev.feature, dev.draft, dev.reference_mean, dev.z
                )));
            }
            if !report.integrity.dropped_numbers.is_empty() {
                lines.push(Line::from(format!(
                    "  Dropped numbers: {}",
                    report.integrity.dropped_numbers.join(", ")
                )));
            }
            if !report.integrity.dropped_urls.is_empty() {
                lines.push(Line::from(format!(
                    "  Dropped URLs: {}",
                    report.integrity.dropped_urls.join(", ")
                )));
            }
            if !report.integrity.malformed_urls.is_empty() {
                lines.push(Line::from(format!(
                    "  Malformed URLs: {}",
                    report.integrity.malformed_urls.join(", ")
                )));
            }
            for hit in &report.avoid_hits {
                lines.push(Line::from(format!(
                    "  Avoid-list: \"{}\" x{}",
                    hit.phrase, hit.count
                )));
            }
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Check draft "),
                )
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_profile(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let text: Vec<Line> = self
            .store
            .profile_markdown
            .lines()
            .map(Line::from)
            .collect();
        frame.render_widget(
            Paragraph::new(text)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Profile — j/k scroll "),
                )
                .scroll((self.profile_scroll, 0))
                .wrap(Wrap { trim: false }),
            area,
        );
    }
}

fn snapshot(screen: Screen, home: PathBuf, width: u16, height: u16) -> Result<String, String> {
    let mut app = App::new(home)?;
    app.screen = screen;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).map_err(|e| e.to_string())?;
    terminal
        .draw(|f| app.render(f))
        .map_err(|e| e.to_string())?;
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        let mut line = String::new();
        for x in 0..buffer.area.width {
            line.push_str(buffer[(x, y)].symbol());
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    Ok(out)
}

fn run_tui(home: PathBuf) -> Result<(), String> {
    let mut app = App::new(home)?;
    crossterm::terminal::enable_raw_mode().map_err(|e| e.to_string())?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)
        .map_err(|e| e.to_string())?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| e.to_string())?;
    let result = (|| {
        while !app.should_quit {
            terminal
                .draw(|f| app.render(f))
                .map_err(|e| e.to_string())?;
            if crossterm::event::poll(std::time::Duration::from_millis(250))
                .map_err(|e| e.to_string())?
                && let crossterm::event::Event::Key(key) =
                    crossterm::event::read().map_err(|e| e.to_string())?
            {
                app.handle_key(key);
            }
        }
        Ok::<(), String>(())
    })();
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    );
    result
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut home: Option<PathBuf> = None;
    let mut snapshot_screen: Option<Screen> = None;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--home" => {
                home = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--snapshot" => {
                snapshot_screen = args.get(i + 1).and_then(|s| Screen::parse(s));
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    let home = home.unwrap_or_else(Store::default_dir);
    if let Some(screen) = snapshot_screen {
        return match snapshot(screen, home, 110, 34) {
            Ok(text) => {
                print!("{text}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("eigenstyle-tui: {e}");
                ExitCode::from(2)
            }
        };
    }
    match run_tui(home) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("eigenstyle-tui: {e}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn test_app(tag: &str) -> (App, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("eigenstyle-tui-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let app = App::new(dir.clone()).expect("app opens seeded store");
        (app, dir)
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
    }

    #[test]
    fn corpus_screen_lists_seed_and_toggles() {
        let (mut app, dir) = test_app("corpus");
        assert_eq!(app.store.corpus.samples().len(), 6);
        app.handle_key(key(KeyCode::Char(' ')));
        assert!(!app.store.corpus.samples()[0].included);
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.corpus_selected, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_flow_creates_sample_with_attribution() {
        let (mut app, dir) = test_app("add");
        app.handle_key(key(KeyCode::Char('2')));
        assert_eq!(app.screen, Screen::Add);
        type_text(&mut app, "A short safety note");
        app.handle_key(key(KeyCode::Tab));
        type_text(&mut app, "project-prose");
        app.handle_key(key(KeyCode::Tab)); // authorship
        app.handle_key(key(KeyCode::Char(' '))); // -> blended
        assert_eq!(app.add_authorship, Authorship::Blended);
        app.handle_key(key(KeyCode::Tab)); // source
        type_text(&mut app, "tui test");
        app.handle_key(key(KeyCode::Tab)); // file path (skip)
        app.handle_key(key(KeyCode::Tab)); // paste
        type_text(
            &mut app,
            "Safety is the precondition for every claim we make about these systems, and the record \
             should be checked twice before any claim is printed in our name or offered to patients.",
        );
        app.handle_key(ctrl(KeyCode::Char('s')));
        assert_eq!(app.screen, Screen::Corpus);
        let sample = app.store.corpus.samples().last().expect("added");
        assert_eq!(sample.authorship, Authorship::Blended);
        assert_eq!(sample.genre, "project-prose");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn check_flow_scores_holdout_close() {
        let (mut app, dir) = test_app("check");
        app.handle_key(key(KeyCode::Char('4')));
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../eigenstyle-core/tests/fixtures/jas_holdout.txt");
        type_text(&mut app, &path.to_string_lossy());
        app.handle_key(key(KeyCode::Enter));
        let report = app.check_result.expect("check ran");
        assert_eq!(report.verdict.as_str(), "CLOSE");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn check_flow_bad_path_is_an_error_not_a_panic() {
        let (mut app, dir) = test_app("checkbad");
        app.handle_key(key(KeyCode::Char('4')));
        type_text(&mut app, "/nonexistent/draft.txt");
        app.handle_key(key(KeyCode::Enter));
        assert!(app.check_result.is_none());
        assert!(app.message.contains("Cannot read draft"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fingerprint_preview_then_apply_and_correction_cycle() {
        let (mut app, dir) = test_app("fingerprint");
        app.handle_key(key(KeyCode::Char('3')));
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.pending_recompute.is_some());
        app.handle_key(key(KeyCode::Char('R')));
        assert!(app.pending_recompute.is_none());
        // Correction: edit first feature's mean to 42.
        app.handle_key(key(KeyCode::Char('e')));
        assert!(app.editing_feature.is_some());
        type_text(&mut app, "42");
        app.handle_key(key(KeyCode::Enter));
        let feature = app.store.fingerprint.features[0].clone();
        assert!(app.store.fingerprint.is_corrected(&feature));
        assert_eq!(
            app.store.fingerprint.effective_reference()[&feature].mean,
            42.0
        );
        app.handle_key(key(KeyCode::Char('x')));
        assert!(!app.store.fingerprint.is_corrected(&feature));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn render_corpus_snapshot_shows_seed_titles() {
        let dir =
            std::env::temp_dir().join(format!("eigenstyle-tui-test-render-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let text = snapshot(Screen::Corpus, dir.clone(), 110, 34).expect("snapshot");
        assert!(text.contains("included words"), "header renders: {text}");
        assert!(text.contains("[x] #1"), "first sample row renders");
        assert!(
            text.contains("Value-based Healthcare"),
            "seed title renders"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
