use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::mpsc::Sender;

use buffer::{self, Buffer, BufferFlags, BufferType, BufferCreationError};
use vertex::{Vertex, VerticesSource, IntoVerticesSource, PerInstance};
use program::Program;
use transform_feedback;
use vertex::{Vertex, VerticesSource, IntoVerticesSource};
use vertex::format::VertexFormat;

use BufferExt;
use GlObject;

use backend::Facade;
use version::{Api, Version};

use gl;
use sync;

/// A list of vertices loaded in the graphics card's memory.
#[derive(Debug)]
pub struct VertexBuffer<T> {
    buffer: VertexBufferAny,
    marker: PhantomData<T>,
}

/// Represents a slice of a `VertexBuffer`.
pub struct VertexBufferSlice<'b, T: 'b> {
    buffer: &'b VertexBuffer<T>,
    offset: usize,
    length: usize,
}

impl<T: Vertex + 'static + Send> VertexBuffer<T> {
    /// Builds a new vertex buffer.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # #[macro_use]
    /// # extern crate glium;
    /// # extern crate glutin;
    /// # fn main() {
    /// #[derive(Copy, Clone)]
    /// struct Vertex {
    ///     position: [f32; 3],
    ///     texcoords: [f32; 2],
    /// }
    ///
    /// implement_vertex!(Vertex, position, texcoords);
    ///
    /// # let display: glium::Display = unsafe { ::std::mem::uninitialized() };
    /// let vertex_buffer = glium::VertexBuffer::new(&display, vec![
    ///     Vertex { position: [0.0,  0.0, 0.0], texcoords: [0.0, 1.0] },
    ///     Vertex { position: [5.0, -3.0, 2.0], texcoords: [1.0, 0.0] },
    /// ]);
    /// # }
    /// ```
    ///
    pub fn new<F>(facade: &F, data: Vec<T>) -> VertexBuffer<T> where F: Facade {
        let bindings = <T as Vertex>::build_bindings();

        let buffer = Buffer::new(facade, &data, BufferType::ArrayBuffer,
                                 BufferFlags::simple()).unwrap();
        let elements_size = buffer.get_elements_size();

        VertexBuffer {
            buffer: VertexBufferAny {
                buffer: buffer,
                bindings: bindings,
                elements_size: elements_size,
            },
            marker: PhantomData,
        }
    }

    /// Builds a new vertex buffer.
    ///
    /// This function will create a buffer that has better performance when it is modified frequently.
    pub fn new_dynamic<F>(facade: &F, data: Vec<T>) -> VertexBuffer<T> where F: Facade {
        let bindings = <T as Vertex>::build_bindings();

        let buffer = Buffer::new(facade, &data, BufferType::ArrayBuffer,
                                 BufferFlags::simple()).unwrap();
        let elements_size = buffer.get_elements_size();

        VertexBuffer {
            buffer: VertexBufferAny {
                buffer: buffer,
                bindings: bindings,
                elements_size: elements_size,
            },
            marker: PhantomData,
        }
    }

    /// Builds a new vertex buffer with persistent mapping.
    ///
    /// ## Features
    ///
    /// Only available if the `gl_persistent_mapping` feature is enabled.
    #[cfg(feature = "gl_persistent_mapping")]
    pub fn new_persistent<F>(facade: &F, data: Vec<T>) -> VertexBuffer<T> where F: Facade {
        VertexBuffer::new_persistent_if_supported(facade, data).unwrap()
    }

    /// Builds a new vertex buffer with persistent mapping, or `None` if this is not supported.
    pub fn new_persistent_if_supported<F>(facade: &F, data: Vec<T>)
                                          -> Option<VertexBuffer<T>>
                                          where F: Facade
    {
        let bindings = <T as Vertex>::build_bindings();

        let buffer = match Buffer::new(facade, &data, BufferType::ArrayBuffer,
                                       BufferFlags::persistent())
        {
            Err(BufferCreationError::PersistentMappingNotSupported) => return None,
            b => b.unwrap()
        };

        let elements_size = buffer.get_elements_size();

        Some(VertexBuffer {
            buffer: VertexBufferAny {
                buffer: buffer,
                bindings: bindings,
                elements_size: elements_size,
            },
            marker: PhantomData,
        })
    }

    /// Borrows the vertex buffer to start transform feedback on it.
    pub fn as_transform_feedback_session_if_supported<'a>(&'a mut self, program: &'a Program)
                            -> Option<transform_feedback::TransformFeedbackSession<'a>>
    {
        transform_feedback::new_session(self.buffer.buffer.get_display(), &mut self.buffer.buffer,
                                        &self.buffer.bindings, program)
    }
}

