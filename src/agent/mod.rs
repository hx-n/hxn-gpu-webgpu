//! The backend for a threaded Ash wasm library. Each call is encoded onto the wire (`wire.rs`,
//! generated from WebGPU's IDL by x-idl) into one batch, which the browser agent runs on
//! `navigator.gpu` in a Worker of its own; the browser agent is `page/gpu_webgpu.mjs`, which the web page
//! starts when `Instance.create` asks for it through `ash_host_agent`. Objects are kept by the
//! browser agent under handles this library picks, so making one needs no round trip; a batch goes out
//! when a call needs an answer, and on submit. A promise's answer is a reply record in the
//! program's shared memory, which Ash's watch on the mailbox settles into an hxn future on
//! the program's thread.

// Generated for all of WebGPU's IDL, of which this backend uses a part.
#[allow(dead_code, clippy::all)]
mod wire;

use std::sync::atomic::{AtomicI32, AtomicU32, Ordering::SeqCst};
use std::sync::{Arc, Mutex, MutexGuard, Once};

use hxn_future::{Completer, Future};
use wire::{Decode, Decoder, Encoder, Handle, Mailbox};

use crate::{
    AddressMode, BindingKind, Constant, Filter, Format, GroupLayout, LayoutEntry, SampleType,
    StorageAccess, Topology, VertexAttribute, VertexFormat, ViewDimension,
};

/// `navigator.gpu`, as the browser agent keeps it.
const GPU: Handle = Handle(1);

/// The handles the browser agent holds before this library makes any: the GPU, and the page's canvas
/// context and element.
const ROOTS: u32 = 3;

/// The name of the browser agent, and of its shim beside the web page: `gpu_webgpu.mjs`.
const AGENT: &str = "gpu_webgpu";

static MAILBOX: Mailbox = Mailbox::new();

unsafe extern "C" {
    fn ash_host_agent(name: *const u8, name_len: u32, address: u32) -> i32;
    fn ash_host_watch(
        word: *const u32,
        handler: unsafe extern "C" fn(*mut std::ffi::c_void),
        context: *mut std::ffi::c_void,
    ) -> *const u32;
}

/// Run by Ash on the program's thread whenever the browser agent has settled another reply.
unsafe extern "C" fn settled(_: *mut std::ffi::c_void) {
    settle_answered();
}

fn u32_of(value: i32, what: &str) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| format!("{what} cannot be {value}"))
}

fn u64_of(value: i64, what: &str) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| format!("{what} cannot be {value}"))
}

/// A reply record: the browser agent's answer to one command, in the program's memory, and the buffer
/// that takes what it carries. State 0 is pending, 1 done, 2 rejected, 3 too small.
#[repr(C)]
struct Reply {
    state: AtomicI32,
    len: AtomicU32,
    address: u32,
    cap: u32,
}

/// A reply with a buffer of its own, boxed so its address stays put until it settles.
struct Answer {
    reply: Reply,
    buffer: Box<[u8]>,
}

impl Answer {
    fn new(capacity: usize) -> Box<Answer> {
        let buffer = vec![0u8; capacity].into_boxed_slice();
        let address = buffer.as_ptr() as usize as u32;
        Box::new(Answer {
            reply: Reply {
                state: AtomicI32::new(0),
                len: AtomicU32::new(0),
                address,
                cap: capacity as u32,
            },
            buffer,
        })
    }

    fn at(&self) -> u32 {
        &self.reply as *const Reply as usize as u32
    }

    fn state(&self) -> i32 {
        self.reply.state.load(SeqCst)
    }

    fn bytes(&self) -> &[u8] {
        let len = (self.reply.len.load(SeqCst) as usize).min(self.buffer.len());
        &self.buffer[..len]
    }

    fn decode<T: Decode>(&self) -> Option<T> {
        T::decode(&mut Decoder::new(self.bytes()))
    }

    /// What a rejection's reply carries.
    fn message(&self) -> String {
        if self.state() == 3 {
            return "the browser agent's answer is too long to carry".into();
        }
        String::from_utf8_lossy(self.bytes()).into_owned()
    }
}

/// What a settled promise becomes.
enum Settle {
    Adapter(Completer<crate::Adapter>, Handle),
    Device(Completer<crate::Device>, Handle),
    Unit(Completer<()>),
    /// A buffer mapped for reading: its bytes are copied out and it is unmapped.
    Read(Completer<Vec<u8>>, Handle, u64, usize),
}

struct State {
    commands: Encoder,
    /// Bytes the batch's commands point at, kept until it is sent.
    staged: Vec<Box<[u8]>>,
    next: u32,
    pending: Vec<(Box<Answer>, Settle)>,
}

static STATE: Mutex<State> = Mutex::new(State {
    commands: Encoder { bytes: Vec::new() },
    staged: Vec::new(),
    next: ROOTS,
    pending: Vec::new(),
});

