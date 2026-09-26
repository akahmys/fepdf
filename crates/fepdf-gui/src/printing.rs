//! Printing: the document as it stands, handed to the platform's spooler (ROADMAP W-17).
//!
//! **What can be checked ends at the spooler.** The file handed over is the document as
//! edited, written as an export writes it, and the spooler's answer — a job, or why not —
//! is said in the window. Whether ink reached paper is not something this program can
//! know, and nothing here claims it.
//!
//! **Bound through the platform's own spooler, as a process**, for the reason speech is
//! (Rule 3): `lp` on macOS and Linux, which CUPS provides, and on Windows the default
//! application's print verb.

use crate::speech::Platform;
use std::path::Path;

/// What the print drawer holds between frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintForm {
    /// The printers the spooler lists, read when the drawer opens.
    pub printers: Option<Vec<String>>,
    /// Which to print on; `None` is the spooler's default.
    pub printer: Option<String>,
    /// How many copies.
    pub copies: u32,
    /// Which pages, as a reader writes them: `1-3,5`; empty for all.
    pub pages: String,
    /// Whether a job is being handed over.
    pub waiting: bool,
}

impl Default for PrintForm {
    fn default() -> Self {
        Self { printers: None, printer: None, copies: 1, pages: String::new(), waiting: false }
    }
}

/// Why a print was not handed over, as the locale key that says so.
pub type Refusal = &'static str;

/// Whether `pages` is a page list the spooler reads — digits, commas and hyphens, and
/// nothing else, since it goes into the spooler's arguments.
fn page_list(pages: &str) -> Result<Option<String>, Refusal> {
    let pages: String = pages.chars().filter(|c| !c.is_whitespace()).collect();
    if pages.is_empty() {
        return Ok(None);
    }
    let well_formed = pages.split(',').all(|part| {
        let mut ends = part.splitn(2, '-');
        let number = |n: Option<&str>| {
            n.is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) && n != "0")
        };
        number(ends.next()) && ends.next().is_none_or(|end| number(Some(end)))
    });
    if well_formed { Ok(Some(pages)) } else { Err("print_bad_pages") }
}

/// The process that hands `file` to the spooler on `platform`: its program, its
/// arguments, and the environment it is given.
///
/// # Errors
/// When the page list is not one.
pub fn command(
    platform: Platform,
    file: &Path,
    form: &PrintForm,
) -> Result<(String, Vec<String>, Vec<(String, String)>), Refusal> {
    let pages = page_list(&form.pages)?;
    let file = file.display().to_string();
    match platform {
        Platform::MacOs | Platform::Linux => {
            let mut args = Vec::new();
            if let Some(printer) = &form.printer {
                args.extend(["-d".to_owned(), printer.clone()]);
            }
            args.extend(["-n".to_owned(), form.copies.max(1).to_string()]);
            if let Some(pages) = pages {
                args.extend(["-P".to_owned(), pages]);
            }
            args.extend(["--".to_owned(), file]);
            Ok(("lp".to_owned(), args, Vec::new()))
        }
        Platform::Windows => {
            // The default application prints the whole document once, on the default
            // printer; the path goes in through the environment, not the script.
            if form.printer.is_some() || form.copies > 1 || pages.is_some() {
                return Err("print_windows_default_only");
            }
            let script = "Start-Process -FilePath $env:FEPDF_PRINT -Verb Print".to_owned();
            let args = vec!["-NoProfile".to_owned(), "-Command".to_owned(), script];
            Ok(("powershell".to_owned(), args, vec![("FEPDF_PRINT".to_owned(), file)]))
        }
    }
}

/// The printers the spooler lists: the first word of each line `lpstat -a` prints.
pub fn printers_in(listing: &str) -> Vec<String> {
    listing
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| !name.is_empty() && !name.ends_with(':'))
        .map(str::to_owned)
        .collect()
}

/// The printers this machine's spooler knows, or none where it has no list to give.
pub fn printers(platform: Platform) -> Vec<String> {
    if platform == Platform::Windows {
        return Vec::new();
    }
    // `lpstat` answers in the reader's language, and a machine with no printers says so
    // in a sentence; it also exits unsuccessfully then, which is the signal read here.
    std::process::Command::new("lpstat")
        .arg("-a")
        .env("LC_ALL", "C")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| printers_in(&String::from_utf8_lossy(&out.stdout)))
        .unwrap_or_default()
}