impl<T: Send + Copy + 'static> VertexBuffer<T> {
    /// Builds a new vertex buffer from an indeterminate data type and bindings.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # extern crate glium;
    /// # extern crate glutin;
    /// # fn main() {
    /// use std::borrow::Cow;
    ///
    /// let bindings = vec![(
    ///         Cow::Borrowed("position"), 0,
    ///         glium::vertex::AttributeType::F32F32,
    ///     ), (
    ///         Cow::Borrowed("color"), 2 * ::std::mem::size_of::<f32>(),
    ///         glium::vertex::AttributeType::F32,
    ///     ),
    /// ];
    ///
    /// # let display: glium::Display = unsafe { ::std::mem::uninitialized() };
    /// let data = vec![
    ///     1.0, -0.3, 409.0,
    ///     -0.4, 2.8, 715.0f32
    /// ];
    ///
    /// let vertex_buffer = unsafe {
    ///     glium::VertexBuffer::new_raw(&display, data, bindings, 3 * ::std::mem::size_of::<f32>())
    /// };
    /// # }
    /// ```
    ///
    pub unsafe fn new_raw<F>(facade: &F, data: Vec<T>,
                             bindings: VertexFormat, elements_size: usize) -> VertexBuffer<T>
                             where F: Facade
    {
        VertexBuffer {
            buffer: VertexBufferAny {
                buffer: Buffer::new(facade, &data, BufferType::ArrayBuffer,
                                    BufferFlags::simple()).unwrap(),
                bindings: bindings,
                elements_size: elements_size,
            },
            marker: PhantomData,
        }
    }

    /// Accesses a slice of the buffer.
    ///
    /// Returns `None` if the slice is out of range.
    pub fn slice(&self, offset: usize, len: usize) -> Option<VertexBufferSlice<T>> {
        if offset > self.len() || offset + len > self.len() {
            return None;
        }

        Some(VertexBufferSlice {
            buffer: self,
            offset: offset,
            length: len
        })
    }

    /// Maps the buffer to allow write access to it.
    ///
    /// This function will block until the buffer stops being used by the backend.
    /// This operation is much faster if the buffer is persistent.
    pub fn map<'a>(&'a mut self) -> Mapping<'a, T> {
        let len = self.buffer.buffer.get_elements_count();
        let mapping = self.buffer.buffer.map(0, len);
        Mapping(mapping)
    }

    /// Reads the content of the buffer.
    ///
    /// This function is usually better if are just doing one punctual read, while `map`
    /// is better if you want to have multiple small reads.
    ///
    /// # Features
    ///
    /// Only available if the `gl_read_buffer` feature is enabled.
    #[cfg(feature = "gl_read_buffer")]
    pub fn read(&self) -> Vec<T> {
        self.buffer.buffer.read()
    }

    /// Reads the content of the buffer.
    ///
    /// This function is usually better if are just doing one punctual read, while `map`
    /// is better if you want to have multiple small reads.
    pub fn read_if_supported(&self) -> Option<Vec<T>> {
        self.buffer.buffer.read_if_supported()
    }

    /// Replaces the content of the buffer.
    ///
    /// ## Panic
    ///
    /// Panics if the length of `data` is different from the length of this buffer.
    pub fn write(&self, data: Vec<T>) {
        assert!(data.len() == self.len());
        self.buffer.buffer.upload(0, data)
    }
}

