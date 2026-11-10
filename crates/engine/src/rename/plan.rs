//! Planning a rename: for each chosen song, where its file is and where the pattern puts it,
//! and whether it can go there. Pure: what the disk says comes in through [`Disk`], so every
//! rule is tested without touching a file.
//!
//! A song is renamed only when nothing stands in the way. Songs that cannot be renamed are
//! *skipped* (their file is not there to rename, or a read-only folder protects it), and songs
//! whose new name is taken are *in conflict*: two songs given one name, a file already there,
//! or a name another song in the library has. Names are compared as the disk compares them
//! ([`CaseRule`]), so on Windows and macOS `Intro.flac` and `intro.flac` clash.

use crate::file_check::FileStatus;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tunebox_model::naming::NamePattern;
use tunebox_model::organize::CaseRule;
use tunebox_model::{TrackId, TrackMeta};

/// Where renamed files are filed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenameBase {
    /// Inside the library folder each song is in (or the media folder, for songs in it), so
    /// files stay where they are kept.
    #[default]
    OwnFolder,
    /// Inside the media folder.
    MediaFolder,
}

/// What the listener asked for: these songs, named by this pattern, filed under this base.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenameRequest {
    pub tracks: Vec<TrackId>,
    pub pattern: String,
    pub base: RenameBase,
}

/// What would happen to one song.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenameItem {
    pub id: TrackId,
    pub from: PathBuf,
    /// The folder the new path is inside (the song's library folder or the media folder), for
    /// showing paths short; `None` when skipped before one was known.
    pub base: Option<PathBuf>,
    pub planned: Planned,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Planned {
    Rename {
        to: PathBuf,
    },
    /// Already named as the pattern says.
    Unchanged,
    Skipped(SkipReason),
    Conflict {
        to: PathBuf,
        reason: Conflict,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// The file is not where the library says.
    Missing,
    /// The drive or share is not connected; it is not touched at all.
    Unavailable(String),
    /// It, or where it would go, is in a read-only library folder.
    ReadOnly,
    /// Not inside a library folder or the media folder, so there is no folder of its own to
    /// file it in.
    OutsideFolders,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conflict {
    /// Other chosen songs would get the same name.
    SameNameAs(Vec<TrackId>),
    /// A file that is not this song is already there.
    FileExists,
    /// Another song in the library is at that name already (its file may be missing).
    TakenBy(TrackId),
}

impl SkipReason {
    pub fn text(&self) -> String {
        match self {
            SkipReason::Missing => "its file is missing".into(),
            SkipReason::Unavailable(place) => format!("“{place}” is not connected"),
            SkipReason::ReadOnly => "it is in a read-only library folder".into(),
            SkipReason::OutsideFolders => "it is not in a library folder or the media folder".into(),
        }
    }
}

impl Conflict {
    pub fn text(&self) -> String {
        match self {
            Conflict::SameNameAs(others) if others.len() == 1 => "another chosen song gets the same name".into(),
            Conflict::SameNameAs(others) => format!("{} other chosen songs get the same name", others.len()),
            Conflict::FileExists => "a file with that name is already there".into(),
            Conflict::TakenBy(_) => "another song in the library has that name".into(),
        }
    }
}

/// The plan for a whole request: one item per song, in the order asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenamePlan {
    pub request: RenameRequest,
    pub items: Vec<RenameItem>,
}

impl RenamePlan {
    /// The songs that would be renamed: (song, from, to).
    pub fn renames(&self) -> impl Iterator<Item = (TrackId, &Path, &Path)> {
        self.items.iter().filter_map(|item| match &item.planned {
            Planned::Rename { to } => Some((item.id, item.from.as_path(), to.as_path())),
            _ => None,
        })
    }

    pub fn count(&self, kind: fn(&Planned) -> bool) -> usize {
        self.items.iter().filter(|item| kind(&item.planned)).count()
    }

    pub fn renamed(&self) -> usize {
        self.count(|p| matches!(p, Planned::Rename { .. }))
    }

    pub fn unchanged(&self) -> usize {
        self.count(|p| matches!(p, Planned::Unchanged))
    }

    pub fn skipped(&self) -> usize {
        self.count(|p| matches!(p, Planned::Skipped(_)))
    }

    pub fn conflicts(&self) -> usize {
        self.count(|p| matches!(p, Planned::Conflict { .. }))
    }

    /// One line: "12 to rename, 3 already named so, 1 skipped, 2 in conflict."
    pub fn summary(&self) -> String {
        let mut parts = vec![match self.renamed() {
            1 => "1 file to rename".to_owned(),
            n => format!("{n} files to rename"),
        }];
        let unchanged = self.unchanged();
        if unchanged > 0 {
            parts.push(format!("{unchanged} already named so"));
        }
        let skipped = self.skipped();
        if skipped > 0 {
            parts.push(format!("{skipped} skipped"));
        }
        let conflicts = self.conflicts();
        if conflicts > 0 {
            parts.push(format!("{conflicts} in conflict"));
        }
        format!("{}.", parts.join(", "))
    }
}

/// A chosen song, as planning needs it.
pub(crate) struct Song<'a> {
    pub id: TrackId,
    pub location: &'a Path,
    pub meta: &'a TrackMeta,
    pub status: FileStatus,
}

