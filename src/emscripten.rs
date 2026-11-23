//! The backend over webgpu.h, as Emscripten's emdawnwebgpu port implements it on the page's
//! `navigator.gpu`: each call a wasm import into the port's JavaScript.

use std::ffi::c_void;
use std::sync::Arc;

use crate::ffi::*;
use crate::{
    AddressMode, BindingKind, Constant, Filter, Format, GroupLayout, LayoutEntry, Topology,
    VertexAttribute,
};
use hxn_future::{Completer, Future};

const STRLEN: usize = usize::MAX;
const WHOLE_SIZE: u64 = u64::MAX;
const DEPTH_SLICE_UNDEFINED: u32 = u32::MAX;

/// A WebGPU object. The page has one thread, and every call reaches the same JavaScript
/// object, so these cross no thread.
struct Shared<T>(T);
unsafe impl<T> Send for Shared<T> {}
unsafe impl<T> Sync for Shared<T> {}

fn view(text: &str) -> WGPUStringView {
    WGPUStringView {
        data: text.as_ptr().cast(),
        length: text.len(),
    }
}

/// No string: WebGPU takes its default.
fn unset() -> WGPUStringView {
    WGPUStringView {
        data: std::ptr::null(),
        length: STRLEN,
    }
}

fn text(view: WGPUStringView) -> String {
    if view.data.is_null() || view.length == 0 {
        return String::new();
    }
    let length = if view.length == STRLEN {
        unsafe { std::ffi::CStr::from_ptr(view.data) }
            .to_bytes()
            .len()
    } else {
        view.length
    };
    let bytes = unsafe { std::slice::from_raw_parts(view.data.cast::<u8>(), length) };
    String::from_utf8_lossy(bytes).into_owned()
}

fn u32_of(value: i32, what: &str) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| format!("{what} cannot be {value}"))
}

fn u64_of(value: i64, what: &str) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| format!("{what} cannot be {value}"))
}

/// A WebGPU instance: `navigator.gpu`.
pub struct Instance {
    raw: Shared<WGPUInstance>,
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe { wgpuInstanceRelease(self.raw.0) };
    }
}

/// A `GPUAdapter`.
pub struct Adapter {
    raw: Shared<WGPUAdapter>,
}

impl Drop for Adapter {
    fn drop(&mut self) {
        unsafe { wgpuAdapterRelease(self.raw.0) };
    }
}

/// A `GPUShaderModule` of WGSL.
pub struct ShaderModule {
    raw: Shared<WGPUShaderModule>,
}

impl Drop for ShaderModule {
    fn drop(&mut self) {
        unsafe { wgpuShaderModuleRelease(self.raw.0) };
    }
}

/// A `GPUPipelineLayout`.
pub struct PipelineLayout {
    raw: Shared<WGPUPipelineLayout>,
}

impl Drop for PipelineLayout {
    fn drop(&mut self) {
        unsafe { wgpuPipelineLayoutRelease(self.raw.0) };
    }
}

/// A `GPUComputePipeline`.
pub struct ComputePipeline {
    raw: Shared<WGPUComputePipeline>,
}

impl Drop for ComputePipeline {
    fn drop(&mut self) {
        unsafe { wgpuComputePipelineRelease(self.raw.0) };
    }
}

/// A `GPURenderPipeline`.
pub struct RenderPipeline {
    raw: Shared<WGPURenderPipeline>,
}

impl Drop for RenderPipeline {
    fn drop(&mut self) {
        unsafe { wgpuRenderPipelineRelease(self.raw.0) };
    }
}

/// A `GPUSampler`.
pub struct Sampler {
    raw: Shared<WGPUSampler>,
}

impl Drop for Sampler {
    fn drop(&mut self) {
        unsafe { wgpuSamplerRelease(self.raw.0) };
    }
}

/// A `GPUBindGroup`.
pub struct BindGroup {
    raw: Shared<WGPUBindGroup>,
}

impl Drop for BindGroup {
    fn drop(&mut self) {
        unsafe { wgpuBindGroupRelease(self.raw.0) };
    }
}

/// Boxes `completer` for a callback's `userdata1`.
fn userdata<T>(completer: Completer<T>) -> *mut c_void {
    Box::into_raw(Box::new(completer)).cast()
}

/// The completer `userdata` made. A callback takes it once.
unsafe fn completer<T>(userdata: *mut c_void) -> Completer<T> {
    *unsafe { Box::from_raw(userdata.cast::<Completer<T>>()) }
}

unsafe extern "C" fn adapter_requested(
    status: WGPURequestAdapterStatus,
    adapter: WGPUAdapter,
    message: WGPUStringView,
    userdata1: *mut c_void,
    _: *mut c_void,
) {
    let completer = unsafe { completer::<crate::Adapter>(userdata1) };
    if status == WGPURequestAdapterStatus_Success && !adapter.is_null() {
        completer.resolve(crate::Adapter {
            inner: Adapter {
                raw: Shared(adapter),
            },
        });
    } else {
        completer.reject(format!("no WebGPU adapter: {}", text(message)));
    }
}

impl Instance {
    pub fn claim_canvas(&self) -> hxn_future::Future<()> {
        let (completer, future) = hxn_future::pending();
        completer.resolve(());
        future
    }

    pub fn create() -> Result<Instance, String> {
        let raw = unsafe { wgpuCreateInstance(std::ptr::null()) };
        if raw.is_null() {
            return Err("this browser has no WebGPU".into());
        }
        Ok(Instance { raw: Shared(raw) })
    }

