//! What a save writes, held against the Arlington PDF Model (ROADMAP Y-F24).
//!
//! **Output conforming to ISO 32000-2 is what the engine is for**, and until this nothing
//! measured it: read back, every sample's save reported no decision at all, because the
//! reader does not check what a writer has to get right — a date's syntax among them.
//! The PDF Association's model (`external/arlington`, a submodule pinned like the CMap
//! resources) states, per dictionary of the standard, each key's types, whether it is
//! required, and the version that deprecates it.
//!
//! The file is read back by this engine, which is the weakness: a defect the reader shares
//! with the writer is not seen. What is checked is what the model states without a
//! condition — a `fn:` predicate is not evaluated, so a key it makes required, or a value
//! it allows, is left alone.

use fepdf::PdfDocument;
use fepdf_model::{Handle, Object, PdfArena};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// One key of one dictionary of the model.
#[derive(Debug, Clone)]
struct Row {
    /// Each type the key may take, with the dictionaries a value of that type is linked to
    /// and the values it may hold when the model lists them without a condition.
    types: Vec<(String, Vec<String>, Vec<String>)>,
    required: bool,
    deprecated: bool,
}

/// The model: each dictionary's rows, by key.
struct Model {
    tables: BTreeMap<String, BTreeMap<String, Row>>,
}

/// Splits `field` on `;` outside parentheses and brackets.
fn split_top(field: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth) = (Vec::new(), String::new(), 0i32);
    for c in field.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ';' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    out.push(cur);
    out
}

/// The values a `PossibleValues` part lists, or none when it does not constrain.
///
/// `[]` lists nothing, which is no constraint rather than an empty allowance. A value a
/// version wraps — `fn:SinceVersion(1.1,FitB)`, `fn:Deprecated(2.0,X)` — is the value it
/// wraps; any other `fn:` is a condition this does not evaluate, so the list does not
/// constrain at all.
fn allowed_values(part: &str) -> Vec<String> {
    let listed = part.trim().trim_start_matches('[').trim_end_matches(']');
    let mut out = Vec::new();
    let (mut depth, mut cur) = (0i32, String::new());
    for c in listed.chars().chain(std::iter::once(',')) {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                let item = std::mem::take(&mut cur);
                let item = item.trim();
                if item.is_empty() {
                    continue;
                }
                let wraps =
                    ["fn:SinceVersion(", "fn:Deprecated(", "fn:BeforeVersion(", "fn:IsPDFVersion("];
                if wraps.iter().any(|w| item.starts_with(w)) {
                    out.push(bare_type(item));
                } else if item.starts_with("fn:") {
                    return Vec::new();
                } else {
                    out.push(item.to_string());
                }
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    out
}

/// The bare type a `Type` part names, under any `fn:` wrapper: `fn:SinceVersion(1.5,stream)`
/// is `stream`.
fn bare_type(part: &str) -> String {
    let part = part.trim();
    if part.starts_with("fn:") {
        let inner = part.rsplit(',').next().unwrap_or(part);
        return inner.trim_end_matches(')').trim().to_string();
    }
    part.to_string()
}

impl Model {
    fn load(dir: &Path) -> Self {
        let mut raw = BTreeMap::new();
        for entry in std::fs::read_dir(dir).expect("the Arlington TSVs are there").flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("tsv") {
                continue;
            }
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
            raw.insert(name, std::fs::read_to_string(&path).expect("a TSV reads"));
        }
        let names: BTreeSet<String> = raw.keys().cloned().collect();
        let tables =
            raw.into_iter().map(|(name, text)| (name, Self::rows(&text, &names))).collect();
        Self { tables }
    }

    fn rows(text: &str, names: &BTreeSet<String>) -> BTreeMap<String, Row> {
        let mut rows = BTreeMap::new();
        for line in text.lines().skip(1) {
            let cols: Vec<&str> = line.split('\t').collect();
            if cols.len() < 11 {
                continue;
            }
            let types = split_top(cols[1]);
            let links = split_top(cols[10]);
            let values = split_top(cols[8]);
            let typed = types
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let link = links.get(i).map_or("", String::as_str);
                    let linked = link
                        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                        .filter(|w| names.contains(*w))
                        .map(str::to_string)
                        .collect();
                    let value = values.get(i).map_or("", String::as_str);
                    let allowed = allowed_values(value);
                    (bare_type(t), linked, allowed)
                })
                .collect();
            let deprecated = cols[3].trim().parse::<f32>().is_ok_and(|v| v <= 2.0);
            rows.insert(
                cols[0].to_string(),
                Row { types: typed, required: cols[4].trim() == "TRUE", deprecated },
            );
        }
        rows
    }
}

