//! [`LocalCas`], the content store of a local workspace.
//!
//! Its own file because it is where the store's rules meet: what is held is never trusted over
//! the bytes in hand, nothing a build does removes a held file that is sound, and a file of the
//! workspace never lets a write reach the store.
//!
//! Responsibilities: [`LocalCas`] and its [`ContentStore`] implementation.
//!
//! Non-responsibilities: where a digest's files are, which `BlobPath` decides; and deciding
//! what is stored, cleared or placed, which the build does.

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use buildl_core::{
    ContentStore, Diagnostic, Digest, Error, FileMode, OutputPath, PortName, Result,
};

use crate::cas::blob::BlobPath;
use crate::cas::placement::Placement;
use crate::state::{Access, StateLayout, clear_path, write_whole};

/// Holds content under a workspace's state directory, and makes held content a file below the
/// workspace root.
///
/// Each digest has a blob, a read-only file holding the bytes, and may have an executable twin,
/// a second read-only file with the same bytes that may be run. A file of the workspace that
/// may be run is made from the twin, any other from the blob.
///
/// A file of the workspace is a hard link to the held file where a write through the link is
/// seen to be refused, and a copy of it everywhere else, so that writing to an output never
/// changes what the store holds.
#[derive(Debug)]
pub struct LocalCas {
    root: PathBuf,
    blobs: PathBuf,
    staging: PathBuf,
    placement: OnceLock<Placement>,
}

/// The store's failure doing `what` to `path`.
fn failed(what: &str, path: &Path, error: &io::Error) -> Error {
    refused(format!("{what} {}: {error}", path.display()))
}

/// A failure of the store, worded by `message`.
fn refused(message: String) -> Error {
    Error::Adapter {
        port: PortName::ContentStore,
        diagnostic: Diagnostic::new(message),
    }
}

/// Whether a regular file is at `path`. A path that cannot be examined holds none.
fn is_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
}

/// Whether something other than `bytes` is at `twin`. Nothing being there is not a difference:
/// only a placement makes a twin.
fn twin_differs(twin: &Path, bytes: &[u8]) -> bool {
    match fs::read(twin) {
        Ok(held) => held != bytes,
        Err(error) => error.kind() != ErrorKind::NotFound,
    }
}

impl LocalCas {
    /// A store for the workspace at `root`, keeping content where `layout` says.
    ///
    /// Touches nothing on disk: the store's directories are made by the first
    /// [`put`](ContentStore::put).
    #[must_use]
    pub fn new(root: &Path, layout: &StateLayout) -> Self {
        Self {
            root: root.to_path_buf(),
            blobs: layout.blobs(),
            staging: layout.blob_staging(),
            placement: OnceLock::new(),
        }
    }

    /// How this store places a file: probed on the first call, and kept.
    fn placement(&self) -> Result<Placement> {
        if let Some(known) = self.placement.get() {
            return Ok(*known);
        }
        let probed = Placement::probe(&self.staging)
            .map_err(|error| failed("probing", &self.staging, &error))?;
        Ok(*self.placement.get_or_init(|| probed))
    }

    /// Makes the executable twin of a blob when it is missing, from the blob's bytes once they
    /// are seen to have the digest.
    fn ensure_twin(&self, paths: &BlobPath, digest: &Digest) -> Result<()> {
        let twin = paths.for_mode(FileMode::Executable);
        if is_file(twin) {
            return Ok(());
        }
        let blob = paths.for_mode(FileMode::Plain);
        let bytes = fs::read(blob).map_err(|error| failed("reading", blob, &error))?;
        if Digest::of(&bytes) != *digest {
            return Err(refused(format!(
                "{} does not hold the bytes its name says",
                blob.display()
            )));
        }
        write_whole(&self.staging, twin, &bytes, Access::ReadOnlyRunnable)
            .map_err(|error| failed("writing", twin, &error))
    }

    /// The nearest path above `target`, below the workspace root, that holds something other
    /// than a directory.
    fn in_the_way(&self, target: &Path) -> Option<PathBuf> {
        target
            .ancestors()
            .skip(1)
            .take_while(|above| *above != self.root)
            .find(|above| fs::symlink_metadata(above).is_ok() && !above.is_dir())
            .map(Path::to_path_buf)
    }

