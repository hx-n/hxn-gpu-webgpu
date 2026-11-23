//! WebGPU for Haxe in the browser, keeping WebGPU's conventions. Shaders are hxnsl's WGSL,
//! which the browser compiles, with bind group layouts from hxnsl's `LAYOUT`. WebGPU's
//! asynchronous calls (`requestAdapter`, `requestDevice`, `mapAsync`, `onSubmittedWorkDone`)
//! return hxn futures, resolved when the browser's promises resolve.
//!
//! Two backends, by how the program reaches the page: without threads, webgpu.h as Emscripten's
//! emdawnwebgpu port implements it; with threads, under Ash or Emscripten, a wire of commands in
//! shared memory, served by a browser agent the web page starts through `ash_host_agent`. This file is the API
//! both share, and the one hxn generates the Haxe classes from.

#![cfg(any(
    target_os = "emscripten",
    all(target_os = "wasi", target_feature = "atomics")
))]
// The browser agent backend's wire waits and notifies with wasm's atomics.
#![cfg_attr(target_feature = "atomics", feature(stdarch_wasm_atomic_wait))]

// Without threads, Emscripten's webgpu.h on the page's thread.
#[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
mod ffi;

#[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
#[path = "emscripten.rs"]
mod backend;

// With threads, under Ash or Emscripten, a browser agent in a Worker of its own, so the
// program's thread can wait for what the browser settles.
#[cfg(target_feature = "atomics")]
#[path = "agent/mod.rs"]
mod backend;

use hxn_future::Future;

hxn::package!("hxn.gpu.webgpu");

/// How a pipeline blends what it draws into its color target, as `GPUBlendState`. The factors
/// are hxn-gpu's shared codes, in the order of `GPUBlendFactor` from 0 `"zero"` to 10
/// `"src-alpha-saturated"`, and the operations in the order of `GPUBlendOperation` from 0
/// `"add"` to 4 `"max"`. With `enabled` false, the pipeline has no blend state.
#[hxn::data]
pub struct Blend {
    pub enabled: bool,
    pub color_operation: i32,
    pub color_source: i32,
    pub color_destination: i32,
    pub alpha_operation: i32,
    pub alpha_source: i32,
    pub alpha_destination: i32,
}

/// Checks the codes of a blend: factors 0 to 10, operations 0 to 4.
pub(crate) fn check_blend(blend: &Blend) -> Result<(), String> {
    for (code, what, top) in [
        (blend.color_operation, "the color operation", 4),
        (blend.color_source, "the color source", 10),
        (blend.color_destination, "the color destination", 10),
        (blend.alpha_operation, "the alpha operation", 4),
        (blend.alpha_source, "the alpha source", 10),
        (blend.alpha_destination, "the alpha destination", 10),
    ] {
        if !(0..=top).contains(&code) {
            return Err(format!("{what} is {code}, and it is 0 to {top}"));
        }
    }
    Ok(())
}

/// A texture format, webgpu.h's values.
#[hxn::data]
#[derive(Clone, Copy)]
pub enum Format {
    R8Unorm = 1,
    R32Float = 14,
    R32Uint = 15,
    Rgba8Unorm = 22,
    Rgba8UnormSrgb = 23,
    Bgra8Unorm = 27,
    Bgra8UnormSrgb = 28,
    Rg32Float = 33,
    Rgba16Float = 40,
    Rgba32Float = 41,
    Depth24Plus = 46,
    Depth32Float = 48,
}

/// A vertex attribute's format, webgpu.h's values.
#[hxn::data]
#[derive(Clone, Copy)]
pub enum VertexFormat {
    Unorm8x4 = 9,
    Float32 = 28,
    Float32x2 = 29,
    Float32x3 = 30,
    Float32x4 = 31,
    Uint32 = 32,
    Uint32x2 = 33,
    Uint32x4 = 35,
    Sint32 = 36,
}

/// `GPUBufferUsage` bits, webgpu.h's values, or-ed together.
#[hxn::data]
#[derive(Clone, Copy)]
pub enum BufferUsage {
    MapRead = 1,
    MapWrite = 2,
    CopySrc = 4,
    CopyDst = 8,
    Index = 16,
    Vertex = 32,
    Uniform = 64,
    Storage = 128,
}

