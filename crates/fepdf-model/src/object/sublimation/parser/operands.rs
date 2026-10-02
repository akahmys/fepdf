//! The operands an operator takes off the stack, read as the type it needs: a number, a
//! point, a rectangle, a matrix, a colour, a name or an array of numbers.

use super::super::IrObject;
use super::Sublimator;
use crate::graphics::Color;
use crate::object::PdfName;
use kurbo::{Affine, Point, Rect};

impl<'a> Sublimator<'a> {
    pub(super) fn pop_f64(&mut self) -> Option<f64> {
        match self.stack.pop() {
            Some(IrObject::Real(f)) => Some(f),
            Some(IrObject::Integer(i)) => Some(i as f64),
            _ => None,
        }
    }

    pub(super) fn pop_i64(&mut self) -> Option<i64> {
        match self.stack.pop() {
            Some(IrObject::Integer(i)) => Some(i),
            Some(IrObject::Real(f)) => Some(f as i64),
            _ => None,
        }
    }

    pub(super) fn pop_point(&mut self) -> Option<Point> {
        let y = self.pop_f64()?;
        let x = self.pop_f64()?;
        Some(Point::new(x, y))
    }

    pub(super) fn pop_three_points(&mut self) -> Option<(Point, Point, Point)> {
        let p3 = self.pop_point()?;
        let p2 = self.pop_point()?;
        let p1 = self.pop_point()?;
        Some((p1, p2, p3))
    }

    pub(super) fn pop_rect(&mut self) -> Option<Rect> {
        let h = self.pop_f64()?;
        let w = self.pop_f64()?;
        let y = self.pop_f64()?;
        let x = self.pop_f64()?;
        Some(Rect::from_origin_size(Point::new(x, y), kurbo::Size::new(w, h)))
    }

    pub(super) fn pop_affine(&mut self) -> Option<Affine> {
        let f = self.pop_f64()?;
        let e = self.pop_f64()?;
        let d = self.pop_f64()?;
        let c = self.pop_f64()?;
        let b = self.pop_f64()?;
        let a = self.pop_f64()?;
        Some(Affine::new([a, b, c, d, e, f]))
    }

    pub(super) fn pop_rgb(&mut self) -> Option<Color> {
        let b = self.pop_f64()?;
        let g = self.pop_f64()?;
        let r = self.pop_f64()?;
        Some(Color::Rgb(r, g, b))
    }

    pub(super) fn pop_cmyk(&mut self) -> Option<Color> {
        let k = self.pop_f64()?;
        let y = self.pop_f64()?;
        let m = self.pop_f64()?;
        let c = self.pop_f64()?;
        Some(Color::Cmyk(c, m, y, k))
    }

    pub(super) fn pop_name(&mut self) -> Option<PdfName> {
        match self.stack.pop() {
            Some(IrObject::Name(s)) => Some(PdfName::new(&s)),
            _ => None,
        }
    }

    pub(super) fn pop_f64_array(&mut self) -> Option<Vec<f64>> {
        match self.stack.pop()? {
            IrObject::Array(arr) => {
                let mut vals = Vec::new();
                for item in arr {
                    if let Some(f) = item.as_f64() {
                        vals.push(f);
                    }
                }
                Some(vals)
            }
            _ => None,
        }
    }
}
