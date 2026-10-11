//! voice-gui — GPUI desktop front-end over voice-core.
//!
//! Same five surfaces as the TUI: Corpus (browse, include/exclude),
//! Add writing (fields + paste area, explicit genre + authorship),
//! Fingerprint (dashboard + per-genre stats, corrections editor,
//! recompute with preview-before-save), Check draft (paste or file →
//! verdict + features + integrity + avoid hits), Profile (readable
//! profile + avoid-list editor).
//!
//! Input model: the root view owns the only focus handle; clicking a
//! field makes it the active field and keystrokes route to it (the
//! TUI's state machine, ported). Text entry is append/backspace — no
//! selection or IME; that is a stated limit of this first GUI, not a
//! hidden one.
//!
//! Platform: gpui-ce 0.2.2 + gpui_ce_platform 0.1.0, Wayland first
//! (primo's desktop is Hyprland). X11 under Xvfb presents black with
//! this build (verified 2026-10-11) — use a Wayland session.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, FocusHandle, KeyDownEvent, MouseButton, Render, ScrollHandle, Window,
    WindowBounds, WindowOptions, div, px, rgb, size,
};

use voice_core::checker::CheckReport;
use voice_core::corpus::{Authorship, NewSample};
use voice_core::fingerprint::FingerprintDoc;
use voice_core::{Store, check_draft};

// Theme.
const BG: u32 = 0x141e28;
const PANEL: u32 = 0x1d2b3a;
const PANEL_ALT: u32 = 0x243447;
const BORDER: u32 = 0x33506b;
const TEXT: u32 = 0xe8eef4;
const MUTED: u32 = 0x93a7ba;
const ACCENT: u32 = 0x5ea0e0;
const GOOD: u32 = 0x57c785;
const WARN: u32 = 0xe0b45e;
const BAD: u32 = 0xe06c5e;

/// Tabs in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Corpus,
    Add,
    Fingerprint,
    Check,
    Profile,
}

impl Tab {
    fn all() -> [Tab; 5] {
        [
            Tab::Corpus,
            Tab::Add,
            Tab::Fingerprint,
            Tab::Check,
            Tab::Profile,
        ]
    }
    fn title(&self) -> &'static str {
        match self {
            Tab::Corpus => "Corpus",
            Tab::Add => "Add writing",
            Tab::Fingerprint => "Fingerprint",
            Tab::Check => "Check draft",
            Tab::Profile => "Profile",
        }
    }
    fn parse(name: &str) -> Option<Tab> {
        match name {
            "corpus" => Some(Tab::Corpus),
            "add" => Some(Tab::Add),
            "fingerprint" => Some(Tab::Fingerprint),
            "check" => Some(Tab::Check),
            "profile" => Some(Tab::Profile),
            _ => None,
        }
    }
}

/// Which text field currently receives keystrokes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    AddTitle,
    AddGenre,
    AddSource,
    AddPath,
    AddText,
    CheckPath,
    CheckSourcePath,
    CheckText,
    CorrectMean,
    AvoidNew,
}

/// The root view: all GUI state lives here; rendering is a pure
/// function of it plus the store.
struct VoiceApp {
    store: Store,
    focus_handle: FocusHandle,
    tab: Tab,
    active: Option<Field>,
    corpus_selected: usize,
    feature_selected: usize,
    add_title: String,
    add_genre: String,
    add_source: String,
    add_path: String,
    add_text: String,
    add_authorship: Authorship,
    check_path: String,
    check_source_path: String,
    check_text: String,
    check_report: Option<CheckReport>,
    correct_mean: String,
    avoid_new: String,
    pending: Option<(FingerprintDoc, String)>,
    message: String,
    corpus_scroll: ScrollHandle,
    feature_scroll: ScrollHandle,
    profile_scroll: ScrollHandle,
}