/// `GPUTextureUsage` bits, webgpu.h's values, or-ed together.
#[hxn::data]
#[derive(Clone, Copy)]
pub enum TextureUsage {
    CopySrc = 1,
    CopyDst = 2,
    TextureBinding = 4,
    StorageBinding = 8,
    RenderAttachment = 16,
}

/// `GPUShaderStage` bits, which WebGPU and hxnsl's `LAYOUT` share.
#[hxn::data]
#[derive(Clone, Copy)]
pub enum ShaderStage {
    Vertex = 1,
    Fragment = 2,
    Compute = 4,
}

/// What a bind group layout entry binds.
#[hxn::data]
#[derive(Clone, Copy, PartialEq)]
pub enum BindingKind {
    Uniform,
    Storage,
    ReadOnlyStorage,
    Sampler,
    NonFilteringSampler,
    ComparisonSampler,
    Texture,
    StorageTexture,
}

/// A texture binding's sample type, webgpu.h's values.
#[hxn::data]
#[derive(Clone, Copy)]
pub enum SampleType {
    Float = 2,
    UnfilterableFloat = 3,
    Depth = 4,
    Sint = 5,
    Uint = 6,
}

/// A texture binding's view dimension, webgpu.h's values.
#[hxn::data]
#[derive(Clone, Copy)]
pub enum ViewDimension {
    D1 = 1,
    D2 = 2,
    D2Array = 3,
    Cube = 4,
    CubeArray = 5,
    D3 = 6,
}

/// A storage texture binding's access, webgpu.h's values.
#[hxn::data]
#[derive(Clone, Copy)]
pub enum StorageAccess {
    WriteOnly = 2,
    ReadOnly = 3,
    ReadWrite = 4,
}

/// A render pipeline's primitive topology, webgpu.h's values.
#[hxn::data]
#[derive(Clone, Copy)]
pub enum Topology {
    PointList = 1,
    LineList = 2,
    LineStrip = 3,
    TriangleList = 4,
    TriangleStrip = 5,
}

/// A sampler's filter, webgpu.h's values.
#[hxn::data]
#[derive(Clone, Copy)]
pub enum Filter {
    Nearest = 1,
    Linear = 2,
}

/// A sampler's address mode, webgpu.h's values.
#[hxn::data]
#[derive(Clone, Copy)]
pub enum AddressMode {
    ClampToEdge = 1,
    Repeat = 2,
    MirrorRepeat = 3,
}

/// One entry of a bind group layout. `sample_type`, `view_dimension`, `multisampled`,
/// `format` and `access` apply to texture and storage texture entries.
#[hxn::data]
#[derive(Clone)]
pub struct LayoutEntry {
    pub binding: i32,
    /// `ShaderStage` bits.
    pub visibility: i32,
    pub kind: BindingKind,
    pub sample_type: SampleType,
    pub view_dimension: ViewDimension,
    pub multisampled: bool,
    pub format: Format,
    pub access: StorageAccess,
}

/// The layout of one bind group.
#[hxn::data]
#[derive(Clone)]
pub struct GroupLayout {
    pub entries: Vec<LayoutEntry>,
}

/// A pipeline-overridable constant: a WGSL `override` by its name, as hxnsl's `CONST_` keys
/// give it.
#[hxn::data]
#[derive(Clone)]
pub struct Constant {
    pub key: String,
    pub value: f64,
}

/// One attribute of a vertex buffer: `buffer` is its index among the pipeline's buffers.
#[hxn::data]
#[derive(Clone)]
pub struct VertexAttribute {
    pub location: i32,
    pub format: VertexFormat,
    pub offset: i32,
    pub buffer: i32,
}

/// One vertex buffer of a render pipeline: `stride` bytes per vertex, or per instance when
/// `per_instance`.
#[hxn::data]
#[derive(Clone)]
pub struct VertexBuffer {
    pub stride: i32,
    pub per_instance: bool,
}

/// A WebGPU instance: `navigator.gpu`.
#[hxn::interface]
pub struct Instance {
    inner: backend::Instance,
}

