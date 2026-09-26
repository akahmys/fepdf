//! Reading a document aloud through the platform's own synthesiser (ROADMAP W-19b).
//!
//! **Bound, not built — and bound through a process.** Every target ships a synthesiser,
//! and each one's Rust bindings are calls this workspace cannot make: `AVSpeechSynthesizer`
//! through `objc2` is `unsafe fn`, SAPI is COM, and Rule 3 forbids `unsafe` with no
//! override. Each platform also ships a front end to the same synthesiser, and a process
//! is safe Rust: `say` on macOS, `spd-say` on Linux (a client of speech-dispatcher, not a
//! library linked here — Rule 9), and PowerShell's `System.Speech` on Windows. No crate is
//! added for any of them.
//!
//! **One passage at a time**, each in a voice for its language, so a reader can stop
//! between them and the window can say which is being read. The words go in on standard
//! input rather than as an argument where the front end reads it, and nothing a document
//! says — its words, its `/Lang` — is ever placed where a shell would read it.
//!
//! **What is not passed:** the lexicons and the phonemes. `say` and `spd-say` take
//! neither in the forms PDF gives them — a PLS file, an IPA or X-SAMPA transcription — so
//! the drawer says how many lexicons the document carries rather than implying they are
//! used.

use fepdf::reading::Passage;
use std::collections::VecDeque;
use std::io::Write as _;
use std::process::{Child, Command, Stdio};

/// The platforms a synthesiser is bound on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// `say`.
    MacOs,
    /// `spd-say`.
    Linux,
    /// PowerShell and `System.Speech`.
    Windows,
}

impl Platform {
    /// The platform this build runs on, when it is one of the three.
    pub const fn this() -> Option<Self> {
        if cfg!(target_os = "macos") {
            Some(Self::MacOs)
        } else if cfg!(target_os = "linux") {
            Some(Self::Linux)
        } else if cfg!(target_os = "windows") {
            Some(Self::Windows)
        } else {
            None
        }
    }
}

/// A process to run for one passage: what, with which arguments, fed what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utter {
    /// The program.
    pub program: String,
    /// Its arguments, none of them read by a shell.
    pub args: Vec<String>,
    /// What it is fed on standard input.
    pub input: String,
}

/// A language tag reduced to what a tag can hold — letters, digits and hyphens — or
/// nothing, when it holds anything else.
///
/// **A `/Lang` is the document's to write**, and on Windows it is placed inside a script.
/// A tag is ASCII letters, digits and hyphens (BCP 47), so anything else is not a tag and
/// is dropped rather than escaped.
fn tag_of(lang: Option<&str>) -> Option<&str> {
    let tag = lang?.trim();
    let well_formed = !tag.is_empty()
        && tag.len() <= 35
        && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !tag.starts_with('-');
    well_formed.then_some(tag)
}

/// The process that reads `passage` aloud on `platform`, in a voice from `voices` for its
/// language when one is listed.
pub fn utter(platform: Platform, passage: &Passage, voices: &[Voice]) -> Utter {
    let tag = tag_of(passage.lang.as_deref());
    let input = passage.text.clone();
    match platform {
        Platform::MacOs => {
            let mut args = Vec::new();
            if let Some(voice) = tag.and_then(|tag| voice_for(voices, tag)) {
                args.extend(["-v".to_owned(), voice.name.clone()]);
            }
            Utter { program: "say".to_owned(), args, input }
        }
        Platform::Linux => {
            // `--wait` so the process lasts as long as the speech does, which is how the
            // next passage knows when to start; the words are the one argument after
            // `--`, since `spd-say` reads standard input only in a mode that echoes it.
            let mut args = vec!["--wait".to_owned()];
            if let Some(tag) = tag {
                args.extend(["--language".to_owned(), tag.to_owned()]);
            }
            args.extend(["--".to_owned(), passage.text.clone()]);
            Utter { program: "spd-say".to_owned(), args, input: String::new() }
        }
        Platform::Windows => {
            let choose = tag.map_or_else(String::new, |tag| {
                format!(
                    "try {{ $s.SelectVoiceByHints('NotSet', 'NotSet', 0, \
                     [Globalization.CultureInfo]'{tag}') }} catch {{}}; "
                )
            });
            let script = format!(
                "Add-Type -AssemblyName System.Speech; \
                 $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; {choose}\
                 $s.Speak([Console]::In.ReadToEnd())"
            );
            Utter {
                program: "powershell".to_owned(),
                args: vec![
                    "-NoProfile".to_owned(),
                    "-NonInteractive".to_owned(),
                    "-Command".to_owned(),
                    script,
                ],
                input,
            }
        }
    }
}