/// What the model calls the type of `object`, as every name it could go by.
fn kinds(object: &Object) -> &'static [&'static str] {
    match object {
        Object::Boolean(_) => &["boolean"],
        Object::Integer(_) => &["integer", "number", "bitmask"],
        Object::Real(_) => &["number"],
        Object::String(_) | Object::Hex(_) | Object::Text(_) => {
            &["string", "string-byte", "string-ascii", "string-text", "date"]
        }
        Object::Name(_) => &["name"],
        Object::Array(_) => &["array", "rectangle", "matrix"],
        Object::Dictionary(_) => &["dictionary", "name-tree", "number-tree"],
        Object::Stream(..) => &["stream"],
        Object::Null => &["null"],
        Object::Reference(_) => &[],
    }
}

/// Whether `text` is a date as 7.9.4 writes one: `D:YYYYMMDDHHmmSSOHH'mm`, every part
/// after the year optional, the apostrophe after the minutes allowed.
fn is_date(text: &str) -> bool {
    let Some(rest) = text.strip_prefix("D:") else { return false };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 4 || digits.len() > 14 || !digits.len().is_multiple_of(2) {
        return false;
    }
    let zone = &rest[digits.len()..];
    if zone.is_empty() || zone == "Z" {
        return true;
    }
    let Some(offset) = zone.strip_prefix('+').or_else(|| zone.strip_prefix('-')) else {
        return false;
    };
    let offset = offset.trim_end_matches('\'');
    let parts: Vec<&str> = offset.split('\'').collect();
    parts.iter().all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_digit())) && parts.len() <= 2
}

/// The text of a string object, for the date check.
fn text_of(object: &Object) -> Option<String> {
    match object {
        Object::Text(t) => Some(t.clone()),
        Object::String(b) | Object::Hex(b) => Some(String::from_utf8_lossy(b).into_owned()),
        _ => None,
    }
}

/// What reading one saved file against the model found, by kind.
#[derive(Default)]
struct Findings {
    by_kind: BTreeMap<String, BTreeSet<String>>,
}

impl Findings {
    fn add(&mut self, kind: &str, what: String) {
        self.by_kind.entry(kind.to_string()).or_default().insert(what);
    }
}

/// Walks a saved document from its catalogue and its information dictionary.
struct Walk<'a> {
    model: &'a Model,
    arena: &'a PdfArena,
    seen: BTreeSet<(usize, String)>,
    findings: Findings,
    /// What is still to be read, and as which table: a queue, so a deep outline or page
    /// tree costs heap and not stack.
    queue: Vec<(Object, String)>,
}

