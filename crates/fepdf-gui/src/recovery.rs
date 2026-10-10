//! Autosave: what the window does to a document, on disk as it is done
//! ([ADR-0114](../../../docs/adr/0114-the-window-journals-its-acts-and-a-crash-replays-them.md)).
//!
//! **A journal of acts, not a copy of the document.** The history already rebuilds a
//! document by opening its origin again and replaying what remains, which is how undo
//! works. So a session writes the origin once, at the first act, and then appends each
//! act, undo and redo as it happens. Recovery is the same replay, and an undo after a
//! crash still has something to take back.
//!
//! **Whether a session is a crash is the operating system's answer.** The window holds a
//! lock on a file in its session's directory for as long as it runs, and the lock goes
//! when the process does, however it goes. A directory whose lock can be taken belongs
//! to a window that did not exit normally; one whose lock cannot be taken belongs to a
//! window still open, which is never offered as a crash.

use bytes::Bytes;
use fepdf::{AesV5Spec, Operation, SecurityHandler};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const ORIGIN: &str = "origin.pdf";
const JOURNAL: &str = "journal";
const LOCK: &str = "lock";

/// The revision the seal's strings are made at: 7.6.4.4's, which PDF 2.0 standardised.
const SEAL_REVISION: i32 = 6;

/// One thing the reader did, as the history records it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Entry {
    /// An act: the operations one gesture applied, in order.
    Act(Vec<Operation>),
    /// The last act was taken back.
    Undo,
    /// The last act taken back was put back.
    Redo,
}

/// An [`Entry`] as it is written, borrowing the act rather than taking it, so the
/// history keeps its own. **Serialised exactly as [`Entry`] is**: the same variant names,
/// and a slice is a sequence as a `Vec` is.
#[derive(Debug, Serialize)]
pub enum EntryRef<'a> {
    /// An act.
    Act(&'a [Operation]),
    /// An undo.
    Undo,
    /// A redo.
    Redo,
}

/// Where autosave stands for the open document.
pub enum Journal {
    /// Nowhere to write: the platform named no data directory.
    Off,
    /// Nothing done yet, so nothing written. The first act starts a session in `dir`,
    /// sealed with `seal` where the document was encrypted.
    Ready { dir: PathBuf, seal: Option<String> },
    /// A session is being written.
    Writing(Session),
    /// Writing failed. The reader was told once, and nothing more is attempted, since a
    /// journal with a hole in it replays a different document.
    Failed,
}

impl Journal {
    /// A journal that will write to `dir`, or none when there is no `dir`.
    pub fn ready(dir: Option<PathBuf>) -> Self {
        dir.map_or(Self::Off, |dir| Self::Ready { dir, seal: None })
    }

    /// Seals what is written with `password`, the one the document opened with.
    pub fn seal_with(&mut self, password: String) {
        if let Self::Ready { seal, .. } = self {
            *seal = Some(password);
        }
    }

    /// Writes `entry`, starting the session with `origin` and `name` if this is the
    /// first. Says why, the first time it cannot.
    pub fn record(
        &mut self,
        entry: &EntryRef<'_>,
        origin: &[u8],
        name: Option<&str>,
    ) -> Option<String> {
        if let Self::Ready { dir, seal } = self {
            match Session::begin(dir.clone(), origin, name, seal.as_deref()) {
                Ok(session) => *self = Self::Writing(session),
                Err(e) => return self.fail(&e),
            }
        }
        let Self::Writing(session) = self else { return None };
        match session.append(entry) {
            Ok(()) => None,
            Err(e) => self.fail(&e),
        }
    }

    fn fail(&mut self, why: &RecoveryError) -> Option<String> {
        *self = Self::Failed;
        Some(why.to_string())
    }

    /// Ends the session, removing what it wrote. Says why, if it could not.
    pub fn close(self) -> Option<String> {
        match self {
            Self::Writing(session) => session.end().err().map(|e| e.to_string()),
            Self::Off | Self::Ready { .. } | Self::Failed => None,
        }
    }
}

