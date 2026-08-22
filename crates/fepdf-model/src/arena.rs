use crate::PdfResult;
use crate::handle::Handle;
use crate::object::{Object, ObjectEntry, PdfName, SublimatedData};
use bytes::Bytes;
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::sync::Arc;

/// A sequential arena for PDF objects, optimized for cache locality and thread safety.
///
/// This implementation uses an `Arc<ArenaInner>` to enable zero-copy cloning of the arena
/// handle, allowing multiple components (Ingestor, Refinery, CLI) to share the same
/// document state without expensive duplication.
#[derive(Default, Clone)]
pub struct PdfArena {
    inner: Arc<ArenaInner>,
}

/// The internal heap-allocated state of the PdfArena.
///
/// All pools are wrapped in `RwLock` to allow thread-safe interior mutability, enabling the
/// "Pass-based" refinement system where objects are updated in-place as they move
/// through the normalization pipeline.
#[derive(Default)]
struct ArenaInner {
    /// Contiguous pool of objects for maximum cache efficiency.
    objects: RwLock<Vec<ObjectEntry>>,
    /// Dedicated pools for complex types to allow typesafe handles.
    /// This separation prevents "Handle Confusion" (e.g., using an Array handle to access a Dictionary).
    dicts: RwLock<Vec<BTreeMap<Handle<PdfName>, Object>>>,
    arrays: RwLock<Vec<Vec<Object>>>,
    /// Interned names to ensure all `/Name` references in the document point to a single memory location.
    names: RwLock<Vec<PdfName>>,
    /// Index for fast name lookup during interning.
    name_map: RwLock<BTreeMap<PdfName, Handle<PdfName>>>,
    /// The document version (e.g., 1.7, 2.0).
    version: RwLock<f32>,
    /// Index for fast lookup: Map of Object value to list of indirect handles containing that value.
    object_index: RwLock<BTreeMap<Object, Vec<Handle<Object>>>>,
}

impl PdfArena {
    /// Creates an empty arena declaring PDF 1.7.
    pub fn new() -> Self {
        Self::with_version(1.7)
    }

    /// Creates an empty arena declaring `version`.
    pub fn with_version(version: f32) -> Self {
        let arena = Self::default();
        *arena.inner.version.write() = version;
        arena
    }

    /// The document version this arena will write.
    pub fn version(&self) -> f32 {
        *self.inner.version.read()
    }

    /// Sets the document version this arena will write.
    pub fn set_version(&self, version: f32) {
        *self.inner.version.write() = version;
    }

    /// Interns a name, returning a deduplicated handle.
    pub fn intern_name(&self, name: PdfName) -> Handle<PdfName> {
        if let Some(h) = self.inner.name_map.read().get(&name) {
            return *h;
        }

        let mut map = self.inner.name_map.write();
        let mut names = self.inner.names.write();

        // Re-check after acquiring write lock
        if let Some(h) = map.get(&name) {
            return *h;
        }

        let idx = u32::try_from(names.len()).unwrap_or(0);
        let h = Handle::new(idx);
        names.push(name.clone());
        map.insert(name, h);
        h
    }

    /// Returns a handle for a name, interning it if necessary (Get-or-Create).
    pub fn name(&self, name: &str) -> Handle<PdfName> {
        if let Some(h) = self.inner.name_map.read().get(name) {
            return *h;
        }
        self.intern_name(PdfName::new(name))
    }

    /// Returns the string representation of a name handle.
    pub fn get_name_str(&self, handle: Handle<PdfName>) -> Option<String> {
        self.inner.names.read().get(handle.index() as usize).map(|n| n.as_str().to_string())
    }

    /// Looks up an already-interned name, without interning a new one.
    pub fn get_name_by_str(&self, name: &str) -> Option<Handle<PdfName>> {
        self.inner.name_map.read().get(name).copied()
    }

    /// Resolves a name handle back to the name.
    pub fn get_name(&self, handle: Handle<PdfName>) -> Option<PdfName> {
        self.inner.names.read().get(handle.index() as usize).cloned()
    }