/// Hands `file` to the spooler and answers what it said: its job line, or its refusal.
///
/// # Errors
/// The spooler's refusal, or why it could not be started.
pub fn print(platform: Platform, file: &Path, form: &PrintForm) -> Result<String, PrintFailure> {
    let (program, args, env) = command(platform, file, form).map_err(PrintFailure::Refused)?;
    let out = std::process::Command::new(&program)
        .args(&args)
        .envs(env)
        .output()
        .map_err(|why| PrintFailure::Spooler(format!("{program}: {why}")))?;
    let said = |bytes: &[u8]| String::from_utf8_lossy(bytes).trim().to_owned();
    if out.status.success() {
        Ok(said(&out.stdout))
    } else {
        Err(PrintFailure::Spooler(said(&out.stderr)))
    }
}

/// Why nothing was handed to the spooler, or why it would not take it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrintFailure {
    /// The form asked for something that cannot be asked for; its locale key.
    Refused(Refusal),
    /// The spooler said no, in its own words.
    Spooler(String),
}

/// The drawer. Answers whether the reader asked to print.
pub fn show(form: &mut PrintForm, ui: &mut egui::Ui, tr: &dyn Fn(&str) -> String) -> bool {
    use crate::app::theme::space;
    ui.label(tr("print_how"));
    ui.add_space(space::ITEM);
    ui.label(tr("print_printer"));
    let printers = form.printers.clone().unwrap_or_default();
    if printers.is_empty() {
        ui.label(egui::RichText::new(tr("print_no_printers")).weak());
    }
    ui.selectable_value(&mut form.printer, None, tr("print_default"));
    for printer in printers {
        let chosen = form.printer.as_deref() == Some(printer.as_str());
        if ui.selectable_label(chosen, printer.as_str()).clicked() {
            form.printer = Some(printer);
        }
    }
    ui.horizontal(|ui| {
        ui.label(tr("print_copies"));
        ui.add(egui::DragValue::new(&mut form.copies).range(1..=99));
    });
    ui.horizontal(|ui| {
        ui.label(tr("print_pages"));
        ui.text_edit_singleline(&mut form.pages);
    });
    ui.add_space(space::ITEM);
    ui.add_enabled(!form.waiting, egui::Button::new(tr("print_go"))).clicked()
}

/// What each platform is asked to run.
#[cfg(test)]
mod commands {
    use super::{PrintForm, command, page_list, printers_in};
    use crate::speech::Platform;
    use std::path::Path;

    fn form(printer: Option<&str>, copies: u32, pages: &str) -> PrintForm {
        PrintForm {
            printer: printer.map(str::to_owned),
            copies,
            pages: pages.to_owned(),
            ..PrintForm::default()
        }
    }

    /// `lp` is told the printer, the copies and the pages, and the file after `--`.
    #[test]
    fn lp_is_told_what_was_chosen() {
        let (program, args, _) =
            command(Platform::MacOs, Path::new("/tmp/-x.pdf"), &form(Some("Office"), 2, "1-3, 5"))
                .expect("a command");
        assert_eq!(program, "lp");
        assert_eq!(args, ["-d", "Office", "-n", "2", "-P", "1-3,5", "--", "/tmp/-x.pdf"]);
    }

    /// **A page list is digits, commas and hyphens**, and nothing else reaches the spooler.
    #[test]
    fn a_page_list_is_a_page_list() {
        assert_eq!(page_list(""), Ok(None));
        assert_eq!(page_list("2-4,7"), Ok(Some("2-4,7".to_owned())));
        for bad in ["0", "1-", "a", "1;rm", "1--2", "-3"] {
            assert_eq!(page_list(bad), Err("print_bad_pages"), "{bad:?} was taken");
        }
    }

    /// Windows prints once, on the default printer, and says so rather than ignoring the
    /// rest; the path goes in through the environment.
    #[test]
    fn windows_prints_the_default_way_or_says_it_cannot() {
        let refused = command(Platform::Windows, Path::new("C:\\a.pdf"), &form(None, 2, ""));
        assert_eq!(refused.err(), Some("print_windows_default_only"));
        let (program, args, env) =
            command(Platform::Windows, Path::new("C:\\a'b.pdf"), &form(None, 1, "")).expect("ok");
        assert_eq!(program, "powershell");
        assert!(!args.iter().any(|a| a.contains("a'b")), "the path reached the script");
        assert_eq!(env, [("FEPDF_PRINT".to_owned(), "C:\\a'b.pdf".to_owned())]);
    }

    /// The printers are the first word of each line.
    #[test]
    fn printers_are_read_from_the_list() {
        let listing =
            "Office accepting requests since Mon 1 Sep\nLabel_Printer accepting requests\n";
        assert_eq!(printers_in(listing), ["Office", "Label_Printer"]);
    }
}