    /// The web page's `#canvas`, which Emscripten's page has.
    pub fn create_canvas_surface(&self) -> Result<Surface, String> {
        let mut source = WGPUEmscriptenSurfaceSourceCanvasHTMLSelector {
            chain: WGPUChainedStruct {
                next: std::ptr::null_mut(),
                sType: WGPUSType_EmscriptenSurfaceSourceCanvasHTMLSelector,
            },
            // The port reads the selector as a C string, up to its NUL.
            selector: WGPUStringView {
                data: c"#canvas".as_ptr(),
                length: 7,
            },
        };
        let descriptor = WGPUSurfaceDescriptor {
            nextInChain: (&mut source.chain) as *mut WGPUChainedStruct,
            label: unset(),
        };
        let raw = unsafe { wgpuInstanceCreateSurface(self.raw.0, &descriptor) };
        if raw.is_null() {
            return Err("the web page has no #canvas to render into".into());
        }
        Ok(Surface { raw: Shared(raw) })
    }

    /// The browser's adapter, its default for power preference.
    pub fn request_adapter(&self) -> Future<crate::Adapter> {
        let (completer, future) = hxn_future::pending();
        let callback = WGPURequestAdapterCallbackInfo {
            nextInChain: std::ptr::null_mut(),
            mode: WGPUCallbackMode_AllowSpontaneous,
            callback: Some(adapter_requested),
            userdata1: userdata(completer),
            userdata2: std::ptr::null_mut(),
        };
        unsafe { wgpuInstanceRequestAdapter(self.raw.0, std::ptr::null(), callback) };
        future
    }
}

unsafe extern "C" fn device_requested(
    status: WGPURequestDeviceStatus,
    device: WGPUDevice,
    message: WGPUStringView,
    userdata1: *mut c_void,
    _: *mut c_void,
) {
    let completer = unsafe { completer::<crate::Device>(userdata1) };
    if status == WGPURequestDeviceStatus_Success && !device.is_null() {
        let queue = unsafe { wgpuDeviceGetQueue(device) };
        completer.resolve(crate::Device {
            inner: Device {
                inner: Arc::new(DeviceInner {
                    device: Shared(device),
                    queue: Shared(queue),
                }),
            },
        });
    } else {
        completer.reject(format!("no WebGPU device: {}", text(message)));
    }
}

/// Queues uncaptured WebGPU diagnostics for the application logging sink.
unsafe extern "C" fn uncaptured(
    _: *const WGPUDevice,
    kind: WGPUErrorType,
    message: WGPUStringView,
    _: *mut c_void,
    _: *mut c_void,
) {
    let kind = if kind == WGPUErrorType_Validation {
        "validation"
    } else if kind == WGPUErrorType_OutOfMemory {
        "out of memory"
    } else if kind == WGPUErrorType_Internal {
        "internal"
    } else {
        "unknown"
    };
    hxn::log::error!(target: "hxn.gpu.webgpu", "WebGPU {kind} error: {}", text(message));
}

unsafe extern "C" fn lost(
    _: *const WGPUDevice,
    reason: WGPUDeviceLostReason,
    message: WGPUStringView,
    _: *mut c_void,
    _: *mut c_void,
) {
    // Destroyed: the program released it.
    if reason != WGPUDeviceLostReason_Destroyed && reason != WGPUDeviceLostReason_CallbackCancelled
    {
        hxn::log::error!(target: "hxn.gpu.webgpu", "WebGPU device lost: {}", text(message));
    }
}

