//! Driving the running window from outside, a step at a time (ROADMAP Z-5).
//!
//! `--capture` reads a plan once and closes the window when it runs out, so the steps are
//! fixed before the window is seen. A defect in the window is found by looking and then
//! acting on what was seen: five fixes here went wrong because the gesture that showed the
//! defect could not be repeated, and the fix was tested with a call the gesture ends in
//! instead. `--control <dir>` keeps the window open and takes steps as they arrive.
//!
//! ```bash
//! ./target/debug/fepdf-gui --control /tmp/fepdf-control samples/constitution.pdf
//! echo 'inspect zoom' >> /tmp/fepdf-control/in
//! echo 'clicklabel Zoom in' >> /tmp/fepdf-control/in
//! echo 'shot after' >> /tmp/fepdf-control/in
//! cat /tmp/fepdf-control/out
//! ```
//!
//! **A directory and two files, not a socket.** Nothing listens on any port; the steps
//! are lines appended to `<dir>/in`, and what each did is a line in `<dir>/out`. The
//! directory is made readable and writable by its owner alone, so another user on the
//! machine can neither drive the window nor read what it reports.
//!
//! The verbs are the plan's (`capture.rs`), and five that a person at the window has:
//! `clickat`, `clicklabel`, `key`, `type`, and `inspect`, which lists what the last frame
//! drew, by the name AccessKit gives each widget and where it is.

use std::io::Write;
use std::path::{Path, PathBuf};

/// The two files a controlled window reads and writes.
pub struct Control {
    dir: PathBuf,
    /// How many bytes of `in` have been taken as steps.
    taken: usize,
}

impl Control {
    /// Makes `dir` for its owner alone, and empty `in` and `out` files inside it.
    ///
    /// # Errors
    /// Fails when the directory or the files cannot be made.
    pub fn open(dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
        std::fs::write(dir.join("in"), b"")?;
        std::fs::write(dir.join("out"), b"")?;
        Ok(Self { dir: dir.to_path_buf(), taken: 0 })
    }

    /// Where a `shot` is written.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The whole lines appended to `in` since the last call. A line still being written,
    /// with no newline yet, waits for the next.
    pub fn arrived(&mut self) -> Vec<String> {
        let Ok(bytes) = std::fs::read(self.dir.join("in")) else { return Vec::new() };
        let Some(fresh) = bytes.get(self.taken..) else { return Vec::new() };
        let Some(end) = fresh.iter().rposition(|&b| b == b'\n') else { return Vec::new() };
        self.taken += end + 1;
        String::from_utf8_lossy(&fresh[..end])
            .lines()
            .map(|line| line.split('#').next().unwrap_or("").trim().to_owned())
            .filter(|line| !line.is_empty())
            .collect()
    }

    /// Appends one line to `out`.
    pub fn report(&self, line: &str) {
        let written = std::fs::OpenOptions::new()
            .append(true)
            .open(self.dir.join("out"))
            .and_then(|mut out| writeln!(out, "{line}"));
        if let Err(e) = written {
            log::error!("control: {}: {e}", self.dir.join("out").display());
        }
    }
}

/// A widget the last frame drew, as AccessKit describes it.
#[derive(Debug, Clone)]
pub struct Widget {
    /// AccessKit's role for it, such as `Button`.
    pub role: String,
    /// Its label, or its value where it has no label.
    pub name: String,
    /// Where it is, in the points a pointer event is given in.
    pub rect: egui::Rect,
}

/// Keeps the widgets of the last frame's AccessKit tree, which egui builds after the
/// frame's `update` has returned: an output hook is the one place that sees it.
#[derive(Default)]
pub struct WidgetTree {
    /// The widgets with a name and a place, in the tree's order.
    pub widgets: Vec<Widget>,
}

impl egui::Plugin for WidgetTree {
    fn debug_name(&self) -> &'static str {
        "fepdf_control_widget_tree"
    }

    fn output_hook(&mut self, output: &mut egui::FullOutput) {
        let Some(update) = &output.platform_output.accesskit_update else { return };
        self.widgets = update
            .nodes
            .iter()
            .filter_map(|(_, node)| {
                let name = node.label().or_else(|| node.value())?.trim();
                let bounds = node.bounds()?;
                (!name.is_empty()).then(|| Widget {
                    role: format!("{:?}", node.role()),
                    name: name.to_owned(),
                    #[allow(clippy::cast_possible_truncation)]
                    rect: egui::Rect::from_min_max(
                        egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                        egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                    ),
                })
            })
            .collect();
    }
}

/// The widgets whose name contains `query`, ignoring case; every one for an empty query.
pub fn matching<'a>(widgets: &'a [Widget], query: &str) -> Vec<&'a Widget> {
    let query = query.to_lowercase();
    widgets.iter().filter(|w| w.name.to_lowercase().contains(&query)).collect()
}