fn state() -> MutexGuard<'static, State> {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl State {
    fn handle(&mut self) -> Handle {
        self.next += 1;
        Handle(self.next)
    }

    /// Hands the batch to the browser agent and waits until it has run it.
    fn flush(&mut self) {
        MAILBOX.send(&self.commands.bytes);
        self.commands.bytes.clear();
        self.staged.clear();
    }

    /// `bytes`, copied until the batch is sent, as the wire reads them.
    fn stage(&mut self, bytes: &[u8]) -> wire::Bytes {
        let copy: Box<[u8]> = bytes.into();
        let staged = wire::Bytes {
            address: copy.as_ptr() as usize as u32,
            len: copy.len() as u32,
        };
        self.staged.push(copy);
        staged
    }
}

/// Settles every promise the browser agent has answered. Run on the program's thread by Ash's watch.
fn settle_answered() {
    let mut s = state();
    let mut i = 0;
    while i < s.pending.len() {
        if s.pending[i].0.state() == 0 {
            i += 1;
            continue;
        }
        let (answer, settle) = s.pending.swap_remove(i);
        let done = answer.state() == 1;
        match settle {
            Settle::Adapter(completer, adapter) => {
                // The reply is one byte, 0 when the browser's requestAdapter returned null.
                if done && answer.bytes().first() == Some(&0) {
                    completer.reject(String::from(
                        "no WebGPU adapter: WebGPU not available on this browser (requestAdapter returned null)",
                    ));
                } else if done {
                    completer.resolve(crate::Adapter {
                        inner: Adapter {
                            raw: Arc::new(Object(adapter)),
                        },
                    });
                } else {
                    completer.reject(format!("no WebGPU adapter: {}", answer.message()));
                }
            }
            Settle::Device(completer, device) => {
                if done {
                    let queue = s.handle();
                    s.commands.gpu_device_get_queue(device, queue);
                    completer.resolve(crate::Device {
                        inner: Device {
                            inner: Arc::new(DeviceInner {
                                device: Object(device),
                                queue: Object(queue),
                            }),
                        },
                    });
                } else {
                    completer.reject(format!("no WebGPU device: {}", answer.message()));
                }
            }
            Settle::Unit(completer) => {
                if done {
                    completer.resolve(());
                } else {
                    completer.reject(answer.message());
                }
            }
            Settle::Read(completer, buffer, offset, length) => {
                if !done {
                    completer.reject(format!("mapping the buffer failed: {}", answer.message()));
                    continue;
                }
                let range = Answer::new(length);
                s.commands.gpu_buffer_get_mapped_range(
                    buffer,
                    range.at(),
                    &Some(offset),
                    &Some(length as u64),
                );
                s.commands.gpu_buffer_unmap(buffer);
                s.flush();
                if range.state() == 1 {
                    completer.resolve(range.bytes().to_vec());
                } else {
                    completer.reject(format!(
                        "reading the mapped range failed: {}",
                        range.message()
                    ));
                }
            }
        }
    }
}

/// Starts the browser agent, and has Ash settle what it answers, once.
fn start() -> Result<(), String> {
    static STARTED: Once = Once::new();
    static AGENT_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    STARTED.call_once(|| {
        // Watched before any request is sent, so no reply is missed. Ash runs the handler on
        // the program's thread while it waits; the browser agent adds one to the word Ash
        // answers, and notifies it, after settling a reply, which wakes that wait.
        let wake = unsafe {
            ash_host_watch(
                MAILBOX.settled.as_ptr().cast_const().cast(),
                settled,
                std::ptr::null_mut(),
            )
        };
        if wake.is_null() {
            return;
        }
        MAILBOX.wake.store(wake as usize as u32, SeqCst);
        let address = &MAILBOX as *const Mailbox as usize as u32;
        let ran = unsafe { ash_host_agent(AGENT.as_ptr(), AGENT.len() as u32, address) };
        if ran == 1 {
            AGENT_RUNNING.store(true, SeqCst);
        }
    });
    if AGENT_RUNNING.load(SeqCst) {
        Ok(())
    } else {
        Err(format!(
            "no web page started the WebGPU browser agent: run the program in the web page Ash writes, served by `ash serve`, with {AGENT}.mjs beside it"
        ))
    }
}

/// Registers `answer` to settle as `settle`, and sends the batch that asks for it.
fn await_answer(mut s: MutexGuard<'static, State>, answer: Box<Answer>, settle: Settle) {
    s.pending.push((answer, settle));
    s.flush();
}

/// An object the browser agent keeps under its handle, forgotten when this is dropped.
struct Object(Handle);

impl Drop for Object {
    fn drop(&mut self) {
        state().commands.release(self.0);
    }
}

