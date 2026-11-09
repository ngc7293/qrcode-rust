//! SVG rendering support.
//!
//! # Example
//!
//! ```
//! use qrcode::QrCode;
//! use qrcode::render::svg;
//!
//! let code = QrCode::new(b"Hello").unwrap();
//! let svg_xml = code.render::<svg::Color>().build();
//! println!("{svg_xml}");

#![cfg(feature = "svg")]

use alloc::format;
use alloc::string::String;
use core::fmt::Write;

use crate::render::{Canvas as RenderCanvas, Corners, Pixel};
use crate::types::Color as ModuleColor;

/// An SVG color.
#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Color<'a>(pub &'a str);

impl<'a> Pixel for Color<'a> {
    type Canvas = Canvas<'a>;
    type Image = String;

    fn default_color(color: ModuleColor) -> Self {
        Color(color.select("#000", "#fff"))
    }
}

#[doc(hidden)]
pub struct Canvas<'a> {
    width: u32,
    height: u32,
    dark_pixel: Color<'a>,
    light_pixel: Color<'a>,
    path: String,
    is_rounded: bool,
}

impl<'a> RenderCanvas for Canvas<'a> {
    type Pixel = Color<'a>;
    type Image = String;

    fn new(width: u32, height: u32, dark_pixel: Color<'a>, light_pixel: Color<'a>) -> Self {
        Canvas { width, height, dark_pixel, light_pixel, path: String::new(), is_rounded: false }
    }

    fn draw_dark_pixel(&mut self, x: u32, y: u32) {
        self.draw_dark_rect(x, y, 1, 1);
    }

    fn draw_dark_rect(&mut self, left: u32, top: u32, width: u32, height: u32) {
        write!(self.path, "M{left} {top}h{width}v{height}h-{width}z").unwrap();
    }

    fn draw_dark_rounded_rect(
        &mut self,
        left: u32,
        top: u32,
        width: u32,
        height: u32,
        radius: (u32, u32),
        corners: Corners,
    ) {
        if corners.is_empty() {
            self.draw_dark_rect(left, top, width, height);
            return;
        }
        self.is_rounded = true;

        let top_left = corner_radius(corners, Corners::TOP_LEFT, radius);
        let top_right = corner_radius(corners, Corners::TOP_RIGHT, radius);
        let bottom_right = corner_radius(corners, Corners::BOTTOM_RIGHT, radius);
        let bottom_left = corner_radius(corners, Corners::BOTTOM_LEFT, radius);

        // Traced clockwise like `draw_dark_rect`, so overlapping subpaths don't
        // cancel each other out under the nonzero fill rule.
        write!(self.path, "M{} {top}", left + top_left.0).unwrap();
        write_line(&mut self.path, "h", width - top_left.0 - top_right.0);
        write_arc(&mut self.path, top_right, 1, "", "");
        write_line(&mut self.path, "v", height - top_right.1 - bottom_right.1);
        write_arc(&mut self.path, bottom_right, 1, "-", "");
        write_line(&mut self.path, "h-", width - bottom_right.0 - bottom_left.0);
        write_arc(&mut self.path, bottom_left, 1, "-", "-");
        write_line(&mut self.path, "v-", height - bottom_left.1 - top_left.1);
        write_arc(&mut self.path, top_left, 1, "", "-");
        self.path.push('z');
    }

    fn draw_dark_fillet(
        &mut self,
        left: u32,
        top: u32,
        width: u32,
        height: u32,
        (rx, ry): (u32, u32),
        corners: Corners,
    ) {
        if corners.is_empty() || rx == 0 || ry == 0 {
            return;
        }
        self.is_rounded = true;

        // Each fillet goes from the corner along one edge, then curves back to
        // the other edge. Traced clockwise, like the other subpaths.
        let (right, bottom) = (left + width, top + height);
        if corners.contains(Corners::TOP_LEFT) {
            write!(self.path, "M{left} {top}h{rx}").unwrap();
            write_arc(&mut self.path, (rx, ry), 0, "-", "");
            self.path.push('z');
        }
        if corners.contains(Corners::TOP_RIGHT) {
            write!(self.path, "M{right} {top}v{ry}").unwrap();
            write_arc(&mut self.path, (rx, ry), 0, "-", "-");
            self.path.push('z');
        }
        if corners.contains(Corners::BOTTOM_RIGHT) {
            write!(self.path, "M{right} {bottom}h-{rx}").unwrap();
            write_arc(&mut self.path, (rx, ry), 0, "", "-");
            self.path.push('z');
        }
        if corners.contains(Corners::BOTTOM_LEFT) {
            write!(self.path, "M{left} {bottom}v-{ry}").unwrap();
            write_arc(&mut self.path, (rx, ry), 0, "", "");
            self.path.push('z');
        }
    }

