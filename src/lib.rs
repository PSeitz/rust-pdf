//! A library for creating pdf files.
//!
//! Currently, simple vector graphics and text set in the 14 built-in
//! fonts are supported.
//! The main entry point of the crate is the [struct Pdf](struct.Pdf.html),
//! representing a PDF file being written.

//! # Example
//!
//! ```
//! use pdf::{Pdf, BuiltinFont, FontSource};
//! use pdf::graphicsstate::Color;
//!
//! let mut document = Pdf::create("example.pdf")
//!     .expect("Create pdf file");
//! // The 14 builtin fonts are available
//! let font = BuiltinFont::Times_Roman;
//!
//! // Add a page to the document.  This page will be 180 by 240 pt large.
//! document.render_page(180.0, 240.0, |canvas| {
//!     // This closure defines the content of the page
//!     let hello = "Hello World!";
//!     let w = font.get_width(24.0, hello) + 8.0;
//!
//!     // Some simple graphics
//!     try!(canvas.set_stroke_color(Color::rgb(0, 0, 248)));
//!     try!(canvas.rectangle(90.0 - w / 2.0, 194.0, w, 26.0));
//!     try!(canvas.stroke());
//!
//!     // Some text
//!     canvas.center_text(90.0, 200.0, font, 24.0, hello)
//! }).expect("Write page");
//! // Write all pending content, including the trailer and index
//! document.finish().expect("Finish pdf document");
//! ```
//!
//! To use this library you need to add it as a dependency in your
//! `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! pdf = "*"
//! ```
//!
//! Some more working usage examples exists in [the examples directory]
//! (https://github.com/kaj/rust-pdf/tree/master/examples).
#![deny(missing_docs)]

#[macro_use]
extern crate lazy_static;

extern crate time;

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs::File;
use std::io::{self, Seek, SeekFrom, Write};

mod fontsource;
pub use fontsource::{BuiltinFont, FontSource};

mod fontref;
pub use fontref::FontRef;

mod fontmetrics;
pub use fontmetrics::FontMetrics;

mod encoding;
pub use encoding::Encoding;

pub mod graphicsstate;

mod outline;
use outline::OutlineItem;

mod canvas;
pub use canvas::Canvas;

mod textobject;
pub use textobject::TextObject;

/// The top-level object for writing a PDF.
///
/// A PDF file is created with the `create` or `new` methods.
/// Some metadata can be stored with `set_foo` methods, and pages
/// are appended with the `render_page` method.
/// Don't forget to call `finish` when done, to write the document
/// trailer, without it the written file won't be a proper PDF.
pub struct Pdf {
    output: File,
    object_offsets: Vec<i64>,
    page_objects_ids: Vec<usize>,
    all_font_object_ids: HashMap<BuiltinFont, usize>,
    outline_items: Vec<OutlineItem>,
    document_info: BTreeMap<String, String>,
}

const ROOT_OBJECT_ID: usize = 1;
const PAGES_OBJECT_ID: usize = 2;

impl Pdf {
    /// Create a new PDF document as a new file with given filename.
    pub fn create(filename: &str) -> io::Result<Pdf> {
        let file = try!(File::create(filename));
        Pdf::new(file)
    }

    /// Create a new PDF document, writing to `output`.
    pub fn new(output: File) -> io::Result<Pdf> {
        let mut output = output; // Strange but needed
        // TODO Maybe use a lower version?  Possibly decide by features used?
        try!(output.write_all(b"%PDF-1.7\n%\xB5\xED\xAE\xFB\n"));
        Ok(Pdf {
            output: output,
            // Object ID 0 is special in PDF.
            // We reserve IDs 1 and 2 for the catalog and page tree.
            object_offsets: vec![-1, -1, -1],
            page_objects_ids: vec![],
            all_font_object_ids: HashMap::new(),
            outline_items: Vec::new(),
            document_info: BTreeMap::new(),
        })
    }
    /// Set metadata: the document's title.
    pub fn set_title(&mut self, title: &str) {
        self.document_info.insert("Title".to_string(), title.to_string());
    }
    /// Set metadata: the name of the person who created the document.
    pub fn set_author(&mut self, author: &str) {
        self.document_info.insert("Author".to_string(), author.to_string());
    }
    /// Set metadata: the subject of the document.
    pub fn set_subject(&mut self, subject: &str) {
        self.document_info.insert("Subject".to_string(), subject.to_string());
    }
    /// Set metadata: keywords associated with the document.
    pub fn set_keywords(&mut self, keywords: &str) {
        self.document_info.insert("Subject".to_string(), keywords.to_string());
    }
    /// Set metadata: If the document was converted to PDF from another
    /// format, the name of the conforming product that created the original
    /// document from which it was converted.
    pub fn set_creator(&mut self, creator: &str) {
        self.document_info.insert("Creator".to_string(), creator.to_string());
    }
    /// Set metadata: If the document was converted to PDF from another
    /// format, the name of the conforming product that converted it to PDF.
    pub fn set_producer(&mut self, producer: &str) {
        self.document_info.insert("Producer".to_string(), producer.to_string());
    }