    /// Returns all valid dictionary handles in the arena.
    pub fn all_dict_handles(&self) -> Vec<Handle<BTreeMap<Handle<PdfName>, Object>>> {
        let count = self.inner.dicts.read().len() as u32;
        (0..count).map(Handle::new).collect()
    }

    /// Registers a new object, returning a unique handle.
    pub fn alloc_object(&self, object: Object) -> Handle<Object> {
        let mut objects = self.inner.objects.write();
        let h = Handle::new(objects.len() as u32);
        objects.push(ObjectEntry { object: object.clone(), generation: 0 });

        let mut idx = self.inner.object_index.write();
        idx.entry(object).or_default().push(h);
        h
    }

    /// Allocates a dictionary.
    pub fn alloc_dict(
        &self,
        dict: BTreeMap<Handle<PdfName>, Object>,
    ) -> Handle<BTreeMap<Handle<PdfName>, Object>> {
        let mut dicts = self.inner.dicts.write();
        let index = u32::try_from(dicts.len()).unwrap_or(0);
        dicts.push(dict);
        Handle::new(index)
    }

    /// Allocates an array.
    pub fn alloc_array(&self, array: Vec<Object>) -> Handle<Vec<Object>> {
        let mut arrays = self.inner.arrays.write();
        let index = u32::try_from(arrays.len()).unwrap_or(0);
        arrays.push(array);
        Handle::new(index)
    }

    /// Reads an object by handle.
    pub fn get_object(&self, handle: Handle<Object>) -> Option<Object> {
        self.inner.objects.read().get(handle.index() as usize).map(|e| e.object.clone())
    }

    /// Replaces the object at `handle`.
    pub fn set_object(&self, handle: Handle<Object>, object: Object) {
        let mut objects = self.inner.objects.write();
        if let Some(e) = objects.get_mut(handle.index() as usize) {
            let old_val = e.object.clone();
            if old_val == object {
                return;
            }
            e.object = object.clone();

            let mut idx = self.inner.object_index.write();
            // Remove from old entry list
            if let Some(list) = idx.get_mut(&old_val) {
                if let Some(pos) = list.iter().position(|&x| x == handle) {
                    list.remove(pos);
                }
                if list.is_empty() {
                    idx.remove(&old_val);
                }
            }
            // Add to new entry list
            idx.entry(object).or_default().push(handle);
        }
    }

    /// Reads an object together with its generation.
    pub fn get_object_entry(&self, handle: Handle<Object>) -> Option<ObjectEntry> {
        self.inner.objects.read().get(handle.index() as usize).cloned()
    }

    /// Replaces the object and generation at `handle`.
    pub fn set_object_entry(&self, handle: Handle<Object>, entry: ObjectEntry) {
        let mut objects = self.inner.objects.write();
        if let Some(e) = objects.get_mut(handle.index() as usize) {
            let old_val = e.object.clone();
            let new_val = entry.object.clone();
            *e = entry;

            let mut idx = self.inner.object_index.write();
            // Remove from old entry list
            if let Some(list) = idx.get_mut(&old_val) {
                if let Some(pos) = list.iter().position(|&x| x == handle) {
                    list.remove(pos);
                }
                if list.is_empty() {
                    idx.remove(&old_val);
                }
            }
            // Add to new entry list
            idx.entry(new_val).or_default().push(handle);
        }
    }

    /// Retrieves a dictionary.
    pub fn get_dict(
        &self,
        handle: Handle<BTreeMap<Handle<PdfName>, Object>>,
    ) -> Option<BTreeMap<Handle<PdfName>, Object>> {
        self.inner.dicts.read().get(handle.index() as usize).cloned()
    }

    /// Updates an existing dictionary.
    pub fn set_dict(
        &self,
        handle: Handle<BTreeMap<Handle<PdfName>, Object>>,
        dict: BTreeMap<Handle<PdfName>, Object>,
    ) {
        if let Some(d) = self.inner.dicts.write().get_mut(handle.index() as usize) {
            *d = dict;
        }
    }

    /// Retrieves an array.
    pub fn get_array(&self, handle: Handle<Vec<Object>>) -> Option<Vec<Object>> {
        self.inner.arrays.read().get(handle.index() as usize).cloned()
    }