/// The folders songs can be filed in.
pub(crate) struct Folders<'a> {
    /// The library folders and the media folder.
    pub roots: &'a [PathBuf],
    pub media_folder: &'a Path,
}

impl Folders<'_> {
    /// The deepest root a song is in, as the deepest library folder decides its rules.
    fn own_root(&self, location: &Path) -> Option<&Path> {
        self.roots
            .iter()
            .filter(|root| location.starts_with(root))
            .max_by_key(|root| root.components().count())
            .map(PathBuf::as_path)
    }
}

/// What is at a path on disk, compared to the song's own file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Occupant {
    Nothing,
    /// The song's own file: the new name differs from the old only in case, on a disk that
    /// does not tell case apart.
    Itself,
    Other,
}

/// What planning asks the disk.
pub(crate) trait Disk {
    /// Whether TuneBox may change `path` (`files::may_write`).
    fn writable(&self, path: &Path) -> bool;
    /// What is at `to`, given that the song's file is at `from`.
    fn occupant(&self, from: &Path, to: &Path) -> Occupant;
}

/// Plans `songs`. `taken` says which song in the library is at a path, if any.
pub(crate) fn plan(
    songs: &[Song<'_>],
    pattern: &NamePattern,
    base: RenameBase,
    folders: &Folders<'_>,
    case: CaseRule,
    taken: impl Fn(&Path) -> Option<TrackId>,
    disk: &impl Disk,
) -> Vec<RenameItem> {
    let mut items: Vec<RenameItem> = songs.iter().map(|song| first_look(song, pattern, base, folders, disk)).collect();

    // Names already in use by chosen songs that stay where they are.
    let staying: HashMap<String, TrackId> = items
        .iter()
        .filter(|item| !matches!(item.planned, Planned::Rename { .. }))
        .map(|item| (case.key(&item.from), item.id))
        .collect();
    let mut wanted: HashMap<String, Vec<TrackId>> = HashMap::new();
    for item in &items {
        if let Planned::Rename { to } = &item.planned {
            wanted.entry(case.key(to)).or_default().push(item.id);
        }
    }

    for item in &mut items {
        let Planned::Rename { to } = &item.planned else { continue };
        let key = case.key(to);
        let others: Vec<TrackId> =
            wanted.get(&key).into_iter().flatten().copied().filter(|other| *other != item.id).collect();
        let conflict = if !others.is_empty() {
            Some(Conflict::SameNameAs(others))
        } else if let Some(other) = staying.get(&key).copied().or_else(|| taken(to)).filter(|other| *other != item.id) {
            Some(Conflict::TakenBy(other))
        } else if disk.occupant(&item.from, to) == Occupant::Other {
            Some(Conflict::FileExists)
        } else {
            None
        };
        if let Some(reason) = conflict {
            item.planned = Planned::Conflict { to: to.clone(), reason };
        }
    }
    items
}

/// One song on its own: skipped, unchanged, or a rename still to be checked against the rest.
fn first_look(
    song: &Song<'_>,
    pattern: &NamePattern,
    base: RenameBase,
    folders: &Folders<'_>,
    disk: &impl Disk,
) -> RenameItem {
    let item =
        |base: Option<PathBuf>, planned| RenameItem { id: song.id, from: song.location.to_path_buf(), base, planned };
    match &song.status {
        FileStatus::Present => {}
        FileStatus::Missing => return item(None, Planned::Skipped(SkipReason::Missing)),
        FileStatus::Unavailable(place) => return item(None, Planned::Skipped(SkipReason::Unavailable(place.name()))),
    }
    let root = match base {
        RenameBase::OwnFolder => folders.own_root(song.location),
        RenameBase::MediaFolder => Some(folders.media_folder),
    };
    let Some(root) = root else { return item(None, Planned::Skipped(SkipReason::OutsideFolders)) };
    let stem = song.location.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let extension = song.location.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let to = root.join(pattern.render(song.meta, &stem, &extension));
    let base = Some(root.to_path_buf());
    if to == song.location {
        item(base, Planned::Unchanged)
    } else if !disk.writable(song.location) || !disk.writable(&to) {
        item(base, Planned::Skipped(SkipReason::ReadOnly))
    } else {
        item(base, Planned::Rename { to })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::places::Place;
    use std::collections::HashSet;
    use tunebox_model::NumberOf;

    /// A pretend disk: these files exist, these folders are read only, and on a
    /// case-insensitive disk a name differing in case is the file itself.
    struct FakeDisk {
        files: HashSet<PathBuf>,
        read_only: Vec<PathBuf>,
        case: CaseRule,
    }

    impl Disk for FakeDisk {
        fn writable(&self, path: &Path) -> bool {
            !self.read_only.iter().any(|folder| path.starts_with(folder))
        }
        fn occupant(&self, from: &Path, to: &Path) -> Occupant {
            match self.files.iter().find(|f| self.case.same_name(f, to)) {
                None => Occupant::Nothing,
                Some(found) if self.case.same_name(found, from) => Occupant::Itself,
                Some(_) => Occupant::Other,
            }
        }
    }

    fn meta(title: &str, track: u32) -> TrackMeta {
        TrackMeta {
            title: title.into(),
            artist: "Quiet Harbour".into(),
            album: "Lanterns".into(),
            track: NumberOf { number: Some(track), total: None },
            ..TrackMeta::default()
        }
    }

    struct Setup {
        songs: Vec<(TrackId, PathBuf, TrackMeta, FileStatus)>,
        disk: FakeDisk,
        roots: Vec<PathBuf>,
    }

    impl Setup {
        fn new(case: CaseRule) -> Self {
            Setup {
                songs: Vec::new(),
                disk: FakeDisk { files: HashSet::new(), read_only: Vec::new(), case },
                roots: vec![PathBuf::from("/music"), PathBuf::from("/media")],
            }
        }

        fn song(&mut self, id: u64, path: &str, meta: TrackMeta) {
            self.disk.files.insert(PathBuf::from(path));
            self.songs.push((TrackId(id), PathBuf::from(path), meta, FileStatus::Present));
        }

        fn plan(&self, pattern: &str, base: RenameBase, taken: &[(&str, u64)]) -> Vec<Planned> {
            let songs: Vec<Song<'_>> = self
                .songs
                .iter()
                .map(|(id, location, meta, status)| Song { id: *id, location, meta, status: status.clone() })
                .collect();
            let folders = Folders { roots: &self.roots, media_folder: Path::new("/media") };
            let taken = |path: &Path| taken.iter().find(|(p, _)| Path::new(p) == path).map(|(_, id)| TrackId(*id));
            let pattern = NamePattern::parse(pattern).unwrap();
            plan(&songs, &pattern, base, &folders, self.disk.case, taken, &self.disk)
                .into_iter()
                .map(|item| item.planned)
                .collect()
        }
    }

    fn rename(to: &str) -> Planned {
        Planned::Rename { to: PathBuf::from(to) }
    }

    #[test]
    fn songs_are_filed_inside_their_own_folder_or_the_media_folder() {
        let mut setup = Setup::new(CaseRule::Sensitive);
        setup.song(1, "/music/old/a.flac", meta("First Light", 1));
        setup.song(2, "/music/Quiet Harbour/Lanterns/02 Second Light.mp3", meta("Second Light", 2));
        setup.song(3, "/downloads/c.flac", meta("Third", 3));
        let pattern = "{artist}/{album}/{track:02} {title}";
        assert_eq!(
            setup.plan(pattern, RenameBase::OwnFolder, &[]),
            [
                rename("/music/Quiet Harbour/Lanterns/01 First Light.flac"),
                Planned::Unchanged,
                Planned::Skipped(SkipReason::OutsideFolders),
            ]
        );
        assert_eq!(
            setup.plan(pattern, RenameBase::MediaFolder, &[]),
            [
                rename("/media/Quiet Harbour/Lanterns/01 First Light.flac"),
                rename("/media/Quiet Harbour/Lanterns/02 Second Light.mp3"),
                rename("/media/Quiet Harbour/Lanterns/03 Third.flac"),
            ]
        );
    }

    #[test]
    fn the_deepest_folder_is_the_songs_own() {
        let mut setup = Setup::new(CaseRule::Sensitive);
        setup.roots.push(PathBuf::from("/music/Imports"));
        setup.song(1, "/music/Imports/x/a.flac", meta("A", 1));
        assert_eq!(setup.plan("{title}", RenameBase::OwnFolder, &[]), [rename("/music/Imports/A.flac")]);
    }

    #[test]
    fn missing_unavailable_and_read_only_songs_are_skipped() {
        let mut setup = Setup::new(CaseRule::Sensitive);
        setup.song(1, "/music/a.flac", meta("A", 1));
        setup.song(2, "/music/b.flac", meta("B", 2));
        setup.song(3, "/music/protected/c.flac", meta("C", 3));
        setup.song(4, "/music/d.flac", meta("D", 4));
        setup.songs[0].3 = FileStatus::Missing;
        setup.songs[1].3 = FileStatus::Unavailable(Place { root: PathBuf::from("/music"), network: true });
        setup.disk.read_only.push(PathBuf::from("/music/protected"));
        setup.disk.read_only.push(PathBuf::from("/media"));
        assert_eq!(
            setup.plan("{title}", RenameBase::OwnFolder, &[]),
            [
                Planned::Skipped(SkipReason::Missing),
                Planned::Skipped(SkipReason::Unavailable("music".into())),
                Planned::Skipped(SkipReason::ReadOnly),
                rename("/music/D.flac"),
            ]
        );
        // Out of a writable folder into a read-only one is skipped too.
        assert_eq!(setup.plan("{title}", RenameBase::MediaFolder, &[])[3], Planned::Skipped(SkipReason::ReadOnly));
    }

    #[test]
    fn two_songs_never_get_one_name() {
        let mut setup = Setup::new(CaseRule::Sensitive);
        setup.song(1, "/music/a.flac", meta("Intro", 1));
        setup.song(2, "/music/b.flac", meta("Intro", 1));
        setup.song(3, "/music/c.flac", meta("Outro", 9));
        let planned = setup.plan("{title}", RenameBase::OwnFolder, &[]);
        let same = |other| Planned::Conflict {
            to: PathBuf::from("/music/Intro.flac"),
            reason: Conflict::SameNameAs(vec![TrackId(other)]),
        };
        assert_eq!(planned, [same(2), same(1), rename("/music/Outro.flac")]);
    }

    #[test]
    fn names_differing_only_in_case_clash_where_the_disk_ignores_case() {
        let mut insensitive = Setup::new(CaseRule::Insensitive);
        insensitive.song(1, "/music/a.flac", meta("Intro", 1));
        insensitive.song(2, "/music/b.flac", meta("intro", 1));
        let planned = insensitive.plan("{title}", RenameBase::OwnFolder, &[]);
        assert!(planned.iter().all(|p| matches!(p, Planned::Conflict { reason: Conflict::SameNameAs(_), .. })));

        let mut sensitive = Setup::new(CaseRule::Sensitive);
        sensitive.songs = insensitive.songs;
        sensitive.disk.files = insensitive.disk.files;
        assert_eq!(
            sensitive.plan("{title}", RenameBase::OwnFolder, &[]),
            [rename("/music/Intro.flac"), rename("/music/intro.flac")]
        );
    }

    #[test]
    fn a_name_in_use_is_a_conflict_unless_it_is_the_song_itself() {
        let mut setup = Setup::new(CaseRule::Insensitive);
        setup.song(1, "/music/first light.flac", meta("First Light", 1));
        setup.song(2, "/music/b.flac", meta("Second", 2));
        setup.song(3, "/music/c.flac", meta("Third", 3));
        setup.song(4, "/music/Fourth.flac", meta("Fourth", 4));
        setup.song(5, "/music/e.flac", meta("Fourth", 5));
        setup.disk.files.insert(PathBuf::from("/music/second.flac"));
        let planned = setup.plan("{title}", RenameBase::OwnFolder, &[("/music/Third.flac", 99)]);
        assert_eq!(planned[0], rename("/music/First Light.flac"), "a case-only rename of itself");
        assert_eq!(
            planned[1],
            Planned::Conflict { to: PathBuf::from("/music/Second.flac"), reason: Conflict::FileExists }
        );
        assert_eq!(
            planned[2],
            Planned::Conflict { to: PathBuf::from("/music/Third.flac"), reason: Conflict::TakenBy(TrackId(99)) },
            "a song in the library whose file is missing keeps its name"
        );
        assert_eq!(planned[3], Planned::Unchanged);
        assert_eq!(
            planned[4],
            Planned::Conflict { to: PathBuf::from("/music/Fourth.flac"), reason: Conflict::TakenBy(TrackId(4)) },
            "a chosen song that stays where it is keeps its name"
        );
    }

    #[test]
    fn the_summary_counts_each_kind() {
        let item = |planned| RenameItem { id: TrackId(1), from: PathBuf::new(), base: None, planned };
        let request = RenameRequest { tracks: Vec::new(), pattern: String::new(), base: RenameBase::OwnFolder };
        let plan = RenamePlan {
            request,
            items: vec![
                item(rename("/a")),
                item(rename("/b")),
                item(Planned::Unchanged),
                item(Planned::Skipped(SkipReason::Missing)),
            ],
        };
        assert_eq!(plan.summary(), "2 files to rename, 1 already named so, 1 skipped.");
        assert_eq!(plan.renames().count(), 2);
    }
}