/// The journal's first record. **Never sealed**, because recovery has to read it to know
/// whether to ask for a password at all.
#[derive(Debug, Serialize, Deserialize)]
struct Header {
    /// What the window called the document.
    name: Option<String>,
    /// The `/U`, `/UE`, `/O` and `/OE` strings the seal's key is recovered from, when the
    /// document was encrypted. The key itself is never written.
    seal: Option<SealStrings>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SealStrings {
    u: Vec<u8>,
    ue: Vec<u8>,
    o: Vec<u8>,
    oe: Vec<u8>,
}

impl SealStrings {
    /// The key these strings wrap, if `password` unwraps it (Algorithm 2.A).
    fn open(&self, password: &str) -> Option<SecurityHandler> {
        let spec = AesV5Spec {
            u: &self.u,
            ue: &self.ue,
            o: &self.o,
            oe: &self.oe,
            revision: SEAL_REVISION,
            encrypt_metadata: true,
        };
        SecurityHandler::new_aes256(password, &spec)
    }
}

/// What went wrong with a journal.
#[derive(Debug)]
pub enum RecoveryError {
    /// The file system refused.
    Io(io::Error),
    /// A record would not serialise or parse.
    Json(serde_json::Error),
    /// The seal could not be made or applied.
    Seal(String),
    /// The journal is sealed, and the password given does not open it.
    WrongPassword,
    /// The journal has no header, so nothing in it can be read.
    NoHeader,
    /// Another window is recovering it, or still writing it.
    Held,
}

impl std::fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Json(e) => write!(f, "a journal record does not read: {e}"),
            Self::Seal(e) => write!(f, "the journal's seal failed: {e}"),
            Self::WrongPassword => write!(f, "that password does not open the journal"),
            Self::NoHeader => write!(f, "the journal has no header"),
            Self::Held => write!(f, "another window holds this session"),
        }
    }
}

impl From<io::Error> for RecoveryError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for RecoveryError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// Where sessions live: the platform's per-user data directory, then `fepdf/recovery`.
///
/// **Read from the environment rather than from a crate.** Three variables answer it on
/// the three platforms this window ships for, and a dependency for three lines is a
/// dependency `deny.toml` has to judge.
pub fn root() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        var("HOME").map(|home| home.join("Library").join("Application Support"))
    } else if cfg!(target_os = "windows") {
        var("LOCALAPPDATA")
    } else {
        var("XDG_STATE_HOME").or_else(|| var("HOME").map(|home| home.join(".local").join("state")))
    };
    base.map(|base| base.join("fepdf").join("recovery"))
}

/// A directory name for this window's session: its process and the moment it started,
/// so that a process number the system reuses does not land on a crashed window's
/// directory.
pub fn session_name() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    format!("{}-{millis}", std::process::id())
}

/// A journal being written.
pub struct Session {
    dir: PathBuf,
    journal: File,
    seal: Option<SecurityHandler>,
    /// How many entries are in the journal, which is also the next one's number.
    written: u32,
    /// Held for as long as the session runs; see the module's second paragraph.
    lock: File,
}