/// A voice `say` lists: its name, and the locale it speaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Voice {
    /// What `say -v` is given.
    pub name: String,
    /// Its locale as `say` writes it: `ja_JP`.
    pub locale: String,
}

/// Apple's novelty voices, which read text as sound effects.
///
/// **They come first in the list for `en_US`**, so "the first voice for the language"
/// read English in `Bad News`.
const NOVELTY: [&str; 19] = [
    "Albert",
    "Bad News",
    "Bahh",
    "Bells",
    "Boing",
    "Bubbles",
    "Cellos",
    "Fred",
    "Good News",
    "Jester",
    "Junior",
    "Kathy",
    "Organ",
    "Ralph",
    "Superstar",
    "Trinoids",
    "Whisper",
    "Wobble",
    "Zarvox",
];

/// The voices in what `say -v '?'` printed: a name, a locale, then `#` and a sample.
pub fn voices_in(listing: &str) -> Vec<Voice> {
    listing
        .lines()
        .filter_map(|line| {
            let described = line.split(" # ").next()?.trim_end();
            let (name, locale) = described.rsplit_once(char::is_whitespace)?;
            let name = name.trim();
            (!name.is_empty() && locale.contains('_'))
                .then(|| Voice { name: name.to_owned(), locale: locale.to_owned() })
        })
        .collect()
}

/// The voice for `tag`: one for the tag's locale if there is one, else one for its
/// language; of those, one that is neither a novelty voice nor one of the voices listed
/// once per locale with the locale in its name, when there is such a voice.
fn voice_for<'a>(voices: &'a [Voice], tag: &str) -> Option<&'a Voice> {
    let wanted = tag.replace('-', "_").to_ascii_lowercase();
    let language = wanted.split('_').next().unwrap_or_default().to_owned();
    let speaks = |voice: &&Voice, exact: bool| {
        let locale = voice.locale.to_ascii_lowercase();
        if exact { locale == wanted } else { locale.split('_').next() == Some(language.as_str()) }
    };
    let plain =
        |voice: &&Voice| !NOVELTY.contains(&voice.name.as_str()) && !voice.name.contains('(');
    for exact in [true, false] {
        let mut candidates = voices.iter().filter(|v| speaks(v, exact)).peekable();
        let first = candidates.peek().copied();
        if let Some(voice) = candidates.find(plain).or(first) {
            return Some(voice);
        }
    }
    None
}

/// The voices this machine has, when it is a Mac; none elsewhere, where the voice is
/// chosen by language rather than by name.
pub fn installed_voices(platform: Platform) -> Vec<Voice> {
    if platform != Platform::MacOs {
        return Vec::new();
    }
    Command::new("say")
        .args(["-v", "?"])
        .output()
        .map(|out| voices_in(&String::from_utf8_lossy(&out.stdout)))
        .unwrap_or_default()
}

/// The passages being read, and the process reading the current one.
pub struct Speaker {
    /// What to run for a passage.
    uttering: Box<dyn Fn(&Passage) -> Utter>,
    queue: VecDeque<Passage>,
    current: Option<(Passage, Child)>,
}

impl Speaker {
    /// A speaker for `platform`, choosing among `voices`.
    pub fn new(platform: Platform, voices: Vec<Voice>) -> Self {
        Self::uttering(move |passage| utter(platform, passage, &voices))
    }

    /// A speaker that runs what `uttering` says for each passage.
    pub fn uttering(uttering: impl Fn(&Passage) -> Utter + 'static) -> Self {
        Self { uttering: Box::new(uttering), queue: VecDeque::new(), current: None }
    }

    /// Reads `passages` in order, after stopping whatever was being read.
    ///
    /// # Errors
    /// When the platform's front end cannot be started — most likely, that it is not
    /// installed.
    pub fn read(&mut self, passages: Vec<Passage>) -> std::io::Result<()> {
        self.stop();
        self.queue = passages.into();
        self.next()
    }

