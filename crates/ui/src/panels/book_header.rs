//! The audiobook page's header, laid out like the CD page's (`album_header`): the book's cover
//! (or the disc), its title and author as titles, narrator · series · year, a line saying
//! where the book has got to, and its actions.

use crate::actions::Action;
use crate::panels::album_header::{
    BUTTON, META_SIZE, background, details_line, dot, fit, paint_art, quiet_field, secondary_button, text_column,
};
use crate::panels::{PanelContext, widgets};
use crate::theme;
use egui::{Color32, FontId, Rect, Sense, Ui, pos2, vec2};
use tunebox_engine::commands::CommandId;
use tunebox_engine::model::audiobook::{Book, BookDetails, seconds};
use tunebox_engine::model::format_total_time;
use tunebox_engine::{JobKind, Session};

/// A few words on where the book has got to, for the source list and the details line.
pub fn state_word(session: &Session) -> String {
    let Some(book) = session.audiobook() else { return String::new() };
    if session.is_busy(JobKind::BuildBook) {
        "building".to_owned()
    } else if session.is_busy(JobKind::RipBookDisc) {
        format!("ripping disc {}", ripping_disc(session, book))
    } else if session.audiobook_question().is_some() {
        "a disc to check".to_owned()
    } else if let Some(next) = session.audiobook_waiting_for() {
        format!("insert disc {next}")
    } else if book.ripping_done {
        "ready to build".to_owned()
    } else {
        let pending = book.unripped_kept().first().map_or(book.discs.len() as u16, |t| t.disc);
        format!("disc {pending} ready")
    }
}

/// The disc being ripped: the one in the drive.
pub fn ripping_disc(session: &Session, book: &Book) -> u16 {
    let in_drive = session.cd().and_then(|cd| match book.check_disc(&cd.disc_id) {
        tunebox_engine::model::audiobook::DiscCheck::InBook { disc, .. } => Some(disc),
        _ => None,
    });
    in_drive.unwrap_or(book.discs.len() as u16)
}

/// Draws the header across the top of `area`; returns its bottom edge.
pub fn show(ui: &mut Ui, cx: &mut PanelContext, area: Rect) -> f32 {
    let Some(book) = cx.session.audiobook() else { return area.top() };
    let (header, art) = background(ui, area);
    let top = art.top();
    let cover = cx.session.audiobook_cover().map(|c| (format!("book:{}", c.data.len()), c.data.as_slice()));
    paint_art(ui, cx, art, cover.as_ref().map(|(key, data)| (key.as_str(), *data)));

    let (left, right) = text_column(header, art);
    let mut details = book.details.clone();
    let mut changed = false;
    let title = Rect::from_min_max(pos2(left, top - 2.0), pos2(right, top + 30.0));
    changed |= quiet_field(ui, title, &mut details.title, "Title", crate::fonts::bold(22.0), theme::TEXT);
    let author = Rect::from_min_max(pos2(left, top + 32.0), pos2(right, top + 56.0));
    let author_color = Color32::from_rgb(0x4A, 0x4E, 0x55);
    changed |= quiet_field(ui, author, &mut details.author, "Author", FontId::proportional(16.0), author_color);
    changed |= meta_row(ui, &mut details, pos2(left, top + 58.0));
    if changed {
        cx.act(Action::SetBookDetails(Box::new(details)));
    }
    details_line(ui, header, pos2(left, top + 92.0), &details_text(cx.session, book));

    let y = art.bottom() - BUTTON;
    let primary = Rect::from_min_size(pos2(left, y), vec2(150.0, BUTTON));
    primary_button(ui, cx, book, primary);
    let mut x = primary.right() + 10.0;
    if !book.ripping_done {
        let last = Rect::from_min_size(pos2(x, y), vec2(170.0, BUTTON));
        let enabled = cx.session.is_enabled(CommandId::FinishRipping, book_context());
        if secondary_button(ui, last, "That Was the Last Disc", enabled).clicked() {
            cx.act(Action::Run(CommandId::FinishRipping));
        }
        x = last.right() + 10.0;
    }
    let suggestions = cx.session.audiobook_suggestions().len();
    if suggestions > 0 {
        let accept = Rect::from_min_size(pos2(x, y), vec2(190.0, BUTTON));
        let label = format!("Accept All Suggestions ({suggestions})");
        if secondary_button(ui, accept, &label, true).clicked() {
            cx.act(Action::Run(CommandId::AcceptAllSuggestions));
        }
    }
    header.bottom()
}

