//! The read-aloud drawer: reading the document from a page onward, and stopping
//! (ROADMAP W-19b).
//!
//! **The reading is asked for each time, not kept.** `PdfDocument::reading` is computed on
//! the worker — `volvo_xc90.pdf`'s 9,375 passages take half a second — and a reading kept
//! from before an edit would read words the document no longer has.

use crate::speech::{Platform, Speaker, Utter};
use fepdf::reading::{Passage, Reading};

/// What the drawer holds between frames.
#[derive(Default)]
pub struct ReadAloud {
    /// The page the reader asked to start from, while the worker reads the document.
    pub waiting_from: Option<usize>,
    /// Reading now, or ready to.
    speaker: Option<Speaker>,
    /// How many passages this reading has, and how many lexicons the document names.
    pub counted: Option<(usize, usize)>,
    /// What is being read now, for the drawer to show.
    pub now: Option<Passage>,
    /// Where a capture plan asked the speech to be written rather than heard.
    pub recording_into: Option<std::path::PathBuf>,
}

/// What the drawer asked for.
pub enum Asked {
    /// Nothing.
    Nothing,
    /// To read from the page on screen.
    Read,
    /// To stop.
    Stop,
}

impl ReadAloud {
    /// Whether anything is being read or waits to be.
    pub fn is_reading(&self) -> bool {
        self.speaker.as_ref().is_some_and(Speaker::is_reading)
    }

    /// Starts reading `reading` from the first passage on `from` or after it.
    ///
    /// A passage no page is named for is read where it falls in the order — it is before
    /// the start only when a passage before it is.
    ///
    /// # Errors
    /// The locale key of why nothing is read: no platform to read on, a document with no
    /// structure to read in order, or a synthesiser that would not start.
    pub fn start(
        &mut self,
        reading: Reading,
        from: usize,
    ) -> Result<(), (&'static str, Option<String>)> {
        let Some(platform) = Platform::this() else { return Err(("speech_no_platform", None)) };
        let start = reading
            .passages
            .iter()
            .position(|p| p.page.is_some_and(|page| page >= from))
            .unwrap_or(0);
        let passages: Vec<Passage> = reading.passages.into_iter().skip(start).collect();
        if passages.is_empty() {
            return Err(("speech_nothing_to_read", None));
        }
        self.counted = Some((passages.len(), reading.lexicons.len()));
        let speaker = self.speaker.get_or_insert_with(|| speaker_for(platform, None));
        if let Some(folder) = &self.recording_into {
            *speaker = speaker_for(platform, Some(folder.clone()));
        }
        speaker.read(passages).map_err(|why| ("speech_failed", Some(why.to_string())))
    }

    /// Stops reading.
    pub fn stop(&mut self) {
        if let Some(speaker) = self.speaker.as_mut() {
            speaker.stop();
        }
        self.now = None;
    }

    /// Moves on when a passage has been read.
    ///
    /// # Errors
    /// What the next passage's process said when it would not start.
    pub fn poll(&mut self) -> std::io::Result<()> {
        let Some(speaker) = self.speaker.as_mut() else { return Ok(()) };
        let now = speaker.poll()?;
        self.now = now.cloned();
        Ok(())
    }
}

/// A speaker for `platform`, writing each passage into `recording` when a folder is given.
///
/// **Recording is for the capture plan**, so that the window's reading can be checked on
/// a machine without anyone listening; only `say` writes a file, so elsewhere it is read
/// aloud as usual.
fn speaker_for(platform: Platform, recording: Option<std::path::PathBuf>) -> Speaker {
    let voices = crate::speech::installed_voices(platform);
    let Some(folder) = recording.filter(|_| platform == Platform::MacOs) else {
        return Speaker::new(platform, voices);
    };
    let counter = std::cell::Cell::new(0_usize);
    Speaker::uttering(move |passage| {
        let Utter { program, mut args, input } = crate::speech::utter(platform, passage, &voices);
        let file = folder.join(format!("{:03}.aiff", counter.replace(counter.get() + 1)));
        args.extend(["-o".to_owned(), file.display().to_string()]);
        Utter { program, args, input }
    })
}

/// The drawer.
pub fn show(state: &ReadAloud, ui: &mut egui::Ui, tr: &dyn Fn(&str) -> String) -> Asked {
    use crate::app::theme::space;
    ui.label(tr("speech_how"));
    ui.add_space(space::ITEM);
    let mut asked = Asked::Nothing;
    ui.horizontal(|ui| {
        let busy = state.waiting_from.is_some();
        if ui.add_enabled(!busy, egui::Button::new(tr("speech_read_from_here"))).clicked() {
            asked = Asked::Read;
        }
        if ui.add_enabled(state.is_reading(), egui::Button::new(tr("speech_stop"))).clicked() {
            asked = Asked::Stop;
        }
    });
    ui.add_space(space::ITEM);
    if let Some((passages, lexicons)) = state.counted {
        ui.label(tr("speech_passages").replacen("{}", &passages.to_string(), 1));
        if lexicons > 0 {
            ui.label(tr("speech_lexicons_unused").replacen("{}", &lexicons.to_string(), 1));
        }
    }
    if let Some(now) = &state.now {
        ui.add_space(space::ITEM);
        let head: String = now.text.chars().take(120).collect();
        ui.label(format!("<{}> {}", now.tag, head));
    }
    asked
}
