use serde::{Deserialize, Serialize};

/// An axis-aligned bounding box.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn center_x(&self) -> f32 {
        self.x + self.width / 2.0
    }

    pub fn center_y(&self) -> f32 {
        self.y + self.height / 2.0
    }

    /// IoU (Intersection over Union) with another rect.
    pub fn iou(&self, other: &Rect) -> f32 {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = self.right().min(other.right());
        let y2 = self.bottom().min(other.bottom());

        let intersection = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
        let area_self = self.width * self.height;
        let area_other = other.width * other.height;
        let union = area_self + area_other - intersection;

        if union <= 0.0 {
            0.0
        } else {
            intersection / union
        }
    }

    /// Check if this rect contains a point.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.right() && py >= self.y && py <= self.bottom()
    }

    /// Check if this rect overlaps with another rect.
    pub fn overlaps(&self, other: &Rect) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }
}

/// A 2D point.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A quadrilateral defined by four ordered points in clockwise or counter-clockwise order.
/// Convention: top-left, top-right, bottom-right, bottom-left.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Quad {
    pub p1: Point,
    pub p2: Point,
    pub p3: Point,
    pub p4: Point,
}

impl Quad {
    pub fn new(p1: Point, p2: Point, p3: Point, p4: Point) -> Self {
        Self { p1, p2, p3, p4 }
    }

    /// Compute the axis-aligned bounding rectangle.
    pub fn bounding_rect(&self) -> Rect {
        let xs = [self.p1.x, self.p2.x, self.p3.x, self.p4.x];
        let ys = [self.p1.y, self.p2.y, self.p3.y, self.p4.y];
        let min_x = xs.iter().cloned().reduce(f32::min).unwrap();
        let max_x = xs.iter().cloned().reduce(f32::max).unwrap();
        let min_y = ys.iter().cloned().reduce(f32::min).unwrap();
        let max_y = ys.iter().cloned().reduce(f32::max).unwrap();
        Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }

    /// Compute the polygon area using the shoelace formula.
    pub fn area(&self) -> f32 {
        let n = 4;
        let pts = [self.p1, self.p2, self.p3, self.p4];
        let mut area = 0.0;
        for i in 0..n {
            let j = (i + 1) % n;
            area += pts[i].x * pts[j].y;
            area -= pts[j].x * pts[i].y;
        }
        area.abs() / 2.0
    }

    /// Compute the centroid of the quadrilateral.
    pub fn center(&self) -> Point {
        Point::new(
            (self.p1.x + self.p2.x + self.p3.x + self.p4.x) / 4.0,
            (self.p1.y + self.p2.y + self.p3.y + self.p4.y) / 4.0,
        )
    }

    /// Scale all points by (sx, sy) relative to origin.
    pub fn scale(&self, sx: f32, sy: f32) -> Self {
        Self {
            p1: Point::new(self.p1.x * sx, self.p1.y * sy),
            p2: Point::new(self.p2.x * sx, self.p2.y * sy),
            p3: Point::new(self.p3.x * sx, self.p3.y * sy),
            p4: Point::new(self.p4.x * sx, self.p4.y * sy),
        }
    }

    /// Translate all points by (dx, dy).
    pub fn translate(&self, dx: f32, dy: f32) -> Self {
        Self {
            p1: Point::new(self.p1.x + dx, self.p1.y + dy),
            p2: Point::new(self.p2.x + dx, self.p2.y + dy),
            p3: Point::new(self.p3.x + dx, self.p3.y + dy),
            p4: Point::new(self.p4.x + dx, self.p4.y + dy),
        }
    }

    /// Clip quad to be within bounds (0, 0, max_x, max_y).
    pub fn clamp(&self, max_x: f32, max_y: f32) -> Self {
        let clamp_p = |p: &Point| Point::new(p.x.clamp(0.0, max_x), p.y.clamp(0.0, max_y));
        Self {
            p1: clamp_p(&self.p1),
            p2: clamp_p(&self.p2),
            p3: clamp_p(&self.p3),
            p4: clamp_p(&self.p4),
        }
    }

    /// Convert to a vec of (i32, i32) pairs for contour/polygon operations.
    pub fn to_i32_pairs(&self) -> Vec<(i32, i32)> {
        vec![
            (self.p1.x as i32, self.p1.y as i32),
            (self.p2.x as i32, self.p2.y as i32),
            (self.p3.x as i32, self.p3.y as i32),
            (self.p4.x as i32, self.p4.y as i32),
        ]
    }

    /// Estimate target (width, height) for perspective warp based on longest edge.
    pub fn warp_target_size(&self) -> (u32, u32) {
        let top_w = distance(self.p1, self.p2);
        let bottom_w = distance(self.p4, self.p3);
        let left_h = distance(self.p1, self.p4);
        let right_h = distance(self.p2, self.p3);
        let w = top_w.max(bottom_w).ceil() as u32;
        let h = left_h.max(right_h).ceil() as u32;
        (w.max(1), h.max(1))
    }

    /// Sort points into [top-left, top-right, bottom-right, bottom-left] order.
    pub fn sorted(&self) -> Self {
        let pts = [self.p1, self.p2, self.p3, self.p4];
        // Sort by y then x using centroid to determine top/bottom
        let cy = pts.iter().map(|p| p.y).sum::<f32>() / 4.0;
        let mut top: Vec<Point> = pts.iter().filter(|p| p.y <= cy).copied().collect();
        let mut bottom: Vec<Point> = pts.iter().filter(|p| p.y > cy).copied().collect();
        top.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        bottom.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        // Pad if splitting failed (e.g. all y equal)
        while top.len() < 2 {
            top.push(Point::new(0.0, 0.0));
        }
        while bottom.len() < 2 {
            bottom.push(Point::new(0.0, 0.0));
        }
        Self {
            p1: top[0],
            p2: top[1],
            p3: bottom[1],
            p4: bottom[0],
        }
    }
}

fn distance(a: Point, b: Point) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

/// A 2D size.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}
