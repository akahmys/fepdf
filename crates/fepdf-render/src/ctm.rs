use fepdf_model::Matrix;
use kurbo::Affine;

/// Internal state of the graphics stack level.
#[derive(Debug, Clone)]
struct CtmState {
    ctm: Affine,
    clips_at_this_level: usize,
}

/// Manages the Current Transformation Matrix (CTM) stack and clipping depth.
///
/// Follows ISO 32000-2:2020 Clause 8.3 and 8.4 requirements for graphics state
/// stack management.
#[derive(Debug, Clone)]
pub struct CtmStack {
    stack: Vec<CtmState>,
    page_height: f64,
}

impl CtmStack {
    /// Creates a new CTM stack with an initial identity matrix.
    pub fn new(page_height: f64) -> Self {
        Self {
            stack: vec![CtmState { ctm: Affine::IDENTITY, clips_at_this_level: 0 }],
            page_height,
        }
    }

    /// Pushes the current state onto the stack (corresponds to `q` operator).
    pub fn push(&mut self) {
        let current = self
            .stack
            .last()
            .cloned()
            .unwrap_or(CtmState { ctm: Affine::IDENTITY, clips_at_this_level: 0 });
        // Reset clips_at_this_level for the NEW level.
        // PDF spec: Q restores the previous state, which includes the previous clip.
        // Vello: We only need to know how many layers were pushed *since* the last q.
        self.stack.push(CtmState { ctm: current.ctm, clips_at_this_level: 0 });
    }

    /// Pops the state and returns the number of clips that need to be popped from Vello.
    pub fn pop(&mut self) -> Option<usize> {
        if self.stack.len() > 1 { self.stack.pop().map(|s| s.clips_at_this_level) } else { None }
    }

    /// Increments the clip depth for the current stack level.
    pub fn add_clip(&mut self) {
        if let Some(state) = self.stack.last_mut() {
            state.clips_at_this_level += 1;
        }
    }

    /// Concatenates a matrix to the current CTM (corresponds to `cm` operator).
    pub fn concat(&mut self, matrix: &Matrix) {
        if let Some(state) = self.stack.last_mut() {
            state.ctm = matrix.as_affine() * state.ctm;
        }
    }

    /// Returns the current CTM in PDF space.
    pub fn current(&self) -> Affine {
        self.stack.last().map(|s| s.ctm).unwrap_or(Affine::IDENTITY)
    }

    /// Returns the CTM mapped to the target coordinate system (top-left origin).
    pub fn current_for_display(&self) -> Affine {
        let flip = Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, self.page_height]);
        self.current() * flip
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ctm_flip() {
        let ctm = CtmStack::new(100.0);
        // Current is identity
        let p_pdf = kurbo::Point::new(10.0, 10.0); // 10 up from bottom
        let p_disp = ctm.current_for_display() * p_pdf;

        assert_eq!(p_disp.x, 10.0);
        assert_eq!(p_disp.y, 90.0); // 90 down from top
    }
}
