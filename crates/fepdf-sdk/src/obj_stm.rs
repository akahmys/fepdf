use fepdf_core::PdfResult;
use std::io::Write;

/// A packer for creating PDF Object Streams (Type /ObjStm).
/// This allows multiple Section 6 objects to be packed into a single stream.
pub struct ObjectStreamPacker {
    /// Mappings of object IDs to their offsets within the uncompressed stream data.
    pub indices: Vec<(u32, usize)>,
    /// The uncompressed data payload of the packed objects.
    pub data: Vec<u8>,
}

impl Default for ObjectStreamPacker {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectStreamPacker {
    /// Creates a new packer.
    pub fn new() -> Self {
        Self { indices: Vec::new(), data: Vec::new() }
    }

    /// Adds an object to the stream.
    /// The object must be serialized using the provided writer's logic but into our internal buffer.
    pub fn add_object<F>(&mut self, id: u32, write_fn: F) -> PdfResult<()>
    where
        F: FnOnce(&mut Vec<u8>) -> PdfResult<()>,
    {
        let offset = self.data.len();
        self.indices.push((id, offset));
        write_fn(&mut self.data)?;
        self.data.push(b' '); // Separator
        Ok(())
    }

    /// Returns the number of objects in the stream.
    pub fn count(&self) -> usize {
        self.indices.len()
    }

    /// Finalizes the stream data, returning the dictionary entries (/N and /First) and the full stream bytes.
    pub fn finish(self) -> (usize, usize, Vec<u8>) {
        let mut full_data = Vec::new();
        let n = self.indices.len();

        // Write index: <obj_id> <offset>
        for (id, offset) in &self.indices {
            let _ = write!(full_data, "{id} {offset} ");
        }

        let first_offset = full_data.len();
        full_data.extend_from_slice(&self.data);

        (n, first_offset, full_data)
    }

    /// Returns the mapping of object IDs to their index within this stream.
    pub fn get_mappings(&self) -> Vec<(u32, usize)> {
        self.indices.iter().enumerate().map(|(idx, (id, _))| (*id, idx)).collect()
    }
}