impl VoiceApp {
    fn new(cx: &mut Context<Self>, home: PathBuf) -> VoiceApp {
        let store = Store::open(&home).unwrap_or_else(|e| {
            eprintln!("voice-gui: cannot open store at {}: {e}", home.display());
            std::process::exit(2);
        });
        VoiceApp {
            store,
            focus_handle: cx.focus_handle(),
            tab: Tab::Corpus,
            active: None,
            corpus_selected: 0,
            feature_selected: 0,
            add_title: String::new(),
            add_genre: String::new(),
            add_source: String::new(),
            add_path: String::new(),
            add_text: String::new(),
            add_authorship: Authorship::Sole,
            check_path: String::new(),
            check_source_path: String::new(),
            check_text: String::new(),
            check_report: None,
            correct_mean: String::new(),
            avoid_new: String::new(),
            pending: None,
            message: "Click a field to type. Tabs switch above; Esc leaves a field; q quits when no field is active.".to_string(),
            corpus_scroll: ScrollHandle::new(),
            feature_scroll: ScrollHandle::new(),
            profile_scroll: ScrollHandle::new(),
        }
    }

    fn active_buffer(&mut self) -> Option<&mut String> {
        match self.active? {
            Field::AddTitle => Some(&mut self.add_title),
            Field::AddGenre => Some(&mut self.add_genre),
            Field::AddSource => Some(&mut self.add_source),
            Field::AddPath => Some(&mut self.add_path),
            Field::AddText => Some(&mut self.add_text),
            Field::CheckPath => Some(&mut self.check_path),
            Field::CheckSourcePath => Some(&mut self.check_source_path),
            Field::CheckText => Some(&mut self.check_text),
            Field::CorrectMean => Some(&mut self.correct_mean),
            Field::AvoidNew => Some(&mut self.avoid_new),
        }
    }

    fn is_multiline(field: Field) -> bool {
        matches!(field, Field::AddText | Field::CheckText)
    }

    fn on_key_down(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let key = ev.keystroke.key.as_str();
        let ctrl = ev.keystroke.modifiers.control;
        if let Some(field) = self.active {
            match key {
                "escape" => self.active = None,
                "backspace" => {
                    if let Some(buf) = self.active_buffer() {
                        buf.pop();
                    }
                }
                "enter" => {
                    if Self::is_multiline(field) {
                        if let Some(buf) = self.active_buffer() {
                            buf.push('\n');
                        }
                    } else {
                        self.active = None;
                        self.on_field_enter(field, cx);
                    }
                }
                "tab" => self.active = None,
                _ => {
                    if ctrl && key == "s" && self.tab == Tab::Add {
                        self.submit_add(cx);
                    } else if !ctrl
                        && let Some(ch) = &ev.keystroke.key_char
                        && let Some(buf) = self.active_buffer()
                        && buf.len() < 300_000
                    {
                        buf.push_str(ch);
                    }
                }
            }
            cx.notify();
            return;
        }
        // No active field: global keys.
        match key {
            "q" => cx.quit(),
            "1" | "2" | "3" | "4" | "5" => {
                let idx = usize::from(key.bytes().next().unwrap_or(b'1') - b'1');
                self.tab = Tab::all()[idx];
            }
            "tab" => {
                let idx = Tab::all().iter().position(|t| *t == self.tab).unwrap_or(0);
                self.tab = Tab::all()[(idx + 1) % 5];
            }
            "down" | "j" => self.move_selection(1),
            "up" | "k" => self.move_selection(-1),
            "space" if self.tab == Tab::Corpus => self.toggle_selected(cx),
            _ => {}
        }
        cx.notify();
    }

