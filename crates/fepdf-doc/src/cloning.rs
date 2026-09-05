use fepdf_model::{Handle, Object, PdfArena, PdfName, PdfResult};
use std::collections::BTreeMap as Dict;

/// What the arena calls a dictionary handle, which is otherwise three lines wide.
type DictHandle = Handle<Dict<Handle<PdfName>, Object>>;
use std::collections::BTreeMap;

/// Utility for cloning PDF objects and migrating them between arenas or contexts.
///
/// RR-15 COMPLIANT: This implementation is iterative to prevent stack overflow
/// and uses BTreeMap for deterministic output.
pub struct ObjectCloner<'a> {
    source: &'a PdfArena,
    target: &'a PdfArena,
    /// Mapping from source Handle<Object> to target Handle<Object>.
    handle_map: BTreeMap<Handle<Object>, Handle<Object>>,
    /// WORK STACK ENTRY: (SourceHandle, TargetHandle, Phase)
    /// Phase 0: Start cloning object
    /// Phase 1: Children are queued, finalize container
    stack: Vec<CloningTask>,
}

#[derive(Debug)]
enum CloningTask {
    CloneHandle(Handle<Object>, Handle<Object>),
}

/// Whether a cloned object is the body of an indirect object or a value inside one.
///
/// The only thing it decides is what happens to a stream, and that is enough to need a
/// name: a stream is never a direct object (7.3.8), so one appearing inside a dictionary or
/// an array has to be promoted, and one that is the object itself must not be.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Position {
    TopLevel,
    Nested,
}

impl<'a> ObjectCloner<'a> {
    /// Creates a new object cloner for migrating objects between source and target arenas.
    pub fn new(source: &'a PdfArena, target: &'a PdfArena) -> Self {
        Self { source, target, handle_map: BTreeMap::new(), stack: Vec::new() }
    }

    /// Clones a specific handle's object and returns the new handle.
    /// This is the primary entry point for iterative cloning.
    pub fn clone_handle(&mut self, source_h: Handle<Object>) -> PdfResult<Handle<Object>> {
        let target_h = self.queue_clone(source_h);
        self.process_queue()?;
        Ok(target_h)
    }

    /// Clones an object and everything it refers to.
    ///
    /// **Cloning is two halves and only this one runs both.** Walking an object turns each
    /// reference into a placeholder and a task; `process_queue` is what fills the
    /// placeholders in. A caller who stopped after the walk got a page dictionary whose
    /// `/Contents`, `/Resources` and `/Annots` were all `Null`, and the document answered
    /// `expected a stream, found null` when the clone was rendered. Six call sites across
    /// two crates did exactly that, which is why the walk is not reachable on its own.
    ///
    /// The validation pass at the end of `process_queue` exists to catch that very thing,
    /// and could not, because it lives inside the half that was never run.
    pub fn clone_complete(&mut self, obj: &Object) -> PdfResult<Object> {
        let cloned = self.walk(obj, Position::TopLevel);
        self.process_queue()?;
        Ok(cloned)
    }

    /// Internal helper to queue a handle for cloning and return a target placeholder.
    fn queue_clone(&mut self, source_h: Handle<Object>) -> Handle<Object> {
        if let Some(&target_h) = self.handle_map.get(&source_h) {
            return target_h;
        }

        // Allocate a placeholder object in the target arena
        let target_h = self.target.alloc_object(Object::Null);
        self.handle_map.insert(source_h, target_h);
        self.stack.push(CloningTask::CloneHandle(source_h, target_h));
        target_h
    }

    /// Iteratively processes the work stack to complete cloning of all queued objects.
    fn process_queue(&mut self) -> PdfResult<()> {
        while let Some(task) = self.stack.pop() {
            match task {
                CloningTask::CloneHandle(source_h, target_h) => {
                    let source_obj = self.source.get_object(source_h).ok_or_else(|| {
                        fepdf_model::PdfError::Other("Dangling reference in source".into())
                    })?;

                    let target_obj = self.walk(&source_obj, Position::TopLevel);
                    self.target.set_object(target_h, target_obj);
                }
            }
        }

        // RR-15 HARDENING: Final validation pass to ensure no objects were left as Null
        // unless they were explicitly Null in the source.
        for (&source_h, &target_h) in &self.handle_map {
            let source_obj = self.source.get_object(source_h).unwrap_or(Object::Null);
            let target_obj = self.target.get_object(target_h).unwrap_or(Object::Null);
            if matches!(target_obj, Object::Null) && !matches!(source_obj, Object::Null) {
                return Err(fepdf_model::PdfError::Other(
                    format!("Cloning failed: Object {target_h:?} remains Null in target").into(),
                ));
            }
        }
        Ok(())
    }

