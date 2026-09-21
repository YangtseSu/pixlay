//! Normalized geometry primitives: points, rectangles and polygons.
//!
//! Every coordinate in `pixlay-core` lives in `[0, 1]` canvas space, with
//! `(0, 0)` at the top-left corner and `(1, 1)` at the bottom-right. Absolute
//! pixels exist only at the render and export boundary.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Tolerance for "same point / on the boundary / collinear" comparisons in
/// normalized units. 1e-9 is ~1.4e-4 px on an A0 sheet at 300 dpi: far below
/// anything the product can express, far above float noise from the same
/// arithmetic.
pub const EPSILON: f64 = 1e-9;

/// A point in normalized canvas space.
///
/// Serialized as a two-element array (`[0.04, 0.08]`): the format is read by
/// humans when reviewing a `.pixlay`, and coordinate pairs are the conventional
/// shape for it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const ORIGIN: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// True when both coordinates are finite: no NaN, no infinity.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    /// Distance from this point to the segment `a`–`b`.
    pub fn distance_to_segment(self, a: Self, b: Self) -> f64 {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len2 = dx * dx + dy * dy;
        if len2 <= 0.0 {
            return (self.x - a.x).hypot(self.y - a.y);
        }
        let t = (((self.x - a.x) * dx + (self.y - a.y) * dy) / len2).clamp(0.0, 1.0);
        (self.x - (a.x + t * dx)).hypot(self.y - (a.y + t * dy))
    }
}

impl Serialize for Point {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (self.x, self.y).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Point {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (x, y) = <(f64, f64)>::deserialize(deserializer)?;
        Ok(Self { x, y })
    }
}

/// An axis-aligned rectangle in normalized canvas space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Rect {
    /// Bounding box of `points`, or `None` when there are none.
    pub fn from_points(points: &[Point]) -> Option<Self> {
        let first = *points.first()?;
        let mut rect = Self {
            x0: first.x,
            y0: first.y,
            x1: first.x,
            y1: first.y,
        };
        for p in &points[1..] {
            rect.x0 = rect.x0.min(p.x);
            rect.y0 = rect.y0.min(p.y);
            rect.x1 = rect.x1.max(p.x);
            rect.y1 = rect.y1.max(p.y);
        }
        Some(rect)
    }

    pub fn width(self) -> f64 {
        self.x1 - self.x0
    }

    pub fn height(self) -> f64 {
        self.y1 - self.y0
    }

    pub fn center(self) -> Point {
        Point::new((self.x0 + self.x1) / 2.0, (self.y0 + self.y1) / 2.0)
    }
}

/// A closed polygon in normalized canvas space.
///
/// The last vertex connects back to the first. The winding direction is free:
/// the renderer fills with the default winding rule and the probes use an
/// even-odd containment test, so neither depends on it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Polygon {
    pub points: Vec<Point>,
}

impl Polygon {
    /// A polygon needs a real interior, so at least three vertices.
    pub const MIN_VERTICES: usize = 3;

