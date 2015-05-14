/*!
Pixel buffers are buffers that contain two-dimensional texture data.

Contrary to textures, pixel buffers are stored in a client-defined format. They are used
to transfer data to or from the video memory, before or after being turned into a texture.
 */
use std::borrow::Cow;
use std::marker::PhantomData;

use backend::Facade;

use texture::{RawImage2d, Texture2dDataSink, ClientFormat, PixelValue};

use GlObject;
use buffer::{Buffer, BufferType};
use gl;

/// Buffer that stores the content of a texture.
///
/// The generic type represents the type of pixels that the buffer contains.
pub struct PixelBuffer<T> {
    buffer: Buffer,
    dimensions: Option<(u32, u32)>,
    marker: PhantomData<T>,
}

impl<T> PixelBuffer<T> {
    /// Builds a new buffer with an uninitialized content.
    pub fn new_empty<F>(facade: &F, capacity: usize) -> PixelBuffer<T> where F: Facade {
        PixelBuffer {
            buffer: Buffer::empty(facade, BufferType::PixelPackBuffer, 1, capacity,
                                  false).unwrap(),
            dimensions: None,
            format: None,
            marker: PhantomData,
        }
    }

    /// Returns the length of the buffer, in number of pixels.
    pub fn len(&self) -> usize {
        self.buffer.get_elements_count()
    }
}

impl<T> GlObject for PixelBuffer<T> {
    type Id = gl::types::GLuint;
    fn get_id(&self) -> gl::types::GLuint {
        self.buffer.get_id()
    }
}

// TODO: remove this hack
#[doc(hidden)]
pub fn store_infos<T>(b: &mut PixelBuffer<T>, dimensions: (u32, u32), format: ClientFormat) {
    b.dimensions = Some(dimensions);
    b.format = Some(format);
}
