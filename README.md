# hxn-gpu-webgpu

[![Browser](https://github.com/hx-n/hxn-gpu-webgpu/actions/workflows/browser.yml/badge.svg)](https://github.com/hx-n/hxn-gpu-webgpu/actions/workflows/browser.yml)

WebGPU for Haxe in the browser. It keeps WebGPU's own API: an instance
gives an adapter, an adapter a device, and a device makes buffers, textures,
bind groups and pipelines, and submits what command encoders record. The
library is Rust over webgpu.h, which Emscripten's emdawnwebgpu port
implements on the page's `navigator.gpu`.

| Runtime | In the browser |
|---|---|
| hxcpp | Yes: `-cpp` with `-D emscripten`, which compiles the program to wasm. |
| Ash | Not yet. Ash compiles HashLink bytecode to wasm, and loads this library as a side module that reaches WebGPU through `ash_host_agent`; that backend is next. |
| HashLink | No: the HashLink VM does not run in a browser; Ash is how HashLink bytecode reaches one. |
| CPPIA | No: hxn builds the script's host as a native hxcpp program. |

Outside the browser, HashLink, Ash, hxcpp and CPPIA use the native drivers:
[hxn-gpu-metal](https://github.com/hx-n/hxn-gpu-metal),
[hxn-gpu-vulkan](https://github.com/hx-n/hxn-gpu-vulkan),
[hxn-gpu-d3d12](https://github.com/hx-n/hxn-gpu-d3d12) and
[hxn-gpu-gles](https://github.com/hx-n/hxn-gpu-gles).

Shaders come from [hxnsl](https://github.com/hx-n/hxnsl), which writes WGSL
at compile time and reports the bind group layouts in `LAYOUT`. The browser
compiles the WGSL itself.

WebGPU's asynchronous calls return promises in the browser:
`requestAdapter`, `requestDevice`, `mapAsync` and `onSubmittedWorkDone`.
Here they return an `hxn.Future`, which resolves when the promise does; its
`then` callback runs on the page's event loop:

```haxe
import gpu.webgpu.*;

Instance.create().requestAdapter().then(adapter -> {
	adapter.requestDevice().then(device -> {
		var module = device.createShaderModule(Tripled.WGSL);             // an hxnsl shader
		var layouts = gpu.webgpu.Shaders.bindGroupLayouts(device, Tripled.LAYOUT);
		var pipeline = device.createComputePipeline(module, "compute_main", device.createPipelineLayout(layouts), []);
		var data = device.createBufferWith(input, BufferUsage.Storage | BufferUsage.CopySrc);
		var entries = BindGroupEntries.create();
		entries.buffer(Tripled.BUFFER_data, data, 0, -1);
		var group = device.createBindGroup(layouts, Tripled.BUFFER_data_GROUP, entries);

		var encoder = device.createCommandEncoder();
		var pass = encoder.beginComputePass();
		pass.setPipeline(pipeline);
		pass.setBindGroup(Tripled.BUFFER_data_GROUP, group);
		pass.dispatch(4, 1, 1);
		pass.end();
		encoder.copyBufferToBuffer(data, 0, readback, 0, input.length);
		device.submit(encoder);
		readback.read(0, input.length).then(bytes -> trace(bytes));
	});
});
```

[`examples/check/Main.hx`](https://github.com/hx-n/hxn-gpu-webgpu/blob/main/examples/check/Main.hx)
is the whole program: a compute pass over a buffer, a render pass into a
texture, and a pass sampling a texture, each read back through a mapped
buffer.

## Compiling

```
haxelib install hxn-gpu-webgpu
haxelib install hxnsl
haxe -lib hxn-gpu-webgpu -lib hxnsl -main Main -cpp out -D emscripten -D disable_unicode_strings -D HXCPP_LINK_EMSCRIPTEN_EXT=.html
```

That needs the [Emscripten SDK](https://emscripten.org/docs/getting_started/downloads.html)
on `PATH` and hxcpp from git, whose Emscripten toolchain compiles with wasm
exceptions. `out/Main.html` is the page, beside `Main.js` and `Main.wasm`;
serve the directory and open it in a browser with WebGPU. The haxelib
carries the library prebuilt for `wasm32-unknown-emscripten`, and its link
adds Emscripten's WebGPU (`--use-port=emdawnwebgpu`); a program needs no
Rust.

In a page, use `then`: `await` blocks the page's only thread, and the
browser's callbacks never run while it does.

## Classes

All in the package `gpu.webgpu`:

| Class | |
|---|---|
| `Instance` | `create()` and `requestAdapter()`. |
| `Adapter` | `info()` and `requestDevice()`. |
| `Device` | Buffers, textures, samplers, shader modules, bind group layouts, pipeline layouts, pipelines, bind groups and command encoders; `submit`, `writeBuffer`, `writeTexture` and `completed`. |
| `Buffer` | `read(offset, length)`, a future of the bytes, through a mapping. |
| `Texture` | A 2D texture of one level, with one view of all of it. |
| `BindGroupLayouts`, `PipelineLayout` | The bind group layouts of a pipeline, and the pipeline layout of them. |
| `BindGroupEntries`, `BindGroup` | The resources of a bind group by binding, and the bind group made of them. |
| `ComputePipeline`, `RenderPipeline` | A compute pipeline, and a render pipeline drawing into one color target from one vertex buffer. |
| `CommandEncoder`, `ComputePass`, `RenderPass` | Passes and copies, recorded until `Device.submit`. |

Every object holds its WebGPU object until `close()`. A failure WebGPU
reports at once is thrown as a Haxe exception. A validation error it reports
later, outside any error scope, is printed to stderr as
`WebGPU validation error: ...`.

## Shaders from hxnsl

A shader for this driver implements `gpu.webgpu.Shader`, which extends
`hxnsl.Shader` and gets hxnsl's `WGSL` and `LAYOUT` but none of its other
languages. `gpu.webgpu.Shaders.bindGroupLayouts(device, Shader.LAYOUT)`
makes the bind group layouts. The entry points are `vertex_main`,
`fragment_main` and `compute_main`, and a texture's sampler has the binding
after the texture's. Pipeline constants are `{key: Shader.CONST_<name>, value}`.

## Building from source

Requires the [hxn](https://github.com/hx-n/hxn) CLI, Rust with the
`wasm32-unknown-emscripten` target, Haxe 4.3, hxcpp from git, hxnsl, the
Emscripten SDK and Chrome. `hxn check cpp --wasm --browser` builds the
library and the check as a page, serves it with `emrun` to headless Chrome,
and prints what the page prints. `HXN_BROWSER_ARGS` adds Chrome flags; CI
runs it on macOS with the runner's GPU and on Linux with SwiftShader:

```
HXN_BROWSER_ARGS=--enable-unsafe-webgpu hxn check cpp --wasm --browser
```

`src/ffi.rs` is webgpu.h from the emdawnwebgpu port, generated by bindgen;
its header describes how to generate it again.

## Agent skill

[`skills/hxn-gpu-webgpu/SKILL.md`](https://github.com/hx-n/hxn-gpu-webgpu/blob/main/skills/hxn-gpu-webgpu/SKILL.md)
describes the library for coding agents. The haxelib includes it.

## License

Apache-2.0.