    /// Updates an existing array in place, keeping its handle valid.
    pub fn set_array(&self, handle: Handle<Vec<Object>>, array: Vec<Object>) {
        if let Some(a) = self.inner.arrays.write().get_mut(handle.index() as usize) {
            *a = array;
        }
    }

    /// Searches for an existing indirect object that matches the provided object.
    pub fn find_object(&self, object: &Object) -> Option<Handle<Object>> {
        let idx = self.inner.object_index.read();
        idx.get(object).and_then(|list| list.first().copied())
    }

    /// Number of object slots allocated.
    pub fn object_count(&self) -> u32 {
        self.inner.objects.read().len() as u32
    }

    /// Applies filters to data using the stream dictionary context.
    pub fn process_filters(
        &self,
        data: &[u8],
        dict: &BTreeMap<Handle<PdfName>, Object>,
    ) -> PdfResult<Bytes> {
        crate::filters::process_arena_filters(data, dict, self)
    }

    /// Performs sublimation (normalization) on all objects in the arena.
    pub fn sublimate_all(&self) -> PdfResult<()> {
        for i in 0..self.object_count() {
            let handle = Handle::new(i);
            if let Some(Object::Stream(dh, data_arc)) = self.get_object(handle) {
                // If it's already structured, skip
                if matches!(
                    &*data_arc,
                    SublimatedData::Commands { .. } | SublimatedData::Image { .. }
                ) {
                    continue;
                }

                // Determine if it's a content stream
                let is_content =
                    Object::Stream(dh, data_arc.clone()).is_likely_content_stream(self);

                let is_image = if let Some(dict) = self.get_dict(dh) {
                    let subtype = dict
                        .get(&self.name("Subtype"))
                        .and_then(|o| o.resolve(self).as_name())
                        .and_then(|n| self.get_name(n));
                    subtype.as_ref().map(|n| n.as_str() == "Image").unwrap_or(false)
                } else {
                    false
                };

                let is_font = if let Some(dict) = self.get_dict(dh) {
                    dict.contains_key(&self.name("Length1"))
                        || dict.contains_key(&self.name("Length2"))
                        || dict.contains_key(&self.name("Length3"))
                } else {
                    false
                };

                if (is_image || is_font) && matches!(&*data_arc, SublimatedData::Raw(_)) {
                    // Preservation: Already Raw (encoded), skip re-processing
                    continue;
                }

                // Get raw bytes (decompressing if it was Compressed)
                let raw_bytes = self.get_stream_bytes(&data_arc)?;
                self.sublimate_stream(handle, raw_bytes, is_content)?;
            }
        }
        Ok(())
    }

    /// Sublimates raw stream data into a compressed or structured format.
    pub fn sublimate_stream(
        &self,
        handle: Handle<Object>,
        data: Bytes,
        is_content_stream: bool,
    ) -> PdfResult<()> {
        let sublimated = if is_content_stream {
            // Keep raw bytes for subsequent parser extraction in SDK pipeline
            SublimatedData::Raw(data)
        } else if let Some(Object::Stream(dh, _)) = self.get_object(handle)
            && let Some(dict) = self.get_dict(dh)
            && {
                let subtype = dict
                    .get(&self.name("Subtype"))
                    .and_then(|o: &Object| o.resolve(self).as_name())
                    .and_then(|n: Handle<PdfName>| self.get_name(n));
                let is_image = subtype.as_ref().map(|n| n.as_str() == "Image").unwrap_or(false);
                let is_font = dict.contains_key(&self.name("Length1"))
                    || dict.contains_key(&self.name("Length2"))
                    || dict.contains_key(&self.name("Length3"));
                is_image || is_font
            }
        {
            // High-Fidelity Preservation (Phase 2 & 3):
            // Do NOT decode images or fonts to raw pixels/data during ingestion.
            // Preserve the original encoded bytes (Solid state) for lossless serialization.
            // On-demand sublimation (Gas state) occurs during interpretation/rendering.
            SublimatedData::Raw(data)
        } else if data.len() > 1024 {
            // Heuristic: If it already looks like Zstd, don't double compress
            if data.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
                SublimatedData::Compressed {
                    original_len: 0, // Unknown if we don't parse it
                    data: data.to_vec(),
                }
            } else {
                // Compress large non-content streams (images, fonts) in memory.
                let compressed = crate::filters::flate::deflate(&data)
                    .map_err(|e| crate::PdfError::Other(e.to_string().into()))?;
                SublimatedData::Compressed { original_len: data.len(), data: compressed }
            }
        } else {
            SublimatedData::Raw(data)
        };