    fn move_selection(&mut self, delta: i32) {
        match self.tab {
            Tab::Corpus => {
                let count = self.store.corpus.samples().len();
                if count > 0 {
                    let next = i64::try_from(self.corpus_selected).unwrap_or(0) + i64::from(delta);
                    self.corpus_selected =
                        usize::try_from(next.clamp(0, i64::try_from(count - 1).unwrap_or(0)))
                            .unwrap_or(0);
                }
            }
            Tab::Fingerprint => {
                let count = self.store.fingerprint.features.len();
                if count > 0 {
                    let next = i64::try_from(self.feature_selected).unwrap_or(0) + i64::from(delta);
                    self.feature_selected =
                        usize::try_from(next.clamp(0, i64::try_from(count - 1).unwrap_or(0)))
                            .unwrap_or(0);
                }
            }
            _ => {}
        }
    }

    fn on_field_enter(&mut self, field: Field, cx: &mut Context<Self>) {
        match field {
            Field::AddPath => self.load_add_file(cx),
            Field::CheckPath => self.load_check_file(cx),
            Field::CorrectMean => self.apply_correction(cx),
            Field::AvoidNew => self.add_avoid(cx),
            _ => {}
        }
    }

    fn toggle_selected(&mut self, cx: &mut Context<Self>) {
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
        cx.notify();
    }

    fn load_add_file(&mut self, cx: &mut Context<Self>) {
        match std::fs::read_to_string(self.add_path.trim()) {
            Ok(text) => {
                self.add_text = text;
                self.message =
                    "File loaded into the text area. Review, then Add sample.".to_string();
            }
            Err(e) => self.message = format!("Cannot read file: {e}"),
        }
        cx.notify();
    }

    fn load_check_file(&mut self, cx: &mut Context<Self>) {
        match std::fs::read_to_string(self.check_path.trim()) {
            Ok(text) => {
                self.check_text = text;
                self.message = "Draft loaded. Run check.".to_string();
            }
            Err(e) => self.message = format!("Cannot read file: {e}"),
        }
        cx.notify();
    }

    fn submit_add(&mut self, cx: &mut Context<Self>) {
        let text = if !self.add_text.trim().is_empty() {
            self.add_text.clone()
        } else if !self.add_path.trim().is_empty() {
            match std::fs::read_to_string(self.add_path.trim()) {
                Ok(t) => t,
                Err(e) => {
                    self.message = format!("Cannot read file: {e}");
                    cx.notify();
                    return;
                }
            }
        } else {
            self.message = "Nothing to add: paste text or give a file path.".to_string();
            cx.notify();
            return;
        };
        let new = NewSample {
            title: self.add_title.clone(),
            genre: self.add_genre.clone(),
            authorship: self.add_authorship,
            source: self.add_source.clone(),
            text,
        };
        match self.store.corpus.add_sample(new) {
            Ok(sample) => {
                let _ = self.store.save_corpus();
                self.add_title.clear();
                self.add_genre.clear();
                self.add_source.clear();
                self.add_path.clear();
                self.add_text.clear();
                self.active = None;
                self.corpus_selected = self.store.corpus.samples().len().saturating_sub(1);
                self.tab = Tab::Corpus;
                self.message = format!(
                    "Added sample {} ({} words). Fingerprint NOT recomputed — see the Fingerprint tab.",
                    sample.id,
                    sample.word_count()
                );
            }
            Err(e) => self.message = format!("Add rejected: {e}"),
        }
        cx.notify();
    }