fn texture_format(format: Format) -> wire::GPUTextureFormat {
    use wire::GPUTextureFormat as F;
    match format {
        Format::R8Unorm => F::R8unorm,
        Format::R32Float => F::R32float,
        Format::R32Uint => F::R32uint,
        Format::Rgba8Unorm => F::Rgba8unorm,
        Format::Rgba8UnormSrgb => F::Rgba8unormSrgb,
        Format::Bgra8Unorm => F::Bgra8unorm,
        Format::Bgra8UnormSrgb => F::Bgra8unormSrgb,
        Format::Rg32Float => F::Rg32float,
        Format::Rgba16Float => F::Rgba16float,
        Format::Rgba32Float => F::Rgba32float,
        Format::Depth24Plus => F::Depth24plus,
        Format::Depth32Float => F::Depth32float,
    }
}

fn vertex_format(format: VertexFormat) -> wire::GPUVertexFormat {
    use wire::GPUVertexFormat as F;
    match format {
        VertexFormat::Unorm8x4 => F::Unorm8x4,
        VertexFormat::Float32 => F::Float32,
        VertexFormat::Float32x2 => F::Float32x2,
        VertexFormat::Float32x3 => F::Float32x3,
        VertexFormat::Float32x4 => F::Float32x4,
        VertexFormat::Uint32 => F::Uint32,
        VertexFormat::Uint32x2 => F::Uint32x2,
        VertexFormat::Uint32x4 => F::Uint32x4,
        VertexFormat::Sint32 => F::Sint32,
    }
}

fn view_dimension(dimension: ViewDimension) -> wire::GPUTextureViewDimension {
    use wire::GPUTextureViewDimension as D;
    match dimension {
        ViewDimension::D1 => D::V1d,
        ViewDimension::D2 => D::V2d,
        ViewDimension::D2Array => D::V2dArray,
        ViewDimension::Cube => D::Cube,
        ViewDimension::CubeArray => D::CubeArray,
        ViewDimension::D3 => D::V3d,
    }
}