#[hxn::methods]
impl Instance {
    pub fn create() -> Result<Instance, String> {
        hxn::log::debug!(target: "hxn.gpu.webgpu", "opening browser WebGPU");
        Ok(Instance {
            inner: backend::Instance::create()?,
        })
    }

    /// The browser's adapter, its default for power preference.
    pub fn request_adapter(&self) -> Future<Adapter> {
        self.inner.request_adapter()
    }

    /// Takes the web page's canvas for this program's WebGPU, which `createCanvasSurface`
    /// then renders into. It fails when the page has no canvas or another browser agent holds
    /// it. Without threads the program holds the canvas, and it settles at once.
    pub fn claim_canvas(&self) -> Future<()> {
        self.inner.claim_canvas()
    }

    /// The web page's canvas, as a surface to render frames into.
    pub fn create_canvas_surface(&self) -> Result<Surface, String> {
        Ok(Surface {
            inner: self.inner.create_canvas_surface()?,
        })
    }
}

/// The web page's canvas. Render each frame into `currentTexture`, then show it with `present`.
#[hxn::interface]
pub struct Surface {
    inner: backend::Surface,
}

#[hxn::methods]
impl Surface {
    /// The format the browser prefers for the canvas, `Bgra8Unorm` or `Rgba8Unorm`.
    pub fn preferred_format(&self, adapter: &Adapter) -> Format {
        self.inner.preferred_format(&adapter.inner)
    }

    /// Sizes the canvas to `width` by `height` and has `device` render into it, in `format`,
    /// for `usage` (`TextureUsage` bits).
    pub fn configure(
        &self,
        device: &Device,
        format: Format,
        usage: i32,
        width: i32,
        height: i32,
    ) -> Result<(), String> {
        self.inner
            .configure(&device.inner, format, usage, width, height)
    }

    /// This frame's texture, the same one until `present`.
    pub fn current_texture(&self) -> Result<Texture, String> {
        Ok(Texture {
            inner: self.inner.current_texture()?,
        })
    }

    /// Shows the frame rendered into `currentTexture`, after the work submitted before.
    pub fn present(&self) {
        self.inner.present()
    }
}

/// A `GPUAdapter`.
#[hxn::interface]
pub struct Adapter {
    pub(crate) inner: backend::Adapter,
}

#[hxn::methods]
impl Adapter {
    /// The adapter's vendor, architecture and description, as the browser reports them.
    pub fn info(&self) -> String {
        self.inner.info()
    }

    /// A device with the default limits and no extra features.
    pub fn request_device(&self) -> Future<Device> {
        self.inner.request_device()
    }
}

/// A `GPUDevice` and its queue.
#[hxn::interface]
pub struct Device {
    pub(crate) inner: backend::Device,
}

#[hxn::methods]
impl Device {
    /// A buffer of `size` bytes, a multiple of 4, for `usage` (`BufferUsage` bits).
    pub fn create_buffer(&self, size: i64, usage: i32) -> Result<Buffer, String> {
        Ok(Buffer {
            inner: self.inner.create_buffer(size, usage)?,
        })
    }

    /// A buffer holding `bytes`, padded with zeros to a multiple of 4, for `usage`
    /// (`BufferUsage` bits).
    pub fn create_buffer_with(&self, bytes: &[u8], usage: i32) -> Result<Buffer, String> {
        Ok(Buffer {
            inner: self.inner.create_buffer_with(bytes, usage)?,
        })
    }

    /// A 2D texture of one level, for `usage` (`TextureUsage` bits), with one view of all of it.
    pub fn create_texture(
        &self,
        width: i32,
        height: i32,
        format: Format,
        usage: i32,
    ) -> Result<Texture, String> {
        Ok(Texture {
            inner: self.inner.create_texture(width, height, format, usage)?,
        })
    }

    /// A sampler filtering with `filter` for magnification, minification and mipmaps, and
    /// addressing with `address` on every axis.
    pub fn create_sampler(&self, filter: Filter, address: AddressMode) -> Result<Sampler, String> {
        Ok(Sampler {
            inner: self.inner.create_sampler(filter, address)?,
        })
    }

    /// A shader module of WGSL source.
    pub fn create_shader_module(&self, wgsl: &str) -> Result<ShaderModule, String> {
        Ok(ShaderModule {
            inner: self.inner.create_shader_module(wgsl)?,
        })
    }

