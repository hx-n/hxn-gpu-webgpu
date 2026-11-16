# hxn-gpu-webgpu

[![Browser](https://github.com/hx-n/hxn-gpu-webgpu/actions/workflows/browser.yml/badge.svg)](https://github.com/hx-n/hxn-gpu-webgpu/actions/workflows/browser.yml)

WebGPU bindings for Haxe programs that run in the browser. The API follows
WebGPU: `Instance`, `Adapter`, `Device`, buffers, textures, bind groups,
pipelines and command encoders. The library is written in Rust.

| Runtime | Browser support |
|---|---|
| hxcpp | Supported, with or without threads. Compile with `-cpp -D emscripten`, and add `-D hxn_threads` for threads. |
| Ash | Supported in threaded builds. Compile with `-hl -D wasm -D hxn_threads`. |
| HashLink | Not supported: the HashLink VM does not run in a browser. Use Ash. |
| CPPIA | Not supported: the script host is a native hxcpp program. |

Outside the browser, use the native drivers:
[hxn-gpu-metal](https://github.com/hx-n/hxn-gpu-metal),
[hxn-gpu-vulkan](https://github.com/hx-n/hxn-gpu-vulkan),
[hxn-gpu-d3d12](https://github.com/hx-n/hxn-gpu-d3d12) and
[hxn-gpu-gles](https://github.com/hx-n/hxn-gpu-gles).

Shaders come from [hxnsl](https://github.com/hx-n/hxnsl), which generates WGSL
and the bind group layouts (`LAYOUT`) at compile time. The browser compiles
the WGSL.

`requestAdapter`, `requestDevice`, `mapAsync` and `onSubmittedWorkDone`
return an `hxn.Future`, which resolves when the browser's promise does:

```haxe
import hxn.gpu.webgpu.*;

Instance.create().requestAdapter().then(adapter -> {
	adapter.requestDevice().then(device -> {
		var module = device.createShaderModule(Tripled.WGSL);             // an hxnsl shader
		var layouts = hxn.gpu.webgpu.Shaders.bindGroupLayouts(device, Tripled.LAYOUT);
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
texture, a pass sampling a texture, and a frame of the web page's canvas,
each read back through a mapped buffer.

## Compiling

```
haxelib install hxn-gpu-webgpu
haxelib install hxnsl
haxe -lib hxn-gpu-webgpu -lib hxnsl -main Main -cpp out -D emscripten -D disable_unicode_strings -D HXCPP_LINK_EMSCRIPTEN_EXT=.html
```

Requires the [Emscripten SDK](https://emscripten.org/docs/getting_started/downloads.html)
on `PATH` and hxcpp from git (its Emscripten toolchain uses wasm exceptions).
The output is `out/Main.html` with `Main.js` and `Main.wasm`; serve the
directory and open it in a browser with WebGPU. The haxelib ships the
library prebuilt for `wasm32-unknown-emscripten` and links Emscripten's
WebGPU port (`--use-port=emdawnwebgpu`), so no Rust toolchain is needed.

With `-D hxn_threads`, the program links the library built for threaded wasm
and runs on Emscripten's pthreads. Its `main` runs on a pthread, and the web
page's canvas is transferred to that thread. Threaded builds use shared
memory, so the web page must be served with cross-origin isolation headers
(COOP/COEP), which `emrun` sends:

```
haxe -lib hxn-gpu-webgpu -lib hxnsl -main Main -cpp out -D emscripten -D hxn_threads -D disable_unicode_strings -D HXCPP_LINK_EMSCRIPTEN_EXT=.html
emrun out/Main.html
```

For Ash, with [Ash](https://github.com/rayzor-blade/ash) 0.1.20 or later:

```
haxe -lib hxn-gpu-webgpu -lib hxnsl -main Main -hl out/main.hl -D wasm -D hxn_threads
ash --build out/main.wasm --target wasm32-wasip1-threads --native-lib out/libgpu_webgpu.a out/main.hl
ash serve out
```

The haxelib ships the library for `wasm32-wasip1-threads` and its JavaScript
files; compiling copies them next to `main.hl`, Ash links the library into
`main.wasm` and writes the web page next to it. Threaded builds use shared
memory, so the web page must be served with cross-origin isolation headers
(COOP/COEP), which `ash serve` sets.

Under Ash, WebGPU runs in a browser agent, a Worker the web page starts, which
takes the page's canvas only when the program calls `instance.claimCanvas()`.
Claim it before `createCanvasSurface`. A page can carry other browser agents,
such as hxn-gpu-webgl2's, and the canvas goes to the first that claims it. An
agent's command that throws ends the web page with the command's name and the
error.

Under hxcpp, with or without threads, deliver a future with `then`, not
`await`. The browser resolves WebGPU's promises on the thread the program
runs on, and only while that thread is not blocked. Under Ash, `await` works:
WebGPU runs in a browser agent of its own, a separate Worker.

## Classes

All in the package `hxn.gpu.webgpu`:

| Class | |
|---|---|
| `Instance` | `create()`, `requestAdapter()`, `claimCanvas()` and `createCanvasSurface()`. |
| `Surface` | The web page's canvas, once `claimCanvas()` has settled: `preferredFormat(adapter)`, `configure(device, format, usage, width, height)` and `currentTexture()`. A frame is shown when the program returns to the browser's event loop. |
| `Adapter` | `info()` and `requestDevice()`. |
| `Device` | Buffers, textures, samplers, shader modules, bind group layouts, pipeline layouts, pipelines, bind groups and command encoders; `submit`, `writeBuffer`, `writeTexture` and `completed`. |
| `Buffer` | `read(offset, length)`, a future of the bytes, through a mapping. |
| `Texture` | A 2D texture of one level, with one view of all of it. |
| `BindGroupLayouts`, `PipelineLayout` | The bind group layouts of a pipeline, and the pipeline layout of them. |
| `BindGroupEntries`, `BindGroup` | The resources of a bind group by binding, and the bind group made of them. |
| `ComputePipeline`, `RenderPipeline` | A compute pipeline, and a render pipeline drawing into one color target from one vertex buffer. |
| `CommandEncoder`, `ComputePass`, `RenderPass` | Passes and copies, recorded until `Device.submit`. |

Every object holds its WebGPU object until `close()`. A failure WebGPU
reports at once is thrown as a Haxe exception. Validation errors are logged
in the browser console.

## Shaders from hxnsl

A shader for this driver implements `hxn.gpu.webgpu.Shader`, which extends
`hxnsl.Shader` and generates only `WGSL` and `LAYOUT`.

- `hxn.gpu.webgpu.Shaders.bindGroupLayouts(device, Shader.LAYOUT)` creates the
  bind group layouts.
- Entry points are `vertex_main`, `fragment_main` and `compute_main`.
- A texture's sampler is at the texture's binding plus one.
- Pipeline constants are `{key: Shader.CONST_<name>, value}`.

## Building from source

Requires the [hxn](https://github.com/hx-n/hxn) CLI, Rust nightly with
`rust-src` and the `wasm32-unknown-emscripten` and `wasm32-wasip1-threads`
targets, Haxe 4.3, hxcpp from git, hxnsl, the Emscripten SDK, Ash, node 22
and Chrome. `hxn check ... --browser` builds the library and the check as a
page, opens it in headless Chrome and prints what the program prints.
`HXN_BROWSER_ARGS` adds Chrome flags; CI runs each on macOS with the
runner's GPU and on Linux with SwiftShader:

```
HXN_BROWSER_ARGS=--enable-unsafe-webgpu hxn check cpp --wasm --browser
HXN_BROWSER_ARGS=--enable-unsafe-webgpu hxn check cpp --wasm --threads --browser
HXN_BROWSER_ARGS=--enable-unsafe-webgpu hxn check hl --wasm --threads --browser
```

To regenerate `src/agent/wire.rs` and `page/gpu_webgpu_service.mjs` from
WebGPU's IDL:

```
cargo run --manifest-path tools/wire/Cargo.toml -- ../xgpu/api/spec/webgpu.idl ../xgpu/api/spec/canvas.idl
```

`src/ffi.rs` is webgpu.h from the emdawnwebgpu port, generated by bindgen;
its header describes how to generate it again.

## Agent skill

[`skills/hxn-gpu-webgpu/SKILL.md`](https://github.com/hx-n/hxn-gpu-webgpu/blob/main/skills/hxn-gpu-webgpu/SKILL.md)
describes the library for coding agents. The haxelib includes it.

## License

Apache-2.0.
