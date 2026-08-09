use fepdf_core::{Handle, Object, PdfArena, PdfResult};
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

    /// Recursively clones a high-level Object into the target arena.
    /// Scalar values are cloned immediately; containers and references are queued.
    /// NOTE: This is still "shallowly" recursive for nested arrays/dicts passed as values,
    /// but the core handle migration is iterative.
    pub fn clone_object(&mut self, obj: &Object) -> PdfResult<Object> {
        match obj {
            Object::Boolean(b) => Ok(Object::Boolean(*b)),
            Object::Integer(i) => Ok(Object::Integer(*i)),
            Object::Real(f) => Ok(Object::Real(*f)),
            Object::String(s) => Ok(Object::String(s.clone())),
            Object::Hex(s) => Ok(Object::Hex(s.clone())),
            Object::Null => Ok(Object::Null),
            Object::Name(h) => {
                let name_str = self.source.get_name_str(*h).unwrap_or_default();
                Ok(Object::Name(self.target.name(&name_str)))
            }
            Object::Reference(h) => {
                let target_h = self.queue_clone(*h);
                Ok(Object::Reference(target_h))
            }
            Object::Array(h) => {
                let source_arr = self.source.get_array(*h).unwrap_or_default();
                let mut target_arr = Vec::with_capacity(source_arr.len());
                for item in source_arr {
                    target_arr.push(self.clone_object_shallow(&item));
                }
                Ok(Object::Array(self.target.alloc_array(target_arr)))
            }
            Object::Dictionary(h) => {
                let source_dict = self.source.get_dict(*h).unwrap_or_default();
                let mut target_dict = BTreeMap::new();
                for (k, v) in source_dict {
                    let k_str = self.source.get_name_str(k).unwrap_or_default();
                    let target_k = self.target.name(&k_str);
                    target_dict.insert(target_k, self.clone_object_shallow(&v));
                }
                Ok(Object::Dictionary(self.target.alloc_dict(target_dict)))
            }
            Object::Stream(dh, data) => {
                let source_dict = self.source.get_dict(*dh).unwrap_or_default();
                let mut target_dict = BTreeMap::new();
                for (k, v) in source_dict {
                    let k_str = self.source.get_name_str(k).unwrap_or_default();
                    let target_k = self.target.name(&k_str);
                    target_dict.insert(target_k, self.clone_object_shallow(&v));
                }
                let target_dh = self.target.alloc_dict(target_dict);
                Ok(Object::Stream(target_dh, data.clone()))
            }
            Object::Text(s) => Ok(Object::Text(s.clone())),
        }
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
                        fepdf_core::PdfError::Other("Dangling reference in source".into())
                    })?;

                    let target_obj = self.clone_object(&source_obj)?;
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
                return Err(fepdf_core::PdfError::Other(
                    format!("Cloning failed: Object {target_h:?} remains Null in target").into(),
                ));
            }
        }
        Ok(())
    }

    /// Clone an object "shallowly" by converting references to queued handles.
    fn clone_object_shallow(&mut self, obj: &Object) -> Object {
        match obj {
            Object::Boolean(b) => Object::Boolean(*b),
            Object::Integer(i) => Object::Integer(*i),
            Object::Real(f) => Object::Real(*f),
            Object::String(s) => Object::String(s.clone()),
            Object::Hex(s) => Object::Hex(s.clone()),
            Object::Null => Object::Null,
            Object::Name(h) => {
                let name_str = self.source.get_name_str(*h).unwrap_or_default();
                Object::Name(self.target.name(&name_str))
            }
            Object::Reference(h) => Object::Reference(self.queue_clone(*h)),
            Object::Array(h) => {
                let source_arr = self.source.get_array(*h).unwrap_or_default();
                let mut target_arr = Vec::with_capacity(source_arr.len());
                for item in source_arr {
                    target_arr.push(self.clone_object_shallow(&item));
                }
                Object::Array(self.target.alloc_array(target_arr))
            }
            Object::Dictionary(h) => {
                let source_dict = self.source.get_dict(*h).unwrap_or_default();
                let mut target_dict = BTreeMap::new();
                for (k, v) in source_dict {
                    let k_str = self.source.get_name_str(k).unwrap_or_default();
                    let target_k = self.target.name(&k_str);
                    target_dict.insert(target_k, self.clone_object_shallow(&v));
                }
                Object::Dictionary(self.target.alloc_dict(target_dict))
            }
            Object::Stream(dh, data) => {
                // RR-15: Streams MUST be indirect objects. If we encounter a stream
                // variant during shallow cloning (e.g., inside a dictionary),
                // we must promote it to a top-level object and return a reference.
                let source_dict = self.source.get_dict(*dh).unwrap_or_default();
                let mut target_dict = BTreeMap::new();
                for (k, v) in source_dict {
                    let k_str = self.source.get_name_str(k).unwrap_or_default();
                    let target_k = self.target.name(&k_str);
                    target_dict.insert(target_k, self.clone_object_shallow(&v));
                }
                let target_dh = self.target.alloc_dict(target_dict);
                let stream_obj = Object::Stream(target_dh, data.clone());
                let target_h = self.target.alloc_object(stream_obj);
                Object::Reference(target_h)
            }
            Object::Text(s) => Object::Text(s.clone()),
        }
    }
}