    /// A bind group layout for each of `groups`, group 0 first.
    pub fn create_bind_group_layouts(
        &self,
        groups: Vec<GroupLayout>,
    ) -> Result<BindGroupLayouts, String> {
        Ok(BindGroupLayouts {
            inner: self.inner.create_bind_group_layouts(groups)?,
        })
    }

    /// A pipeline layout of `layouts`.
    pub fn create_pipeline_layout(
        &self,
        layouts: &BindGroupLayouts,
    ) -> Result<PipelineLayout, String> {
        Ok(PipelineLayout {
            inner: self.inner.create_pipeline_layout(&layouts.inner)?,
        })
    }

    /// A compute pipeline running `entry` of `module`, with `constants` set.
    pub fn create_compute_pipeline(
        &self,
        module: &ShaderModule,
        entry: &str,
        layout: &PipelineLayout,
        constants: Vec<Constant>,
    ) -> Result<ComputePipeline, String> {
        Ok(ComputePipeline {
            inner: self.inner.create_compute_pipeline(
                &module.inner,
                entry,
                &layout.inner,
                constants,
            )?,
        })
    }

    /// Creates a render pipeline drawing `topology` into one color target of `format`, from
    /// the vertex `buffers` that `attributes` are read from, with `constants` set in both
    /// stages, blending as `blend` describes.
    #[allow(clippy::too_many_arguments)]
    pub fn create_render_pipeline(
        &self,
        module: &ShaderModule,
        vertex_entry: &str,
        fragment_entry: &str,
        layout: &PipelineLayout,
        format: Format,
        topology: Topology,
        buffers: Vec<VertexBuffer>,
        attributes: Vec<VertexAttribute>,
        constants: Vec<Constant>,
        blend: Blend,
    ) -> Result<RenderPipeline, String> {
        check_blend(&blend)?;
        if let Some(a) = attributes
            .iter()
            .find(|a| a.buffer < 0 || a.buffer as usize >= buffers.len())
        {
            return Err(format!(
                "attribute {} reads buffer {} of {}",
                a.location,
                a.buffer,
                buffers.len()
            ));
        }
        Ok(RenderPipeline {
            inner: self.inner.create_render_pipeline(
                &module.inner,
                vertex_entry,
                fragment_entry,
                &layout.inner,
                format,
                topology,
                buffers,
                attributes,
                constants,
                blend,
            )?,
        })
    }

    /// Bind group `group` of `layouts`, of the resources `entries` holds.
    pub fn create_bind_group(
        &self,
        layouts: &BindGroupLayouts,
        group: i32,
        entries: &BindGroupEntries,
    ) -> Result<BindGroup, String> {
        Ok(BindGroup {
            inner: self
                .inner
                .create_bind_group(&layouts.inner, group, &entries.inner)?,
        })
    }

    /// An empty command encoder.
    pub fn create_command_encoder(&self) -> Result<CommandEncoder, String> {
        Ok(CommandEncoder {
            inner: self.inner.create_command_encoder()?,
        })
    }

    /// Finishes `encoder` and submits its commands to the queue.
    pub fn submit(&self, encoder: &mut CommandEncoder) -> Result<(), String> {
        self.inner.submit(&mut encoder.inner)
    }

    /// Writes `bytes`, a multiple of 4, at `offset` in `buffer`, in queue order.
    pub fn write_buffer(&self, buffer: &Buffer, offset: i64, bytes: &[u8]) -> Result<(), String> {
        self.inner.write_buffer(&buffer.inner, offset, bytes)
    }

    /// Writes all of `texture` from `bytes`, rows `bytes_per_row` apart, in queue order.
    pub fn write_texture(
        &self,
        texture: &Texture,
        bytes: &[u8],
        bytes_per_row: i32,
    ) -> Result<(), String> {
        let (width, height) = (texture.width(), texture.height());
        self.inner
            .write_texture(&texture.inner, 0, 0, width, height, bytes, bytes_per_row)
    }