    fn run_check(&mut self, cx: &mut Context<Self>) {
        let draft = if !self.check_text.trim().is_empty() {
            self.check_text.clone()
        } else if !self.check_path.trim().is_empty() {
            match std::fs::read_to_string(self.check_path.trim()) {
                Ok(t) => t,
                Err(e) => {
                    self.message = format!("Cannot read draft: {e}");
                    cx.notify();
                    return;
                }
            }
        } else {
            self.message = "No draft: paste text or give a file path.".to_string();
            cx.notify();
            return;
        };
        let source = if self.check_source_path.trim().is_empty() {
            None
        } else {
            match std::fs::read_to_string(self.check_source_path.trim()) {
                Ok(t) => Some(t),
                Err(e) => {
                    self.message = format!("Cannot read source: {e}");
                    cx.notify();
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
                self.message = format!("Verdict: {}", report.verdict.as_str());
                self.check_report = Some(report);
            }
            Err(e) => self.message = format!("Check failed: {e}"),
        }
        cx.notify();
    }

    fn preview_recompute(&mut self, cx: &mut Context<Self>) {
        match self.store.recompute(true) {
            Ok((doc, deltas, old, new)) => {
                let summary = format!(
                    "Preview: {} pieces, {} features change, bands {:.3}/{:.3} -> {:.3}/{:.3}. Apply saves it; corrections are carried across.",
                    doc.corpus_pieces.len(),
                    deltas.len(),
                    old.close_max,
                    old.drifting_max,
                    new.close_max,
                    new.drifting_max
                );
                self.pending = Some((doc, summary));
                self.message = "Recompute preview ready — review, then Apply.".to_string();
            }
            Err(e) => self.message = format!("Recompute preview failed: {e}"),
        }
        cx.notify();
    }

    fn apply_recompute(&mut self, cx: &mut Context<Self>) {
        if self.pending.take().is_none() {
            self.message = "Nothing to apply — run Preview first.".to_string();
            cx.notify();
            return;
        }
        match self.store.recompute(false) {
            Ok((_, deltas, _, _)) => {
                self.message = format!(
                    "Fingerprint recomputed and saved ({} features changed).",
                    deltas.len()
                );
            }
            Err(e) => self.message = format!("Recompute failed: {e}"),
        }
        cx.notify();
    }

    fn apply_correction(&mut self, cx: &mut Context<Self>) {
        let Some(feature) = self
            .store
            .fingerprint
            .features
            .get(self.feature_selected)
            .cloned()
        else {
            return;
        };
        match self.correct_mean.trim().parse::<f64>() {
            Ok(value) => match self
                .store
                .fingerprint
                .set_correction(&feature, Some(value), None)
            {
                Ok(()) => {
                    let _ = self.store.save_fingerprint();
                    self.message = format!(
                        "{feature} mean corrected to {value} (user-corrected; survives recompute)."
                    );
                    self.correct_mean.clear();
                    self.active = None;
                }
                Err(e) => self.message = format!("Correction rejected: {e}"),
            },
            Err(_) => self.message = "Correction rejected: not a number.".to_string(),
        }
        cx.notify();
    }

    fn clear_corrections(&mut self, cx: &mut Context<Self>) {
        self.store.fingerprint.clear_corrections();
        let _ = self.store.save_fingerprint();
        self.message = "All user corrections cleared; computed values restored.".to_string();
        cx.notify();
    }

    fn add_avoid(&mut self, cx: &mut Context<Self>) {
        let phrase = self.avoid_new.clone();
        match self.store.add_avoid_phrase(&phrase) {
            Ok(()) => {
                let _ = self.store.save_settings();
                self.message = format!("Avoid-list: added \"{}\".", phrase.trim());
                self.avoid_new.clear();
                self.active = None;
            }
            Err(e) => self.message = format!("Avoid-list rejected: {e}"),
        }
        cx.notify();
    }

    fn remove_avoid(&mut self, phrase: &str, cx: &mut Context<Self>) {
        match self.store.remove_avoid_phrase(phrase) {
            Ok(()) => {
                let _ = self.store.save_settings();
                self.message = format!("Avoid-list: removed \"{phrase}\".");
            }
            Err(e) => self.message = format!("Avoid-list: {e}"),
        }
        cx.notify();
    }
}

// ----- rendering helpers -----

fn button(
    id: &'static str,
    label: &str,
    primary: bool,
    cx: &mut Context<VoiceApp>,
    handler: impl Fn(&mut VoiceApp, &mut Context<VoiceApp>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .px(px(12.))
        .py(px(6.))
        .rounded(px(6.))
        .border_1()
        .border_color(rgb(BORDER))
        .bg(rgb(if primary { 0x2c4a6e } else { PANEL_ALT }))
        .text_color(rgb(TEXT))
        .cursor(gpui::CursorStyle::PointingHand)
        .hover(|s| s.bg(rgb(0x35587f)))
        .on_click(cx.listener(move |app, _ev, _window, cx| handler(app, cx)))
        .child(label.to_string())
}

impl VoiceApp {
    fn field_box(
        &self,
        field: Field,
        label: &str,
        value: &str,
        height: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = self.active == Some(field);
        let display = if is_active {
            format!("{value}▏")
        } else if value.is_empty() {
            "(click to type)".to_string()
        } else {
            value.to_string()
        };
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .text_size(px(12.))
                    .child(label.to_string()),
            )
            .child(
                div()
                    .id(match field {
                        Field::AddTitle => "f-add-title",
                        Field::AddGenre => "f-add-genre",
                        Field::AddSource => "f-add-source",
                        Field::AddPath => "f-add-path",
                        Field::AddText => "f-add-text",
                        Field::CheckPath => "f-check-path",
                        Field::CheckSourcePath => "f-check-source",
                        Field::CheckText => "f-check-text",
                        Field::CorrectMean => "f-correct-mean",
                        Field::AvoidNew => "f-avoid-new",
                    })
                    .h(px(height))
                    .px(px(10.))
                    .py(px(6.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgb(if is_active { ACCENT } else { BORDER }))
                    .bg(rgb(0x111a24))
                    .text_color(rgb(if value.is_empty() && !is_active {
                        MUTED
                    } else {
                        TEXT
                    }))
                    .overflow_hidden()
                    .cursor(gpui::CursorStyle::IBeam)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |app, _ev, _window, cx| {
                            app.active = Some(field);
                            cx.notify();
                        }),
                    )
                    .child(display),
            )
    }

