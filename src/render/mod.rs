//! Render a QR code into image.

use crate::cast::As;
use crate::types::Color;
use core::cmp::max;
use core::ops::{BitOr, BitOrAssign};

pub mod eps;
pub mod image;
pub mod pic;
pub mod string;
pub mod svg;
pub mod unicode;

//------------------------------------------------------------------------------
//{{{ Corners

/// A set of corners of a module.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Corners(u8);

impl Corners {
    /// No corner.
    pub const NONE: Self = Corners(0);
    /// The top-left corner.
    pub const TOP_LEFT: Self = Corners(1);
    /// The top-right corner.
    pub const TOP_RIGHT: Self = Corners(2);
    /// The bottom-right corner.
    pub const BOTTOM_RIGHT: Self = Corners(4);
    /// The bottom-left corner.
    pub const BOTTOM_LEFT: Self = Corners(8);

    /// Whether all corners of `other` are in `self`.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether the set contains no corner.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl BitOr for Corners {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Corners(self.0 | other.0)
    }
}

impl BitOrAssign for Corners {
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

/// Whether the pixel at (`x`, `y`), relative to the top-left of a
/// `width`×`height` rectangle, falls outside one of its `corners` rounded with
/// radius `rx`×`ry`.
///
/// A pixel is considered inside the rounded corner when its center lies within
/// the ellipse. The radius must not exceed half of the rectangle's size.
#[cfg(feature = "image")]
pub(crate) fn is_outside_rounded_corner(
    width: u32,
    height: u32,
    (rx, ry): (u32, u32),
    corners: Corners,
    x: u32,
    y: u32,
) -> bool {
    if rx == 0 || ry == 0 {
        return false;
    }
    // Distance (in pixels) from the nearest vertical and horizontal edges.
    let (dx, is_right) = if x < rx {
        (x, false)
    } else if x >= width - rx {
        (width - 1 - x, true)
    } else {
        return false;
    };
    let (dy, is_bottom) = if y < ry {
        (y, false)
    } else if y >= height - ry {
        (height - 1 - y, true)
    } else {
        return false;
    };
    let corner = match (is_right, is_bottom) {
        (false, false) => Corners::TOP_LEFT,
        (true, false) => Corners::TOP_RIGHT,
        (true, true) => Corners::BOTTOM_RIGHT,
        (false, true) => Corners::BOTTOM_LEFT,
    };
    if !corners.contains(corner) {
        return false;
    }

    // Everything is doubled so the pixel center (d + 0.5) stays integral.
    let a = u64::from(2 * (rx - dx) - 1);
    let b = u64::from(2 * (ry - dy) - 1);
    let rx2 = u64::from(2 * rx);
    let ry2 = u64::from(2 * ry);
    a * a * ry2 * ry2 + b * b * rx2 * rx2 > rx2 * rx2 * ry2 * ry2
}

//}}}
//------------------------------------------------------------------------------
//{{{ Pixel trait

/// Abstraction of an image pixel.
pub trait Pixel: Copy + Sized {
    /// Type of the finalized image.
    type Image: Sized + 'static;

    /// The type that stores an intermediate buffer before finalizing to a
    /// concrete image
    type Canvas: Canvas<Pixel = Self, Image = Self::Image>;

    /// Obtains the default module size. The result must be at least 1×1.
    fn default_unit_size() -> (u32, u32) {
        (8, 8)
    }

    /// Obtains the default pixel color when a module is dark or light.
    fn default_color(color: Color) -> Self;
}

/// Rendering canvas of a QR code image.
pub trait Canvas: Sized {
    /// Type of an image pixel.
    type Pixel: Sized;

    /// Type of the finalized image.
    type Image: Sized;

    /// Constructs a new canvas of the given dimensions.
    fn new(width: u32, height: u32, dark_pixel: Self::Pixel, light_pixel: Self::Pixel) -> Self;

    /// Draws a single dark pixel at the (x, y) coordinate.
    fn draw_dark_pixel(&mut self, x: u32, y: u32);

    /// Draws a dark rectangle with dimensions `width`×`height` at the (`left`,
    /// `top`) coordinate.
    fn draw_dark_rect(&mut self, left: u32, top: u32, width: u32, height: u32) {
        for y in top..(top + height) {
            for x in left..(left + width) {
                self.draw_dark_pixel(x, y);
            }
        }
    }