impl Adapter {
    /// The adapter's vendor, architecture and description, as the browser reports them.
    pub fn info(&self) -> String {
        let mut info: WGPUAdapterInfo = unsafe { std::mem::zeroed() };
        if unsafe { wgpuAdapterGetInfo(self.raw.0, &mut info) } != WGPUStatus_Success {
            return String::new();
        }
        let parts = [info.vendor, info.architecture, info.description].map(text);
        unsafe { wgpuAdapterInfoFreeMembers(info) };
        parts
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// A device with the default limits and no extra features. Uncaptured validation errors
    /// are queued for the application logging sink.
    pub fn request_device(&self) -> Future<crate::Device> {
        let (completer, future) = hxn_future::pending();
        let mut descriptor: WGPUDeviceDescriptor = unsafe { std::mem::zeroed() };
        descriptor.label = unset();
        descriptor.defaultQueue.label = unset();
        descriptor.deviceLostCallbackInfo.mode = WGPUCallbackMode_AllowSpontaneous;
        descriptor.deviceLostCallbackInfo.callback = Some(lost);
        descriptor.uncapturedErrorCallbackInfo.callback = Some(uncaptured);
        let callback = WGPURequestDeviceCallbackInfo {
            nextInChain: std::ptr::null_mut(),
            mode: WGPUCallbackMode_AllowSpontaneous,
            callback: Some(device_requested),
            userdata1: userdata(completer),
            userdata2: std::ptr::null_mut(),
        };
        unsafe { wgpuAdapterRequestDevice(self.raw.0, &descriptor, callback) };
        future
    }
}

struct DeviceInner {
    device: Shared<WGPUDevice>,
    queue: Shared<WGPUQueue>,
}

impl Drop for DeviceInner {
    fn drop(&mut self) {
        unsafe {
            wgpuQueueRelease(self.queue.0);
            wgpuDeviceRelease(self.device.0);
        }
    }
}

/// A `GPUDevice` and its queue.
pub struct Device {
    inner: Arc<DeviceInner>,
}

/// The bind group layouts of a pipeline, group 0 first.
pub struct BindGroupLayouts {
    raw: Vec<Shared<WGPUBindGroupLayout>>,
}

impl Drop for BindGroupLayouts {
    fn drop(&mut self) {
        for layout in &self.raw {
            unsafe { wgpuBindGroupLayoutRelease(layout.0) };
        }
    }
}

impl BindGroupLayouts {
    /// The number of groups.
    pub fn count(&self) -> i32 {
        self.raw.len() as i32
    }
}

fn layout_entry(entry: &LayoutEntry) -> Result<WGPUBindGroupLayoutEntry, String> {
    let mut out: WGPUBindGroupLayoutEntry = unsafe { std::mem::zeroed() };
    out.binding = u32_of(entry.binding, "a binding")?;
    out.visibility = entry.visibility as WGPUShaderStage;
    match entry.kind {
        BindingKind::Uniform => out.buffer.type_ = WGPUBufferBindingType_Uniform,
        BindingKind::Storage => out.buffer.type_ = WGPUBufferBindingType_Storage,
        BindingKind::ReadOnlyStorage => out.buffer.type_ = WGPUBufferBindingType_ReadOnlyStorage,
        BindingKind::Sampler => out.sampler.type_ = WGPUSamplerBindingType_Filtering,
        BindingKind::NonFilteringSampler => out.sampler.type_ = WGPUSamplerBindingType_NonFiltering,
        BindingKind::ComparisonSampler => out.sampler.type_ = WGPUSamplerBindingType_Comparison,
        BindingKind::Texture => {
            out.texture.sampleType = entry.sample_type as WGPUTextureSampleType;
            out.texture.viewDimension = entry.view_dimension as WGPUTextureViewDimension;
            out.texture.multisampled = entry.multisampled as WGPUBool;
        }
        BindingKind::StorageTexture => {
            out.storageTexture.access = entry.access as WGPUStorageTextureAccess;
            out.storageTexture.format = entry.format as WGPUTextureFormat;
            out.storageTexture.viewDimension = entry.view_dimension as WGPUTextureViewDimension;
        }
    }
    Ok(out)
}

fn constants(given: &[Constant]) -> Vec<WGPUConstantEntry> {
    given
        .iter()
        .map(|constant| WGPUConstantEntry {
            nextInChain: std::ptr::null_mut(),
            key: view(&constant.key),
            value: constant.value,
        })
        .collect()
}

unsafe extern "C" fn work_done(
    status: WGPUQueueWorkDoneStatus,
    message: WGPUStringView,
    userdata1: *mut c_void,
    _: *mut c_void,
) {
    let completer = unsafe { completer::<()>(userdata1) };
    if status == WGPUQueueWorkDoneStatus_Success {
        completer.resolve(());
    } else {
        completer.reject(format!(
            "the queue's work did not finish: {}",
            text(message)
        ));
    }
}

impl Device {
    /// A buffer of `size` bytes, a multiple of 4, for `usage` (`BufferUsage` bits).
    pub fn create_buffer(&self, size: i64, usage: i32) -> Result<Buffer, String> {
        let descriptor = WGPUBufferDescriptor {
            nextInChain: std::ptr::null_mut(),
            label: unset(),
            usage: usage as WGPUBufferUsage,
            size: u64_of(size, "a buffer's size")?,
            mappedAtCreation: 0,
        };
        let raw = unsafe { wgpuDeviceCreateBuffer(self.inner.device.0, &descriptor) };
        if raw.is_null() {
            return Err("creating the buffer failed".into());
        }
        Ok(Buffer {
            raw: Shared(raw),
            size: size as u64,
        })
    }

    /// A buffer holding `bytes`, padded with zeros to a multiple of 4, for `usage`
    /// (`BufferUsage` bits).
    pub fn create_buffer_with(&self, bytes: &[u8], usage: i32) -> Result<Buffer, String> {
        let size = bytes.len().div_ceil(4) * 4;
        let descriptor = WGPUBufferDescriptor {
            nextInChain: std::ptr::null_mut(),
            label: unset(),
            usage: usage as WGPUBufferUsage,
            size: size as u64,
            mappedAtCreation: 1,
        };
        let raw = unsafe { wgpuDeviceCreateBuffer(self.inner.device.0, &descriptor) };
        if raw.is_null() {
            return Err("creating the buffer failed".into());
        }
        unsafe {
            let mapped = wgpuBufferGetMappedRange(raw, 0, size).cast::<u8>();
            if mapped.is_null() {
                wgpuBufferRelease(raw);
                return Err("mapping the new buffer failed".into());
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), mapped, bytes.len());
            wgpuBufferUnmap(raw);
        }
        Ok(Buffer {
            raw: Shared(raw),
            size: size as u64,
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
        let descriptor = WGPUTextureDescriptor {
            nextInChain: std::ptr::null_mut(),
            label: unset(),
            usage: usage as WGPUTextureUsage,
            dimension: WGPUTextureDimension_2D,
            size: WGPUExtent3D {
                width: u32_of(width, "a width")?,
                height: u32_of(height, "a height")?,
                depthOrArrayLayers: 1,
            },
            format: format as WGPUTextureFormat,
            mipLevelCount: 1,
            sampleCount: 1,
            viewFormatCount: 0,
            viewFormats: std::ptr::null(),
        };
        let raw = unsafe { wgpuDeviceCreateTexture(self.inner.device.0, &descriptor) };
        if raw.is_null() {
            return Err("creating the texture failed".into());
        }
        let view = unsafe { wgpuTextureCreateView(raw, std::ptr::null()) };
        Ok(Texture {
            raw: Shared(raw),
            view: Shared(view),
            width,
            height,
        })
    }

    /// A sampler filtering with `filter` for magnification, minification and mipmaps, and
    /// addressing with `address` on every axis.
    pub fn create_sampler(&self, filter: Filter, address: AddressMode) -> Result<Sampler, String> {
        let descriptor = WGPUSamplerDescriptor {
            nextInChain: std::ptr::null_mut(),
            label: unset(),
            addressModeU: address as WGPUAddressMode,
            addressModeV: address as WGPUAddressMode,
            addressModeW: address as WGPUAddressMode,
            magFilter: filter as WGPUFilterMode,
            minFilter: filter as WGPUFilterMode,
            mipmapFilter: filter as WGPUMipmapFilterMode,
            lodMinClamp: 0.0,
            lodMaxClamp: 32.0,
            compare: 0,
            maxAnisotropy: 1,
        };
        let raw = unsafe { wgpuDeviceCreateSampler(self.inner.device.0, &descriptor) };
        if raw.is_null() {
            return Err("creating the sampler failed".into());
        }
        Ok(Sampler { raw: Shared(raw) })
    }

    /// A shader module of WGSL source.
    pub fn create_shader_module(&self, wgsl: &str) -> Result<ShaderModule, String> {
        let mut source = WGPUShaderSourceWGSL {
            chain: WGPUChainedStruct {
                next: std::ptr::null_mut(),
                sType: WGPUSType_ShaderSourceWGSL,
            },
            code: view(wgsl),
        };
        let descriptor = WGPUShaderModuleDescriptor {
            nextInChain: (&mut source.chain) as *mut WGPUChainedStruct,
            label: unset(),
        };
        let raw = unsafe { wgpuDeviceCreateShaderModule(self.inner.device.0, &descriptor) };
        if raw.is_null() {
            return Err("creating the shader module failed".into());
        }
        Ok(ShaderModule { raw: Shared(raw) })
    }

    /// A bind group layout for each of `groups`, group 0 first.
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
            let descriptor = WGPUBindGroupLayoutDescriptor {
                nextInChain: std::ptr::null_mut(),
                label: unset(),
                entryCount: entries.len(),
                entries: entries.as_ptr(),
            };
            let layout =
                unsafe { wgpuDeviceCreateBindGroupLayout(self.inner.device.0, &descriptor) };
            if layout.is_null() {
                return Err("creating a bind group layout failed".into());
            }
            raw.push(Shared(layout));
        }
        Ok(BindGroupLayouts { raw })
    }