/// One widget as a line of `inspect`'s answer: role, name, and its rectangle.
pub fn describe(widget: &Widget) -> String {
    let r = widget.rect;
    format!(
        "{}\t{}\t{:.0} {:.0} {:.0} {:.0}",
        widget.role, widget.name, r.min.x, r.min.y, r.max.x, r.max.y
    )
}

/// The events of a click at `at`, one batch a frame: arrive, press, let go.
pub fn click_events(at: egui::Pos2) -> std::collections::VecDeque<Vec<egui::Event>> {
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    [vec![egui::Event::PointerMoved(at)], vec![button(true)], vec![button(false)]].into()
}

/// A key and what is held with it, from `cmd+shift+K` or `Escape`: the last part is the
/// key, by egui's name for it, and the rest are `cmd`, `ctrl`, `alt` or `shift`.
pub fn key_events(chord: &str) -> Option<std::collections::VecDeque<Vec<egui::Event>>> {
    let mut parts: Vec<&str> = chord.split('+').map(str::trim).collect();
    let key = egui::Key::from_name(parts.pop()?)?;
    let mut modifiers = egui::Modifiers::NONE;
    for held in parts {
        match held {
            "cmd" => modifiers |= egui::Modifiers::COMMAND,
            "ctrl" => modifiers |= egui::Modifiers::CTRL,
            "alt" => modifiers |= egui::Modifiers::ALT,
            "shift" => modifiers |= egui::Modifiers::SHIFT,
            _ => return None,
        }
    }
    let event =
        |pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
    Some([vec![event(true)], vec![event(false)]].into())
}

/// What a frame's key events say is held, which the frame reports as its modifiers: a
/// person pressing ⌘K holds ⌘ for the frame, and a shortcut reads it there
/// (`feed_capture_input`). `None` when the frame has no key.
pub fn held(events: &[egui::Event]) -> Option<egui::Modifiers> {
    events.iter().rev().find_map(|event| {
        if let egui::Event::Key { modifiers, .. } = event { Some(*modifiers) } else { None }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line is a step once its newline has arrived, and not before; and each is taken
    /// once.
    #[test]
    fn a_step_is_taken_once_its_line_is_whole() {
        let dir = std::env::temp_dir().join(format!("fepdf_control_{}", std::process::id()));
        let mut control = Control::open(&dir).expect("it opens");
        let append = |text: &str| {
            let mut file =
                std::fs::OpenOptions::new().append(true).open(dir.join("in")).expect("in is there");
            file.write_all(text.as_bytes()).expect("it appends");
        };
        append("inspect zoom\nclickat 10 2");
        assert_eq!(control.arrived(), ["inspect zoom"]);
        assert!(control.arrived().is_empty(), "taken once");
        append("0 # a comment\n\n");
        assert_eq!(control.arrived(), ["clickat 10 20"]);
        control.report("done");
        let out = std::fs::read_to_string(dir.join("out")).expect("out is there");
        assert_eq!(out, "done\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&dir).expect("there").permissions().mode();
            assert_eq!(mode & 0o077, 0, "nobody but its owner can reach it");
        }
        std::fs::remove_dir_all(&dir).expect("the test removes its own directory");
    }

    /// A chord names its key last and what is held before it; a name egui does not know
    /// is no chord.
    #[test]
    fn a_chord_is_its_key_and_what_is_held() {
        let events = key_events("cmd+shift+K").expect("a chord");
        let egui::Event::Key { key, modifiers, pressed, .. } = &events[0][0] else {
            panic!("a key event")
        };
        assert_eq!(*key, egui::Key::K);
        assert!(*pressed && modifiers.command && modifiers.shift && !modifiers.alt);
        assert!(key_events("Escape").is_some());
        assert!(key_events("hyper+K").is_none());
        assert!(key_events("cmd+NoSuchKey").is_none());
    }

    /// A key's frame holds what the key does, and a frame of pointer events holds nothing
    /// new. `key cmd+K` opened nothing until the frame said ⌘ was held.
    #[test]
    fn a_frame_holds_what_its_key_holds() {
        let chord = key_events("cmd+K").expect("a chord");
        assert!(held(&chord[0]).is_some_and(|m| m.command));
        assert_eq!(held(&click_events(egui::pos2(1.0, 1.0))[1]), None);
    }

    /// A widget is found by any part of its name, whatever the case.
    #[test]
    fn a_widget_is_found_by_part_of_its_name() {
        let widget = |name: &str| Widget {
            role: "Button".into(),
            name: name.into(),
            rect: egui::Rect::from_min_size(egui::pos2(1.0, 2.0), egui::vec2(3.0, 4.0)),
        };
        let widgets = [widget("Zoom in"), widget("Zoom out"), widget("Rotate")];
        assert_eq!(matching(&widgets, "zoom").len(), 2);
        assert_eq!(matching(&widgets, "").len(), 3);
        assert_eq!(describe(&widgets[2]), "Button\tRotate\t1 2 4 6");
    }
}