    /// Makes every directory above `target` that is missing.
    ///
    /// A file standing where a directory is needed is named and left as it is: the output
    /// directory is the user's choice, and the store removes only at a path it was asked to
    /// clear.
    fn make_parent(&self, target: &Path) -> Result<()> {
        let Some(parent) = target.parent() else {
            return Ok(());
        };
        fs::create_dir_all(parent).map_err(|error| {
            self.in_the_way(target).map_or_else(
                || failed("making directory", parent, &error),
                |obstacle| {
                    refused(format!(
                        "placing a file at {}: {} is in the way and is not a directory",
                        target.display(),
                        obstacle.display()
                    ))
                },
            )
        })
    }

    /// Makes `target` a file of its own holding the bytes of `source`, replacing what is there.
    fn copy(&self, source: &Path, target: &Path, access: Access) -> Result<()> {
        let bytes = fs::read(source).map_err(|error| failed("reading", source, &error))?;
        clear_path(target).map_err(|error| failed("clearing", target, &error))?;
        self.make_parent(target)?;
        let beside = target.parent().unwrap_or(&self.root);
        write_whole(beside, target, &bytes, access)
            .map_err(|error| failed("writing", target, &error))
    }

    /// [`place`](ContentStore::place), with the step that makes a hard link handed in.
    ///
    /// Where the store links, a link that fails for want of a directory, or because something
    /// is at the path, is tried once more after that is seen to; a link that fails for any
    /// other reason, or a second time, is followed by a copy, whose failure is the one
    /// reported. The link is never made beside the path and renamed onto it: renaming one name
    /// of a file onto another name of the same file does nothing, and leaves both.
    fn place_by(
        &self,
        digest: &Digest,
        mode: FileMode,
        at: &OutputPath,
        link: impl Fn(&Path, &Path) -> io::Result<()>,
    ) -> Result<()> {
        let paths = BlobPath::of(&self.blobs, digest);
        let target = self.root.join(at.as_str());
        if !is_file(paths.for_mode(FileMode::Plain)) {
            return Err(refused(format!(
                "placing {digest} at {}: the store does not hold it",
                target.display()
            )));
        }
        let access = match mode {
            FileMode::Plain => Access::ReadOnly,
            FileMode::Executable => {
                self.ensure_twin(&paths, digest)?;
                Access::ReadOnlyRunnable
            }
        };
        let source = paths.for_mode(mode);
        if self.placement()? == Placement::Link {
            let again = match link(source, &target) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    self.make_parent(&target)?;
                    if mode == FileMode::Executable {
                        self.ensure_twin(&paths, digest)?;
                    }
                    true
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    clear_path(&target).map_err(|error| failed("clearing", &target, &error))?;
                    true
                }
                Err(_) => false,
            };
            if again && link(source, &target).is_ok() {
                return Ok(());
            }
        }
        self.copy(source, &target, access)
    }
}

impl ContentStore for LocalCas {
    /// Stores `bytes` and returns their digest.
    ///
    /// The blob, and the twin when there is one, are each read and compared with `bytes`, and
    /// written again only when they differ: a name is never trusted over the bytes in hand, so
    /// storing content again heals a damaged file. No twin is made and no file is removed.
    fn put(&self, bytes: &[u8]) -> Result<Digest> {
        let digest = Digest::of(bytes);
        let paths = BlobPath::of(&self.blobs, &digest);
        let blob = paths.for_mode(FileMode::Plain);
        if !fs::read(blob).is_ok_and(|held| held == bytes) {
            write_whole(&self.staging, blob, bytes, Access::ReadOnly)
                .map_err(|error| failed("writing", blob, &error))?;
        }
        let twin = paths.for_mode(FileMode::Executable);
        if twin_differs(twin, bytes) {
            write_whole(&self.staging, twin, bytes, Access::ReadOnlyRunnable)
                .map_err(|error| failed("writing", twin, &error))?;
        }
        Ok(digest)
    }

    fn contains(&self, digest: &Digest) -> bool {
        is_file(BlobPath::of(&self.blobs, digest).for_mode(FileMode::Plain))
    }