    /// Writes the `width` by `height` pixels at `x`, `y` of `texture` from `bytes`, rows
    /// `bytes_per_row` apart, in queue order.
    #[allow(clippy::too_many_arguments)]
    pub fn write_texture_region(
        &self,
        texture: &Texture,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        bytes: &[u8],
        bytes_per_row: i32,
    ) -> Result<(), String> {
        self.inner
            .write_texture(&texture.inner, x, y, width, height, bytes, bytes_per_row)
    }

    /// Settles when the GPU has finished the work submitted so far.
    pub fn completed(&self) -> Future<()> {
        self.inner.completed()
    }
}

/// A `GPUBuffer`.
#[hxn::interface]
pub struct Buffer {
    inner: backend::Buffer,
}

#[hxn::methods]
impl Buffer {
    pub fn size(&self) -> i64 {
        self.inner.size()
    }

    /// Maps `length` bytes at `offset` for reading, copies them out and unmaps the buffer.
    /// The buffer needs `BufferUsage.MapRead`; `offset` is a multiple of 8, `length` of 4.
    pub fn read(&self, offset: i64, length: i64) -> Result<Future<Vec<u8>>, String> {
        self.inner.read(offset, length)
    }
}

/// A 2D `GPUTexture` of one level, with one view of all of it.
#[hxn::interface]
pub struct Texture {
    inner: backend::Texture,
}

#[hxn::methods]
impl Texture {
    pub fn width(&self) -> i32 {
        self.inner.width()
    }

    pub fn height(&self) -> i32 {
        self.inner.height()
    }
}

/// A `GPUSampler`.
#[hxn::interface]
pub struct Sampler {
    inner: backend::Sampler,
}

#[hxn::methods]
impl Sampler {}

/// A `GPUShaderModule` of WGSL.
#[hxn::interface]
pub struct ShaderModule {
    inner: backend::ShaderModule,
}

#[hxn::methods]
impl ShaderModule {}

/// The bind group layouts of a pipeline, group 0 first.
#[hxn::interface]
pub struct BindGroupLayouts {
    inner: backend::BindGroupLayouts,
}

#[hxn::methods]
impl BindGroupLayouts {
    /// The number of groups.
    pub fn count(&self) -> i32 {
        self.inner.count()
    }
}

/// A `GPUPipelineLayout`.
#[hxn::interface]
pub struct PipelineLayout {
    inner: backend::PipelineLayout,
}

#[hxn::methods]
impl PipelineLayout {}

/// A `GPUComputePipeline`.
#[hxn::interface]
pub struct ComputePipeline {
    inner: backend::ComputePipeline,
}

#[hxn::methods]
impl ComputePipeline {}

/// A `GPURenderPipeline`.
#[hxn::interface]
pub struct RenderPipeline {
    inner: backend::RenderPipeline,
}

#[hxn::methods]
impl RenderPipeline {}

/// The resources of a bind group, by binding, before `Device.createBindGroup` makes it.
#[hxn::interface]
pub struct BindGroupEntries {
    inner: backend::BindGroupEntries,
}

#[hxn::methods]
impl BindGroupEntries {
    pub fn create() -> BindGroupEntries {
        BindGroupEntries {
            inner: backend::BindGroupEntries::create(),
        }
    }

    /// `size` bytes of `buffer` from `offset` at `binding`; a negative `size` binds the rest.
    pub fn buffer(
        &mut self,
        binding: i32,
        buffer: &Buffer,
        offset: i64,
        size: i64,
    ) -> Result<(), String> {
        self.inner.buffer(binding, &buffer.inner, offset, size)
    }

    /// The view of `texture` at `binding`.
    pub fn texture(&mut self, binding: i32, texture: &Texture) -> Result<(), String> {
        self.inner.texture(binding, &texture.inner)
    }

    /// `sampler` at `binding`.
    pub fn sampler(&mut self, binding: i32, sampler: &Sampler) -> Result<(), String> {
        self.inner.sampler(binding, &sampler.inner)
    }
}

/// A `GPUBindGroup`.
#[hxn::interface]
pub struct BindGroup {
    inner: backend::BindGroup,
}

#[hxn::methods]
impl BindGroup {}

/// A `GPUCommandEncoder`, recording until `Device.submit`.
#[hxn::interface]
pub struct CommandEncoder {
    inner: backend::CommandEncoder,
}

