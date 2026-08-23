//! Soft masks, and the bracket that lets one be applied (ISO 32000-2, 11.6.5.2).
//!
//! # Why this is a scope and not an operator pair
//!
//! `BDC`/`EMC` and `q`/`Q` come in pairs, so an interpreter can match them. A soft mask
//! does not: `gs` sets one, and it lasts until the graphics state that set it is
//! restored, or until another `gs` sets `/SMask /None`. So the bracket has to be tracked
//! against the `q` depth, which is what [`MaskScope::depth`] is for.
//!
//! # Why the group is replayed at the end rather than the beginning
//!
//! A mask modifies content that has already been drawn — there is no way to apply one to
//! marks that have not been made yet without holding them somewhere first. So the order
//! the backend sees is content, then mask:
//!
//! ```text
//! begin_masked_content()      gs set an /SMask
//!   ...the page's drawing...
//!   begin_soft_mask(spec)     the scope ended: Q, or /SMask /None
//!     ...the group replayed...
//!   end_soft_mask()
//! ```
//!
//! This is also the order Vello's `push_luminance_mask_layer` takes, which is not a
//! coincidence: it is the order the operation itself has.
//!
//! **The group is replayed in the matrix `gs` was executed under.** 11.6.5.2 puts the
//! mask in the coordinate system current when the mask was set, and by the time the scope
//! closes the CTM has usually moved — often several times.
//!
//! # What is not done here
//!
//! Nothing about `/BC` or `/TR` or `/S /Alpha` is interpreted away. All three reach the
//! backend inside [`SoftMaskSpec`], because which of them a backend can honour is a
//! question about that backend and not about the document. What this module guarantees is
//! that the backend is *told*.

use crate::RenderBackend;
use crate::interpreter::{Interpreter, MaskScope};
use fepdf_model::graphics::{Color, SoftMaskKind, SoftMaskSpec};
use fepdf_model::{Handle, Object, PdfName, PdfResult};
use std::collections::BTreeMap;
use std::sync::Arc;