impl<T> VertexBuffer<T> {
    /// Returns true if the buffer is mapped in a permanent way in memory.
    pub fn is_persistent(&self) -> bool {
        self.buffer.buffer.is_persistent()
    }

    /// Returns the number of bytes between two consecutive elements in the buffer.
    pub fn get_elements_size(&self) -> usize {
        self.buffer.elements_size
    }

    /// Returns the associated `VertexFormat`.
    pub fn get_bindings(&self) -> &VertexFormat {
        &self.buffer.bindings
    }

    /// Discard the type information and turn the vertex buffer into a `VertexBufferAny`.
    pub fn into_vertex_buffer_any(self) -> VertexBufferAny {
        self.buffer
    }

    /// Returns the number of elements in the buffer.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Creates a marker that instructs glium to use multiple instances.
    ///
    /// Instead of calling `surface.draw(&vertex_buffer, ...)` you can call
    /// `surface.draw(vertex_buffer.per_instance(), ...)`. This will draw one instance of the
    /// geometry for each element in this buffer. The attributes are still passed to the
    /// vertex shader, but each entry is passed for each different instance.
    ///
    /// Returns `None` if the backend doesn't support instancing.
    pub fn per_instance_if_supported(&self) -> Option<PerInstance> {
        if self.buffer.buffer.get_context().get_version() < &Version(Api::Gl, 3, 3) &&
            !self.buffer.buffer.get_context().get_extensions().gl_arb_instanced_arrays
        {
            return None;
        }

        Some(PerInstance(VertexBufferAnySlice { buffer: &self.buffer, offset: 0, length: self.len() }))
    }

    /// Creates a marker that instructs glium to use multiple instances.
    ///
    /// Instead of calling `surface.draw(&vertex_buffer, ...)` you can call
    /// `surface.draw(vertex_buffer.per_instance(), ...)`. This will draw one instance of the
    /// geometry for each element in this buffer. The attributes are still passed to the
    /// vertex shader, but each entry is passed for each different instance.
    ///
    /// # Features
    ///
    /// Only available if the `gl_instancing` feature is enabled.
    #[cfg(feature = "gl_instancing")]
    pub fn per_instance(&self) -> PerInstance {
        self.per_instance_if_supported().unwrap()
    }
}

impl<T> GlObject for VertexBuffer<T> {
    type Id = gl::types::GLuint;
    fn get_id(&self) -> gl::types::GLuint {
        self.buffer.get_id()
    }
}

impl<T> BufferExt for VertexBuffer<T> {
    fn add_fence(&self) -> Option<Sender<sync::LinearSyncFence>> {
        self.buffer.add_fence()
    }
}

impl<'a, T> IntoVerticesSource<'a> for &'a VertexBuffer<T> {
    fn into_vertices_source(self) -> VerticesSource<'a> {
        (&self.buffer).into_vertices_source()
    }
}

impl<'b, T> VertexBufferSlice<'b, T> where T: Send + Copy + 'static {
    /// Reads the content of the slice.
    ///
    /// This function is usually better if are just doing one punctual read, while `map`
    /// is better if you want to have multiple small reads.
    ///
    /// # Features
    ///
    /// Only available if the `gl_read_buffer` feature is enabled.
    #[cfg(feature = "gl_read_buffer")]
    pub fn read(&self) -> Vec<T> {
        self.buffer.buffer.buffer.read_slice(self.offset, self.length)
    }

    /// Reads the content of the buffer.
    ///
    /// This function is usually better if are just doing one punctual read, while `map`
    /// is better if you want to have multiple small reads.
    pub fn read_if_supported(&self) -> Option<Vec<T>> {
        self.buffer.buffer.buffer.read_slice_if_supported(self.offset, self.length)
    }

    /// Writes some vertices to the buffer.
    ///
    /// ## Panic
    ///
    /// Panics if the length of `data` is different from the length of this slice.
    pub fn write(&self, data: Vec<T>) {
        assert!(data.len() == self.length);
        self.buffer.buffer.buffer.upload(self.offset, data)
    }
}