    /// The content held under that digest.
    ///
    /// A blob whose bytes do not have the digest it is named by is removed with its twin, and
    /// the content is then not held.
    fn get(&self, digest: &Digest) -> Result<Option<Vec<u8>>> {
        let paths = BlobPath::of(&self.blobs, digest);
        let blob = paths.for_mode(FileMode::Plain);
        let bytes = match fs::read(blob) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(failed("reading", blob, &error)),
        };
        if Digest::of(&bytes) == *digest {
            return Ok(Some(bytes));
        }
        for damaged in [blob, paths.for_mode(FileMode::Executable)] {
            clear_path(damaged).map_err(|error| failed("removing", damaged, &error))?;
        }
        Ok(None)
    }

    fn clear(&self, at: &OutputPath) -> Result<()> {
        let target = self.root.join(at.as_str());
        clear_path(&target).map_err(|error| failed("clearing", &target, &error))
    }

    /// Makes the content with that digest the file at `at`.
    ///
    /// A file that may be run is made from the executable twin, which is made first when it is
    /// missing; any other from the blob. The file is a hard link where the store links, and a
    /// copy where it does not or where the link cannot be made.
    fn place(&self, digest: &Digest, mode: FileMode, at: &OutputPath) -> Result<()> {
        self.place_by(digest, mode, at, |source, target| {
            fs::hard_link(source, target)
        })
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::cell::Cell;
    use std::fs::{self, OpenOptions};
    use std::io::{self, ErrorKind};
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::thread;

    use buildl_core::{
        ContentStore, Digest, Directory, Error, FileMode, Label, OutputName, OutputPath, PortName,
    };

    use super::LocalCas;
    use crate::cas::blob::BlobPath;
    use crate::cas::placement::Placement;
    use crate::state::StateLayout;

    /// The blob of the bytes `hello`, below the workspace root.
    const HELLO: &str =
        ".buildl/cas/2c/f24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    /// A store in a new, empty workspace, with the workspace's root.
    fn store() -> (tempfile::TempDir, PathBuf, LocalCas) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let layout = StateLayout::new(&root, &Directory::parse(".buildl").unwrap());
        let cas = LocalCas::new(&root, &layout);
        (dir, root, cas)
    }

    /// The place of the output `name` of a target at the workspace root: `out/<name>`.
    fn at(name: &str) -> OutputPath {
        OutputPath::of(
            &Directory::parse("out").unwrap(),
            &Label::parse("//:t").unwrap(),
            &OutputName::parse(name).unwrap(),
        )
    }

    /// The permission bits of the file at `path`.
    fn mode_of(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    /// The permission bits the umask lets through: those of a file made asking for all of them.
    fn allowed(root: &Path) -> u32 {
        let path = root.join("mask-probe");
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o777)
            .open(&path)
            .unwrap();
        let mode = mode_of(&path);
        fs::remove_file(&path).unwrap();
        mode
    }

    /// The inode of the file at `path`.
    fn inode_of(path: &Path) -> u64 {
        fs::metadata(path).unwrap().ino()
    }

    /// Makes the file at `path`, which the store wrote read-only, hold `bytes` instead.
    fn damage(path: &Path, bytes: &[u8]) {
        fs::remove_file(path).unwrap();
        fs::write(path, bytes).unwrap();
    }

    /// Every file the store holds: its path below `root`, its mode and its bytes, in path order.
    /// The staging directory is left out.
    fn held(root: &Path) -> Vec<(String, u32, Vec<u8>)> {
        let mut files = Vec::new();
        let mut pending = vec![root.join(".buildl/cas")];
        while let Some(directory) = pending.pop() {
            let Ok(entries) = fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    if !path.ends_with("cas/tmp") {
                        pending.push(path);
                    }
                } else {
                    let name = path
                        .strip_prefix(root)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned();
                    files.push((name, mode_of(&path), fs::read(&path).unwrap()));
                }
            }
        }
        files.sort();
        files
    }

    /// How many entries the store's staging directory holds.
    fn staged(root: &Path) -> usize {
        fs::read_dir(root.join(".buildl/cas/tmp")).unwrap().count()
    }

    #[test]
    fn put_holds_the_bytes_under_their_digest_read_only() {
        let (_dir, root, cas) = store();
        assert!(!cas.contains(&Digest::of(b"hello")));

        let digest = cas.put(b"hello").unwrap();

        assert_eq!(digest, Digest::of(b"hello"));
        assert_eq!(
            held(&root),
            [(HELLO.to_owned(), 0o444 & allowed(&root), b"hello".to_vec())]
        );
        assert!(cas.contains(&digest));
        assert!(!cas.contains(&Digest::of(b"other")));
        assert_eq!(cas.get(&digest).unwrap(), Some(b"hello".to_vec()));
        assert_eq!(staged(&root), 0);
    }

    #[test]
    fn put_of_held_bytes_keeps_the_inode_of_the_blob_and_of_its_twin() {
        let (_dir, root, cas) = store();
        let digest = cas.put(b"hello").unwrap();
        cas.place(&digest, FileMode::Executable, &at("tool"))
            .unwrap();
        let blob = root.join(HELLO);
        let twin = root.join(format!("{HELLO}.x"));
        let before = (inode_of(&blob), inode_of(&twin));

        assert_eq!(cas.put(b"hello").unwrap(), digest);

        assert_eq!((inode_of(&blob), inode_of(&twin)), before);
    }

    #[test]
    fn put_replaces_a_blob_whose_bytes_differ() {
        let (_dir, root, cas) = store();
        let digest = cas.put(b"hello").unwrap();
        let blob = root.join(HELLO);
        damage(&blob, b"EVIL!");

        assert_eq!(cas.put(b"hello").unwrap(), digest);

        assert_eq!(
            held(&root),
            [(HELLO.to_owned(), 0o444 & allowed(&root), b"hello".to_vec())]
        );
    }

    #[test]
    fn put_replaces_a_twin_whose_bytes_differ_and_removes_none() {
        let (_dir, root, cas) = store();
        let digest = cas.put(b"hello").unwrap();
        cas.place(&digest, FileMode::Executable, &at("tool"))
            .unwrap();
        let blob = root.join(HELLO);
        let twin = root.join(format!("{HELLO}.x"));
        let sound = [
            (HELLO.to_owned(), 0o444 & allowed(&root), b"hello".to_vec()),
            (
                format!("{HELLO}.x"),
                0o555 & allowed(&root),
                b"hello".to_vec(),
            ),
        ];

        // A damaged twin beside a sound blob: the twin is written again, the blob is left alone.
        damage(&twin, b"EVIL!");
        let untouched = inode_of(&blob);
        cas.put(b"hello").unwrap();
        assert_eq!(held(&root), sound);
        assert_eq!(inode_of(&blob), untouched);

        // A damaged twin beside a damaged blob: both are written again, and both are still there.
        damage(&blob, b"EVIL!");
        damage(&twin, b"EVIL!");
        cas.put(b"hello").unwrap();
        assert_eq!(held(&root), sound);
    }

    #[test]
    fn put_makes_no_twin() {
        let (_dir, root, cas) = store();

        cas.put(b"hello").unwrap();
        cas.put(b"hello").unwrap();

        assert!(!root.join(format!("{HELLO}.x")).exists());
    }

    /// One whole call of the store, for one of two targets whose outputs hold the same bytes:
    /// `X` is placed as a file that may be run, `Y` as one that may not.
    #[derive(Debug, Clone, Copy)]
    enum Call {
        PutX,
        PlaceX,
        PutY,
        PlaceY,
    }

    #[test]
    fn the_twin_stays_whatever_order_puts_and_places_come_in() {
        use Call::{PlaceX, PlaceY, PutX, PutY};

        // Each target's put comes before its own place; these are the six ways two such pairs
        // can interleave.
        let orders = [
            [PutX, PlaceX, PutY, PlaceY],
            [PutX, PutY, PlaceX, PlaceY],
            [PutX, PutY, PlaceY, PlaceX],
            [PutY, PutX, PlaceX, PlaceY],
            [PutY, PutX, PlaceY, PlaceX],
            [PutY, PlaceY, PutX, PlaceX],
        ];
        for order in orders {
            let (_dir, root, cas) = store();
            let digest = Digest::of(b"hello");
            for call in order {
                match call {
                    PutX | PutY => assert_eq!(cas.put(b"hello").unwrap(), digest),
                    PlaceX => cas.place(&digest, FileMode::Executable, &at("x")).unwrap(),
                    PlaceY => cas.place(&digest, FileMode::Plain, &at("y")).unwrap(),
                }
            }

            assert_eq!(
                held(&root),
                [
                    (HELLO.to_owned(), 0o444 & allowed(&root), b"hello".to_vec()),
                    (
                        format!("{HELLO}.x"),
                        0o555 & allowed(&root),
                        b"hello".to_vec()
                    ),
                ],
                "{order:?}"
            );
            let (x, y) = (root.join("out/x"), root.join("out/y"));
            assert_eq!(
                (mode_of(&x), fs::read(&x).unwrap()),
                (0o555 & allowed(&root), b"hello".to_vec()),
                "{order:?}"
            );
            assert_eq!(
                (mode_of(&y), fs::read(&y).unwrap()),
                (0o444 & allowed(&root), b"hello".to_vec()),
                "{order:?}"
            );
        }
    }

    #[test]
    fn eight_threads_put_the_same_bytes() {
        let (_dir, root, cas) = store();
        let bytes = vec![7_u8; 1 << 20];

        // Every thread is started before any is waited for, so the puts overlap.
        let digests: Vec<Digest> = thread::scope(|scope| {
            let mut running = Vec::new();
            for _ in 0..8 {
                running.push(scope.spawn(|| cas.put(&bytes).unwrap()));
            }
            running
                .into_iter()
                .map(|thread| thread.join().unwrap())
                .collect()
        });

        assert_eq!(digests, [Digest::of(&bytes); 8]);
        let name = BlobPath::of(Path::new(".buildl/cas"), &Digest::of(&bytes));
        let name = name.for_mode(FileMode::Plain).to_str().unwrap().to_owned();
        assert_eq!(held(&root), [(name, 0o444 & allowed(&root), bytes)]);
        assert_eq!(staged(&root), 0);
    }

    #[test]
    fn get_of_nothing_is_none() {
        let (_dir, _root, cas) = store();
        assert_eq!(cas.get(&Digest::of(b"hello")).unwrap(), None);

        // The same once the store's directories exist.
        cas.put(b"other").unwrap();
        assert_eq!(cas.get(&Digest::of(b"hello")).unwrap(), None);
    }

    #[test]
    fn get_drops_a_blob_that_does_not_hash_to_its_name() {
        let (_dir, root, cas) = store();
        let digest = cas.put(b"hello").unwrap();
        cas.place(&digest, FileMode::Executable, &at("tool"))
            .unwrap();
        damage(&root.join(HELLO), b"EVIL!");

        assert_eq!(cas.get(&digest).unwrap(), None);

        assert_eq!(held(&root), []);
        assert!(!cas.contains(&digest));
    }

    #[test]
    fn clear_removes_a_file_or_a_directory_and_nothing_is_no_error() {
        let (_dir, root, cas) = store();
        let digest = cas.put(b"hello").unwrap();

        cas.clear(&at("app")).unwrap();

        cas.place(&digest, FileMode::Plain, &at("app")).unwrap();
        cas.clear(&at("app")).unwrap();
        assert!(!root.join("out/app").exists());

        cas.place(&digest, FileMode::Plain, &at("app/share/text"))
            .unwrap();
        cas.clear(&at("app")).unwrap();
        assert!(!root.join("out/app").exists());
        assert!(root.join("out").is_dir());
        assert_eq!(cas.get(&digest).unwrap(), Some(b"hello".to_vec()));
    }

    #[test]
    fn place_makes_directories_and_replaces_a_file_or_a_directory() {
        let (_dir, root, cas) = store();
        let hello = cas.put(b"hello").unwrap();
        let other = cas.put(b"other").unwrap();
        let target = root.join("out/lib/deep/text");

        // Nothing above the file exists yet.
        cas.place(&hello, FileMode::Plain, &at("lib/deep/text"))
            .unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"hello");

        // A file is there.
        cas.place(&other, FileMode::Executable, &at("lib/deep/text"))
            .unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"other");
        assert_eq!(mode_of(&target), 0o555 & allowed(&root));

        // A directory is there, with a file in it.
        cas.place(&hello, FileMode::Plain, &at("lib/deep")).unwrap();
        assert_eq!(fs::read(root.join("out/lib/deep")).unwrap(), b"hello");
        assert_eq!(mode_of(&root.join("out/lib/deep")), 0o444 & allowed(&root));
    }

    #[test]
    fn place_of_a_digest_not_held_fails() {
        let (_dir, root, cas) = store();
        let digest = Digest::of(b"hello");

        let error = cas.place(&digest, FileMode::Plain, &at("app")).unwrap_err();

        let Error::Adapter { port, diagnostic } = error else {
            unreachable!("an adapter error, got {error:?}")
        };
        assert_eq!(port, PortName::ContentStore);
        assert_eq!(
            diagnostic.as_str(),
            format!(
                "placing {digest} at {}: the store does not hold it",
                root.join("out/app").display()
            )
        );
        assert!(!root.join("out").exists());
    }

    #[test]
    fn place_names_a_file_standing_where_a_directory_is_needed() {
        let (_dir, root, cas) = store();
        let digest = cas.put(b"hello").unwrap();
        fs::create_dir_all(root.join("out")).unwrap();
        fs::write(root.join("out/app"), b"an older output").unwrap();

        for name in ["app/bin", "app/share/doc/text"] {
            let error = cas.place(&digest, FileMode::Plain, &at(name)).unwrap_err();

            let Error::Adapter { port, diagnostic } = error else {
                unreachable!("an adapter error, got {error:?}")
            };
            assert_eq!(port, PortName::ContentStore);
            assert_eq!(
                diagnostic.as_str(),
                format!(
                    "placing a file at {}: {} is in the way and is not a directory",
                    root.join("out").join(name).display(),
                    root.join("out/app").display()
                )
            );
            assert_eq!(fs::read(root.join("out/app")).unwrap(), b"an older output");
        }
    }

    /// Whether a store in the workspace at `root` links: not for a user the permission bits do
    /// not bind, and not on a filesystem that does not keep them.
    fn links(root: &Path) -> bool {
        Placement::probe(&root.join(".buildl/cas/tmp")).unwrap() == Placement::Link
    }

    #[test]
    fn a_plain_file_is_a_link_to_the_blob() {
        let (_dir, root, cas) = store();
        if !links(&root) {
            return;
        }
        let digest = cas.put(b"hello").unwrap();

        cas.place(&digest, FileMode::Plain, &at("text")).unwrap();

        let placed = root.join("out/text");
        assert_eq!(inode_of(&placed), inode_of(&root.join(HELLO)));
        assert_eq!(mode_of(&placed), 0o444 & allowed(&root));
    }

    #[test]
    fn one_blob_placed_plain_and_executable_uses_two_inodes() {
        let (_dir, root, cas) = store();
        if !links(&root) {
            return;
        }
        let digest = cas.put(b"hello").unwrap();

        cas.place(&digest, FileMode::Plain, &at("lib/text"))
            .unwrap();
        cas.place(&digest, FileMode::Executable, &at("bin/tool"))
            .unwrap();
        cas.place(&digest, FileMode::Executable, &at("bin/other"))
            .unwrap();

        let blob = inode_of(&root.join(HELLO));
        let twin = inode_of(&root.join(format!("{HELLO}.x")));
        assert_ne!(blob, twin);
        assert_eq!(inode_of(&root.join("out/lib/text")), blob);
        assert_eq!(inode_of(&root.join("out/bin/tool")), twin);
        assert_eq!(inode_of(&root.join("out/bin/other")), twin);
        assert_eq!(mode_of(&root.join("out/lib/text")), 0o444 & allowed(&root));
        assert_eq!(mode_of(&root.join("out/bin/tool")), 0o555 & allowed(&root));
    }

    #[test]
    fn a_write_through_a_placed_file_is_refused() {
        let (_dir, root, cas) = store();
        if !links(&root) {
            return;
        }
        let digest = cas.put(b"hello").unwrap();
        cas.place(&digest, FileMode::Plain, &at("text")).unwrap();
        cas.place(&digest, FileMode::Executable, &at("tool"))
            .unwrap();

        let mut write = OpenOptions::new();
        write.write(true);
        let mut truncate = OpenOptions::new();
        truncate.write(true).truncate(true);
        let mut append = OpenOptions::new();
        append.append(true);
        for placed in ["out/text", "out/tool"] {
            for how in [&write, &truncate, &append] {
                let error = how.open(root.join(placed)).unwrap_err();
                assert_eq!(error.kind(), ErrorKind::PermissionDenied, "{placed}");
            }
        }

        assert_eq!(
            held(&root),
            [
                (HELLO.to_owned(), 0o444 & allowed(&root), b"hello".to_vec()),
                (
                    format!("{HELLO}.x"),
                    0o555 & allowed(&root),
                    b"hello".to_vec()
                ),
            ]
        );
    }

    #[test]
    fn a_store_that_copies_shares_no_inode() {
        let (_dir, root, cas) = store();
        cas.placement.set(Placement::Copy).unwrap();
        let digest = cas.put(b"hello").unwrap();

        cas.place(&digest, FileMode::Plain, &at("text")).unwrap();
        cas.place(&digest, FileMode::Executable, &at("tool"))
            .unwrap();

        let held = [
            inode_of(&root.join(HELLO)),
            inode_of(&root.join(format!("{HELLO}.x"))),
        ];
        let (text, tool) = (root.join("out/text"), root.join("out/tool"));
        assert!(!held.contains(&inode_of(&text)));
        assert!(!held.contains(&inode_of(&tool)));
        assert_eq!(
            (mode_of(&text), fs::read(&text).unwrap()),
            (0o444 & allowed(&root), b"hello".to_vec())
        );
        assert_eq!(
            (mode_of(&tool), fs::read(&tool).unwrap()),
            (0o555 & allowed(&root), b"hello".to_vec())
        );

        // A copy replaces a directory standing at the path, as a link does.
        fs::create_dir_all(root.join("out/tree/inner")).unwrap();
        fs::write(root.join("out/tree/inner/old"), b"old").unwrap();
        cas.place(&digest, FileMode::Plain, &at("tree")).unwrap();
        assert_eq!(fs::read(root.join("out/tree")).unwrap(), b"hello");
    }

    #[test]
    fn a_link_that_fails_is_followed_by_a_copy() {
        let (_dir, root, cas) = store();
        cas.placement.set(Placement::Link).unwrap();
        let digest = cas.put(b"hello").unwrap();
        let placed = root.join("out/bin/tool");

        // Whatever the link answers, and it answers differently on each system, the file is made.
        for kind in [
            ErrorKind::Unsupported,
            ErrorKind::CrossesDevices,
            ErrorKind::NotFound,
            ErrorKind::AlreadyExists,
        ] {
            let failing = |_: &Path, _: &Path| Err(io::Error::from(kind));

            cas.place_by(&digest, FileMode::Executable, &at("bin/tool"), failing)
                .unwrap();

            assert_eq!(
                (mode_of(&placed), fs::read(&placed).unwrap()),
                (0o555 & allowed(&root), b"hello".to_vec()),
                "{kind:?}"
            );
            assert_ne!(
                inode_of(&placed),
                inode_of(&root.join(format!("{HELLO}.x"))),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn a_twin_gone_when_the_link_is_made_is_made_again() {
        let (_dir, root, cas) = store();
        cas.placement.set(Placement::Link).unwrap();
        let digest = cas.put(b"hello").unwrap();
        let attempts = Cell::new(0);
        // The first attempt finds the twin gone, as it would be had something removed it since
        // the store saw it; the second is the real link.
        let link = |source: &Path, target: &Path| {
            attempts.set(attempts.get() + 1);
            if attempts.get() == 1 {
                fs::remove_file(source)?;
            }
            fs::hard_link(source, target)
        };

        cas.place_by(&digest, FileMode::Executable, &at("bin/tool"), link)
            .unwrap();

        assert_eq!(attempts.get(), 2);
        assert_eq!(
            inode_of(&root.join("out/bin/tool")),
            inode_of(&root.join(format!("{HELLO}.x")))
        );
        assert_eq!(fs::read(root.join("out/bin/tool")).unwrap(), b"hello");
    }
}