impl Session {
    /// Starts a session in `dir`: its lock, the origin, and the header.
    ///
    /// `seal` is the password to seal the journal with, for a document that was
    /// encrypted. **It is the password the document opened with**, empty where it opened
    /// with none, and it is not written anywhere.
    ///
    /// # Errors
    /// When the directory, the origin or the header cannot be written, or the seal
    /// cannot be made.
    pub fn begin(
        dir: PathBuf,
        origin: &[u8],
        name: Option<&str>,
        seal: Option<&str>,
    ) -> Result<Self, RecoveryError> {
        fs::create_dir_all(&dir)?;
        let lock = take_lock(&dir)?.ok_or(RecoveryError::Held)?;
        write_synced(&dir.join(ORIGIN), origin)?;
        let (seal, strings) = match seal {
            Some(password) => {
                let (handler, made) = SecurityHandler::encrypt_new(password, password, -1, true)
                    .map_err(|e| RecoveryError::Seal(e.to_string()))?;
                (
                    Some(handler),
                    Some(SealStrings { u: made.u, ue: made.ue, o: made.o, oe: made.oe }),
                )
            }
            None => (None, None),
        };
        let mut journal =
            OpenOptions::new().create(true).write(true).truncate(true).open(dir.join(JOURNAL))?;
        let header = Header { name: name.map(str::to_owned), seal: strings };
        write_record(&mut journal, &serde_json::to_vec(&header)?)?;
        Ok(Self { dir, journal, seal, written: 0, lock })
    }

    /// Picks up a session recovered from `dir`, so what is done next is journaled after
    /// what was done before the crash.
    ///
    /// # Errors
    /// When its journal will not open.
    pub fn resume(dir: PathBuf, recovered: Recovered) -> Result<Self, RecoveryError> {
        let Recovered { lock, seal, entries, intact_len, .. } = recovered;
        let journal = OpenOptions::new().append(true).open(dir.join(JOURNAL))?;
        // A record the crash cut short is cut off here, so what follows is read after
        // what came before it rather than inside it.
        journal.set_len(intact_len)?;
        journal.sync_data()?;
        let written = u32::try_from(entries.len()).unwrap_or(u32::MAX);
        Ok(Self { dir, journal, seal, written, lock })
    }

    /// Appends `entry`, and returns once it is on the disk.
    ///
    /// # Errors
    /// When it cannot be serialised, sealed or written.
    pub fn append(&mut self, entry: &EntryRef<'_>) -> Result<(), RecoveryError> {
        let plain = serde_json::to_vec(entry)?;
        let record = match &self.seal {
            Some(seal) => seal
                .encrypt_stream(&plain, self.written, 0)
                .map_err(|e| RecoveryError::Seal(e.to_string()))?,
            None => plain,
        };
        write_record(&mut self.journal, &record)?;
        self.written = self.written.saturating_add(1);
        Ok(())
    }

    /// Ends the session and removes what it wrote: the document is closed, so there is
    /// nothing left to recover.
    ///
    /// # Errors
    /// When the directory cannot be removed.
    pub fn end(self) -> io::Result<()> {
        let Self { dir, journal, lock, .. } = self;
        // Closed first, because Windows will not remove a directory holding an open file.
        drop(journal);
        drop(lock);
        fs::remove_dir_all(dir)
    }
}

/// A session a window left behind.
#[derive(Debug, Clone)]
pub struct Found {
    /// Its directory.
    pub dir: PathBuf,
    /// What the window called the document.
    pub name: Option<String>,
    /// Whether opening it needs the document's password.
    pub sealed: bool,
    /// How many entries the journal holds intact.
    pub entries: usize,
}

/// Every session under `root` whose window did not exit normally, and which holds at
/// least one entry. **A session with none is removed**, since there is nothing in it to
/// offer: its window crashed between writing the header and the first act.
pub fn find(root: &Path) -> Vec<Found> {
    let Ok(dirs) = fs::read_dir(root) else { return Vec::new() };
    let mut found = Vec::new();
    for dir in dirs.flatten().map(|entry| entry.path()).filter(|dir| dir.is_dir()) {
        match left_behind(&dir) {
            Some(session) if session.entries > 0 => found.push(session),
            // Nothing to offer. Should it not go, it is offered as nothing again next
            // time, which costs a directory read and nothing more.
            Some(_) => drop(discard(&dir)),
            None => {}
        }
    }
    // Newest first: the name starts with the process number, so this orders by the
    // directory's time stamp rather than by the name.
    found.sort_by_key(|session| {
        std::cmp::Reverse(fs::metadata(&session.dir).and_then(|m| m.modified()).ok())
    });
    found
}