    /// Copies one object into the target arena, queueing every reference it holds.
    ///
    /// **The two walkers this replaces differed in one arm out of nine.** `clone_object`
    /// and `clone_object_shallow` were the same code for numbers, strings, names,
    /// references, arrays and dictionaries, and parted company only over streams — one
    /// returning the stream, the other promoting it to an object of its own and returning a
    /// reference to it. Neither name said which was which, and one of them additionally
    /// returned a `PdfResult` that no arm ever failed.
    ///
    /// The distinction is [`Position`], and it is real: a stream that *is* the object has
    /// to stay a stream at that handle, and a stream found inside a dictionary or an array
    /// cannot stay there, because a stream is never a direct object.
    fn walk(&mut self, obj: &Object, position: Position) -> Object {
        match obj {
            Object::Boolean(b) => Object::Boolean(*b),
            Object::Integer(i) => Object::Integer(*i),
            Object::Real(f) => Object::Real(*f),
            Object::String(s) => Object::String(s.clone()),
            Object::Hex(s) => Object::Hex(s.clone()),
            Object::Text(s) => Object::Text(s.clone()),
            Object::Null => Object::Null,
            Object::Name(h) => {
                let name_str = self.source.get_name_str(*h).unwrap_or_default();
                Object::Name(self.target.name(&name_str))
            }
            Object::Reference(h) => Object::Reference(self.queue_clone(*h)),
            Object::Array(h) => {
                let source_arr = self.source.get_array(*h).unwrap_or_default();
                let items: Vec<Object> =
                    source_arr.iter().map(|item| self.walk(item, Position::Nested)).collect();
                Object::Array(self.target.alloc_array(items))
            }
            Object::Dictionary(h) => Object::Dictionary(self.walk_dict(*h)),
            Object::Stream(dh, data) => {
                let target_dh = self.walk_dict(*dh);
                let stream = Object::Stream(target_dh, data.clone());
                match position {
                    Position::TopLevel => stream,
                    // A stream is never a direct object, so one found inside a container
                    // becomes an object of its own and leaves a reference behind.
                    Position::Nested => Object::Reference(self.target.alloc_object(stream)),
                }
            }
        }
    }

    /// Copies a dictionary's keys and values, which both walkers did identically.
    fn walk_dict(&mut self, source_dh: DictHandle) -> DictHandle {
        let source_dict = self.source.get_dict(source_dh).unwrap_or_default();
        let mut target_dict = BTreeMap::new();
        for (k, v) in source_dict {
            let k_str = self.source.get_name_str(k).unwrap_or_default();
            let target_k = self.target.name(&k_str);
            let value = self.walk(&v, Position::Nested);
            target_dict.insert(target_k, value);
        }
        self.target.alloc_dict(target_dict)
    }
}

#[cfg(test)]
mod position {
    use super::{ObjectCloner, Position};
    use fepdf_model::{Object, PdfArena};

    /// **A stream that is the object stays a stream; a stream inside a dictionary becomes
    /// a reference to one.** This is the whole of the difference between the two walkers
    /// that used to exist, and the only reason [`Position`] is a type rather than a
    /// comment. A stream is never a direct object (7.3.8).
    #[test]
    fn a_stream_is_promoted_only_where_it_cannot_stay() {
        let source = PdfArena::new();
        let target = PdfArena::new();
        let dh = source.alloc_dict(std::collections::BTreeMap::new());
        let data = std::sync::Arc::new(fepdf_model::object::SublimatedData::Commands {
            items: Vec::new(),
        });
        let stream = Object::Stream(dh, data);

        let mut cloner = ObjectCloner::new(&source, &target);
        assert!(
            matches!(cloner.walk(&stream, Position::TopLevel), Object::Stream(..)),
            "the body of an indirect object keeps its stream"
        );
        assert!(
            matches!(cloner.walk(&stream, Position::Nested), Object::Reference(_)),
            "and one found inside a container is promoted out of it"
        );
    }

    /// Everything else is the same in both positions, which is why one walker does.
    #[test]
    fn nothing_else_depends_on_the_position() {
        let source = PdfArena::new();
        let target = PdfArena::new();
        let mut cloner = ObjectCloner::new(&source, &target);

        for obj in [Object::Integer(7), Object::Boolean(true), Object::Null] {
            let top = cloner.walk(&obj, Position::TopLevel);
            let nested = cloner.walk(&obj, Position::Nested);
            assert_eq!(format!("{top:?}"), format!("{nested:?}"), "{obj:?} differs by position");
        }
    }
}