    /// Stops reading, now and for the rest.
    pub fn stop(&mut self) {
        self.queue.clear();
        if let Some((_, mut child)) = self.current.take() {
            // A process that has already ended cannot be killed and has nothing to say
            // about it; the one that has not is ended here, and neither is an error to
            // the reader who pressed stop.
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// The passage being read, having moved on to the next when the last one finished.
    ///
    /// # Errors
    /// When the next passage's process cannot be started.
    pub fn poll(&mut self) -> std::io::Result<Option<&Passage>> {
        let finished = match self.current.as_mut() {
            Some((_, child)) => child.try_wait()?.is_some(),
            None => false,
        };
        if finished {
            self.current = None;
            self.next()?;
        }
        Ok(self.current.as_ref().map(|(passage, _)| passage))
    }

    /// Whether anything is being read or waits to be.
    pub fn is_reading(&self) -> bool {
        self.current.is_some() || !self.queue.is_empty()
    }

    /// Starts the next passage, if there is one.
    fn next(&mut self) -> std::io::Result<()> {
        let Some(passage) = self.queue.pop_front() else { return Ok(()) };
        let child = spawn(&(self.uttering)(&passage))?;
        self.current = Some((passage, child));
        Ok(())
    }
}

impl Drop for Speaker {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Starts `utter`, feeds it its input, and closes that input so it knows there is no more.
fn spawn(utter: &Utter) -> std::io::Result<Child> {
    let mut child = Command::new(&utter.program)
        .args(&utter.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(utter.input.as_bytes())?;
    }
    Ok(child)
}

/// What each platform is asked to run, and which voice a language gets.
#[cfg(test)]
mod uttered {
    use super::{Platform, Voice, utter, voice_for, voices_in};
    use fepdf::reading::{Passage, Spoken};

    fn passage(text: &str, lang: Option<&str>) -> Passage {
        Passage {
            text: text.to_owned(),
            lang: lang.map(str::to_owned),
            tag: "P".to_owned(),
            page: Some(0),
            spoken: Spoken::Content,
            phoneme: None,
        }
    }

    const LISTING: &str = "\
Albert              en_US    # Hello! My name is Albert.
Bad News            en_US    # Hello! My name is Bad News.
Eddy (英語（アメリカ）)     en_US    # Hello! My name is Eddy.
Samantha            en_US    # Hello! My name is Samantha.
Daniel              en_GB    # Hello! My name is Daniel.
Eddy (日本語（日本）)  ja_JP    # こんにちは！
Kyoko               ja_JP    # こんにちは！私の名前はKyokoです。
";

    #[test]
    fn a_listing_reads_as_names_and_locales() {
        let voices = voices_in(LISTING);
        assert_eq!(voices.len(), 7);
        assert_eq!(
            voices[2],
            Voice {
                name: "Eddy (英語（アメリカ）)".to_owned(), locale: "en_US".to_owned()
            }
        );
        assert_eq!(voices[6].name, "Kyoko");
    }

    /// **English is read by Samantha, not Albert or Bad News**, and Japanese by Kyoko
    /// rather than by a voice listed once per locale.
    #[test]
    fn a_language_gets_its_plain_voice() {
        let voices = voices_in(LISTING);
        let name = |tag| voice_for(&voices, tag).map(|v| v.name.as_str());
        assert_eq!(name("en-US"), Some("Samantha"));
        assert_eq!(name("ja"), Some("Kyoko"));
        assert_eq!(name("en-GB"), Some("Daniel"));
        // A region nothing speaks falls back to the language.
        assert_eq!(name("en-AU"), Some("Samantha"));
        assert_eq!(name("fr"), None);
    }

    /// The words go in on standard input, and the voice is chosen by the passage's tag.
    #[test]
    fn a_mac_is_given_the_words_on_its_input() {
        let voices = voices_in(LISTING);
        let asked = utter(Platform::MacOs, &passage("日本国憲法", Some("ja")), &voices);
        assert_eq!(asked.program, "say");
        assert_eq!(asked.args, ["-v", "Kyoko"]);
        assert_eq!(asked.input, "日本国憲法");
    }

    /// **A `/Lang` that is not a tag goes nowhere near the script.** The document writes
    /// it, and on Windows it would otherwise be spliced into PowerShell.
    #[test]
    fn a_lang_that_is_not_a_tag_is_dropped() {
        let hostile = passage("words", Some("en'); Remove-Item -Recurse C:\\; ('"));
        let asked = utter(Platform::Windows, &hostile, &[]);
        let script = asked.args.last().expect("a script");
        assert!(!script.contains("Remove-Item"), "the document's /Lang reached the script");
        assert_eq!(asked.input, "words");
        let tagged = utter(Platform::Windows, &passage("mots", Some("fr-FR")), &[]);
        assert!(tagged.args.last().is_some_and(|s| s.contains("'fr-FR'")));
    }

    /// Linux waits for the speech to end and is told the language.
    #[test]
    fn linux_is_told_the_language_and_waits() {
        let asked = utter(Platform::Linux, &passage("-rf words", Some("en-GB")), &[]);
        assert_eq!(asked.program, "spd-say");
        assert_eq!(asked.args, ["--wait", "--language", "en-GB", "--", "-rf words"]);
    }
}

/// The queue, driven by processes that write rather than speak.
#[cfg(test)]
mod reading_in_turn {
    use super::{Speaker, Utter};
    use fepdf::reading::{Passage, Spoken};

    fn passages(texts: &[&str]) -> Vec<Passage> {
        texts
            .iter()
            .map(|text| Passage {
                text: (*text).to_owned(),
                lang: None,
                tag: "P".to_owned(),
                page: None,
                spoken: Spoken::Content,
                phoneme: None,
            })
            .collect()
    }

    /// A speaker whose "speech" is a line appended to `into`.
    fn writing(into: std::path::PathBuf) -> Speaker {
        Speaker::uttering(move |passage| Utter {
            program: "sh".to_owned(),
            args: vec!["-c".to_owned(), format!("cat >> '{0}'; echo >> '{0}'", into.display())],
            input: passage.text.clone(),
        })
    }

    fn until_done(speaker: &mut Speaker) {
        for _ in 0..500 {
            if speaker.poll().expect("the next starts").is_none() && !speaker.is_reading() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the reading never finished");
    }

    /// **Every passage is read, one after another, in order.**
    #[test]
    fn passages_are_read_in_turn() {
        let into =
            std::env::temp_dir().join(format!("fepdf_speech_{}_turn.txt", std::process::id()));
        let _ = std::fs::remove_file(&into);
        let mut speaker = writing(into.clone());
        speaker.read(passages(&["one", "two", "three"])).expect("it starts");
        until_done(&mut speaker);
        let said = std::fs::read_to_string(&into).expect("something was said");
        let _ = std::fs::remove_file(&into);
        assert_eq!(said, "one\ntwo\nthree\n");
    }

    /// **Stopping stops the rest**, not only the passage under way.
    #[test]
    fn stopping_stops_the_rest() {
        let into =
            std::env::temp_dir().join(format!("fepdf_speech_{}_stop.txt", std::process::id()));
        let _ = std::fs::remove_file(&into);
        let mut speaker = writing(into.clone());
        speaker.read(passages(&["one", "two", "three"])).expect("it starts");
        speaker.stop();
        assert!(!speaker.is_reading());
        std::thread::sleep(std::time::Duration::from_millis(100));
        let said = std::fs::read_to_string(&into).unwrap_or_default();
        let _ = std::fs::remove_file(&into);
        assert!(!said.contains("two"), "a passage after the stop was read: {said:?}");
    }
}

/// The real synthesiser, heard by nobody: `say` writes what it would have said.
#[cfg(test)]
mod on_this_mac {
    use super::{Platform, installed_voices, utter};
    use fepdf::reading::{Passage, Spoken};

    /// **Japanese and English each come out as speech**, in the voices chosen for them.
    #[cfg(target_os = "macos")]
    #[test]
    fn say_speaks_each_language_into_a_file() {
        let voices = installed_voices(Platform::MacOs);
        for (text, lang) in [("日本国憲法", "ja"), ("The reading order", "en-US")] {
            let passage = Passage {
                text: text.to_owned(),
                lang: Some(lang.to_owned()),
                tag: "P".to_owned(),
                page: None,
                spoken: Spoken::Content,
                phoneme: None,
            };
            let mut asked = utter(Platform::MacOs, &passage, &voices);
            let file =
                std::env::temp_dir().join(format!("fepdf_say_{}_{lang}.aiff", std::process::id()));
            asked.args.extend(["-o".to_owned(), file.display().to_string()]);
            let mut child = super::spawn(&asked).expect("say starts");
            assert!(child.wait().expect("say ends").success(), "say failed for {lang}");
            let size = std::fs::metadata(&file).map_or(0, |m| m.len());
            let _ = std::fs::remove_file(&file);
            // An AIFF header alone is under a hundred bytes; a word is thousands of samples.
            assert!(size > 8_000, "{lang}: {size} bytes is no speech");
        }
    }
}