    /// Return the current read/write position in the output file.
    fn tell(&mut self) -> io::Result<u64> {
        self.output.seek(SeekFrom::Current(0))
    }

    /// Create a new page in the PDF document.
    ///
    /// The page will be `width` x `height` points large, and the
    /// actual content of the page will be created by the function
    /// `render_contents` by applying drawing methods on the Canvas.
    pub fn render_page<F>(&mut self,
                          width: f32,
                          height: f32,
                          render_contents: F)
                          -> io::Result<()>
        where F: FnOnce(&mut Canvas) -> io::Result<()>
    {
        let (contents_object_id, content_length, fonts, outline_items) =
            try!(self.write_new_object(move |contents_object_id, pdf| {
                use canvas::create_canvas;
                // Guess the ID of the next object. (We’ll assert it below.)
                try!(write!(pdf.output,
                            "<< /Length {} 0 R >>\n\
                             stream\n",
                            contents_object_id + 1));

                let start = try!(pdf.tell());
                try!(write!(pdf.output, "/DeviceRGB cs /DeviceRGB CS\n"));
                let mut fonts = HashMap::new();
                let mut outline_items: Vec<OutlineItem> = Vec::new();
                try!(render_contents(&mut create_canvas(&mut pdf.output,
                                                        &mut fonts,
                                                        &mut outline_items)));
                let end = try!(pdf.tell());

                try!(write!(pdf.output, "endstream\n"));
                Ok((contents_object_id, end - start, fonts, outline_items))
            }));
        try!(self.write_new_object(|length_object_id, pdf| {
            assert!(length_object_id == contents_object_id + 1);
            write!(pdf.output, "{}\n", content_length)
        }));

        let mut font_oids = NamedRefs::new();
        for (src, r) in &fonts {
            if let Some(&object_id) = self.all_font_object_ids.get(&src) {
                font_oids.insert(r.clone(), object_id);
            } else {
                let object_id = try!(src.write_object(self));
                font_oids.insert(r.clone(), object_id);
                self.all_font_object_ids.insert(*src, object_id);
            }
        }
        let page_oid = try!(self.write_page_dict(contents_object_id,
                                                 width, height, font_oids));
        // Take the outline_items from this page, mark them with the page ref,
        // and save them for the document outline.
        for i in &outline_items {
            let mut item = i.clone();
            item.set_page(page_oid);
            self.outline_items.push(item);
        }
        self.page_objects_ids.push(page_oid);
        Ok(())
    }

    fn write_page_dict(&mut self,
                       content_oid: usize,
                       width: f32,
                       height: f32,
                       font_oids: NamedRefs)
                       -> io::Result<usize> {
        self.write_new_object(|page_oid, pdf| {
            write!(pdf.output,
                   "<< /Type /Page\n   \
                       /Parent {parent} 0 R\n   \
                       /Resources << /Font << {fonts}>> >>\n   \
                       /MediaBox [ 0 0 {width} {height} ]\n   \
                       /Contents {c_oid} 0 R\n\
                    >>\n",
                   parent = PAGES_OBJECT_ID,
                   fonts = font_oids,
                   width = width,
                   height = height,
                   c_oid = content_oid)
                .map(|_| page_oid)
        })
    }

    fn write_new_object<F, T>(&mut self, write_content: F) -> io::Result<T>
        where F: FnOnce(usize, &mut Pdf) -> io::Result<T>
    {
        let id = self.object_offsets.len();
        let (result, offset) =
            try!(self.write_object(id, |pdf| write_content(id, pdf)));
        self.object_offsets.push(offset);
        Ok(result)
    }

    fn write_object_with_id<F, T>(&mut self,
                                  id: usize,
                                  write_content: F)
                                  -> io::Result<T>
        where F: FnOnce(&mut Pdf) -> io::Result<T>
    {
        assert!(self.object_offsets[id] == -1);
        let (result, offset) = try!(self.write_object(id, write_content));
        self.object_offsets[id] = offset;
        Ok(result)
    }

    fn write_object<F, T>(&mut self,
                          id: usize,
                          write_content: F)
                          -> io::Result<(T, i64)>
        where F: FnOnce(&mut Pdf) -> io::Result<T>
    {
        // `as i64` here would overflow for PDF files bigger than 2**63 bytes
        let offset = try!(self.tell()) as i64;
        try!(write!(self.output, "{} 0 obj\n", id));
        let result = try!(write_content(self));
        try!(write!(self.output, "endobj\n"));
        Ok((result, offset))
    }

