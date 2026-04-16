/// A simple point in 2D space.
///
/// This struct is used throughout the geometry module.
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A rectangle defined by its top-left corner, width, and height.
#[derive(Debug, Clone, PartialEq)]
pub struct Rectangle {
    pub origin: Point,
    pub width: f64,
    pub height: f64,
}

impl Rectangle {
    /// Creates a new rectangle.
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Rectangle {
            origin: Point { x, y },
            width,
            height,
        }
    }

    /// Computes the area of the rectangle.
    pub fn area(&self) -> f64 {
        self.width * self.height
    }
}

/// Configuration for the geometry module.
#[derive(Debug)]
pub struct Config {
    pub precision: u32,
}