impl Walk<'_> {
    /// The table among `candidates` that `dict` is: the one whose `/Type` and `/Subtype`
    /// name what it carries, which lacks fewest of its required keys, and which names most
    /// of the keys it has — the first of them in a tie.
    fn pick<'m>(
        &self,
        candidates: &'m [String],
        dict: &BTreeMap<Handle<fepdf_model::PdfName>, Object>,
    ) -> Option<&'m String> {
        let value = |key: &str| {
            dict.get(&self.arena.name(key))
                .map(|o| o.resolve(self.arena))
                .and_then(|o| o.as_name())
                .and_then(|n| self.arena.get_name_str(n))
        };
        let present: BTreeSet<String> =
            dict.keys().filter_map(|k| self.arena.get_name_str(*k)).collect();
        let score = |name: &String| -> i64 {
            let Some(table) = self.model.tables.get(name) else { return i64::MIN };
            let mut score = 0;
            for key in ["Type", "Subtype", "FT", "S"] {
                let allowed = table
                    .get(key)
                    .map(|r| r.types.iter().flat_map(|t| t.2.clone()).collect::<Vec<_>>());
                match (allowed, value(key)) {
                    (Some(a), Some(v)) if a.contains(&v) => score += 100,
                    (Some(a), Some(_)) if !a.is_empty() => score -= 100,
                    _ => {}
                }
            }
            let count = |n: usize| i64::try_from(n).unwrap_or(i64::MAX);
            let missing = table
                .iter()
                .filter(|(k, r)| r.required && *k != "*" && !present.contains(*k))
                .count();
            score -= 10 * count(missing);
            score += count(present.iter().filter(|k| table.contains_key(*k)).count());
            score
        };
        candidates.iter().rev().max_by_key(|c| score(c))
    }

    /// Whether `table` describes an array: its keys are positions.
    fn is_array_table(&self, table: &str) -> bool {
        self.model.tables.get(table).is_some_and(|rows| {
            rows.keys().all(|k| k == "*" || k.trim_end_matches('*').parse::<usize>().is_ok())
        })
    }

    /// The table among `candidates` that an array is: an array table whose first
    /// position names the name the array starts with, else the first array table.
    fn pick_array<'m>(&self, candidates: &'m [String], items: &[Object]) -> Option<&'m String> {
        let first = items
            .first()
            .map(|o| o.resolve(self.arena))
            .and_then(|o| o.as_name())
            .and_then(|n| self.arena.get_name_str(n));
        let arrays: Vec<&'m String> =
            candidates.iter().filter(|c| self.is_array_table(c)).collect();
        // The name that says which array this is sits at the first or second position: a
        // colour space's family at the first, a destination's fit at the second.
        let name_at = |i: usize| {
            items
                .get(i)
                .map(|o| o.resolve(self.arena))
                .and_then(|o| o.as_name())
                .and_then(|n| self.arena.get_name_str(n))
        };
        let names = |c: &String| -> bool {
            let Some(table) = self.model.tables.get(c) else { return false };
            (0..2).all(|i| match (table.get(&i.to_string()), name_at(i)) {
                (Some(row), Some(name)) => {
                    row.types.iter().any(|t| t.2.is_empty() || t.2.contains(&name))
                }
                _ => true,
            }) && (0..2).any(|i| {
                table.get(&i.to_string()).is_some_and(|r| r.types.iter().any(|t| !t.2.is_empty()))
            })
        };
        let _ = &first;
        arrays
            .iter()
            .find(|c| names(c))
            .or_else(|| arrays.first())
            .copied()
            .or_else(|| candidates.first())
    }

    /// The rows `dict` is held against: `table`'s, and for a form field merged with its
    /// widget annotation (12.7.4.1), the widget's and the field's together.
    fn rows_for(
        &self,
        table: &str,
        dict: &BTreeMap<Handle<fepdf_model::PdfName>, Object>,
    ) -> BTreeMap<String, Row> {
        let mut rows = self.model.tables.get(table).cloned().unwrap_or_default();
        let name_of = |key: &str| {
            dict.get(&self.arena.name(key))
                .and_then(|o| o.resolve(self.arena).as_name())
                .and_then(|n| self.arena.get_name_str(n))
        };
        // `/FT` is inheritable (Table 226): a kid widget carrying `/DA` or `/V` is a field
        // whose type its parent states.
        let mut ft = name_of("FT");
        let mut at = dict.get(&self.arena.name("Parent")).cloned();
        let mut depth = 0;
        while ft.is_none() && depth < 32 {
            let Some(parent) = at.take() else { break };
            ft = fepdf_model::access::name_in(self.arena, &parent, "FT");
            at = fepdf_model::access::entry(self.arena, &parent, "Parent");
            depth += 1;
        }
        let fields: &[&str] = match ft.as_deref() {
            Some("Tx") => &["FieldTx"],
            Some("Ch") => &["FieldChoice"],
            Some("Sig") => &["FieldSig"],
            Some("Btn") => &["FieldBtnCheckbox", "FieldBtnPush", "FieldBtnRadio"],
            _ => &[],
        };
        let mut extra: Vec<&str> = Vec::new();
        if table.starts_with("Field") && name_of("Subtype").as_deref() == Some("Widget") {
            extra.push("AnnotWidget");
        }
        if table == "AnnotWidget" {
            extra.extend(fields);
        }
        for name in extra {
            for (k, r) in self.model.tables.get(name).into_iter().flatten() {
                rows.entry(k.clone()).or_insert_with(|| r.clone());
            }
        }
        rows
    }

    fn visit(&mut self, object: &Object, table: &str) {
        let identity = match object {
            Object::Reference(h) => h.index() as usize,
            Object::Dictionary(d) | Object::Stream(d, _) => 1 << 40 | d.index() as usize,
            Object::Array(a) => 2 << 40 | a.index() as usize,
            _ => return,
        };
        if !self.seen.insert((identity, table.to_string())) {
            return;
        }
        let resolved = object.resolve(self.arena);
        let Some(rows) = self.model.tables.get(table) else { return };
        match &resolved {
            Object::Dictionary(d) | Object::Stream(d, _) => {
                let dict = self.arena.get_dict(*d).unwrap_or_default();
                let rows = self.rows_for(table, &dict);
                let stream = matches!(resolved, Object::Stream(..));
                self.dictionary(&dict, &rows, table, stream);
            }
            Object::Array(a) => {
                let items = self.arena.get_array(*a).unwrap_or_default();
                for (i, item) in items.iter().enumerate() {
                    // `0*`, `1*` … repeat as a group: element i is the (i mod n)th of them.
                    let repeating: Vec<&Row> = rows
                        .iter()
                        .filter(|(k, _)| k.len() > 1 && k.ends_with('*'))
                        .map(|(_, r)| r)
                        .collect();
                    let fixed = rows.get(&i.to_string()).or_else(|| rows.get("*"));
                    // A repeating group may leave an optional member out (an attribute's
                    // revision number), so an item is held against the member it fits.
                    let row = fixed.or_else(|| {
                        let kinds = kinds(&item.resolve(self.arena));
                        repeating
                            .iter()
                            .find(|r| r.types.iter().any(|t| kinds.contains(&t.0.as_str())))
                            .or_else(|| repeating.get(i % repeating.len().max(1)))
                            .copied()
                    });
                    if let Some(row) = row {
                        self.value(item, row, &format!("{table}[{i}]"));
                    }
                }
            }
            _ => {}
        }
    }

    fn dictionary(
        &mut self,
        dict: &BTreeMap<Handle<fepdf_model::PdfName>, Object>,
        rows: &BTreeMap<String, Row>,
        table: &str,
        stream: bool,
    ) {
        let present: BTreeSet<String> =
            dict.keys().filter_map(|k| self.arena.get_name_str(*k)).collect();
        for (key, row) in rows {
            // A stream's `/Length` is written with its data and is not a dictionary entry
            // the reader keeps.
            if row.required && key != "*" && !present.contains(key) && !(stream && key == "Length")
            {
                self.findings.add("required key absent", format!("{table}: /{key}"));
            }
        }
        for (k, value) in dict {
            let Some(key) = self.arena.get_name_str(*k) else { continue };
            let Some(row) = rows.get(&key).or_else(|| rows.get("*")) else {
                // `Stream` is the model's table for a stream it says nothing more about, so
                // a key it does not name there is not a finding.
                if table != "Stream" {
                    self.findings.add("key the model does not name", format!("{table}: /{key}"));
                }
                continue;
            };
            if row.deprecated && rows.contains_key(&key) {
                self.findings.add("key deprecated in 2.0", format!("{table}: /{key}"));
            }
            self.value(value, row, &format!("{table}/{key}"));
        }
    }

    fn value(&mut self, value: &Object, row: &Row, at: &str) {
        let resolved = value.resolve(self.arena);
        let have = kinds(&resolved);
        let Some((ty, links, allowed)) = row.types.iter().find(|t| have.contains(&t.0.as_str()))
        else {
            let wanted: Vec<&str> = row.types.iter().map(|t| t.0.as_str()).collect();
            self.findings.add(
                "wrong type",
                format!("{at}: {} where {}", have.first().unwrap_or(&"?"), wanted.join("|")),
            );
            return;
        };
        if ty == "date"
            && let Some(text) = text_of(&resolved).filter(|t| !is_date(t))
        {
            self.findings.add("date not in 7.9.4's form", format!("{at}: {text:?}"));
        }
        if ty == "name" && !allowed.is_empty() {
            let name =
                resolved.as_name().and_then(|n| self.arena.get_name_str(n)).unwrap_or_default();
            if !allowed.iter().any(|a| a == &name) {
                self.findings.add("name the model does not allow", format!("{at}: /{name}"));
            }
        }
        if links.is_empty() {
            return;
        }
        match ty.as_str() {
            "name-tree" | "number-tree" => self.tree(&resolved, links, ty == "name-tree", at),
            _ => {
                let target = match &resolved {
                    Object::Dictionary(d) | Object::Stream(d, _) => {
                        let dict = self.arena.get_dict(*d).unwrap_or_default();
                        let fitting: Vec<String> =
                            links.iter().filter(|l| !self.is_array_table(l)).cloned().collect();
                        self.pick(if fitting.is_empty() { links } else { &fitting }, &dict).cloned()
                    }
                    Object::Array(a) => {
                        let items = self.arena.get_array(*a).unwrap_or_default();
                        self.pick_array(links, &items).cloned()
                    }
                    _ => links.first().cloned(),
                };
                if let Some(table) = target {
                    self.queue.push((value.clone(), table));
                }
            }
        }
    }

    /// The values of a name or number tree, each held against `links`.
    fn tree(&mut self, root: &Object, links: &[String], names: bool, at: &str) {
        let mut stack = vec![root.clone()];
        let mut guard = 0;
        while let Some(node) = stack.pop() {
            guard += 1;
            if guard > 100_000 {
                break;
            }
            let entry = |key: &str| fepdf_model::access::items(self.arena, &node, key);
            stack.extend(entry("Kids"));
            let pairs = entry(if names { "Names" } else { "Nums" });
            // A leaf's value is whichever of `links` its kind fits: an array table for an
            // array, a dictionary table otherwise.
            let arrays: Vec<String> =
                links.iter().filter(|l| self.is_array_table(l)).cloned().collect();
            let dicts: Vec<String> =
                links.iter().filter(|l| !self.is_array_table(l)).cloned().collect();
            let row = Row {
                types: vec![
                    ("dictionary".to_string(), dicts.clone(), Vec::new()),
                    ("stream".to_string(), dicts, Vec::new()),
                    ("array".to_string(), arrays, Vec::new()),
                ],
                required: false,
                deprecated: false,
            };
            for leaf in pairs.iter().skip(1).step_by(2) {
                if !matches!(leaf.resolve(self.arena), Object::Null) {
                    self.value(leaf, &row, at);
                }
            }
        }
    }
}