/// What `dir` holds, if its window is gone. The lock taken to find out is released on
/// return.
fn left_behind(dir: &Path) -> Option<Found> {
    let _held = take_lock(dir).ok()??;
    let bytes = fs::read(dir.join(JOURNAL)).ok()?;
    let (records, _) = split_records(&bytes);
    let header: Header = serde_json::from_slice(records.first()?).ok()?;
    Some(Found {
        dir: dir.to_path_buf(),
        name: header.name,
        sealed: header.seal.is_some(),
        entries: records.len().saturating_sub(1),
    })
}

/// Removes a session the reader chose not to recover.
///
/// # Errors
/// When it cannot be removed.
pub fn discard(dir: &Path) -> io::Result<()> {
    fs::remove_dir_all(dir)
}

/// A session read back.
pub struct Recovered {
    /// The document as it was opened.
    pub origin: Bytes,
    /// What the window called it.
    pub name: Option<String>,
    /// Everything the journal holds intact, in order.
    pub entries: Vec<Entry>,
    /// Whether the journal ended in a record the crash cut short, which is lost.
    pub lost_tail: bool,
    /// The seal, so a resumed session goes on sealing with the same key.
    seal: Option<SecurityHandler>,
    /// How many bytes of the journal are intact.
    intact_len: u64,
    /// The session's lock, taken before anything was read, so that two windows offered
    /// the same crash cannot both recover it.
    lock: File,
}

/// Reads the session in `dir` back. `password` opens a sealed one and is ignored
/// otherwise.
///
/// # Errors
/// When the files will not read, the header will not parse, or the journal is sealed and
/// `password` does not open it.
pub fn recover(dir: &Path, password: Option<&str>) -> Result<Recovered, RecoveryError> {
    let lock = take_lock(dir)?.ok_or(RecoveryError::Held)?;
    let bytes = fs::read(dir.join(JOURNAL))?;
    let (records, intact) = split_records(&bytes);
    let mut records = records.into_iter();
    let header: Header = serde_json::from_slice(records.next().ok_or(RecoveryError::NoHeader)?)?;
    let seal = match &header.seal {
        Some(strings) => {
            Some(strings.open(password.unwrap_or_default()).ok_or(RecoveryError::WrongPassword)?)
        }
        None => None,
    };
    let mut entries = Vec::new();
    let mut lost_tail = intact < bytes.len();
    for (nth, record) in records.enumerate() {
        match read_entry(record, seal.as_ref(), u32::try_from(nth).unwrap_or(u32::MAX)) {
            Some(entry) => entries.push(entry),
            None => {
                lost_tail = true;
                break;
            }
        }
    }
    Ok(Recovered {
        origin: Bytes::from(fs::read(dir.join(ORIGIN))?),
        name: header.name,
        entries,
        lost_tail,
        seal,
        intact_len: u64::try_from(intact).unwrap_or(u64::MAX),
        lock,
    })
}

/// One record as an entry, unsealed with `seal` where there is one.
fn read_entry(record: &[u8], seal: Option<&SecurityHandler>, nth: u32) -> Option<Entry> {
    match seal {
        Some(seal) => serde_json::from_slice(&seal.decrypt_bytes(record, nth, 0).ok()?).ok(),
        None => serde_json::from_slice(record).ok(),
    }
}

/// The journal's records, and how many of its bytes they cover. A record whose length
/// runs past the end is where the crash cut it, and is not one of them.
fn split_records(bytes: &[u8]) -> (Vec<&[u8]>, usize) {
    let mut records = Vec::new();
    let mut at = 0usize;
    while let Some(len) = bytes.get(at..).and_then(|rest| rest.first_chunk::<4>()) {
        let len = usize::try_from(u32::from_le_bytes(*len)).unwrap_or(usize::MAX);
        let start = at.saturating_add(4);
        let Some(record) = bytes.get(start..start.saturating_add(len)) else { break };
        records.push(record);
        at = start.saturating_add(len);
    }
    (records, at)
}