/// The context the header's buttons are enabled in: the book page.
fn book_context() -> tunebox_engine::commands::Context {
    tunebox_engine::commands::Context { book_page: true, ..Default::default() }
}

/// Rip Disc N while discs are ripped, Build Audiobook… after the last.
fn primary_button(ui: &mut Ui, cx: &mut PanelContext, book: &Book, rect: Rect) {
    let (label, command) = if book.ripping_done {
        let busy = cx.session.is_busy(JobKind::BuildBook);
        (if busy { "Building…".to_owned() } else { "Build Audiobook…".to_owned() }, CommandId::BuildAudiobook)
    } else if cx.session.is_busy(JobKind::RipBookDisc) {
        (format!("Ripping Disc {}…", ripping_disc(cx.session, book)), CommandId::RipDisc)
    } else {
        let pending = book.unripped_kept().first().map_or(book.discs.len() as u16, |t| t.disc);
        (format!("Rip Disc {pending}"), CommandId::RipDisc)
    };
    let enabled = cx.session.is_enabled(command, book_context());
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let response = ui.interact(rect, ui.scope_id().with("book-primary"), sense);
    widgets::paint_primary(ui, rect, &label, enabled, &response);
    let response = if enabled || book.ripping_done {
        response
    } else {
        response.on_hover_text("Put the disc in the drive first.")
    };
    if response.clicked() {
        cx.act(Action::Run(command));
    }
}

/// Narrated by … · Series, number · Year. Each field is as wide as its text.
fn meta_row(ui: &mut Ui, details: &mut BookDetails, at: egui::Pos2) -> bool {
    let font = FontId::proportional(META_SIZE);
    let mut changed = false;
    let mut x = at.x;
    let field = |ui: &mut Ui, x: &mut f32, value: &mut String, hint: &str| -> bool {
        let rect = Rect::from_min_size(pos2(*x, at.y), vec2(fit(ui, value, hint, &font), 22.0));
        let changed = quiet_field(ui, rect, value, hint, font.clone(), theme::TEXT_MUTED);
        *x = rect.right();
        changed
    };
    changed |= field(ui, &mut x, &mut details.narrator, "Narrator");
    dot(ui, pos2(x + 7.0, at.y + 11.0));
    x += 14.0;
    changed |= field(ui, &mut x, &mut details.series, "Series");
    let mut number = details.series_number.map(|n| n.to_string()).unwrap_or_default();
    x += 6.0;
    if field(ui, &mut x, &mut number, "No.") {
        details.series_number = number.trim().parse().ok();
        changed = true;
    }
    dot(ui, pos2(x + 7.0, at.y + 11.0));
    x += 14.0;
    let mut year = details.year.map(|y| y.to_string()).unwrap_or_default();
    if field(ui, &mut x, &mut year, "Year") {
        details.year = year.trim().parse().ok();
        changed = true;
    }
    changed
}

/// "2 discs ripped · insert disc 3 · 9 chapters, 1:41:05 · 3 suggestions · builds one .m4b, …"
fn details_text(session: &Session, book: &Book) -> String {
    let ripped = book.discs.iter().filter(|d| d.is_ripped()).count();
    let discs = if ripped == 1 { "1 disc ripped".to_owned() } else { format!("{ripped} discs ripped") };
    let chapters = book.output_chapters().len();
    let length = format_total_time(std::time::Duration::from_secs_f64(seconds(book.kept_samples())));
    let mut parts = vec![discs, state_word(session), format!("{chapters} chapters, {length}")];
    let suggestions = session.audiobook_suggestions().len();
    if suggestions > 0 {
        parts.push(if suggestions == 1 { "1 suggestion".to_owned() } else { format!("{suggestions} suggestions") });
    }
    parts.push(format!("builds {}", book.output.describe()));
    parts.join(" · ")
}