/// Reads `bytes` back and holds what it reaches against `model`.
fn findings_for(model: &Model, bytes: Vec<u8>) -> Findings {
    let doc = PdfDocument::open(bytes.into()).expect("the save reads back");
    let inner = doc.inner();
    let mut walk = Walk {
        model,
        arena: inner.arena(),
        seen: BTreeSet::new(),
        findings: Findings::default(),
        queue: vec![(Object::Reference(*inner.root_handle()), "Catalog".to_string())],
    };
    if let Some(info) = inner.info_handle() {
        walk.queue.push((Object::Reference(info), "DocInfo".to_string()));
    }
    while let Some((object, table)) = walk.queue.pop() {
        walk.visit(&object, &table);
    }
    walk.findings
}

fn model_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../external/arlington/tsv/latest")
}

/// The departures `arlington_known.tsv` lists for `sample`, as `(kind, finding)`.
fn known(sample: &str) -> BTreeSet<(String, String)> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/suite/arlington_known.tsv");
    std::fs::read_to_string(path)
        .expect("arlington_known.tsv reads")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let cols: Vec<&str> = l.split('\t').collect();
            (cols.len() == 4 && cols[0] == sample)
                .then(|| (cols[1].to_string(), cols[2].to_string()))
        })
        .collect()
}