        if let Some(Object::Stream(dh, _)) = self.get_object(handle) {
            self.set_object(handle, Object::Stream(dh, std::sync::Arc::new(sublimated)));
        }
        Ok(())
    }

    /// Accesses the raw bytes of a stream, transparently decompressing if necessary.
    pub fn get_stream_bytes(
        &self,
        data: &crate::object::SublimatedData,
    ) -> PdfResult<bytes::Bytes> {
        match data {
            crate::object::SublimatedData::Raw(b) => Ok(b.clone()),
            crate::object::SublimatedData::Compressed { data, .. } => {
                let decoded = crate::filters::flate::inflate(data)
                    .map_err(|e| crate::PdfError::Other(e.to_string().into()))?;
                Ok(bytes::Bytes::from(decoded))
            }
            crate::object::SublimatedData::Commands { items: cmds } => Ok(bytes::Bytes::from(
                crate::object::sublimation::serializer::serialize_commands(cmds),
            )),
            crate::object::SublimatedData::Image { data, .. } => {
                Ok(bytes::Bytes::from(data.clone()))
            }
        }
    }

    /// Reads a stream's sublimated payload, if it has one.
    pub fn get_sublimated_data(
        &self,
        handle: Handle<Object>,
    ) -> Option<std::sync::Arc<crate::object::SublimatedData>> {
        if let Some(Object::Stream(_, data)) = self.get_object(handle) { Some(data) } else { None }
    }

    /// Finds an indirect object handle that points to the given dictionary handle.
    pub fn find_object_by_dict_handle(
        &self,
        dh: Handle<BTreeMap<Handle<PdfName>, Object>>,
    ) -> Option<Handle<Object>> {
        let objects = self.inner.objects.read();
        for (i, entry) in objects.iter().enumerate() {
            if let Object::Dictionary(h) = entry.object
                && h == dh
            {
                return Some(Handle::new(i as u32));
            }
        }
        None
    }

    /// Returns high-level statistics about the arena's memory usage and object counts.
    pub fn get_stats(&self) -> ArenaStats {
        ArenaStats {
            object_count: self.inner.objects.read().len() as u32,
            dictionary_count: self.inner.dicts.read().len() as u32,
            array_count: self.inner.arrays.read().len() as u32,
            name_count: self.inner.names.read().len() as u32,
            version: self.version(),
        }
    }
}

/// High-level statistics about a PdfArena instance.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArenaStats {
    /// Objects held.
    pub object_count: u32,
    /// Dictionaries held.
    pub dictionary_count: u32,
    /// Arrays held.
    pub array_count: u32,
    /// Interned names held.
    pub name_count: u32,
    /// Declared document version.
    pub version: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_alloc_and_retrieve_name() {
        let arena = PdfArena::new();
        let name_handle = arena.name("Catalog");
        let name_obj = arena.get_name(name_handle).unwrap();
        assert_eq!(name_obj.as_str(), "Catalog");
    }

    #[test]
    fn test_arena_alloc_dict_and_object() {
        let arena = PdfArena::new();
        let mut dict = BTreeMap::new();
        dict.insert(arena.name("Type"), Object::Name(arena.name("Page")));

        let dict_handle = arena.alloc_dict(dict);
        let obj_handle = arena.alloc_object(Object::Dictionary(dict_handle));

        if let Some(Object::Dictionary(dh)) = arena.get_object(obj_handle) {
            let fetched_dict = arena.get_dict(dh).unwrap();
            assert!(fetched_dict.contains_key(&arena.name("Type")));
        } else {
            panic!("Expected dictionary object in arena");
        }
    }
}
