use std::io::{Write, self};
use std::collections::HashMap;
use std::sync::Arc;

use ::fontsource::FontSource;
use ::fontref::FontRef;
use ::outline::OutlineItem;
use textobject::TextObject;

/// An visual area where content can be drawn (a page).
///
/// Provides methods for defining and stroking or filling paths, as
/// well as placing text objects.
pub struct Canvas<'a> {
    output: &'a mut Write,
    fonts: &'a mut HashMap<FontSource, FontRef>,
    outline_items: &'a mut Vec<OutlineItem>
}

impl<'a> Canvas<'a> {
    pub fn new(output: &'a mut Write, 
               fonts: &'a mut HashMap<FontSource, FontRef>,
               outline_items: &'a mut Vec<OutlineItem>) -> Canvas<'a> {
        Canvas {
            output: output,
            fonts: fonts,
            outline_items: outline_items
        }
    }
    
    /// Append a closed rectangle with a corern at (x, y) and
    /// extending width × height to the to the current path.
    pub fn rectangle(&mut self, x: f32, y: f32, width: f32, height: f32)
                     -> io::Result<()> {
        write!(self.output, "{} {} {} {} re\n", x, y, width, height)
    }
    /// Set the line width in the graphics state
    pub fn set_line_width(&mut self, w: f32) -> io::Result<()> {
        write!(self.output, "{} w\n", w)
    }
    /// Set rgb color for stroking operations
    pub fn set_stroke_color(&mut self, r: u8, g: u8, b: u8) -> io::Result<()> {
        let norm = |c| { c as f32 / 255.0 };
        write!(self.output, "{} {} {} SC\n", norm(r), norm(g), norm(b))
    }
    /// Set rgb color for non-stroking operations
    pub fn set_fill_color(&mut self, r: u8, g: u8, b: u8) -> io::Result<()> {
        let norm = |c| { c as f32 / 255.0 };
        write!(self.output, "{} {} {} sc\n", norm(r), norm(g), norm(b))
    }
    /// Set gray level for stroking operations
    pub fn set_stroke_gray(&mut self, gray: u8) -> io::Result<()> {
        write!(self.output, "{} G\n", gray as f32 / 255.0)
    }
    /// Set gray level for non-stroking operations
    pub fn set_fill_gray(&mut self, gray: u8) -> io::Result<()> {
        write!(self.output, "{} g\n", gray as f32 / 255.0)
    }
    /// Append a straight line from (x1, y1) to (x2, y2) to the current path.
    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32)
                -> io::Result<()> {
        try!(self.move_to(x1, y1));
        self.line_to(x2, y2)
    }
    /// Begin a new subpath at the point (x, y).
    pub fn move_to(&mut self, x: f32, y: f32) -> io::Result<()> {
        write!(self.output, "{} {} m ", x, y)
    }
    /// Add a straight line from the current point to (x, y) to the
    /// current path.
    pub fn line_to(&mut self, x: f32, y: f32) -> io::Result<()> {
        write!(self.output, "{} {} l ", x, y)
    }
    /// Add an Bézier curve from the current point to (x3, y3) with
    /// (x1, y1) and (x2, y2) as Bézier controll points.
    pub fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32,
                    x3: f32, y3: f32) -> io::Result<()> {
        write!(self.output, "{} {} {} {} {} {} c\n", x1, y1, x2, y2, x3, y3)
    }
    /// Add a circle approximated by four cubic Bézier curves to the
    /// current path.  Based on
    /// http://spencermortensen.com/articles/bezier-circle/
    pub fn circle(&mut self, x: f32, y: f32, r: f32) -> io::Result<()> {
        let t = y - r;
        let b = y + r;
        let left = x - r;
        let right = x + r;
        let c = 0.551915024494;
        let leftp = x - (r * c);
        let rightp = x + (r * c);
        let tp = y - (r * c);
        let bp = y + (r * c);
        try!(self.move_to(x, t));
        try!(self.curve_to(leftp, t, left, tp, left, y));
        try!(self.curve_to(left, bp, leftp, b, x, b));
        try!(self.curve_to(rightp, b, right, bp, right, y));
        try!(self.curve_to(right, tp, rightp, t, x, t));
        Ok(())
    }
    /// Stroke the current path.
    pub fn stroke(&mut self) -> io::Result<()> {
        write!(self.output, "s\n")
    }
    /// Fill the current path.
    pub fn fill(&mut self) -> io::Result<()> {
        write!(self.output, "f\n")
    }
    /// Get a FontRef for a specific font.
    pub fn get_font(&mut self, font: FontSource) -> FontRef {
        if let Some(r) = self.fonts.get(&font) {
            return r.clone();
        }
        let n = self.fonts.len();
        let r = FontRef::new(n, Arc::new(font.get_metrics().unwrap()));
        self.fonts.insert(font, r.clone());
        r
    }
    /// Create a text object.
    ///
    /// The contents of the text object is defined by the function
    /// render_text, by applying methods to the TextObject it gets as
    /// an argument.
    pub fn text<F, T>(&mut self, render_text: F) -> io::Result<T>
        where F: FnOnce(&mut TextObject) -> io::Result<T> {
            try!(write!(self.output, "BT\n"));
            let result =
                try!(render_text(&mut TextObject::new(self.output)));
            try!(write!(self.output, "ET\n"));
            Ok(result)
        }
    /// Utility method for placing a string of text.
    pub fn left_text(&mut self, x: f32, y: f32, font: FontSource, size: f32,
                      text: &str) -> io::Result<()> {
        let font = self.get_font(font);
        self.text(|t| {
            try!(t.set_font(&font, size));
            try!(t.pos(x, y));
            t.show(text)
        })
    }
    /// Utility method for placing a string of text.
    pub fn right_text(&mut self, x: f32, y: f32, font: FontSource, size: f32,
                      text: &str) -> io::Result<()> {
        let font = self.get_font(font);
        self.text(|t| {
            let text_width = font.get_width(size, text);
            try!(t.set_font(&font, size));
            try!(t.pos(x - text_width, y));
            t.show(text)
        })
    }
    /// Utility method for placing a string of text.
    pub fn center_text(&mut self, x: f32, y: f32, font: FontSource, size: f32,
                       text: &str) -> io::Result<()> {
        let font = self.get_font(font);
        self.text(|t| {
            let text_width = font.get_width(size, text);
            try!(t.set_font(&font, size));
            try!(t.pos(x - text_width / 2.0, y));
            t.show(text)
        })
    }
    /// Add an item for this page in the document outline.
    pub fn add_outline(&mut self, title: &str) {
        self.outline_items.push(OutlineItem::new(title));
    }
}