impl<'a, T> BufferExt for VertexBufferSlice<'a, T> {
    fn add_fence(&self) -> Option<Sender<sync::LinearSyncFence>> {
        self.buffer.add_fence()
    }
}

impl<'a, T> IntoVerticesSource<'a> for VertexBufferSlice<'a, T> {
    fn into_vertices_source(self) -> VerticesSource<'a> {
        VerticesSource::VertexBuffer(&self.buffer.buffer, self.offset, self.length, false)
    }
}

/// A list of vertices loaded in the graphics card's memory.
///
/// Contrary to `VertexBuffer`, this struct doesn't know about the type of data
/// inside the buffer. Therefore you can't map or read it.
///
/// This struct is provided for convenience, so that you can have a `Vec<VertexBufferAny>`,
/// or return a `VertexBufferAny` instead of a `VertexBuffer<MyPrivateVertexType>`.
#[derive(Debug)]
pub struct VertexBufferAny {
    buffer: Buffer,
    bindings: VertexFormat,
    elements_size: usize,
}

/// Represents a slice of a `VertexBufferAny`.
pub struct VertexBufferAnySlice<'b> {
    buffer: &'b VertexBufferAny,
    offset: usize,
    length: usize,
}

impl VertexBufferAny {
    /// Returns the number of bytes between two consecutive elements in the buffer.
    pub fn get_elements_size(&self) -> usize {
        self.elements_size
    }

    /// Returns the number of elements in the buffer.
    pub fn len(&self) -> usize {
        self.buffer.get_elements_count()
    }

    /// Returns the associated `VertexFormat`.
    pub fn get_bindings(&self) -> &VertexFormat {
        &self.bindings
    }

    /// Turns the vertex buffer into a `VertexBuffer` without checking the type.
    pub unsafe fn into_vertex_buffer<T>(self) -> VertexBuffer<T> {
        VertexBuffer {
            buffer: self,
            marker: PhantomData,
        }
    }

    /// Accesses a slice of the buffer.
    ///
    /// Returns `None` if the slice is out of range.
    pub fn slice(&self, offset: usize, len: usize) -> Option<VertexBufferAnySlice> {
        if offset >= self.len() || offset + len >= self.len() {
            return None;
        }

        Some(VertexBufferAnySlice {
            buffer: self,
            offset: offset,
            length: len
        })
    }
}

impl BufferExt for VertexBufferAny {
    fn add_fence(&self) -> Option<Sender<sync::LinearSyncFence>> {
        self.buffer.add_fence()
    }
}

impl GlObject for VertexBufferAny {
    type Id = gl::types::GLuint;
    fn get_id(&self) -> gl::types::GLuint {
        self.buffer.get_id()
    }
}

impl<'a> IntoVerticesSource<'a> for &'a VertexBufferAny {
    fn into_vertices_source(self) -> VerticesSource<'a> {
        VerticesSource::VertexBuffer(self, 0, self.len(), false)
    }
}

impl<'a> IntoVerticesSource<'a> for VertexBufferAnySlice<'a> {
    fn into_vertices_source(self) -> VerticesSource<'a> {
        VerticesSource::VertexBuffer(self.buffer, self.offset, self.length, false)
    }
}

/// A mapping of a buffer.
pub struct Mapping<'a, T>(buffer::Mapping<'a, T>);

impl<'a, T> Deref for Mapping<'a, T> {
    type Target = [T];
    fn deref<'b>(&'b self) -> &'b [T] {
        self.0.deref()
    }
}

impl<'a, T> DerefMut for Mapping<'a, T> {
    fn deref_mut<'b>(&'b mut self) -> &'b mut [T] {
        self.0.deref_mut()
    }
}
