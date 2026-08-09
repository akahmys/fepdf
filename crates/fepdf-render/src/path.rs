use kurbo::{BezPath, Point};

/// Helper to build Kurbo paths from PDF path operations.
#[derive(Debug, Clone)]
pub struct PathBuilder {
    path: BezPath,
    current_point: Option<Point>,
}

impl Default for PathBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PathBuilder {
    pub fn new() -> Self {
        Self { path: BezPath::new(), current_point: None }
    }

    pub fn move_to(&mut self, x: f64, y: f64) {
        let p = Point::new(x, y);
        self.path.move_to(p);
        self.current_point = Some(p);
    }

    fn ensure_current_point(&mut self) {
        if self.current_point.is_none() {
            self.move_to(0.0, 0.0);
        }
    }

    pub fn line_to(&mut self, x: f64, y: f64) {
        self.ensure_current_point();
        let p = Point::new(x, y);
        self.path.line_to(p);
        self.current_point = Some(p);
    }

    pub fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        self.ensure_current_point();
        let p3 = Point::new(x3, y3);
        self.path.curve_to(Point::new(x1, y1), Point::new(x2, y2), p3);
        self.current_point = Some(p3);
    }

    /// PDF 'v' operator: Append curve using current point as first control point.
    pub fn curve_v(&mut self, x2: f64, y2: f64, x3: f64, y3: f64) {
        self.ensure_current_point();
        let p0 = self.current_point.unwrap_or(Point::ORIGIN);
        let p3 = Point::new(x3, y3);
        self.path.curve_to(p0, Point::new(x2, y2), p3);
        self.current_point = Some(p3);
    }

    /// PDF 'y' operator: Append curve using final point as second control point.
    pub fn curve_y(&mut self, x1: f64, y1: f64, x3: f64, y3: f64) {
        self.ensure_current_point();
        let p3 = Point::new(x3, y3);
        self.path.curve_to(Point::new(x1, y1), p3, p3);
        self.current_point = Some(p3);
    }

    pub fn close_path(&mut self) {
        self.path.close_path();
        // current_point remains at the point where close_path was called?
        // Actually, PDF spec says it's the start of the subpath.
        // But for common usage, tracking it after close is tricky.
    }

    pub fn rectangle(&mut self, x: f64, y: f64, w: f64, h: f64) {
        // PDF 're' operator: x y w h
        self.move_to(x, y);
        self.line_to(x + w, y);
        self.line_to(x + w, y + h);
        self.line_to(x, y + h);
        self.close_path();
    }

    pub fn finish(self) -> BezPath {
        self.path
    }
}