fn layout_entry(entry: &LayoutEntry) -> Result<wire::GPUBindGroupLayoutEntry, String> {
    let mut out = wire::GPUBindGroupLayoutEntry {
        binding: u32_of(entry.binding, "a binding")?,
        visibility: entry.visibility as u32,
        buffer: None,
        sampler: None,
        texture: None,
        storage_texture: None,
        external_texture: None,
    };
    let buffer = |kind| {
        Some(wire::GPUBufferBindingLayout {
            r#type: Some(kind),
            has_dynamic_offset: None,
            min_binding_size: None,
        })
    };
    let sampler = |kind| Some(wire::GPUSamplerBindingLayout { r#type: Some(kind) });
    match entry.kind {
        BindingKind::Uniform => out.buffer = buffer(wire::GPUBufferBindingType::Uniform),
        BindingKind::Storage => out.buffer = buffer(wire::GPUBufferBindingType::Storage),
        BindingKind::ReadOnlyStorage => {
            out.buffer = buffer(wire::GPUBufferBindingType::ReadOnlyStorage)
        }
        BindingKind::Sampler => out.sampler = sampler(wire::GPUSamplerBindingType::Filtering),
        BindingKind::NonFilteringSampler => {
            out.sampler = sampler(wire::GPUSamplerBindingType::NonFiltering)
        }
        BindingKind::ComparisonSampler => {
            out.sampler = sampler(wire::GPUSamplerBindingType::Comparison)
        }
        BindingKind::Texture => {
            use wire::GPUTextureSampleType as T;
            out.texture = Some(wire::GPUTextureBindingLayout {
                sample_type: Some(match entry.sample_type {
                    SampleType::Float => T::Float,
                    SampleType::UnfilterableFloat => T::UnfilterableFloat,
                    SampleType::Depth => T::Depth,
                    SampleType::Sint => T::Sint,
                    SampleType::Uint => T::Uint,
                }),
                view_dimension: Some(view_dimension(entry.view_dimension)),
                multisampled: Some(entry.multisampled),
            })
        }
        BindingKind::StorageTexture => {
            use wire::GPUStorageTextureAccess as A;
            out.storage_texture = Some(wire::GPUStorageTextureBindingLayout {
                access: Some(match entry.access {
                    StorageAccess::WriteOnly => A::WriteOnly,
                    StorageAccess::ReadOnly => A::ReadOnly,
                    StorageAccess::ReadWrite => A::ReadWrite,
                }),
                format: texture_format(entry.format),
                view_dimension: Some(view_dimension(entry.view_dimension)),
            })
        }
    }
    Ok(out)
}

fn constants(given: &[Constant]) -> Option<Vec<(String, f64)>> {
    (!given.is_empty()).then(|| given.iter().map(|c| (c.key.clone(), c.value)).collect())
}

fn extent(width: i32, height: i32) -> wire::GPUExtent3D {
    wire::GPUExtent3D::GPUExtent3DDict(wire::GPUExtent3DDict {
        width: width as u32,
        height: Some(height as u32),
        depth_or_array_layers: Some(1),
    })
}

pub struct Instance;

impl Instance {
    pub fn create() -> Result<Instance, String> {
        start()?;
        Ok(Instance)
    }

    pub fn create_canvas_surface(&self) -> Result<Surface, String> {
        Ok(Surface {
            size: Mutex::new((0, 0)),
        })
    }

    pub fn request_adapter(&self) -> Future<crate::Adapter> {
        let (completer, future) = hxn_future::pending();
        let mut s = state();
        let adapter = s.handle();
        let answer = Answer::new(1024);
        s.commands
            .gpu_request_adapter(GPU, adapter, answer.at(), &None);
        await_answer(s, answer, Settle::Adapter(completer, adapter));
        future
    }
}

pub struct Adapter {
    raw: Arc<Object>,
}

impl Adapter {
    pub fn info(&self) -> String {
        let mut s = state();
        let info = s.handle();
        s.commands.gpu_adapter_get_info(self.raw.0, info);
        let parts: Vec<Box<Answer>> = (0..3).map(|_| Answer::new(1024)).collect();
        s.commands.gpu_adapter_info_get_vendor(info, parts[0].at());
        s.commands
            .gpu_adapter_info_get_architecture(info, parts[1].at());
        s.commands
            .gpu_adapter_info_get_description(info, parts[2].at());
        s.commands.release(info);
        s.flush();
        parts
            .iter()
            .filter(|part| part.state() == 1)
            .filter_map(|part| part.decode::<String>())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn request_device(&self) -> Future<crate::Device> {
        let (completer, future) = hxn_future::pending();
        let mut s = state();
        let device = s.handle();
        let answer = Answer::new(1024);
        s.commands
            .gpu_adapter_request_device(self.raw.0, device, answer.at(), &None);
        await_answer(s, answer, Settle::Device(completer, device));
        future
    }
}

struct DeviceInner {
    device: Object,
    queue: Object,
}

pub struct Device {
    inner: Arc<DeviceInner>,
}

pub struct Buffer {
    raw: Object,
    size: u64,
}

pub struct Texture {
    raw: Object,
    view: Object,
    width: i32,
    height: i32,
}

pub struct Sampler {
    raw: Object,
}

pub struct ShaderModule {
    raw: Object,
}

pub struct BindGroupLayouts {
    raw: Vec<Object>,
}

impl BindGroupLayouts {
    pub fn count(&self) -> i32 {
        self.raw.len() as i32
    }
}

pub struct PipelineLayout {
    raw: Object,
}

pub struct ComputePipeline {
    raw: Object,
}

pub struct RenderPipeline {
    raw: Object,
}

pub struct BindGroup {
    raw: Object,
}

/// A bind group's resources by binding; the objects are kept until the group is made.
pub struct BindGroupEntries {
    entries: Vec<(u32, wire::GPUBindingResource)>,
}

impl BindGroupEntries {
    pub fn create() -> BindGroupEntries {
        BindGroupEntries {
            entries: Vec::new(),
        }
    }

    pub fn buffer(
        &mut self,
        binding: i32,
        buffer: &Buffer,
        offset: i64,
        size: i64,
    ) -> Result<(), String> {
        let resource = wire::GPUBindingResource::GPUBufferBinding(wire::GPUBufferBinding {
            buffer: buffer.raw.0,
            offset: Some(u64_of(offset, "an offset")?),
            size: (size >= 0).then_some(size as u64),
        });
        self.entries.push((u32_of(binding, "a binding")?, resource));
        Ok(())
    }

    pub fn texture(&mut self, binding: i32, texture: &Texture) -> Result<(), String> {
        let resource = wire::GPUBindingResource::GPUTextureView(texture.view.0);
        self.entries.push((u32_of(binding, "a binding")?, resource));
        Ok(())
    }

    pub fn sampler(&mut self, binding: i32, sampler: &Sampler) -> Result<(), String> {
        let resource = wire::GPUBindingResource::GPUSampler(sampler.raw.0);
        self.entries.push((u32_of(binding, "a binding")?, resource));
        Ok(())
    }
}

impl Device {
    fn device(&self) -> Handle {
        self.inner.device.0
    }

    pub fn create_buffer(&self, size: i64, usage: i32) -> Result<Buffer, String> {
        let size = u64_of(size, "a buffer's size")?;
        let mut s = state();
        let raw = s.handle();
        s.commands.gpu_device_create_buffer(
            self.device(),
            raw,
            &wire::GPUBufferDescriptor {
                label: None,
                size,
                usage: usage as u32,
                mapped_at_creation: None,
            },
        );
        Ok(Buffer {
            raw: Object(raw),
            size,
        })
    }

    pub fn create_buffer_with(&self, bytes: &[u8], usage: i32) -> Result<Buffer, String> {
        let size = bytes.len().div_ceil(4) * 4;
        // A buffer the queue writes needs CopyDst; webgpu.h's version maps it at creation.
        let buffer = self.create_buffer(size as i64, usage | crate::BufferUsage::CopyDst as i32)?;
        let mut padded = bytes.to_vec();
        padded.resize(size, 0);
        self.write_buffer(&buffer, 0, &padded)?;
        Ok(buffer)
    }

    pub fn create_texture(
        &self,
        width: i32,
        height: i32,
        format: Format,
        usage: i32,
    ) -> Result<Texture, String> {
        u32_of(width, "a width")?;
        u32_of(height, "a height")?;
        let mut s = state();
        let raw = s.handle();
        let view = s.handle();
        s.commands.gpu_device_create_texture(
            self.device(),
            raw,
            &wire::GPUTextureDescriptor {
                label: None,
                size: extent(width, height),
                mip_level_count: None,
                sample_count: None,
                dimension: None,
                format: texture_format(format),
                usage: usage as u32,
                view_formats: None,
                texture_binding_view_dimension: None,
            },
        );
        s.commands.gpu_texture_create_view(raw, view, &None);
        Ok(Texture {
            raw: Object(raw),
            view: Object(view),
            width,
            height,
        })
    }

    pub fn create_sampler(&self, filter: Filter, address: AddressMode) -> Result<Sampler, String> {
        let filter_mode = match filter {
            Filter::Nearest => wire::GPUFilterMode::Nearest,
            Filter::Linear => wire::GPUFilterMode::Linear,
        };
        let mipmap = match filter {
            Filter::Nearest => wire::GPUMipmapFilterMode::Nearest,
            Filter::Linear => wire::GPUMipmapFilterMode::Linear,
        };
        let mode = match address {
            AddressMode::ClampToEdge => wire::GPUAddressMode::ClampToEdge,
            AddressMode::Repeat => wire::GPUAddressMode::Repeat,
            AddressMode::MirrorRepeat => wire::GPUAddressMode::MirrorRepeat,
        };
        let mut s = state();
        let raw = s.handle();
        s.commands.gpu_device_create_sampler(
            self.device(),
            raw,
            &Some(wire::GPUSamplerDescriptor {
                label: None,
                address_mode_u: Some(mode),
                address_mode_v: Some(mode),
                address_mode_w: Some(mode),
                mag_filter: Some(filter_mode),
                min_filter: Some(filter_mode),
                mipmap_filter: Some(mipmap),
                lod_min_clamp: None,
                lod_max_clamp: None,
                compare: None,
                max_anisotropy: None,
            }),
        );
        Ok(Sampler { raw: Object(raw) })
    }

    pub fn create_shader_module(&self, wgsl: &str) -> Result<ShaderModule, String> {
        let mut s = state();
        let raw = s.handle();
        s.commands.gpu_device_create_shader_module(
            self.device(),
            raw,
            &wire::GPUShaderModuleDescriptor {
                label: None,
                code: wgsl.to_owned(),
                compilation_hints: None,
            },
        );
        Ok(ShaderModule { raw: Object(raw) })
    }

    pub fn create_bind_group_layouts(
        &self,
        groups: Vec<GroupLayout>,
    ) -> Result<BindGroupLayouts, String> {
        let mut raw = Vec::with_capacity(groups.len());
        for group in &groups {
            let entries = group
                .entries
                .iter()
                .map(layout_entry)
                .collect::<Result<Vec<_>, _>>()?;
            let mut s = state();
            let layout = s.handle();
            s.commands.gpu_device_create_bind_group_layout(
                self.device(),
                layout,
                &wire::GPUBindGroupLayoutDescriptor {
                    label: None,
                    entries,
                },
            );
            raw.push(Object(layout));
        }
        Ok(BindGroupLayouts { raw })
    }

    pub fn create_pipeline_layout(
        &self,
        layouts: &BindGroupLayouts,
    ) -> Result<PipelineLayout, String> {
        let mut s = state();
        let raw = s.handle();
        s.commands.gpu_device_create_pipeline_layout(
            self.device(),
            raw,
            &wire::GPUPipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: layouts.raw.iter().map(|l| Some(l.0)).collect(),
                immediate_size: None,
            },
        );
        Ok(PipelineLayout { raw: Object(raw) })
    }

    pub fn create_compute_pipeline(
        &self,
        module: &ShaderModule,
        entry: &str,
        layout: &PipelineLayout,
        constants: Vec<Constant>,
    ) -> Result<ComputePipeline, String> {
        let mut s = state();
        let raw = s.handle();
        s.commands.gpu_device_create_compute_pipeline(
            self.device(),
            raw,
            &wire::GPUComputePipelineDescriptor {
                label: None,
                layout: wire::GPUPipelineLayoutOrGPUAutoLayoutMode::GPUPipelineLayout(layout.raw.0),
                compute: wire::GPUProgrammableStage {
                    module: module.raw.0,
                    entry_point: Some(entry.to_owned()),
                    constants: self::constants(&constants),
                },
            },
        );
        Ok(ComputePipeline { raw: Object(raw) })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_render_pipeline(
        &self,
        module: &ShaderModule,
        vertex_entry: &str,
        fragment_entry: &str,
        layout: &PipelineLayout,
        format: Format,
        topology: Topology,
        stride: i32,
        attributes: Vec<VertexAttribute>,
        constants: Vec<Constant>,
    ) -> Result<RenderPipeline, String> {
        let attributes = attributes
            .iter()
            .map(|a| {
                Ok(wire::GPUVertexAttribute {
                    format: vertex_format(a.format),
                    offset: u64_of(a.offset as i64, "an attribute's offset")?,
                    shader_location: u32_of(a.location, "a location")?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let buffers = (!attributes.is_empty()).then(|| {
            vec![Some(wire::GPUVertexBufferLayout {
                array_stride: stride as u64,
                step_mode: None,
                attributes,
            })]
        });
        let topology = match topology {
            Topology::PointList => wire::GPUPrimitiveTopology::PointList,
            Topology::LineList => wire::GPUPrimitiveTopology::LineList,
            Topology::LineStrip => wire::GPUPrimitiveTopology::LineStrip,
            Topology::TriangleList => wire::GPUPrimitiveTopology::TriangleList,
            Topology::TriangleStrip => wire::GPUPrimitiveTopology::TriangleStrip,
        };
        let mut s = state();
        let raw = s.handle();
        s.commands.gpu_device_create_render_pipeline(
            self.device(),
            raw,
            &wire::GPURenderPipelineDescriptor {
                label: None,
                layout: wire::GPUPipelineLayoutOrGPUAutoLayoutMode::GPUPipelineLayout(layout.raw.0),
                vertex: wire::GPUVertexState {
                    module: module.raw.0,
                    entry_point: Some(vertex_entry.to_owned()),
                    constants: self::constants(&constants),
                    buffers,
                },
                primitive: Some(wire::GPUPrimitiveState {
                    topology: Some(topology),
                    strip_index_format: None,
                    front_face: None,
                    cull_mode: None,
                    unclipped_depth: None,
                }),
                depth_stencil: None,
                multisample: None,
                fragment: Some(wire::GPUFragmentState {
                    module: module.raw.0,
                    entry_point: Some(fragment_entry.to_owned()),
                    constants: self::constants(&constants),
                    targets: vec![Some(wire::GPUColorTargetState {
                        format: texture_format(format),
                        blend: None,
                        write_mask: None,
                    })],
                }),
            },
        );
        Ok(RenderPipeline { raw: Object(raw) })
    }

    pub fn create_bind_group(
        &self,
        layouts: &BindGroupLayouts,
        group: i32,
        entries: &BindGroupEntries,
    ) -> Result<BindGroup, String> {
        let layout = usize::try_from(group)
            .ok()
            .and_then(|g| layouts.raw.get(g))
            .ok_or_else(|| format!("there is no bind group layout {group}"))?;
        let mut s = state();
        let raw = s.handle();
        s.commands.gpu_device_create_bind_group(
            self.device(),
            raw,
            &wire::GPUBindGroupDescriptor {
                label: None,
                layout: layout.0,
                entries: entries
                    .entries
                    .iter()
                    .map(|(binding, resource)| wire::GPUBindGroupEntry {
                        binding: *binding,
                        resource: resource.clone(),
                    })
                    .collect(),
            },
        );
        Ok(BindGroup { raw: Object(raw) })
    }

    pub fn create_command_encoder(&self) -> Result<CommandEncoder, String> {
        let mut s = state();
        let raw = s.handle();
        s.commands
            .gpu_device_create_command_encoder(self.device(), raw, &None);
        Ok(CommandEncoder {
            raw: Object(raw),
            finished: false,
        })
    }

    pub fn submit(&self, encoder: &mut CommandEncoder) -> Result<(), String> {
        if encoder.finished {
            return Err("the command encoder was submitted already".into());
        }
        encoder.finished = true;
        let mut s = state();
        let commands = s.handle();
        s.commands
            .gpu_command_encoder_finish(encoder.raw.0, commands, &None);
        s.commands
            .gpu_queue_submit(self.inner.queue.0, &vec![commands]);
        s.commands.release(commands);
        s.flush();
        Ok(())
    }

    pub fn write_buffer(&self, buffer: &Buffer, offset: i64, bytes: &[u8]) -> Result<(), String> {
        let offset = u64_of(offset, "an offset")?;
        let mut s = state();
        let data = s.stage(bytes);
        s.commands.gpu_queue_write_buffer(
            self.inner.queue.0,
            &buffer.raw.0,
            &offset,
            &data,
            &None,
            &None,
        );
        Ok(())
    }

    pub fn write_texture(
        &self,
        texture: &Texture,
        bytes: &[u8],
        bytes_per_row: i32,
    ) -> Result<(), String> {
        let bytes_per_row = u32_of(bytes_per_row, "bytes per row")?;
        let mut s = state();
        let data = s.stage(bytes);
        s.commands.gpu_queue_write_texture(
            self.inner.queue.0,
            &wire::GPUTexelCopyTextureInfo {
                texture: texture.raw.0,
                mip_level: None,
                origin: None,
                aspect: None,
            },
            &data,
            &wire::GPUTexelCopyBufferLayout {
                offset: None,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(texture.height as u32),
            },
            &extent(texture.width, texture.height),
        );
        Ok(())
    }

    pub fn completed(&self) -> Future<()> {
        let (completer, future) = hxn_future::pending();
        let mut s = state();
        let answer = Answer::new(1024);
        s.commands
            .gpu_queue_on_submitted_work_done(self.inner.queue.0, answer.at());
        await_answer(s, answer, Settle::Unit(completer));
        future
    }
}

impl Buffer {
    pub fn size(&self) -> i64 {
        self.size as i64
    }

    pub fn read(&self, offset: i64, length: i64) -> Result<Future<Vec<u8>>, String> {
        let offset = u64_of(offset, "an offset")?;
        let length = usize::try_from(length).map_err(|_| format!("a length cannot be {length}"))?;
        let (completer, future) = hxn_future::pending();
        let mut s = state();
        let answer = Answer::new(1024);
        s.commands.gpu_buffer_map_async(
            self.raw.0,
            answer.at(),
            &1, // GPUMapMode.READ
            &Some(offset),
            &Some(length as u64),
        );
        await_answer(
            s,
            answer,
            Settle::Read(completer, self.raw.0, offset, length),
        );
        Ok(future)
    }
}

impl Texture {
    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }
}

pub struct CommandEncoder {
    raw: Object,
    finished: bool,
}

impl CommandEncoder {
    pub fn begin_compute_pass(&mut self) -> Result<ComputePass, String> {
        let mut s = state();
        let raw = s.handle();
        s.commands
            .gpu_command_encoder_begin_compute_pass(self.raw.0, raw, &None);
        Ok(ComputePass {
            raw: Object(raw),
            ended: false,
        })
    }

    pub fn begin_render_pass(
        &mut self,
        target: &Texture,
        r: f64,
        g: f64,
        b: f64,
        a: f64,
    ) -> Result<RenderPass, String> {
        let mut s = state();
        let raw = s.handle();
        s.commands.gpu_command_encoder_begin_render_pass(
            self.raw.0,
            raw,
            &wire::GPURenderPassDescriptor {
                label: None,
                color_attachments: vec![Some(wire::GPURenderPassColorAttachment {
                    view: wire::GPUTextureOrGPUTextureView::GPUTextureView(target.view.0),
                    depth_slice: None,
                    resolve_target: None,
                    clear_value: Some(wire::GPUColor::GPUColorDict(wire::GPUColorDict {
                        r,
                        g,
                        b,
                        a,
                    })),
                    load_op: wire::GPULoadOp::Clear,
                    store_op: wire::GPUStoreOp::Store,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                max_draw_count: None,
            },
        );
        Ok(RenderPass {
            raw: Object(raw),
            ended: false,
        })
    }

    pub fn copy_buffer_to_buffer(
        &mut self,
        source: &Buffer,
        source_offset: i64,
        destination: &Buffer,
        destination_offset: i64,
        size: i64,
    ) -> Result<(), String> {
        let mut s = state();
        s.commands.gpu_command_encoder_copy_buffer_to_buffer_2(
            self.raw.0,
            &source.raw.0,
            &u64_of(source_offset, "an offset")?,
            &destination.raw.0,
            &u64_of(destination_offset, "an offset")?,
            &Some(u64_of(size, "a size")?),
        );
        Ok(())
    }

    pub fn copy_texture_to_buffer(
        &mut self,
        texture: &Texture,
        buffer: &Buffer,
        bytes_per_row: i32,
    ) -> Result<(), String> {
        let bytes_per_row = u32_of(bytes_per_row, "bytes per row")?;
        let mut s = state();
        s.commands.gpu_command_encoder_copy_texture_to_buffer(
            self.raw.0,
            &wire::GPUTexelCopyTextureInfo {
                texture: texture.raw.0,
                mip_level: None,
                origin: None,
                aspect: None,
            },
            &wire::GPUTexelCopyBufferInfo {
                offset: None,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(texture.height as u32),
                buffer: buffer.raw.0,
            },
            &extent(texture.width, texture.height),
        );
        Ok(())
    }
}

pub struct ComputePass {
    raw: Object,
    ended: bool,
}

impl ComputePass {
    pub fn set_pipeline(&mut self, pipeline: &ComputePipeline) {
        state()
            .commands
            .gpu_compute_pass_encoder_set_pipeline(self.raw.0, &pipeline.raw.0);
    }

    pub fn set_bind_group(&mut self, index: i32, group: &BindGroup) -> Result<(), String> {
        let index = u32_of(index, "a group index")?;
        state().commands.gpu_compute_pass_encoder_set_bind_group(
            self.raw.0,
            &index,
            &Some(group.raw.0),
            &None,
        );
        Ok(())
    }

    pub fn dispatch(&mut self, x: i32, y: i32, z: i32) -> Result<(), String> {
        let x = u32_of(x, "a workgroup count")?;
        let y = u32_of(y, "a workgroup count")?;
        let z = u32_of(z, "a workgroup count")?;
        state()
            .commands
            .gpu_compute_pass_encoder_dispatch_workgroups(self.raw.0, &x, &Some(y), &Some(z));
        Ok(())
    }

    pub fn end(&mut self) -> Result<(), String> {
        if self.ended {
            return Err("the compute pass has ended already".into());
        }
        self.ended = true;
        state().commands.gpu_compute_pass_encoder_end(self.raw.0);
        Ok(())
    }
}

pub struct RenderPass {
    raw: Object,
    ended: bool,
}

impl RenderPass {
    pub fn set_pipeline(&mut self, pipeline: &RenderPipeline) {
        state()
            .commands
            .gpu_render_pass_encoder_set_pipeline(self.raw.0, &pipeline.raw.0);
    }

    pub fn set_bind_group(&mut self, index: i32, group: &BindGroup) -> Result<(), String> {
        let index = u32_of(index, "a group index")?;
        state().commands.gpu_render_pass_encoder_set_bind_group(
            self.raw.0,
            &index,
            &Some(group.raw.0),
            &None,
        );
        Ok(())
    }

    pub fn set_vertex_buffer(&mut self, slot: i32, buffer: &Buffer) -> Result<(), String> {
        let slot = u32_of(slot, "a slot")?;
        state().commands.gpu_render_pass_encoder_set_vertex_buffer(
            self.raw.0,
            &slot,
            &Some(buffer.raw.0),
            &None,
            &None,
        );
        Ok(())
    }

    pub fn draw(
        &mut self,
        vertices: i32,
        instances: i32,
        first_vertex: i32,
        first_instance: i32,
    ) -> Result<(), String> {
        let vertices = u32_of(vertices, "a vertex count")?;
        let instances = u32_of(instances, "an instance count")?;
        let first_vertex = u32_of(first_vertex, "a first vertex")?;
        let first_instance = u32_of(first_instance, "a first instance")?;
        state().commands.gpu_render_pass_encoder_draw(
            self.raw.0,
            &vertices,
            &Some(instances),
            &Some(first_vertex),
            &Some(first_instance),
        );
        Ok(())
    }

    pub fn end(&mut self) -> Result<(), String> {
        if self.ended {
            return Err("the render pass has ended already".into());
        }
        self.ended = true;
        state().commands.gpu_render_pass_encoder_end(self.raw.0);
        Ok(())
    }
}

/// The canvas the web page gave the browser agent: its WebGPU context is handle 2, the
/// canvas itself handle 3. Its size is the one `configure` set.
pub struct Surface {
    size: Mutex<(i32, i32)>,
}

const CANVAS: Handle = Handle(2);
const CANVAS_ELEMENT: Handle = Handle(3);

impl Surface {
    pub fn preferred_format(&self, _adapter: &Adapter) -> Format {
        let mut s = state();
        let answer = Answer::new(16);
        s.commands.gpu_get_preferred_canvas_format(GPU, answer.at());
        s.flush();
        match answer.decode::<wire::GPUTextureFormat>() {
            Some(wire::GPUTextureFormat::Rgba8unorm) => Format::Rgba8Unorm,
            _ => Format::Bgra8Unorm,
        }
    }

    pub fn configure(
        &self,
        device: &Device,
        format: Format,
        usage: i32,
        width: i32,
        height: i32,
    ) -> Result<(), String> {
        *self.size.lock().unwrap_or_else(|p| p.into_inner()) = (width, height);
        let width = u32_of(width, "a width")? as u64;
        let height = u32_of(height, "a height")? as u64;
        let mut s = state();
        s.commands
            .offscreen_canvas_set_width(CANVAS_ELEMENT, &width);
        s.commands
            .offscreen_canvas_set_height(CANVAS_ELEMENT, &height);
        s.commands.gpu_canvas_context_configure(
            CANVAS,
            &wire::GPUCanvasConfiguration {
                device: device.device(),
                format: texture_format(format),
                usage: Some(usage as u32),
                view_formats: None,
                tone_mapping: None,
                alpha_mode: Some(wire::GPUCanvasAlphaMode::Opaque),
            },
        );
        Ok(())
    }

    pub fn current_texture(&self) -> Result<Texture, String> {
        let mut s = state();
        let raw = s.handle();
        let view = s.handle();
        s.commands
            .gpu_canvas_context_get_current_texture(CANVAS, raw);
        s.commands.gpu_texture_create_view(raw, view, &None);
        let (width, height) = *self.size.lock().unwrap_or_else(|p| p.into_inner());
        Ok(Texture {
            raw: Object(raw),
            view: Object(view),
            width,
            height,
        })
    }
}
