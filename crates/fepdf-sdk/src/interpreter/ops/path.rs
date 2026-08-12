use crate::interpreter::Interpreter;
use fepdf_model::graphics::WindingRule;
use fepdf_model::{PdfError, PdfResult};

impl Interpreter<'_> {
    pub(crate) fn handle_path_operator(&mut self, op: &str) -> PdfResult<()> {
        match op {
            "m" => {
                let y = self.pop_f64()?;
                let x = self.pop_f64()?;
                self.path.move_to(x, y);
            }
            "l" => {
                let y = self.pop_f64()?;
                let x = self.pop_f64()?;
                self.path.line_to(x, y);
            }
            "c" => {
                let y3 = self.pop_f64()?;
                let x3 = self.pop_f64()?;
                let y2 = self.pop_f64()?;
                let x2 = self.pop_f64()?;
                let y1 = self.pop_f64()?;
                let x1 = self.pop_f64()?;
                self.path.curve_to(x1, y1, x2, y2, x3, y3);
            }
            "v" => {
                let y3 = self.pop_f64()?;
                let x3 = self.pop_f64()?;
                let y2 = self.pop_f64()?;
                let x2 = self.pop_f64()?;
                self.path.curve_v(x2, y2, x3, y3);
            }
            "y" => {
                let y3 = self.pop_f64()?;
                let x3 = self.pop_f64()?;
                let y1 = self.pop_f64()?;
                let x1 = self.pop_f64()?;
                self.path.curve_y(x1, y1, x3, y3);
            }
            "re" => {
                let h = self.pop_f64()?;
                let w = self.pop_f64()?;
                let y = self.pop_f64()?;
                let x = self.pop_f64()?;
                self.path.rectangle(x, y, w, h);
            }
            "h" => self.path.close_path(),
            "W" => self.pending_clip = Some(WindingRule::NonZero),
            "W*" => self.pending_clip = Some(WindingRule::EvenOdd),
            _ => return Err(PdfError::Other(format!("Invalid path op: {op}").into())),
        }
        Ok(())
    }
}
