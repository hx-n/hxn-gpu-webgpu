# hxn-gpu-webgpu

[![Browser](https://github.com/hx-n/hxn-gpu-webgpu/actions/workflows/browser.yml/badge.svg)](https://github.com/hx-n/hxn-gpu-webgpu/actions/workflows/browser.yml)

WebGPU for Haxe in the browser. It keeps WebGPU's own API: an instance
gives an adapter, an adapter a device, and a device makes buffers, textures,
bind groups and pipelines, and submits what command encoders record. The
library is Rust, with a backend for each way a Haxe program reaches a page:

| Runtime | In the browser |
|---|---|
| hxcpp | Yes: `-cpp` with `-D emscripten`, which compiles the program to wasm. The library calls webgpu.h, which Emscripten's emdawnwebgpu port implements on the page's `navigator.gpu`. |
| Ash | Yes, threaded: `-hl` with `-D wasm -D hxn_threads`, which Ash builds for `wasm32-wasip1-threads`. The library is a side module that encodes its calls onto a wire in shared memory, generated from WebGPU's IDL by [x-idl](https://github.com/rayzor-blade/x-idl) as xgpu's is; an agent the page starts through `ash_host_agent` runs them on `navigator.gpu` in a Worker of its own. |
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

For Ash, with [Ash](https://github.com/rayzor-blade/ash) 0.1.18 or later:

```
haxe -lib hxn-gpu-webgpu -lib hxnsl -main Main -hl out/main.hl -D wasm -D hxn_threads
ash --build out/main.wasm --target wasm32-wasip1-threads out/main.hl
ash serve out
```

The haxelib carries the side module, built for `wasm32-wasip1-threads`, and
the agent's files (`gpu_webgpu.mjs`, the Worker and the wire's service),
which the compile copies beside `main.hl`. Ash writes the page beside
`main.wasm`. A threaded program needs shared memory, so the page has to be
served with the cross-origin isolation headers, which `ash serve` sends.

Under hxcpp, use `then` in a page: `await` blocks the page's only thread,
and the browser's callbacks never run while it does. An Ash program runs in
a Worker, where waiting does not block the page.

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
later, outside any error scope, goes to the console: under hxcpp to stderr,
as `WebGPU validation error: ...`; under Ash to the console of the agent's
Worker, as `WebGPU error: ...`, which the program does not see.

## Shaders from hxnsl

A shader for this driver implements `gpu.webgpu.Shader`, which extends
`hxnsl.Shader` and gets hxnsl's `WGSL` and `LAYOUT` but none of its other
languages. `gpu.webgpu.Shaders.bindGroupLayouts(device, Shader.LAYOUT)`
makes the bind group layouts. The entry points are `vertex_main`,
`fragment_main` and `compute_main`, and a texture's sampler has the binding
after the texture's. Pipeline constants are `{key: Shader.CONST_<name>, value}`.

## Building from source

Requires the [hxn](https://github.com/hx-n/hxn) CLI, Rust nightly with
`rust-src` and the `wasm32-unknown-emscripten` and `wasm32-wasip1-threads`
targets, Haxe 4.3, hxcpp from git, hxnsl, the Emscripten SDK, Ash, node 22
and Chrome. `hxn check ... --browser` builds the library and the check as a
page, opens it in headless Chrome and prints what the program prints.
`HXN_BROWSER_ARGS` adds Chrome flags; CI runs both on macOS with the
runner's GPU and on Linux with SwiftShader:

```
HXN_BROWSER_ARGS=--enable-unsafe-webgpu hxn check cpp --wasm --browser
HXN_BROWSER_ARGS=--enable-unsafe-webgpu hxn check hl --wasm --threads --browser
```

`src/agent/wire.rs` and `page/gpu_webgpu_service.mjs` are the wire,
generated from xgpu's copy of WebGPU's IDL by `tools/wire`:
`cargo run --manifest-path tools/wire/Cargo.toml -- ../xgpu/api/spec/webgpu.idl ../xgpu/api/spec/canvas.idl`.

`src/ffi.rs` is webgpu.h from the emdawnwebgpu port, generated by bindgen;
its header describes how to generate it again.

## Agent skill

[`skills/hxn-gpu-webgpu/SKILL.md`](https://github.com/hx-n/hxn-gpu-webgpu/blob/main/skills/hxn-gpu-webgpu/SKILL.md)
describes the library for coding agents. The haxelib includes it.

## License

Apache-2.0.
