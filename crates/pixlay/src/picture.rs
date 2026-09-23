//! A decoded picture, held once: the texture a widget paints and the bytes it was
//! built from.
//!
//! The picker's tiles and its preview pane (S13c) and the layout gallery's
//! candidates (S14) are all "straight RGB bytes that a widget shows", so the
//! wrapper is one type rather than three: a `gdk::MemoryTexture` **holds a
//! reference to the `glib::Bytes` it was built from**, so a picture costs one copy
//! of its pixels rather than two, and [`Picture::texture`] is a refcount bump where
//! a copy on every paint would be a whole image.
//!
//! The bytes are kept — and exposed ([`Picture::bytes`]) — for the tests: a stage's
//! pixel criterion compares what the widget is holding against what
//! `pixlay-render` writes for the same document, and an RGB buffer is what both
//! sides have.

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;

/// One picture: its texture, its buffer and its size.
pub struct Picture {
    texture: gdk::Texture,
    bytes: glib::Bytes,
    width: i32,
    height: i32,
}

impl Picture {
    /// A picture from straight, opaque RGB pixels: `width * height * 3` bytes,
    /// row-major, top-left first.
    pub fn rgb8(width: i32, height: i32, pixels: Vec<u8>) -> Self {
        let bytes = glib::Bytes::from_owned(pixels);
        let texture = gdk::MemoryTexture::new(
            width,
            height,
            gdk::MemoryFormat::R8g8b8,
            &bytes,
            (width * 3) as usize,
        )
        .upcast();
        Self {
            texture,
            bytes,
            width,
            height,
        }
    }

    /// The texture, for a widget to paint.
    pub fn texture(&self) -> &gdk::Texture {
        &self.texture
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    /// The pixels behind the texture, which is what the tests compare.
    pub fn bytes(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}