    fn view_corpus(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .id("corpus-scroll")
            .overflow_y_scroll()
            .track_scroll(&self.corpus_scroll)
            .flex_1();
        for (i, s) in self.store.corpus.samples().iter().enumerate() {
            let selected = i == self.corpus_selected;
            let mark = if s.included { "[x]" } else { "[ ]" };
            let line = format!(
                "{mark} #{} {:>5}w  {:<12} {:<7} {}",
                s.id,
                s.word_count(),
                s.genre,
                s.authorship.as_str(),
                s.title
            );
            list = list.child(
                div()
                    .id(("sample-row", i))
                    .px(px(10.))
                    .py(px(5.))
                    .rounded(px(4.))
                    .bg(rgb(if selected { 0x2c4a6e } else { PANEL }))
                    .text_color(rgb(TEXT))
                    .cursor(gpui::CursorStyle::PointingHand)
                    .on_click(cx.listener(move |app, ev: &gpui::ClickEvent, _window, cx| {
                        app.corpus_selected = i;
                        if let gpui::ClickEvent::Mouse(m) = ev
                            && m.down.click_count >= 2
                        {
                            app.toggle_selected(cx);
                        }
                        cx.notify();
                    }))
                    .child(line),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .size_full()
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .child("Click a row to select, double-click (or select + Space) to include/exclude it from the fingerprint. Excluded samples stay in history."),
            )
            .child(list)
            .child(button("toggle-included", "Include / exclude selected", false, cx, |app, cx| {
                app.toggle_selected(cx);
            }))
    }

    fn view_add(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .size_full()
            .child(self.field_box(Field::AddTitle, "Title", &self.add_title.clone(), 34., cx))
            .child(
                div()
                    .flex()
                    .gap(px(10.))
                    .child(div().flex_1().child(self.field_box(
                        Field::AddGenre,
                        "Genre (e.g. letter, scholarly, peer-review)",
                        &self.add_genre.clone(),
                        34.,
                        cx,
                    )))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_color(rgb(MUTED))
                                    .text_size(px(12.))
                                    .child("Authorship — explicit, never assumed"),
                            )
                            .child(button(
                                "authorship-toggle",
                                if self.add_authorship == Authorship::Sole {
                                    "sole (click for blended)"
                                } else {
                                    "blended (click for sole)"
                                },
                                false,
                                cx,
                                |app, cx| {
                                    app.add_authorship = match app.add_authorship {
                                        Authorship::Sole => Authorship::Blended,
                                        Authorship::Blended => Authorship::Sole,
                                    };
                                    cx.notify();
                                },
                            )),
                    ),
            )
            .child(self.field_box(
                Field::AddSource,
                "Source (publisher, URL, file…)",
                &self.add_source.clone(),
                34.,
                cx,
            ))
            .child(self.field_box(
                Field::AddPath,
                "File path (Enter loads it into the text area)",
                &self.add_path.clone(),
                34.,
                cx,
            ))
            .child(div().flex_1().child(self.field_box(
                Field::AddText,
                "Text (paste)",
                &self.add_text.clone(),
                150.,
                cx,
            )))
            .child(button(
                "submit-add",
                "Add sample (does NOT recompute the fingerprint)",
                true,
                cx,
                |app, cx| {
                    app.submit_add(cx);
                },
            ))
    }

    fn view_fingerprint(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let doc = &self.store.fingerprint;
        let effective = doc.effective_reference();
        let mut rows = div()
            .flex()
            .flex_col()
            .gap(px(1.))
            .id("feature-scroll")
            .overflow_y_scroll()
            .track_scroll(&self.feature_scroll)
            .flex_1();
        for (i, f) in doc.features.iter().enumerate() {
            let stat = effective.get(f).copied();
            let selected = i == self.feature_selected;
            let corrected = if doc.is_corrected(f) {
                "  (user-corrected)"
            } else {
                ""
            };
            let line = format!(
                "{:<28} mean {:>9.3}  sd {:>8.3}{corrected}",
                f,
                stat.map(|s| s.mean).unwrap_or(0.0),
                stat.map(|s| s.sd).unwrap_or(0.0)
            );
            rows = rows.child(
                div()
                    .id(("feature-row", i))
                    .px(px(10.))
                    .py(px(3.))
                    .rounded(px(4.))
                    .bg(rgb(if selected { 0x2c4a6e } else { PANEL }))
                    .text_color(rgb(TEXT))
                    .cursor(gpui::CursorStyle::PointingHand)
                    .on_click(cx.listener(move |app, _ev, _window, cx| {
                        app.feature_selected = i;
                        cx.notify();
                    }))
                    .child(line),
            );
        }
        let mut genres = div().flex().flex_col().gap(px(1.));
        for (genre, fp) in self.store.corpus.genre_fingerprints() {
            genres = genres.child(
                div().text_color(rgb(MUTED)).child(format!(
                    "{:<14} {} words, sentence mean {:.1}, first-person {:.1}/1k, hedges {:.1} vs assertives {:.1}/1k",
                    genre, fp.words, fp.sent_len_mean, fp.first_person_per1k, fp.hedges_per1k, fp.assertives_per1k
                )),
            );
        }
        let mut root = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .size_full()
            .child(div().text_color(rgb(TEXT)).child(format!(
                "Bands: CLOSE <= {:.3} · DRIFTING <= {:.3} · pieces: {} · corrections: {}",
                doc.bands.close_max,
                doc.bands.drifting_max,
                doc.corpus_pieces.len(),
                doc.corrections.len()
            )))
            .child(genres)
            .child(rows);
        if let Some((_, summary)) = &self.pending {
            root = root.child(div().text_color(rgb(WARN)).child(summary.clone()));
        }
        root.child(
            div()
                .flex()
                .gap(px(8.))
                .child(button(
                    "preview-recompute",
                    "Preview recompute (dry run)",
                    false,
                    cx,
                    |app, cx| {
                        app.preview_recompute(cx);
                    },
                ))
                .child(button(
                    "apply-recompute",
                    "Apply previewed recompute",
                    true,
                    cx,
                    |app, cx| {
                        app.apply_recompute(cx);
                    },
                )),
        )
        .child(
            div()
                .flex()
                .gap(px(10.))
                .items_end()
                .child(div().flex_1().child(self.field_box(
                    Field::CorrectMean,
                    "Correct selected feature's mean (Enter applies)",
                    &self.correct_mean.clone(),
                    34.,
                    cx,
                )))
                .child(button(
                    "clear-corrections",
                    "Clear all corrections",
                    false,
                    cx,
                    |app, cx| {
                        app.clear_corrections(cx);
                    },
                )),
        )
    }

    fn view_check(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .size_full()
            .child(self.field_box(
                Field::CheckText,
                "Draft text (paste)",
                &self.check_text.clone(),
                140.,
                cx,
            ))
            .child(self.field_box(
                Field::CheckPath,
                "…or draft file path (Enter loads it)",
                &self.check_path.clone(),
                34.,
                cx,
            ))
            .child(self.field_box(
                Field::CheckSourcePath,
                "Source file path (optional — enables dropped-number / dropped-URL accounting)",
                &self.check_source_path.clone(),
                34.,
                cx,
            ))
            .child(button("run-check", "Run check", true, cx, |app, cx| {
                app.run_check(cx);
            }));
        if let Some(report) = &self.check_report {
            let (verdict_text, color) = match report.verdict {
                voice_core::checker::Verdict::Close => ("CLOSE", GOOD),
                voice_core::checker::Verdict::Drifting => ("DRIFTING", WARN),
                voice_core::checker::Verdict::OffVoice => ("OFF-VOICE", BAD),
                voice_core::checker::Verdict::Unreliable => ("UNRELIABLE (under 200 words)", WARN),
            };
            let mut panel = div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .p(px(12.))
                .rounded(px(8.))
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(PANEL))
                .child(
                    div()
                        .text_size(px(22.))
                        .text_color(rgb(color))
                        .child(format!(
                            "{} — {} words{}",
                            verdict_text,
                            report.words,
                            report
                                .delta
                                .map(|d| format!(
                                    ", delta {d:.3} (CLOSE <= {:.3}, DRIFTING <= {:.3})",
                                    report.close_max, report.drifting_max
                                ))
                                .unwrap_or_default()
                        )),
                );
            for dev in &report.top_features {
                panel = panel.child(div().text_color(rgb(TEXT)).child(format!(
                    "{:<28} draft {:>9.2} vs mean {:>9.2} (z {:.2})",
                    dev.feature, dev.draft, dev.reference_mean, dev.z
                )));
            }
            if !report.integrity.dropped_numbers.is_empty() {
                panel = panel.child(div().text_color(rgb(BAD)).child(format!(
                    "Dropped numbers: {}",
                    report.integrity.dropped_numbers.join(", ")
                )));
            }
            if !report.integrity.dropped_urls.is_empty() {
                panel = panel.child(div().text_color(rgb(BAD)).child(format!(
                    "Dropped URLs: {}",
                    report.integrity.dropped_urls.join(", ")
                )));
            }
            if !report.integrity.malformed_urls.is_empty() {
                panel = panel.child(div().text_color(rgb(BAD)).child(format!(
                    "Malformed URLs: {}",
                    report.integrity.malformed_urls.join(", ")
                )));
            }
            for hit in &report.avoid_hits {
                panel = panel.child(div().text_color(rgb(WARN)).child(format!(
                    "Avoid-list phrase: \"{}\" x{}",
                    hit.phrase, hit.count
                )));
            }
            root = root.child(panel);
        }
        root
    }

    fn view_profile(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut text_col = div().flex().flex_col().gap(px(1.));
        for line in self.store.profile_markdown.lines() {
            text_col = text_col.child(
                div()
                    .text_color(rgb(if line.starts_with('#') { ACCENT } else { TEXT }))
                    .child(if line.is_empty() {
                        " ".to_string()
                    } else {
                        line.to_string()
                    }),
            );
        }
        let mut avoid_col = div().flex().flex_col().gap(px(4.));
        for phrase in self.store.avoid_list.clone() {
            let p = phrase.clone();
            avoid_col = avoid_col.child(
                div()
                    .flex()
                    .gap(px(8.))
                    .items_center()
                    .child(div().flex_1().text_color(rgb(TEXT)).child(phrase.clone()))
                    .child(button(
                        "remove-avoid",
                        "remove",
                        false,
                        cx,
                        move |app, cx| {
                            app.remove_avoid(&p, cx);
                        },
                    )),
            );
        }
        div()
            .flex()
            .gap(px(16.))
            .size_full()
            .child(
                div()
                    .flex_1()
                    .id("profile-scroll")
                    .overflow_y_scroll()
                    .track_scroll(&self.profile_scroll)
                    .child(text_col),
            )
            .child(
                div()
                    .w(px(300.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_color(rgb(TEXT))
                            .child("Avoid list (checker enforces)"),
                    )
                    .child(avoid_col)
                    .child(self.field_box(
                        Field::AvoidNew,
                        "New phrase (Enter adds)",
                        &self.avoid_new.clone(),
                        34.,
                        cx,
                    )),
            )
    }
}