    /// A rectangle in normalized coordinates.
    pub fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self {
            points: vec![
                Point::new(x0, y0),
                Point::new(x1, y0),
                Point::new(x1, y1),
                Point::new(x0, y1),
            ],
        }
    }

    /// Structural check only: vertex count, finiteness, the `[0, 1]` bound and a
    /// non-zero area. Overlap and hole checks between slots belong to the
    /// template library (S2).
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.points.len() < Self::MIN_VERTICES {
            return Err("outline needs at least 3 vertices");
        }
        for p in &self.points {
            if !p.is_finite() {
                return Err("outline vertex is not finite");
            }
            if !(0.0..=1.0).contains(&p.x) || !(0.0..=1.0).contains(&p.y) {
                return Err("outline vertex is outside [0, 1]");
            }
        }
        if !self.area().is_finite() || self.area() <= 0.0 {
            return Err("outline has zero area");
        }
        Ok(())
    }

    pub fn bbox(&self) -> Rect {
        // `validate` guarantees at least one vertex; an empty polygon has no
        // box, and callers only ever see validated outlines.
        Rect::from_points(&self.points).unwrap_or(Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        })
    }

    /// Shoelace sum; the sign encodes the winding direction.
    pub fn signed_area(&self) -> f64 {
        let n = self.points.len();
        if n < Self::MIN_VERTICES {
            return 0.0;
        }
        let mut sum = 0.0;
        let mut j = n - 1;
        for i in 0..n {
            let (a, b) = (self.points[i], self.points[j]);
            sum += a.x * b.y - b.x * a.y;
            j = i;
        }
        sum / 2.0
    }

    /// Area as a fraction of the canvas (`1.0` = the whole canvas).
    pub fn area(&self) -> f64 {
        self.signed_area().abs()
    }

    /// Even-odd containment. Points exactly on the boundary are unspecified:
    /// probes keep a guard band so they never ask.
    pub fn contains(&self, p: Point) -> bool {
        let n = self.points.len();
        if n < Self::MIN_VERTICES {
            return false;
        }
        let mut inside = false;
        let mut j = n - 1;
        for i in 0..n {
            let (a, b) = (self.points[i], self.points[j]);
            if (a.y > p.y) != (b.y > p.y) {
                let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
                if p.x < x {
                    inside = !inside;
                }
            }
            j = i;
        }
        inside
    }

    pub fn edges(&self) -> impl Iterator<Item = (Point, Point)> + '_ {
        let n = self.points.len();
        (0..n).map(move |i| (self.points[i], self.points[(i + 1) % n]))
    }

    /// The part of this polygon inside `rect`, as a closed polygon.
    ///
    /// Sutherland–Hodgman against the rectangle's four half-planes: the clip
    /// region is convex, so the result is exactly the intersection's vertex list
    /// and the test "the photo covers this region" stays a vertex test. A polygon
    /// already inside `rect` comes back **unchanged**, vertex for vertex — which
    /// is what keeps the unframed fit the same arithmetic it was before the frame
    /// existed (S11).
    ///
    /// The result can be empty, or have fewer than [`Polygon::MIN_VERTICES`]
    /// vertices, when the polygon does not reach into the rectangle: a caller
    /// that needs a region with an interior checks `area()`.
    pub fn clipped_to(&self, rect: Rect) -> Self {
        let mut points = self.points.clone();
        for side in 0..4 {
            if points.is_empty() {
                break;
            }
            // `vertical` picks the coordinate, `bound` is where the half-plane
            // starts and `above` says which side of it is inside.
            let (vertical, bound, above) = match side {
                0 => (true, rect.x0, true),
                1 => (true, rect.x1, false),
                2 => (false, rect.y0, true),
                _ => (false, rect.y1, false),
            };
            let coordinate = |p: Point| if vertical { p.x } else { p.y };
            let inside = |p: Point| {
                if above {
                    coordinate(p) >= bound
                } else {
                    coordinate(p) <= bound
                }
            };
            // The clipped coordinate is `bound` itself rather than the
            // interpolation's value, so a vertex that lands on the edge is
            // exactly on it.
            let crossing = |a: Point, b: Point| {
                let t = (bound - coordinate(a)) / (coordinate(b) - coordinate(a));
                if vertical {
                    Point::new(bound, a.y + t * (b.y - a.y))
                } else {
                    Point::new(a.x + t * (b.x - a.x), bound)
                }
            };
            let previous = points;
            let mut next = Vec::with_capacity(previous.len() + 4);
            for (index, current) in previous.iter().copied().enumerate() {
                let prior = previous[(index + previous.len() - 1) % previous.len()];
                match (inside(prior), inside(current)) {
                    (true, true) => next.push(current),
                    (true, false) => next.push(crossing(prior, current)),
                    (false, true) => {
                        next.push(crossing(prior, current));
                        next.push(current);
                    }
                    (false, false) => {}
                }
            }
            points = next;
        }
        Self { points }
    }

    /// Distance from `p` to the nearest edge, in normalized units.
    pub fn distance_to_boundary(&self, p: Point) -> f64 {
        self.edges()
            .map(|(a, b)| p.distance_to_segment(a, b))
            .fold(f64::INFINITY, f64::min)
    }
}
