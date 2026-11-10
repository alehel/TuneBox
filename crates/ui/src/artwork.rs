//! Album artwork: read where the engine says the cover is (the song's file, or the cover TuneBox
//! keeps for a song in a read-only folder), decoded on a background thread, then kept as
//! textures, one per album. Pictures fetched from the Cover Art Archive (the CD's cover, the
//! picker's thumbnails) arrive as bytes and are kept by a name of their own.

use egui::{ColorImage, TextureHandle, TextureOptions};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
use tunebox_engine::model::Track;
use tunebox_engine::{CoverSource, FileStatus, Session};

pub use tunebox_engine::{AlbumKey, album_key};

/// Artwork is stored at this size; tiles and panes scale it down.
const THUMBNAIL: u32 = 300;

pub struct ArtworkCache {
    requests: Sender<(AlbumKey, CoverSource)>,
    results: Receiver<(AlbumKey, Option<ColorImage>)>,
    /// `None`: the album has no artwork.
    textures: HashMap<AlbumKey, Option<TextureHandle>>,
    pending: HashSet<AlbumKey>,
    /// Albums forgotten while their cover was loading: what arrives is the old cover.
    stale: HashSet<AlbumKey>,
    /// Fetched pictures by name; `None`: they could not be decoded.
    pictures: HashMap<String, Option<TextureHandle>>,
}

impl ArtworkCache {
    pub fn new() -> Self {
        let (requests, request_rx) = mpsc::channel::<(AlbumKey, CoverSource)>();
        let (result_tx, results) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("tunebox-artwork".into()).spawn(move || {
            for (key, source) in request_rx {
                if result_tx.send((key, load(&source))).is_err() {
                    break;
                }
            }
        });
        if let Err(e) = spawned {
            log::warn!("artwork will not be shown: {e}");
        }
        Self {
            requests,
            results,
            textures: HashMap::new(),
            pending: HashSet::new(),
            stale: HashSet::new(),
            pictures: HashMap::new(),
        }
    }

    /// `data` (a fetched JPEG or PNG) as a texture, decoded the first time it is asked for by
    /// `name`. They are small (a 250-pixel thumbnail, one CD cover), so decoding here is quick.
    pub fn picture(&mut self, ctx: &egui::Context, name: &str, data: &[u8]) -> Option<TextureHandle> {
        if let Some(texture) = self.pictures.get(name) {
            return texture.clone();
        }
        let texture =
            decode(data).map(|image| ctx.load_texture(format!("picture:{name}"), image, TextureOptions::LINEAR));
        self.pictures.insert(name.to_owned(), texture.clone());
        texture
    }

    /// The cover of `track`'s album. It is read from a song of the album whose file is there:
    /// covers are kept one per album, so reading it from a missing or unreachable song would
    /// leave the whole album without one.
    pub fn cover(&mut self, session: &Session, track: &Track) -> Option<TextureHandle> {
        let present = |t: &Track| *session.file_status(t.id) == FileStatus::Present;
        let key = album_key(track);
        if let Some(texture) = self.textures.get(&key) {
            return texture.clone();
        }
        if self.pending.contains(&key) {
            return None;
        }
        let source = if present(track) {
            track
        } else {
            session.library().tracks().find(|t| present(t) && album_key(t) == key)?
        };
        let source = session.cover_source(source.id)?;
        if self.pending.insert(key.clone()) && self.requests.send((key, source)).is_err() {
            log::debug!("artwork loader has stopped");
        }
        None
    }

    /// Uploads artwork that finished loading. Returns whether anything new arrived.
    pub fn poll(&mut self, ctx: &egui::Context) -> bool {
        let mut arrived = false;
        while let Ok((key, image)) = self.results.try_recv() {
            self.pending.remove(&key);
            if self.stale.remove(&key) {
                arrived = true;
                continue;
            }
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
        self.stale.extend(self.pending.iter().cloned());
    }

    /// Forgets one album's cover, after it changed.
    pub fn forget(&mut self, album: &AlbumKey) {
        self.textures.remove(album);
        if self.pending.contains(album) {
            self.stale.insert(album.clone());
        }
    }
}

/// Reads and decodes a cover. Any failure just means "no artwork".
fn load(source: &CoverSource) -> Option<ColorImage> {
    let artwork = source.load().ok()??;
    decode(&artwork.data)
}

/// Decodes a picture at the size artwork is kept. Any failure just means "no picture".
fn decode(data: &[u8]) -> Option<ColorImage> {
    let image = image::load_from_memory(data).ok()?;
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