    /// Draws a dark rectangle with dimensions `width`×`height` at the (`left`,
    /// `top`) coordinate, with the given `corners` rounded by a radius of
    /// `rx`×`ry`.
    ///
    /// The default implementation ignores the rounding and draws a plain
    /// rectangle.
    fn draw_dark_rounded_rect(
        &mut self,
        left: u32,
        top: u32,
        width: u32,
        height: u32,
        radius: (u32, u32),
        corners: Corners,
    ) {
        let _ = (radius, corners);
        self.draw_dark_rect(left, top, width, height);
    }

    /// Finalize the canvas to a real image.
    fn into_image(self) -> Self::Image;
}

//}}}
//------------------------------------------------------------------------------
//{{{ Renderer

/// A QR code renderer. This is a builder type which converts a bool-vector into
/// an image.
pub struct Renderer<'a, P: Pixel> {
    content: &'a [Color],
    modules_count: u32, // <- we call it `modules_count` here to avoid ambiguity of `width`.
    quiet_zone: u32,
    module_size: (u32, u32),
    corner_radius: f32,

    dark_color: P,
    light_color: P,
    has_quiet_zone: bool,
}

impl<'a, P: Pixel> Renderer<'a, P> {
    /// Creates a new renderer.
    ///
    /// # Panics
    ///
    /// Panics if the length of `content` is not exactly `modules_count * modules_count`.
    pub fn new(content: &'a [Color], modules_count: usize, quiet_zone: u32) -> Self {
        assert!(modules_count * modules_count == content.len());
        Renderer {
            content,
            modules_count: modules_count.as_u32(),
            quiet_zone,
            module_size: P::default_unit_size(),
            corner_radius: 0.0,
            dark_color: P::default_color(Color::Dark),
            light_color: P::default_color(Color::Light),
            has_quiet_zone: true,
        }
    }

    /// Sets color of a dark module. Default is opaque black.
    pub fn dark_color(&mut self, color: P) -> &mut Self {
        self.dark_color = color;
        self
    }

    /// Sets color of a light module. Default is opaque white.
    pub fn light_color(&mut self, color: P) -> &mut Self {
        self.light_color = color;
        self
    }

    /// Whether to include the quiet zone in the generated image.
    pub fn quiet_zone(&mut self, has_quiet_zone: bool) -> &mut Self {
        self.has_quiet_zone = has_quiet_zone;
        self
    }

    /// Sets the size of each module in pixels. Default is 8px.
    #[deprecated(since = "0.4.0", note = "use `.module_dimensions(width, width)` instead")]
    pub fn module_size(&mut self, width: u32) -> &mut Self {
        self.module_dimensions(width, width)
    }

    /// Sets the size of each module in pixels. Default is 8×8.
    pub fn module_dimensions(&mut self, width: u32, height: u32) -> &mut Self {
        self.module_size = (max(width, 1), max(height, 1));
        self
    }

    /// Sets the radius of the rounded corners of dark modules, as a fraction of
    /// the module size. The value is clamped to 0.0–0.5, and the resulting
    /// radius is rounded to whole pixels. Default is 0.0 (square modules).
    ///
    /// Only the corners exposed to light modules are rounded, so adjacent dark
    /// modules stay connected. For instance, with 32×32 modules a ratio of 0.1
    /// gives a 3px radius, and 0.5 turns isolated modules into circles.
    ///
    /// Only the SVG and raster image renderers support rounded corners; other
    /// renderers ignore this setting.
    pub fn corner_radius(&mut self, ratio: f32) -> &mut Self {
        self.corner_radius = ratio.clamp(0.0, 0.5);
        self
    }

    /// Sets the minimum total image width (and thus height) in pixels,
    /// including the quiet zone if applicable. The renderer will try to find
    /// the dimension as small as possible, such that each module in the QR code
    /// has uniform size (no distortion).
    ///
    /// For instance, a version 1 QR code has 19 modules across including the
    /// quiet zone. If we request an image of width ≥200px, we get that each
    /// module's size should be 11px, so the actual image size will be 209px.
    #[deprecated(since = "0.4.0", note = "use `.min_dimensions(width, width)` instead")]
    pub fn min_width(&mut self, width: u32) -> &mut Self {
        self.min_dimensions(width, width)
    }