    fn into_image(self) -> String {
        format!(
            concat!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
                r#"<svg xmlns="http://www.w3.org/2000/svg""#,
                r#" version="1.1" width="{w}" height="{h}""#,
                r#" viewBox="0 0 {w} {h}" shape-rendering="{rendering}">"#,
                r#"<path d="M0 0h{w}v{h}H0z" fill="{bg}"/>"#,
                r#"<path fill="{fg}" d="{path}"/></svg>"#,
            ),
            w = self.width,
            h = self.height,
            rendering = if self.is_rounded { "geometricPrecision" } else { "crispEdges" },
            fg = self.dark_pixel.0,
            bg = self.light_pixel.0,
            path = self.path,
        )
    }
}

/// Returns `radius` if `corner` is one of the rounded `corners`, or zero
/// otherwise.
fn corner_radius(corners: Corners, corner: Corners, radius: (u32, u32)) -> (u32, u32) {
    if corners.contains(corner) && radius.0 > 0 && radius.1 > 0 {
        radius
    } else {
        (0, 0)
    }
}

/// Appends a straight line `command` of the given `length` to the `path`.
/// Nothing is written when the length is zero.
fn write_line(path: &mut String, command: &str, length: u32) {
    if length > 0 {
        write!(path, "{command}{length}").unwrap();
    }
}

/// Appends a quarter-ellipse arc of radius `rx`×`ry` to the `path`, moving by
/// (`x_sign` `rx`, `y_sign` `ry`). Nothing is written when the radius is zero.
fn write_arc(path: &mut String, (rx, ry): (u32, u32), sweep: u8, x_sign: &str, y_sign: &str) {
    if rx > 0 && ry > 0 {
        write!(path, "a{rx} {ry} 0 0 {sweep} {x_sign}{rx} {y_sign}{ry}").unwrap();
    }
}

#[cfg(test)]
mod render_tests {
    use crate::render::svg::Color as SvgColor;
    use crate::render::Renderer;
    use crate::types::Color;

    #[test]
    fn test_render_rounded_corners() {
        let svg = Renderer::<SvgColor>::new(&[Color::Dark, Color::Dark, Color::Light, Color::Light], 2, 1)
            .quiet_zone(false)
            .module_dimensions(4, 4)
            .corner_radius(0.5)
            .build();

        assert!(svg.contains(r#"shape-rendering="geometricPrecision""#));
        assert!(svg.contains(concat!(
            r##"<path fill="#000" d=""##,
            "M2 0h2v4h-2a2 2 0 0 1 -2 -2a2 2 0 0 1 2 -2z",
            "M4 0h2a2 2 0 0 1 2 2a2 2 0 0 1 -2 2h-2v-4z",
            r#""/>"#,
        )));
    }

    #[test]
    fn test_render_rounded_inner_corners() {
        let svg = Renderer::<SvgColor>::new(&[Color::Dark, Color::Dark, Color::Dark, Color::Light], 2, 1)
            .quiet_zone(false)
            .module_dimensions(4, 4)
            .corner_radius(0.5)
            .build();

        assert!(svg.contains(concat!(
            r##"<path fill="#000" d=""##,
            "M2 0h2v4h-4v-2a2 2 0 0 1 2 -2z",
            "M4 0h2a2 2 0 0 1 2 2a2 2 0 0 1 -2 2h-2v-4z",
            "M0 4h4v2a2 2 0 0 1 -2 2a2 2 0 0 1 -2 -2v-2z",
            "M4 4h2a2 2 0 0 0 -2 2z",
            r#""/>"#,
        )));
    }

    #[test]
    fn test_render_rounded_corners_too_small() {
        let svg = Renderer::<SvgColor>::new(&[Color::Dark], 1, 0).module_dimensions(1, 1).corner_radius(0.5).build();

        assert!(svg.contains(r#"shape-rendering="crispEdges""#));
        assert!(svg.contains(r#"d="M0 0h1v1h-1z""#));
    }
}