#[hxn::methods]
impl CommandEncoder {
    /// A compute pass, recording until its `end`.
    pub fn begin_compute_pass(&mut self) -> Result<ComputePass, String> {
        Ok(ComputePass {
            inner: self.inner.begin_compute_pass()?,
        })
    }

    /// Begins a render pass into `target`, cleared first to the color `r`, `g`, `b`, `a`, or
    /// with `keep` drawing over what it holds (`loadOp: "load"`).
    pub fn begin_render_pass(
        &mut self,
        target: &Texture,
        keep: bool,
        r: f64,
        g: f64,
        b: f64,
        a: f64,
    ) -> Result<RenderPass, String> {
        Ok(RenderPass {
            inner: self
                .inner
                .begin_render_pass(&target.inner, keep, r, g, b, a)?,
        })
    }

    /// Copies `size` bytes from `source` at `source_offset` to `destination` at
    /// `destination_offset`.
    pub fn copy_buffer_to_buffer(
        &mut self,
        source: &Buffer,
        source_offset: i64,
        destination: &Buffer,
        destination_offset: i64,
        size: i64,
    ) -> Result<(), String> {
        self.inner.copy_buffer_to_buffer(
            &source.inner,
            source_offset,
            &destination.inner,
            destination_offset,
            size,
        )
    }

    /// Copies all of `texture` into `buffer`, rows `bytes_per_row` apart, a multiple of 256.
    pub fn copy_texture_to_buffer(
        &mut self,
        texture: &Texture,
        buffer: &Buffer,
        bytes_per_row: i32,
    ) -> Result<(), String> {
        self.inner
            .copy_texture_to_buffer(&texture.inner, &buffer.inner, bytes_per_row)
    }
}

/// A `GPUComputePassEncoder`.
#[hxn::interface]
pub struct ComputePass {
    inner: backend::ComputePass,
}

#[hxn::methods]
impl ComputePass {
    pub fn set_pipeline(&mut self, pipeline: &ComputePipeline) {
        self.inner.set_pipeline(&pipeline.inner)
    }

    pub fn set_bind_group(&mut self, index: i32, group: &BindGroup) -> Result<(), String> {
        self.inner.set_bind_group(index, &group.inner)
    }

    pub fn dispatch(&mut self, x: i32, y: i32, z: i32) -> Result<(), String> {
        self.inner.dispatch(x, y, z)
    }

    /// Ends the pass; the encoder records again after it.
    pub fn end(&mut self) -> Result<(), String> {
        self.inner.end()
    }
}

/// A `GPURenderPassEncoder`.
#[hxn::interface]
pub struct RenderPass {
    inner: backend::RenderPass,
}

#[hxn::methods]
impl RenderPass {
    pub fn set_pipeline(&mut self, pipeline: &RenderPipeline) {
        self.inner.set_pipeline(&pipeline.inner)
    }

    pub fn set_bind_group(&mut self, index: i32, group: &BindGroup) -> Result<(), String> {
        self.inner.set_bind_group(index, &group.inner)
    }

    /// Reads vertices for `slot` from `buffer`, starting `offset` bytes in.
    pub fn set_vertex_buffer(
        &mut self,
        slot: i32,
        buffer: &Buffer,
        offset: i64,
    ) -> Result<(), String> {
        self.inner.set_vertex_buffer(slot, &buffer.inner, offset)
    }

    /// Maps clip space onto the `width` by `height` pixels at `x`, `y` of the target.
    pub fn set_viewport(&mut self, x: f64, y: f64, width: f64, height: f64) -> Result<(), String> {
        self.inner.set_viewport(x, y, width, height)
    }

    /// Draws only inside the `width` by `height` pixels at `x`, `y` of the target.
    pub fn set_scissor_rect(
        &mut self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Result<(), String> {
        self.inner.set_scissor_rect(x, y, width, height)
    }

    pub fn draw(
        &mut self,
        vertices: i32,
        instances: i32,
        first_vertex: i32,
        first_instance: i32,
    ) -> Result<(), String> {
        self.inner
            .draw(vertices, instances, first_vertex, first_instance)
    }

    /// Ends the pass; the encoder records again after it.
    pub fn end(&mut self) -> Result<(), String> {
        self.inner.end()
    }
}