    /// Sets the minimum total image size in pixels, including the quiet zone if
    /// applicable. The renderer will try to find the dimension as small as
    /// possible, such that each module in the QR code has uniform size (no
    /// distortion).
    ///
    /// For instance, a version 1 QR code has 19 modules across including the
    /// quiet zone. If we request an image of size ≥200×200, we get that each
    /// module's size should be 11×11, so the actual image size will be 209×209.
    pub fn min_dimensions(&mut self, width: u32, height: u32) -> &mut Self {
        let quiet_zone = if self.has_quiet_zone { 2 } else { 0 } * self.quiet_zone;
        let width_in_modules = self.modules_count + quiet_zone;
        let unit_width = (width + width_in_modules - 1) / width_in_modules;
        let unit_height = (height + width_in_modules - 1) / width_in_modules;
        self.module_dimensions(unit_width, unit_height)
    }

    /// Sets the maximum total image size in pixels, including the quiet zone if
    /// applicable. The renderer will try to find the dimension as large as
    /// possible, such that each module in the QR code has uniform size (no
    /// distortion).
    ///
    /// For instance, a version 1 QR code has 19 modules across including the
    /// quiet zone. If we request an image of size ≤200×200, we get that each
    /// module's size should be 10×10, so the actual image size will be 190×190.
    ///
    /// The module size is at least 1×1, so if the restriction is too small, the
    /// final image *can* be larger than the input.
    pub fn max_dimensions(&mut self, width: u32, height: u32) -> &mut Self {
        let quiet_zone = if self.has_quiet_zone { 2 } else { 0 } * self.quiet_zone;
        let width_in_modules = self.modules_count + quiet_zone;
        let unit_width = width / width_in_modules;
        let unit_height = height / width_in_modules;
        self.module_dimensions(unit_width, unit_height)
    }

    /// Renders the QR code into an image.
    #[deprecated(since = "0.4.0", note = "renamed to `.build()` to de-emphasize the image connection")]
    pub fn to_image(&self) -> P::Image {
        self.build()
    }

    /// Renders the QR code into an image.
    pub fn build(&self) -> P::Image {
        let w = self.modules_count;
        let qz = if self.has_quiet_zone { self.quiet_zone } else { 0 };
        let width = w + 2 * qz;

        let (mw, mh) = self.module_size;
        let real_width = width * mw;
        let real_height = width * mh;

        let radius = self.corner_radius_in_pixels();
        let is_rounded = radius.0 > 0 && radius.1 > 0;

        let mut canvas = P::Canvas::new(real_width, real_height, self.dark_color, self.light_color);
        let mut i = 0;
        for y in 0..width {
            for x in 0..width {
                if qz <= x && x < w + qz && qz <= y && y < w + qz {
                    if self.content[i] != Color::Light {
                        if is_rounded {
                            let corners = self.convex_corners(x - qz, y - qz);
                            canvas.draw_dark_rounded_rect(x * mw, y * mh, mw, mh, radius, corners);
                        } else {
                            canvas.draw_dark_rect(x * mw, y * mh, mw, mh);
                        }
                    }
                    i += 1;
                }
            }
        }

        canvas.into_image()
    }

    /// Converts the corner radius ratio into pixels, so that two rounded
    /// corners never overlap within a module.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
    fn corner_radius_in_pixels(&self) -> (u32, u32) {
        let (mw, mh) = self.module_size;
        let rx = (self.corner_radius * mw as f32 + 0.5) as u32;
        let ry = (self.corner_radius * mh as f32 + 0.5) as u32;
        (rx.min(mw / 2), ry.min(mh / 2))
    }

    /// Whether the module at (`x`, `y`) is dark. Modules outside the symbol
    /// are light.
    fn is_dark(&self, x: isize, y: isize) -> bool {
        let n = self.modules_count.as_isize();
        if x < 0 || y < 0 || x >= n || y >= n {
            return false;
        }
        self.content[(y * n + x).as_usize()] != Color::Light
    }

    /// Returns the corners of the dark module at (`x`, `y`) that are exposed,
    /// i.e. whose two adjacent sides both face a light module.
    fn convex_corners(&self, x: u32, y: u32) -> Corners {
        let (x, y) = (x.as_isize(), y.as_isize());
        let top = self.is_dark(x, y - 1);
        let bottom = self.is_dark(x, y + 1);
        let left = self.is_dark(x - 1, y);
        let right = self.is_dark(x + 1, y);

        let mut corners = Corners::NONE;
        if !top && !left {
            corners |= Corners::TOP_LEFT;
        }
        if !top && !right {
            corners |= Corners::TOP_RIGHT;
        }
        if !bottom && !right {
            corners |= Corners::BOTTOM_RIGHT;
        }
        if !bottom && !left {
            corners |= Corners::BOTTOM_LEFT;
        }
        corners
    }
}

//}}}
