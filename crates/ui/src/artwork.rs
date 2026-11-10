//! Album artwork: read from the files' tags and decoded on a background thread, then kept as
//! textures, one per album.

use egui::{ColorImage, TextureHandle, TextureOptions};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use tunebox_engine::model::Track;
use tunebox_engine::{FileStatus, Session};

/// Artwork is stored at this size; tiles and panes scale it down.
const THUMBNAIL: u32 = 300;

/// Albums are identified by filing artist and title.
pub type AlbumKey = (String, String);

pub fn album_key(track: &Track) -> AlbumKey {
    (track.meta.filing_artist().to_owned(), track.meta.album.clone())
}

pub struct ArtworkCache {
    requests: Sender<(AlbumKey, PathBuf)>,
    results: Receiver<(AlbumKey, Option<ColorImage>)>,
    /// `None`: the album has no artwork.
    textures: HashMap<AlbumKey, Option<TextureHandle>>,
    pending: HashSet<AlbumKey>,
}

impl ArtworkCache {
    pub fn new() -> Self {
        let (requests, request_rx) = mpsc::channel::<(AlbumKey, PathBuf)>();
        let (result_tx, results) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("tunebox-artwork".into()).spawn(move || {
            for (key, path) in request_rx {
                if result_tx.send((key, load(&path))).is_err() {
                    break;
                }
            }
        });
        if let Err(e) = spawned {
            log::warn!("artwork will not be shown: {e}");
        }
        Self { requests, results, textures: HashMap::new(), pending: HashSet::new() }
    }

    /// The artwork of `track`'s album, loading it in the background the first time.
    /// The cover of `track`'s album. It is read from a song of the album whose file is there:
    /// covers are kept one per album, so reading it from a missing or unreachable song would
    /// leave the whole album without one.
    pub fn cover(&mut self, session: &Session, track: &Track) -> Option<TextureHandle> {
        let present = |t: &Track| *session.file_status(t.id) == FileStatus::Present;
        if present(track) {
            return self.get(track);
        }
        let key = album_key(track);
        let source = session.library().tracks().find(|t| present(t) && album_key(t) == key)?;
        self.get(source)
    }

    fn get(&mut self, track: &Track) -> Option<TextureHandle> {
        let key = album_key(track);
        if let Some(texture) = self.textures.get(&key) {
            return texture.clone();
        }
        if self.pending.insert(key.clone()) && self.requests.send((key, track.location.clone())).is_err() {
            log::debug!("artwork loader has stopped");
        }
        None
    }

    /// Uploads artwork that finished loading. Returns whether anything new arrived.
    pub fn poll(&mut self, ctx: &egui::Context) -> bool {
        let mut arrived = false;
        while let Ok((key, image)) = self.results.try_recv() {
            self.pending.remove(&key);
            let name = format!("artwork:{}:{}", key.0, key.1);
            let texture = image.map(|image| ctx.load_texture(name, image, TextureOptions::LINEAR));
            self.textures.insert(key, texture);
            arrived = true;
        }
        arrived
    }

    /// Whether every requested picture has arrived.
    pub fn is_idle(&self) -> bool {
        self.pending.is_empty()
    }

    /// Forgets everything (after artwork changed on disk).
    pub fn clear(&mut self) {
        self.textures.clear();
    }
}

/// Reads and decodes a file's cover. Any failure just means "no artwork".
fn load(path: &std::path::Path) -> Option<ColorImage> {
    let artwork = tunebox_engine::read_artwork(path).ok()??;
    let image = image::load_from_memory(&artwork.data).ok()?;
    let thumbnail = image.thumbnail(THUMBNAIL, THUMBNAIL).to_rgba8();
    let size = [thumbnail.width() as usize, thumbnail.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, thumbnail.as_raw()))
}

/// The picture shown for albums without artwork: a soft grey square with a note.
pub fn paint_placeholder(painter: &egui::Painter, rect: egui::Rect) {
    crate::theme::rounded_gradient(
        painter,
        rect,
        6.0,
        egui::Color32::from_rgb(0xEC, 0xEC, 0xEC),
        egui::Color32::from_rgb(0xC9, 0xC9, 0xC9),
    );
    let glyph = crate::glyphs::centered(rect, rect.width() * 0.42);
    crate::glyphs::paint(painter, glyph, crate::glyphs::Glyph::Note, egui::Color32::from_rgb(0xA5, 0xA5, 0xA5));
}

/// Paints `texture` filling `rect` with softly rounded corners, or the placeholder.
pub fn paint(ui: &egui::Ui, rect: egui::Rect, texture: Option<&TextureHandle>) {
    match texture {
        Some(texture) => egui::Image::from_texture(egui::load::SizedTexture::from_handle(texture))
            .corner_radius(6)
            .paint_at(ui, rect),
        None => paint_placeholder(ui.painter(), rect),
    }
}