    /// Write out the document trailer.
    /// The trailer consists of the pages object, the root object,
    /// the xref list, the trailer object and the startxref position.
    pub fn finish(mut self) -> io::Result<()> {
        try!(self.write_object_with_id(PAGES_OBJECT_ID, |pdf| {
            write!(pdf.output,
                   "<< /Type /Pages\n   \
                       /Count {c}\n   \
                       /Kids [ {pages}]\n\
                    >>\n",
                   c = pdf.page_objects_ids.len(),
                   pages = pdf.page_objects_ids
                       .iter()
                       .map(|id| format!("{} 0 R ", id))
                       .collect::<String>())
        }));
        let document_info_id = if !self.document_info.is_empty() {
            let info = self.document_info.clone();
            try!(self.write_new_object(|page_object_id, pdf| {
                try!(write!(pdf.output, "<<"));
                for (key, value) in info {
                    try!(write!(pdf.output, " /{} ({})\n", key, value));
                }
                if let Ok(now) = time::strftime("%Y%m%d%H%M%S%z",
                                                &time::now()) {
                    try!(write!(pdf.output,
                                " /CreationDate (D:{now})\n \
                                  /ModDate (D:{now})",
                                now = now));
                }
                try!(write!(pdf.output, ">>\n"));
                Ok(Some(page_object_id))
            }))
        } else {
            None
        };

        let outlines_id = try!(self.write_outlines());

        try!(self.write_object_with_id(ROOT_OBJECT_ID, |pdf| {
            try!(write!(pdf.output,
                        "<< /Type /Catalog\n   \
                            /Pages {} 0 R\n",
                        PAGES_OBJECT_ID));
            if let Some(outlines_id) = outlines_id {
                try!(write!(pdf.output, "/Outlines {} 0 R\n", outlines_id));
            }
            try!(write!(pdf.output, ">>\n"));
            Ok(())
        }));
        let startxref = try!(self.tell());
        try!(write!(self.output,
                    "xref\n\
                     0 {}\n\
                     0000000000 65535 f \n",
                    self.object_offsets.len()));
        // Object 0 (above) is special
        // Use [1..] to skip object 0 in self.object_offsets.
        for &offset in &self.object_offsets[1..] {
            assert!(offset >= 0);
            try!(write!(self.output, "{:010} 00000 n \n", offset));
        }
        try!(write!(self.output,
                    "trailer\n\
                     << /Size {size}\n   \
                        /Root {root} 0 R\n",
                    size = self.object_offsets.len(),
                    root = ROOT_OBJECT_ID));
        if let Some(id) = document_info_id {
            try!(write!(self.output, "   /Info {} 0 R\n", id));
        }
        try!(write!(self.output,
                    ">>\n\
                     startxref\n\
                     {}\n\
                     %%EOF\n",
                    startxref));
        Ok(())
    }

    fn write_outlines(&mut self) -> io::Result<Option<usize>> {
        if self.outline_items.is_empty() {
            return Ok(None);
        }

        let parent_id = self.object_offsets.len();
        self.object_offsets.push(-1);
        let count = self.outline_items.len();
        let mut first_id = 0;
        let mut last_id = 0;
        let items = self.outline_items.clone();
        for (i, item) in items.iter().enumerate() {
            let (is_first, is_last) = (i == 0, i == count - 1);
            let id = try!(self.write_new_object(|object_id, pdf| {
                item.write_dictionary(&mut pdf.output,
                                      parent_id,
                                      if is_first {
                                          None
                                      } else {
                                          Some(object_id - 1)
                                      },
                                      if is_last {
                                          None
                                      } else {
                                          Some(object_id + 1)
                                      })
                    .and(Ok(object_id))
            }));
            if is_first {
                first_id = id;
            }
            if is_last {
                last_id = id;
            }
        }
        try!(self.write_object_with_id(parent_id, |pdf| {
            write!(pdf.output,
                   "<< /Type /Outlines\n   \
                    /First {first} 0 R\n   \
                    /Last {last} 0 R\n   \
                    /Count {count}\n\
                    >>\n",
                   last = last_id,
                   first = first_id,
                   count = count)
        }));
        Ok(Some(parent_id))
    }
}

struct NamedRefs {
    oids: HashMap<FontRef, usize>,
}

impl NamedRefs {
    fn new() -> Self {
        NamedRefs { oids: HashMap::new() }
    }
    fn insert(&mut self, name: FontRef, oid: usize) -> Option<usize> {
        self.oids.insert(name, oid)
    }
}


impl fmt::Display for NamedRefs {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (name, id) in self.oids.iter() {
            try!(write!(f, "{} {} 0 R ", name, id));
        }
        Ok(())
    }
}