impl Interpreter<'_> {
    /// Reads an `/SMask` entry from an `/ExtGState` and opens or closes a bracket.
    ///
    /// `/None` closes the innermost scope; a dictionary opens one. Any other value is the
    /// document's error and is left alone — Table 58 admits exactly the two.
    pub(crate) fn handle_soft_mask_entry(&mut self, entry: &Object) -> PdfResult<()> {
        let resolved = entry.resolve(self.doc.arena());
        match resolved {
            Object::Name(n) => {
                if self.doc.arena().get_name(n).is_some_and(|nn| nn.as_str() == "None") {
                    self.state.smask = None;
                    self.close_mask_scope_at_current_depth()?;
                }
            }
            Object::Dictionary(h) => {
                let Some(dict) = self.doc.arena().get_dict(h) else { return Ok(()) };
                let Some((spec, group)) = self.read_soft_mask(&dict) else { return Ok(()) };
                self.state.smask = Some(resolved);
                self.backend.begin_masked_content();
                self.mask_scopes.push(MaskScope {
                    depth: self.state_stack.len(),
                    spec,
                    group,
                    ctm: self.state.ctm,
                });
            }
            _ => {}
        }
        Ok(())
    }

    /// The mask's definition, and the group that draws it (Table 145).
    ///
    /// `None` when there is no `/G`: a soft-mask dictionary without one defines no mask,
    /// and opening a bracket that can never be filled would leave the content inside it
    /// masked by nothing at all.
    fn read_soft_mask(
        &self,
        dict: &BTreeMap<Handle<PdfName>, Object>,
    ) -> Option<(SoftMaskSpec, Object)> {
        let arena = self.doc.arena();
        let group = dict.get(&arena.intern_name(PdfName::new("G")))?.resolve(arena);
        if !matches!(group, Object::Stream(..)) {
            return None;
        }
        let kind = match dict.get(&arena.intern_name(PdfName::new("S"))).map(|o| o.resolve(arena)) {
            Some(Object::Name(n)) if arena.get_name(n).is_some_and(|s| s.as_str() == "Alpha") => {
                SoftMaskKind::Alpha
            }
            _ => SoftMaskKind::Luminosity,
        };
        let backdrop = dict
            .get(&arena.intern_name(PdfName::new("BC")))
            .and_then(|o| self.backdrop_colour(&o.resolve(arena)));
        let transfer = dict
            .get(&arena.intern_name(PdfName::new("TR")))
            .and_then(|o| self.transfer_function(&o.resolve(arena)));
        Some((SoftMaskSpec { kind, backdrop, transfer }, group))
    }

    /// `/BC`, whose components are in the group's own colour space (Table 145).
    ///
    /// The space is read from the group rather than assumed: a one-component array in a
    /// `/DeviceGray` group and in a `/Separation` group mean different colours, and the
    /// component count is the only thing available here that tells them apart.
    fn backdrop_colour(&self, entry: &Object) -> Option<Color> {
        let arena = self.doc.arena();
        let Object::Array(handle) = entry else { return None };
        let values: Vec<f64> = arena
            .get_array(*handle)?
            .iter()
            .filter_map(|component| component.resolve(arena).as_f64())
            .collect();
        match values.as_slice() {
            [grey] => Some(Color::Gray(*grey)),
            [red, green, blue] => Some(Color::Rgb(*red, *green, *blue)),
            [cyan, magenta, yellow, black] => Some(Color::Cmyk(*cyan, *magenta, *yellow, *black)),
            _ => None,
        }
    }

    /// `/TR`, which is a function or the name `/Identity` (Table 145).
    ///
    /// `/Identity` produces `None`, which is the same thing said in the type: a transfer
    /// function that changes nothing is the absence of one, and a backend should not have
    /// to evaluate a function to discover that.
    fn transfer_function(&self, entry: &Object) -> Option<Arc<fepdf_model::function::PdfFunction>> {
        if let Object::Name(n) = entry
            && self.doc.arena().get_name(*n).is_some_and(|s| s.as_str() == "Identity")
        {
            return None;
        }
        fepdf_model::function::PdfFunction::parse(entry, self.doc.arena()).map(Arc::new)
    }

    /// Closes every scope that the graphics state just restored past.
    ///
    /// Called from `Q` after the state has been popped, so `state_stack.len()` is already
    /// the depth being returned to. A single `Q` can close more than one: two `gs`
    /// operators at the same depth each open a bracket.
    pub(crate) fn close_mask_scopes_above(&mut self, depth: usize) -> PdfResult<()> {
        while self.mask_scopes.last().is_some_and(|s| s.depth > depth) {
            let scope = self.mask_scopes.pop().unwrap_or_else(|| unreachable!());
            self.apply_mask_scope(scope)?;
        }
        Ok(())
    }

    /// Closes the innermost scope opened at the depth currently in force.
    ///
    /// This is `/SMask /None`, which ends a mask without restoring anything else.
    fn close_mask_scope_at_current_depth(&mut self) -> PdfResult<()> {
        let depth = self.state_stack.len();
        if self.mask_scopes.last().is_some_and(|s| s.depth == depth) {
            let scope = self.mask_scopes.pop().unwrap_or_else(|| unreachable!());
            self.apply_mask_scope(scope)?;
        }
        Ok(())
    }

    /// Replays one scope's group and closes its bracket.
    fn apply_mask_scope(&mut self, scope: MaskScope) -> PdfResult<()> {
        self.backend.begin_soft_mask(&scope.spec);
        let restore = self.state.ctm;
        self.state.ctm = scope.ctm;
        self.update_backend_transform();
        let replayed = self.replay_mask_group(&scope.group);
        self.state.ctm = restore;
        self.update_backend_transform();
        self.backend.end_soft_mask();
        replayed
    }

    /// Runs the `/G` form XObject that defines the mask.
    ///
    /// The same two paths `Do` takes for a form, because a mask group *is* a form XObject
    /// — 11.6.5.2 requires it to carry a `/Group` of subtype `/Transparency`, which is the
    /// only thing that distinguishes it from any other.
    fn replay_mask_group(&mut self, group: &Object) -> PdfResult<()> {
        let Object::Stream(dh, sd) = group else { return Ok(()) };
        let Some(dict) = self.doc.arena().get_dict(*dh) else { return Ok(()) };
        match sd.as_ref() {
            fepdf_model::object::SublimatedData::Commands { items, .. } => {
                let commands = items.clone();
                self.execute_form_commands(&dict, &commands)
            }
            fepdf_model::object::SublimatedData::Image { .. }
            | fepdf_model::object::SublimatedData::Compressed { .. }
            | fepdf_model::object::SublimatedData::Raw(_) => {
                let bytes = self.doc.arena().get_stream_bytes(sd)?;
                self.render_form_xobject(&dict, &bytes)
            }
        }
    }
}