    /// A pipeline layout of `layouts`.
    pub fn create_pipeline_layout(
        &self,
        layouts: &BindGroupLayouts,
    ) -> Result<PipelineLayout, String> {
        let raw_layouts: Vec<WGPUBindGroupLayout> = layouts.raw.iter().map(|l| l.0).collect();
        let descriptor = WGPUPipelineLayoutDescriptor {
            nextInChain: std::ptr::null_mut(),
            label: unset(),
            bindGroupLayoutCount: raw_layouts.len(),
            bindGroupLayouts: raw_layouts.as_ptr(),
            immediateSize: 0,
        };
        let raw = unsafe { wgpuDeviceCreatePipelineLayout(self.inner.device.0, &descriptor) };
        if raw.is_null() {
            return Err("creating the pipeline layout failed".into());
        }
        Ok(PipelineLayout { raw: Shared(raw) })
    }

    /// A compute pipeline running `entry` of `module`, with `constants` set.
    pub fn create_compute_pipeline(
        &self,
        module: &ShaderModule,
        entry: &str,
        layout: &PipelineLayout,
        constants: Vec<Constant>,
    ) -> Result<ComputePipeline, String> {
        let entries = self::constants(&constants);
        let descriptor = WGPUComputePipelineDescriptor {
            nextInChain: std::ptr::null_mut(),
            label: unset(),
            layout: layout.raw.0,
            compute: WGPUComputeState {
                nextInChain: std::ptr::null_mut(),
                module: module.raw.0,
                entryPoint: view(entry),
                constantCount: entries.len(),
                constants: entries.as_ptr(),
            },
        };
        let raw = unsafe { wgpuDeviceCreateComputePipeline(self.inner.device.0, &descriptor) };
        if raw.is_null() {
            return Err("creating the compute pipeline failed".into());
        }
        Ok(ComputePipeline { raw: Shared(raw) })
    }