/// What `sample`'s save departs from, as `(kind, finding)`.
fn departures(model: &Model, sample: &str) -> BTreeSet<(String, String)> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples").join(sample);
    let source = std::fs::read(&path).expect("samples/ is in the tree");
    let doc = PdfDocument::open(source.into()).expect("the sample opens");
    let out = std::env::temp_dir().join(format!("fepdf-arlington-{}-{sample}", std::process::id()));
    let _ = doc.save_with_options(&out, "2.0", &fepdf::SaveOptions::default()).expect("it saves");
    let found = findings_for(model, std::fs::read(&out).expect("the save was written"));
    let _ = std::fs::remove_file(&out);
    found
        .by_kind
        .into_iter()
        .flat_map(|(kind, items)| items.into_iter().map(move |i| (kind.clone(), i)))
        .collect()
}

/// **A save departs from the model by what is listed, and by nothing else.** A new
/// departure fails here, and so does a listed one the save no longer makes — the line is
/// then removed, so the list is what is left to do and not what once was.
fn holds(sample: &str) {
    let model = Model::load(&model_dir());
    let found = departures(&model, sample);
    let listed = known(sample);
    let new: Vec<_> = found.difference(&listed).collect();
    let gone: Vec<_> = listed.difference(&found).collect();
    assert!(new.is_empty(), "{sample}: departures not in arlington_known.tsv: {new:#?}");
    assert!(gone.is_empty(), "{sample}: listed departures its save no longer makes: {gone:#?}");
}

macro_rules! each_sample {
    ($($name:ident => $file:literal),* $(,)?) => {
        $(#[test] fn $name() { holds($file); })*
    };
}

each_sample! {
    pump_drawing => "02_低段汚水ポンプ電動機.pdf",
    bokutokitan => "bokutokitan.pdf",
    constitution => "constitution.pdf",
    fugaku => "fugaku.pdf",
    fy05 => "fy05.pdf",
    intel_sdm => "intel_sdm.pdf",
    print_sample => "print_sample.pdf",
    sample_02c => "sample_02c.pdf",
    unicode_16 => "unicode_16.pdf",
    volvo_xc90 => "volvo_xc90.pdf",
}

/// Every sample has a test above: one added to `samples/` and not here would be saved by
/// nothing this file runs.
#[test]
fn every_sample_is_held() {
    let this = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/suite/arlington_test.rs"),
    )
    .expect("this file reads");
    let samples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    for entry in std::fs::read_dir(samples).expect("samples/").flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.path().extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
            assert!(this.contains(&format!("\"{name}\"")), "{name} is held by no test here");
        }
    }
}
