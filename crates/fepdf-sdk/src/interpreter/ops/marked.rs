use crate::interpreter::Interpreter;
use fepdf_core::PdfResult;

impl Interpreter<'_> {
    pub(crate) fn handle_marked_content_operator(&mut self, op: &str) -> PdfResult<()> {
        match op {
            "BMC" | "MP" => {
                let _tag = self.pop_name()?;
                // Skeleton: just pop for now
            }
            "BDC" | "DP" => {
                let _props = self.stack.pop();
                let _tag = self.pop_name()?;
                // Skeleton: just pop for now
            }
            "EMC" => {
                // End of marked content
            }
            _ => {}
        }
        Ok(())
    }
}