    /// A render pipeline drawing `topology` into one color target of `format`, from one vertex
    /// buffer of `stride` bytes per vertex laid out as `attributes`, with `constants` set in both
    /// stages.
    #[allow(clippy::too_many_arguments)]
    pub fn create_render_pipeline(
        &self,
        module: &ShaderModule,
        vertex_entry: &str,
        fragment_entry: &str,
        layout: &PipelineLayout,
        format: Format,
        topology: Topology,
        buffers: Vec<crate::VertexBuffer>,
        attributes: Vec<VertexAttribute>,
        constants: Vec<Constant>,
        blend: crate::Blend,
    ) -> Result<RenderPipeline, String> {
        let entries = self::constants(&constants);
        // The C API numbers factors and operations from 1, after `Undefined`.
        let state = WGPUBlendState {
            color: WGPUBlendComponent {
                operation: (blend.color_operation + 1) as WGPUBlendOperation,
                srcFactor: (blend.color_source + 1) as WGPUBlendFactor,
                dstFactor: (blend.color_destination + 1) as WGPUBlendFactor,
            },
            alpha: WGPUBlendComponent {
                operation: (blend.alpha_operation + 1) as WGPUBlendOperation,
                srcFactor: (blend.alpha_source + 1) as WGPUBlendFactor,
                dstFactor: (blend.alpha_destination + 1) as WGPUBlendFactor,
            },
        };
        // Holds the attributes of each buffer. Each buffer's layout points into its list.
        let grouped = (0..buffers.len())
            .map(|index| {
                attributes
                    .iter()
                    .filter(|a| a.buffer as usize == index)
                    .map(|a| {
                        Ok(WGPUVertexAttribute {
                            nextInChain: std::ptr::null_mut(),
                            format: a.format as WGPUVertexFormat,
                            offset: u64_of(a.offset as i64, "an attribute's offset")?,
                            shaderLocation: u32_of(a.location, "a location")?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()
            })
            .collect::<Result<Vec<_>, String>>()?;
        let layouts = buffers
            .iter()
            .zip(&grouped)
            .map(|(b, attributes)| {
                Ok(WGPUVertexBufferLayout {
                    nextInChain: std::ptr::null_mut(),
                    stepMode: if b.per_instance {
                        WGPUVertexStepMode_Instance
                    } else {
                        WGPUVertexStepMode_Vertex
                    },
                    arrayStride: u64_of(b.stride as i64, "a stride")?,
                    attributeCount: attributes.len(),
                    attributes: attributes.as_ptr(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let target = WGPUColorTargetState {
            nextInChain: std::ptr::null_mut(),
            format: format as WGPUTextureFormat,
            blend: if blend.enabled {
                &state
            } else {
                std::ptr::null()
            },
            writeMask: WGPUColorWriteMask_All,
        };
        let fragment = WGPUFragmentState {
            nextInChain: std::ptr::null_mut(),
            module: module.raw.0,
            entryPoint: view(fragment_entry),
            constantCount: entries.len(),
            constants: entries.as_ptr(),
            targetCount: 1,
            targets: &target,
        };
        let mut descriptor: WGPURenderPipelineDescriptor = unsafe { std::mem::zeroed() };
        descriptor.label = unset();
        descriptor.layout = layout.raw.0;
        descriptor.vertex = WGPUVertexState {
            nextInChain: std::ptr::null_mut(),
            module: module.raw.0,
            entryPoint: view(vertex_entry),
            constantCount: entries.len(),
            constants: entries.as_ptr(),
            bufferCount: layouts.len(),
            buffers: layouts.as_ptr(),
        };
        descriptor.primitive.topology = topology as WGPUPrimitiveTopology;
        descriptor.multisample.count = 1;
        descriptor.multisample.mask = u32::MAX;
        descriptor.fragment = &fragment;
        let raw = unsafe { wgpuDeviceCreateRenderPipeline(self.inner.device.0, &descriptor) };
        if raw.is_null() {
            return Err("creating the render pipeline failed".into());
        }
        Ok(RenderPipeline { raw: Shared(raw) })
    }

    /// Bind group `group` of `layouts`, of the resources `entries` holds.
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
        let raw_entries: Vec<WGPUBindGroupEntry> = entries.entries.iter().map(|e| e.raw).collect();
        let descriptor = WGPUBindGroupDescriptor {
            nextInChain: std::ptr::null_mut(),
            label: unset(),
            layout: layout.0,
            entryCount: raw_entries.len(),
            entries: raw_entries.as_ptr(),
        };
        let raw = unsafe { wgpuDeviceCreateBindGroup(self.inner.device.0, &descriptor) };
        if raw.is_null() {
            return Err("creating the bind group failed".into());
        }
        Ok(BindGroup { raw: Shared(raw) })
    }

    /// An empty command encoder.
    pub fn create_command_encoder(&self) -> Result<CommandEncoder, String> {
        let raw = unsafe { wgpuDeviceCreateCommandEncoder(self.inner.device.0, std::ptr::null()) };
        if raw.is_null() {
            return Err("creating the command encoder failed".into());
        }
        Ok(CommandEncoder {
            raw: Shared(raw),
            finished: false,
        })
    }

    /// Finishes `encoder` and submits its commands to the queue.
    pub fn submit(&self, encoder: &mut CommandEncoder) -> Result<(), String> {
        if encoder.finished {
            return Err("the command encoder was submitted already".into());
        }
        encoder.finished = true;
        let commands = unsafe { wgpuCommandEncoderFinish(encoder.raw.0, std::ptr::null()) };
        if commands.is_null() {
            return Err("finishing the command encoder failed".into());
        }
        unsafe {
            wgpuQueueSubmit(self.inner.queue.0, 1, &commands);
            wgpuCommandBufferRelease(commands);
        }
        Ok(())
    }

    /// Writes `bytes`, a multiple of 4, at `offset` in `buffer`, in queue order.
    pub fn write_buffer(&self, buffer: &Buffer, offset: i64, bytes: &[u8]) -> Result<(), String> {
        let offset = u64_of(offset, "an offset")?;
        unsafe {
            wgpuQueueWriteBuffer(
                self.inner.queue.0,
                buffer.raw.0,
                offset,
                bytes.as_ptr().cast(),
                bytes.len(),
            )
        };
        Ok(())
    }

    /// Writes all of `texture` from `bytes`, rows `bytes_per_row` apart, in queue order.
    #[allow(clippy::too_many_arguments)]
    pub fn write_texture(
        &self,
        texture: &Texture,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        bytes: &[u8],
        bytes_per_row: i32,
    ) -> Result<(), String> {
        let destination = WGPUTexelCopyTextureInfo {
            texture: texture.raw.0,
            mipLevel: 0,
            origin: WGPUOrigin3D {
                x: u32_of(x, "x")?,
                y: u32_of(y, "y")?,
                z: 0,
            },
            aspect: WGPUTextureAspect_All,
        };
        let height = u32_of(height, "a height")?;
        let layout = WGPUTexelCopyBufferLayout {
            offset: 0,
            bytesPerRow: u32_of(bytes_per_row, "bytes per row")?,
            rowsPerImage: height,
        };
        let size = WGPUExtent3D {
            width: u32_of(width, "a width")?,
            height,
            depthOrArrayLayers: 1,
        };
        unsafe {
            wgpuQueueWriteTexture(
                self.inner.queue.0,
                &destination,
                bytes.as_ptr().cast(),
                bytes.len(),
                &layout,
                &size,
            )
        };
        Ok(())
    }

    /// Settles when the GPU has finished the work submitted so far.
    pub fn completed(&self) -> Future<()> {
        let (completer, future) = hxn_future::pending();
        let callback = WGPUQueueWorkDoneCallbackInfo {
            nextInChain: std::ptr::null_mut(),
            mode: WGPUCallbackMode_AllowSpontaneous,
            callback: Some(work_done),
            userdata1: userdata(completer),
            userdata2: std::ptr::null_mut(),
        };
        unsafe { wgpuQueueOnSubmittedWorkDone(self.inner.queue.0, callback) };
        future
    }
}

/// A `GPUBuffer`.
pub struct Buffer {
    raw: Shared<WGPUBuffer>,
    size: u64,
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe { wgpuBufferRelease(self.raw.0) };
    }
}

/// A buffer mapping in flight: what the callback reads, and the buffer it holds a reference to.
struct Mapping {
    completer: Completer<Vec<u8>>,
    buffer: WGPUBuffer,
    offset: usize,
    length: usize,
}

unsafe extern "C" fn mapped(
    status: WGPUMapAsyncStatus,
    message: WGPUStringView,
    userdata1: *mut c_void,
    _: *mut c_void,
) {
    let mapping = *unsafe { Box::from_raw(userdata1.cast::<Mapping>()) };
    if status == WGPUMapAsyncStatus_Success {
        let range = unsafe {
            wgpuBufferGetConstMappedRange(mapping.buffer, mapping.offset, mapping.length)
        }
        .cast::<u8>();
        let bytes = if range.is_null() {
            None
        } else {
            Some(unsafe { std::slice::from_raw_parts(range, mapping.length) }.to_vec())
        };
        unsafe { wgpuBufferUnmap(mapping.buffer) };
        match bytes {
            Some(bytes) => mapping.completer.resolve(bytes),
            None => mapping
                .completer
                .reject("the mapped range could not be read"),
        };
    } else {
        mapping
            .completer
            .reject(format!("mapping the buffer failed: {}", text(message)));
    }
    unsafe { wgpuBufferRelease(mapping.buffer) };
}

impl Buffer {
    pub fn size(&self) -> i64 {
        self.size as i64
    }

    /// Maps `length` bytes at `offset` for reading, copies them out and unmaps the buffer.
    /// The buffer needs `BufferUsage.MapRead`; `offset` is a multiple of 8, `length` of 4.
    pub fn read(&self, offset: i64, length: i64) -> Result<Future<Vec<u8>>, String> {
        let offset =
            usize::try_from(offset).map_err(|_| format!("an offset cannot be {offset}"))?;
        let length = usize::try_from(length).map_err(|_| format!("a length cannot be {length}"))?;
        let (completer, future) = hxn_future::pending();
        unsafe { wgpuBufferAddRef(self.raw.0) };
        let mapping = Box::new(Mapping {
            completer,
            buffer: self.raw.0,
            offset,
            length,
        });
        let callback = WGPUBufferMapCallbackInfo {
            nextInChain: std::ptr::null_mut(),
            mode: WGPUCallbackMode_AllowSpontaneous,
            callback: Some(mapped),
            userdata1: Box::into_raw(mapping).cast(),
            userdata2: std::ptr::null_mut(),
        };
        unsafe { wgpuBufferMapAsync(self.raw.0, WGPUMapMode_Read, offset, length, callback) };
        Ok(future)
    }
}

/// A 2D `GPUTexture` of one level, with one view of all of it.
pub struct Texture {
    raw: Shared<WGPUTexture>,
    view: Shared<WGPUTextureView>,
    width: i32,
    height: i32,
}

impl Drop for Texture {
    fn drop(&mut self) {
        unsafe {
            wgpuTextureViewRelease(self.view.0);
            wgpuTextureRelease(self.raw.0);
        }
    }
}

impl Texture {
    fn extent(&self) -> WGPUExtent3D {
        WGPUExtent3D {
            width: self.width as u32,
            height: self.height as u32,
            depthOrArrayLayers: 1,
        }
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

/// A resource of a bind group entry, with the reference it holds until the entry is dropped.
struct Entry {
    raw: WGPUBindGroupEntry,
    release: Option<unsafe extern "C" fn(*mut c_void)>,
    object: *mut c_void,
}

impl Drop for Entry {
    fn drop(&mut self) {
        if let Some(release) = self.release {
            unsafe { release(self.object) };
        }
    }
}

/// The resources of a bind group, by binding, before `Device.createBindGroup` makes it.
pub struct BindGroupEntries {
    entries: Vec<Entry>,
}

unsafe impl Send for BindGroupEntries {}
unsafe impl Sync for BindGroupEntries {}

fn blank(binding: i32) -> Result<WGPUBindGroupEntry, String> {
    let mut raw: WGPUBindGroupEntry = unsafe { std::mem::zeroed() };
    raw.binding = u32_of(binding, "a binding")?;
    Ok(raw)
}

unsafe extern "C" fn release_buffer(object: *mut c_void) {
    unsafe { wgpuBufferRelease(object.cast()) };
}

unsafe extern "C" fn release_view(object: *mut c_void) {
    unsafe { wgpuTextureViewRelease(object.cast()) };
}

unsafe extern "C" fn release_sampler(object: *mut c_void) {
    unsafe { wgpuSamplerRelease(object.cast()) };
}

impl BindGroupEntries {
    pub fn create() -> BindGroupEntries {
        BindGroupEntries {
            entries: Vec::new(),
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
        let mut raw = blank(binding)?;
        raw.buffer = buffer.raw.0;
        raw.offset = u64_of(offset, "an offset")?;
        raw.size = if size < 0 { WHOLE_SIZE } else { size as u64 };
        unsafe { wgpuBufferAddRef(buffer.raw.0) };
        self.entries.push(Entry {
            raw,
            release: Some(release_buffer),
            object: buffer.raw.0.cast(),
        });
        Ok(())
    }

    /// The view of `texture` at `binding`.
    pub fn texture(&mut self, binding: i32, texture: &Texture) -> Result<(), String> {
        let mut raw = blank(binding)?;
        raw.textureView = texture.view.0;
        unsafe { wgpuTextureViewAddRef(texture.view.0) };
        self.entries.push(Entry {
            raw,
            release: Some(release_view),
            object: texture.view.0.cast(),
        });
        Ok(())
    }

    /// `sampler` at `binding`.
    pub fn sampler(&mut self, binding: i32, sampler: &Sampler) -> Result<(), String> {
        let mut raw = blank(binding)?;
        raw.sampler = sampler.raw.0;
        unsafe { wgpuSamplerAddRef(sampler.raw.0) };
        self.entries.push(Entry {
            raw,
            release: Some(release_sampler),
            object: sampler.raw.0.cast(),
        });
        Ok(())
    }
}

/// A `GPUCommandEncoder`, recording until `Device.submit`.
pub struct CommandEncoder {
    raw: Shared<WGPUCommandEncoder>,
    finished: bool,
}

impl Drop for CommandEncoder {
    fn drop(&mut self) {
        unsafe { wgpuCommandEncoderRelease(self.raw.0) };
    }
}

impl CommandEncoder {
    /// A compute pass, recording until its `end`.
    pub fn begin_compute_pass(&mut self) -> Result<ComputePass, String> {
        let raw = unsafe { wgpuCommandEncoderBeginComputePass(self.raw.0, std::ptr::null()) };
        if raw.is_null() {
            return Err("beginning the compute pass failed".into());
        }
        Ok(ComputePass {
            raw: Shared(raw),
            ended: false,
        })
    }

    /// A render pass into `target`, cleared first to the color `r`, `g`, `b`, `a`.
    pub fn begin_render_pass(
        &mut self,
        target: &Texture,
        keep: bool,
        r: f64,
        g: f64,
        b: f64,
        a: f64,
    ) -> Result<RenderPass, String> {
        let attachment = WGPURenderPassColorAttachment {
            nextInChain: std::ptr::null_mut(),
            view: target.view.0,
            depthSlice: DEPTH_SLICE_UNDEFINED,
            resolveTarget: std::ptr::null_mut(),
            loadOp: if keep {
                WGPULoadOp_Load
            } else {
                WGPULoadOp_Clear
            },
            storeOp: WGPUStoreOp_Store,
            clearValue: WGPUColor { r, g, b, a },
        };
        let mut descriptor: WGPURenderPassDescriptor = unsafe { std::mem::zeroed() };
        descriptor.label = unset();
        descriptor.colorAttachmentCount = 1;
        descriptor.colorAttachments = &attachment;
        let raw = unsafe { wgpuCommandEncoderBeginRenderPass(self.raw.0, &descriptor) };
        if raw.is_null() {
            return Err("beginning the render pass failed".into());
        }
        Ok(RenderPass {
            raw: Shared(raw),
            ended: false,
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
        unsafe {
            wgpuCommandEncoderCopyBufferToBuffer(
                self.raw.0,
                source.raw.0,
                u64_of(source_offset, "an offset")?,
                destination.raw.0,
                u64_of(destination_offset, "an offset")?,
                u64_of(size, "a size")?,
            )
        };
        Ok(())
    }

    /// Copies all of `texture` into `buffer`, rows `bytes_per_row` apart, a multiple of 256.
    pub fn copy_texture_to_buffer(
        &mut self,
        texture: &Texture,
        buffer: &Buffer,
        bytes_per_row: i32,
    ) -> Result<(), String> {
        let source = WGPUTexelCopyTextureInfo {
            texture: texture.raw.0,
            mipLevel: 0,
            origin: WGPUOrigin3D { x: 0, y: 0, z: 0 },
            aspect: WGPUTextureAspect_All,
        };
        let destination = WGPUTexelCopyBufferInfo {
            layout: WGPUTexelCopyBufferLayout {
                offset: 0,
                bytesPerRow: u32_of(bytes_per_row, "bytes per row")?,
                rowsPerImage: texture.height as u32,
            },
            buffer: buffer.raw.0,
        };
        let size = texture.extent();
        unsafe { wgpuCommandEncoderCopyTextureToBuffer(self.raw.0, &source, &destination, &size) };
        Ok(())
    }
}

/// A `GPUComputePassEncoder`.
pub struct ComputePass {
    raw: Shared<WGPUComputePassEncoder>,
    ended: bool,
}

impl Drop for ComputePass {
    fn drop(&mut self) {
        unsafe { wgpuComputePassEncoderRelease(self.raw.0) };
    }
}

impl ComputePass {
    pub fn set_pipeline(&mut self, pipeline: &ComputePipeline) {
        unsafe { wgpuComputePassEncoderSetPipeline(self.raw.0, pipeline.raw.0) };
    }

    pub fn set_bind_group(&mut self, index: i32, group: &BindGroup) -> Result<(), String> {
        let index = u32_of(index, "a group index")?;
        unsafe {
            wgpuComputePassEncoderSetBindGroup(self.raw.0, index, group.raw.0, 0, std::ptr::null())
        };
        Ok(())
    }

    pub fn dispatch(&mut self, x: i32, y: i32, z: i32) -> Result<(), String> {
        unsafe {
            wgpuComputePassEncoderDispatchWorkgroups(
                self.raw.0,
                u32_of(x, "a workgroup count")?,
                u32_of(y, "a workgroup count")?,
                u32_of(z, "a workgroup count")?,
            )
        };
        Ok(())
    }

    /// Ends the pass; the encoder records again after it.
    pub fn end(&mut self) -> Result<(), String> {
        if self.ended {
            return Err("the compute pass has ended already".into());
        }
        self.ended = true;
        unsafe { wgpuComputePassEncoderEnd(self.raw.0) };
        Ok(())
    }
}

/// A `GPURenderPassEncoder`.
pub struct RenderPass {
    raw: Shared<WGPURenderPassEncoder>,
    ended: bool,
}

impl Drop for RenderPass {
    fn drop(&mut self) {
        unsafe { wgpuRenderPassEncoderRelease(self.raw.0) };
    }
}

impl RenderPass {
    pub fn set_pipeline(&mut self, pipeline: &RenderPipeline) {
        unsafe { wgpuRenderPassEncoderSetPipeline(self.raw.0, pipeline.raw.0) };
    }

    pub fn set_bind_group(&mut self, index: i32, group: &BindGroup) -> Result<(), String> {
        let index = u32_of(index, "a group index")?;
        unsafe {
            wgpuRenderPassEncoderSetBindGroup(self.raw.0, index, group.raw.0, 0, std::ptr::null())
        };
        Ok(())
    }

    /// Reads vertices for `slot` from `buffer`, starting `offset` bytes in.
    pub fn set_vertex_buffer(
        &mut self,
        slot: i32,
        buffer: &Buffer,
        offset: i64,
    ) -> Result<(), String> {
        let slot = u32_of(slot, "a slot")?;
        let offset = u64_of(offset, "an offset")?;
        unsafe {
            wgpuRenderPassEncoderSetVertexBuffer(self.raw.0, slot, buffer.raw.0, offset, WHOLE_SIZE)
        };
        Ok(())
    }

    pub fn set_viewport(&mut self, x: f64, y: f64, width: f64, height: f64) -> Result<(), String> {
        unsafe {
            wgpuRenderPassEncoderSetViewport(
                self.raw.0,
                x as f32,
                y as f32,
                width as f32,
                height as f32,
                0.0,
                1.0,
            )
        };
        Ok(())
    }

    pub fn set_scissor_rect(
        &mut self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Result<(), String> {
        unsafe {
            wgpuRenderPassEncoderSetScissorRect(
                self.raw.0,
                u32_of(x, "x")?,
                u32_of(y, "y")?,
                u32_of(width, "a width")?,
                u32_of(height, "a height")?,
            )
        };
        Ok(())
    }

    pub fn draw(
        &mut self,
        vertices: i32,
        instances: i32,
        first_vertex: i32,
        first_instance: i32,
    ) -> Result<(), String> {
        unsafe {
            wgpuRenderPassEncoderDraw(
                self.raw.0,
                u32_of(vertices, "a vertex count")?,
                u32_of(instances, "an instance count")?,
                u32_of(first_vertex, "a first vertex")?,
                u32_of(first_instance, "a first instance")?,
            )
        };
        Ok(())
    }

    /// Ends the pass; the encoder records again after it.
    pub fn end(&mut self) -> Result<(), String> {
        if self.ended {
            return Err("the render pass has ended already".into());
        }
        self.ended = true;
        unsafe { wgpuRenderPassEncoderEnd(self.raw.0) };
        Ok(())
    }
}

/// The web page's canvas as a `WGPUSurface`.
pub struct Surface {
    raw: Shared<WGPUSurface>,
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe { wgpuSurfaceRelease(self.raw.0) };
    }
}

impl Surface {
    pub fn preferred_format(&self, adapter: &Adapter) -> Format {
        let mut capabilities: WGPUSurfaceCapabilities = unsafe { std::mem::zeroed() };
        let mut format = Format::Bgra8Unorm;
        if unsafe { wgpuSurfaceGetCapabilities(self.raw.0, adapter.raw.0, &mut capabilities) }
            == WGPUStatus_Success
        {
            if capabilities.formatCount > 0
                && unsafe { *capabilities.formats } == WGPUTextureFormat_RGBA8Unorm
            {
                format = Format::Rgba8Unorm;
            }
            unsafe { wgpuSurfaceCapabilitiesFreeMembers(capabilities) };
        }
        format
    }

    pub fn configure(
        &self,
        device: &Device,
        format: Format,
        usage: i32,
        width: i32,
        height: i32,
    ) -> Result<(), String> {
        let configuration = WGPUSurfaceConfiguration {
            nextInChain: std::ptr::null_mut(),
            device: device.inner.device.0,
            format: format as WGPUTextureFormat,
            usage: usage as WGPUTextureUsage,
            width: u32_of(width, "a width")?,
            height: u32_of(height, "a height")?,
            viewFormatCount: 0,
            viewFormats: std::ptr::null(),
            alphaMode: WGPUCompositeAlphaMode_Opaque,
            presentMode: WGPUPresentMode_Fifo,
        };
        unsafe { wgpuSurfaceConfigure(self.raw.0, &configuration) };
        Ok(())
    }

    pub fn current_texture(&self) -> Result<Texture, String> {
        let mut frame: WGPUSurfaceTexture = unsafe { std::mem::zeroed() };
        unsafe { wgpuSurfaceGetCurrentTexture(self.raw.0, &mut frame) };
        if frame.texture.is_null()
            || (frame.status != WGPUSurfaceGetCurrentTextureStatus_SuccessOptimal
                && frame.status != WGPUSurfaceGetCurrentTextureStatus_SuccessSuboptimal)
        {
            return Err(format!(
                "the canvas has no texture this frame (status {})",
                frame.status
            ));
        }
        let raw = frame.texture;
        let width = unsafe { wgpuTextureGetWidth(raw) } as i32;
        let height = unsafe { wgpuTextureGetHeight(raw) } as i32;
        let view = unsafe { wgpuTextureCreateView(raw, std::ptr::null()) };
        Ok(Texture {
            raw: Shared(raw),
            view: Shared(view),
            width,
            height,
        })
    }

    /// The browser shows the frame when the program returns to its event loop.
    pub fn present(&self) {}
}