impl Render for VoiceApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut tabs = div().flex().gap(px(6.)).items_center();
        for (i, tab) in Tab::all().iter().enumerate() {
            let selected = *tab == self.tab;
            let tab_copy = *tab;
            tabs = tabs.child(
                div()
                    .id(("tab", i))
                    .px(px(14.))
                    .py(px(7.))
                    .rounded(px(6.))
                    .bg(rgb(if selected { 0x2c4a6e } else { PANEL_ALT }))
                    .text_color(rgb(TEXT))
                    .cursor(gpui::CursorStyle::PointingHand)
                    .on_click(cx.listener(move |app, _ev, _window, cx| {
                        app.tab = tab_copy;
                        app.active = None;
                        cx.notify();
                    }))
                    .child(format!("{}. {}", i + 1, tab.title())),
            );
        }
        tabs = tabs.child(div().flex_1().text_color(rgb(MUTED)).child(format!(
            "  {} samples · {} included words · store: {}",
            self.store.corpus.samples().len(),
            self.store.corpus.included_word_count(),
            self.store.dir().display()
        )));
        let content = match self.tab {
            Tab::Corpus => self.view_corpus(cx).into_any_element(),
            Tab::Add => self.view_add(cx).into_any_element(),
            Tab::Fingerprint => self.view_fingerprint(cx).into_any_element(),
            Tab::Check => self.view_check(cx).into_any_element(),
            Tab::Profile => self.view_profile(cx).into_any_element(),
        };
        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .bg(rgb(BG))
            .flex()
            .flex_col()
            .p(px(14.))
            .gap(px(10.))
            .child(tabs)
            .child(div().flex_1().child(content))
            .child(
                div()
                    .px(px(10.))
                    .py(px(6.))
                    .rounded(px(6.))
                    .bg(rgb(PANEL))
                    .text_color(rgb(MUTED))
                    .child(self.message.clone()),
            )
    }
}

fn parse_args(args: &[String]) -> (PathBuf, Tab) {
    let mut home: Option<PathBuf> = None;
    let mut tab = Tab::Corpus;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--home" => {
                home = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--tab" => {
                if let Some(t) = args.get(i + 1).and_then(|s| Tab::parse(s)) {
                    tab = t;
                }
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    (home.unwrap_or_else(Store::default_dir), tab)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (home, tab) = parse_args(&args);
    gpui_ce_platform::application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1180.), px(760.)), cx);
        let home_for_view = home.clone();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |window, cx| {
                let view = cx.new(move |cx| {
                    let mut app = VoiceApp::new(cx, home_for_view);
                    app.tab = tab;
                    app
                });
                let handle = view.read(cx).focus_handle.clone();
                window.focus(&handle, cx);
                view
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