/// Writes `record`, prefixed with its length, and syncs it.
fn write_record(journal: &mut File, record: &[u8]) -> io::Result<()> {
    let len =
        u32::try_from(record.len()).map_err(|_| io::Error::from(io::ErrorKind::FileTooLarge))?;
    let mut framed = Vec::with_capacity(record.len().saturating_add(4));
    framed.extend_from_slice(&len.to_le_bytes());
    framed.extend_from_slice(record);
    journal.write_all(&framed)?;
    journal.sync_data()
}

fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// The lock on `dir`'s session, or `None` when a running window holds it.
///
/// **Asked again for a moment before the answer is no.** A process that starts a child
/// lends it a copy of every open descriptor between the fork and the exec, and a lock
/// lent that way is held until the child execs. This window starts children to print,
/// to speak and to open a second window, so a lock it has just let go can read as held
/// for that instant. The suite caught it: one run in four failed a test that dropped a
/// session and recovered it at once. A window that is really running holds its lock for
/// longer than this waits.
fn take_lock(dir: &Path) -> io::Result<Option<File>> {
    const TRIES: u32 = 10;
    let file = OpenOptions::new().create(true).truncate(false).write(true).open(dir.join(LOCK))?;
    for _ in 0..TRIES {
        match file.try_lock() {
            Ok(()) => return Ok(Some(file)),
            Err(fs::TryLockError::WouldBlock) => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(fs::TryLockError::Error(e)) => return Err(e),
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fepdf::PageSelection;

    /// A directory of its own under the system's temporary one, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(label: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("fepdf-recovery-{label}-{}", session_name()));
            fs::create_dir_all(&dir).expect("a scratch directory");
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(&self.0));
        }
    }

    fn remove(index: usize) -> Vec<Operation> {
        vec![Operation::RemovePages(PageSelection::Single(index))]
    }

    fn edit(text: &str) -> Vec<Operation> {
        vec![Operation::EditRun { page: 0, run: 0, text: text.to_owned() }]
    }

    /// Writes a session holding `acts` then an undo and a redo, and leaves it as a crash
    /// would: the files on disk and the lock released.
    fn crashed(dir: PathBuf, seal: Option<&str>, acts: &[Vec<Operation>]) {
        let mut session =
            Session::begin(dir, b"%PDF-2.0 origin", Some("a.pdf"), seal).expect("begun");
        for act in acts {
            session.append(&EntryRef::Act(act)).expect("appended");
        }
        session.append(&EntryRef::Undo).expect("appended");
        session.append(&EntryRef::Redo).expect("appended");
        // Dropping closes the lock's file, which is what the process ending does.
        drop(session);
    }

    /// **What was journaled is what is read back**, in order, with the origin and name.
    #[test]
    fn a_crashed_session_reads_back_what_it_wrote() {
        let scratch = Scratch::new("plain");
        let dir = scratch.0.join("s");
        crashed(dir.clone(), None, &[remove(0), remove(1)]);

        let found = find(&scratch.0);
        assert_eq!(found.len(), 1, "one session left behind: {found:?}");
        assert_eq!(found.first().map(|f| (f.entries, f.sealed)), Some((4, false)));

        let back = recover(&dir, None).expect("recovered");
        assert_eq!(back.origin.as_ref(), b"%PDF-2.0 origin");
        assert_eq!(back.name.as_deref(), Some("a.pdf"));
        assert_eq!(
            back.entries,
            vec![Entry::Act(remove(0)), Entry::Act(remove(1)), Entry::Undo, Entry::Redo]
        );
        assert!(!back.lost_tail);
    }

    /// **A running window's session is not a crash.** Its lock is held, so it is not
    /// offered, and it cannot be recovered out from under it.
    #[test]
    fn a_running_session_is_not_offered() {
        let scratch = Scratch::new("running");
        let dir = scratch.0.join("s");
        let mut session = Session::begin(dir.clone(), b"origin", None, None).expect("begun");
        session.append(&EntryRef::Act(&remove(0))).expect("appended");

        assert!(find(&scratch.0).is_empty(), "the lock is held");
        assert!(matches!(recover(&dir, None), Err(RecoveryError::Held)));

        session.end().expect("ended");
        assert!(!dir.exists(), "a session that ends removes what it wrote");
    }

    /// **A record the crash cut short is dropped, and said to be.** What comes before it
    /// replays, and a resumed session writes after it rather than inside it.
    #[test]
    fn a_torn_record_is_dropped_and_the_session_goes_on_after_it() {
        let scratch = Scratch::new("torn");
        let dir = scratch.0.join("s");
        crashed(dir.clone(), None, &[remove(0)]);
        let journal = dir.join(JOURNAL);
        let mut bytes = fs::read(&journal).expect("read");
        bytes.extend_from_slice(&[200, 0, 0, 0, b'{']);
        fs::write(&journal, &bytes).expect("written");

        let back = recover(&dir, None).expect("recovered");
        assert!(back.lost_tail, "the cut record is reported");
        assert_eq!(back.entries.len(), 3, "the three before it are kept");

        let mut session = Session::resume(dir.clone(), back).expect("resumed");
        session.append(&EntryRef::Act(&remove(7))).expect("appended");
        drop(session);
        let again = recover(&dir, None).expect("recovered again");
        assert!(!again.lost_tail, "the cut was trimmed before writing on");
        assert_eq!(again.entries.last(), Some(&Entry::Act(remove(7))));
    }

    /// **A sealed journal holds none of what the reader typed in the clear**, opens only
    /// with the password, and reads back the same entries with it.
    #[test]
    fn a_sealed_journal_hides_its_words_and_opens_with_the_password() {
        let scratch = Scratch::new("sealed");
        let dir = scratch.0.join("s");
        let secret = "Confidential-Merger-Terms";
        crashed(dir.clone(), Some("hunter2"), &[edit(secret)]);

        let bytes = fs::read(dir.join(JOURNAL)).expect("read");
        let visible = String::from_utf8_lossy(&bytes);
        assert!(!visible.contains(secret), "the edited words are on disk in the clear");
        assert!(!visible.contains("EditRun"), "the operation's name is on disk in the clear");
        assert!(!visible.contains("hunter2"), "the password is on disk");

        assert_eq!(find(&scratch.0).first().map(|f| f.sealed), Some(true));
        assert!(matches!(recover(&dir, None), Err(RecoveryError::WrongPassword)));
        assert!(matches!(recover(&dir, Some("wrong")), Err(RecoveryError::WrongPassword)));
        let back = recover(&dir, Some("hunter2")).expect("opened with the password");
        assert_eq!(back.entries.first(), Some(&Entry::Act(edit(secret))));
    }

    /// A session that crashed before its first act holds nothing to offer, and is
    /// cleared away rather than offered.
    #[test]
    fn a_session_with_no_entries_is_cleared_not_offered() {
        let scratch = Scratch::new("empty");
        let dir = scratch.0.join("s");
        drop(Session::begin(dir.clone(), b"origin", None, None).expect("begun"));

        assert!(find(&scratch.0).is_empty());
        assert!(!dir.exists(), "cleared away");
    }

    /// **The borrowed form writes what the owned form reads.** The journal is written
    /// through one type and read through another, and they agree only by their names.
    #[test]
    fn entry_ref_serialises_as_entry() {
        let act = remove(3);
        for (written, read) in [
            (EntryRef::Act(&act), Entry::Act(act.clone())),
            (EntryRef::Undo, Entry::Undo),
            (EntryRef::Redo, Entry::Redo),
        ] {
            let json = serde_json::to_vec(&written).expect("serialised");
            assert_eq!(serde_json::from_slice::<Entry>(&json).expect("parsed"), read);
        }
    }
}
