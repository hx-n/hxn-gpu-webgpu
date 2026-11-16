import hxn.gpu.webgpu.*;
import hxn.gpu.webgpu.Shaders as WebGPUShaders;
import haxe.io.Bytes;
import Shaders;

/**
	What CI runs in a web page: a compute shader over a buffer, a render pass into
	a texture, and a pass sampling a texture, each read back through a mapped
	buffer and checked. Every result WebGPU gives later arrives in `then`, from
	the page's event loop. It needs a browser with WebGPU, and prints the same
	lines in each.
**/
class Main {
	static function check(ok:Bool, message:String) {
		if (!ok)
			throw message;
	}

	static function failed(e:haxe.Exception) {
		Sys.println('failed: ${e.message}');
		hxn.Loop.exit(1);
	}

	static function main() {
		var instance = try Instance.create() catch (e:haxe.Exception) {
			Sys.println('no WebGPU: ${e.message}');
			return;
		}
		instance.requestAdapter().then(adapter -> {
			adapter.requestDevice().then(device -> {
				compute(device, () -> render(device, () -> textured(device, () -> canvas(instance, adapter, device, () -> {
					device.close();
					adapter.close();
					instance.close();
				}))));
			}, failed);
		}, e -> Sys.println('no WebGPU adapter: ${e.message}'));
	}

	static function compute(device:Device, next:() -> Void) {
		var module = device.createShaderModule(Tripled.WGSL);
		var layouts = WebGPUShaders.bindGroupLayouts(device, Tripled.LAYOUT);
		var layout = device.createPipelineLayout(layouts);
		var pipeline = device.createComputePipeline(module, "compute_main", layout, []);
		var count = 64;
		var input = Bytes.alloc(count * 4);
		for (i in 0...count)
			input.setInt32(i * 4, i);
		var data = device.createBufferWith(input, BufferUsage.Storage | BufferUsage.CopySrc);
		var readback = device.createBuffer(count * 4, BufferUsage.MapRead | BufferUsage.CopyDst);
		var entries = BindGroupEntries.create();
		entries.buffer(Tripled.BUFFER_data, data, 0, -1);
		var group = device.createBindGroup(layouts, Tripled.BUFFER_data_GROUP, entries);

		var encoder = device.createCommandEncoder();
		var pass = encoder.beginComputePass();
		pass.setPipeline(pipeline);
		pass.setBindGroup(Tripled.BUFFER_data_GROUP, group);
		pass.dispatch(Std.int(count / Tripled.ENTRY_POINTS[0].workgroup[0]), 1, 1);
		pass.end();
		encoder.copyBufferToBuffer(data, 0, readback, 0, count * 4);
		device.submit(encoder);
		Sys.println("compute: submitted");
		readback.read(0, count * 4).then(output -> {
			for (i in 0...count)
				check(output.getInt32(i * 4) == i * 3 + 1, 'value $i is ${output.getInt32(i * 4)}, not ${i * 3 + 1}');
			Sys.println('compute: $count values tripled plus one, the last ${output.getInt32((count - 1) * 4)}');
			for (o in ([pass, encoder, group, entries, readback, data, pipeline, layout, layouts, module] : Array<{function close():Void;}>))
				o.close();
			next();
		}, failed);
	}

	/** Copies a 4x4 Rgba8 target out, then checks every pixel is `expected`. **/
	static function readTarget(device:Device, encoder:CommandEncoder, target:Texture, expected:Array<Int>, label:String, next:() -> Void) {
		// Rows of a texture copy are 256 bytes apart.
		var pitch = 256;
		var readback = device.createBuffer(pitch * 4, BufferUsage.MapRead | BufferUsage.CopyDst);
		encoder.copyTextureToBuffer(target, readback, pitch);
		device.submit(encoder);
		readback.read(0, pitch * 4).then(pixels -> {
			for (y in 0...4)
				for (x in 0...4)
					for (c in 0...4) {
						var at = y * pitch + x * 4 + c;
						check(pixels.get(at) == expected[c], '$label: pixel $x,$y channel $c is ${pixels.get(at)}, not ${expected[c]}');
					}
			Sys.println('$label: 4x4 pixels of ${[for (c in 0...4) pixels.get(c)].join(", ")}');
			readback.close();
			next();
		}, failed);
	}

	static function quad(device:Device, values:Array<Float>):Buffer {
		var bytes = Bytes.alloc(values.length * 4);
		for (i in 0...values.length)
			bytes.setFloat(i * 4, values[i]);
		return device.createBufferWith(bytes, BufferUsage.Vertex);
	}

	static function render(device:Device, next:() -> Void) {
		var module = device.createShaderModule(Tinted.WGSL);
		var layouts = WebGPUShaders.bindGroupLayouts(device, Tinted.LAYOUT);
		var layout = device.createPipelineLayout(layouts);
		var pipeline = device.createRenderPipeline(module, "vertex_main", "fragment_main", layout, Format.Rgba8Unorm, Topology.TriangleStrip, 8,
			[{location: Tinted.INPUT_position, format: VertexFormat.Float32x2, offset: 0}], []);
		var target = device.createTexture(4, 4, Format.Rgba8Unorm, TextureUsage.RenderAttachment | TextureUsage.CopySrc);
		var positions = quad(device, [-1, -1, 1, -1, -1, 1, 1, 1]);
		var params = Bytes.alloc(Tinted.PARAMS_SIZE);
		var tint = [0.25, 0.5, 0.75, 1.0];
		for (i in 0...4)
			params.setFloat(Tinted.PARAMS_tint + i * 4, tint[i]);
		var uniforms = device.createBufferWith(params, BufferUsage.Uniform);
		var entries = BindGroupEntries.create();
		entries.buffer(Tinted.PARAMS_BINDING, uniforms, 0, -1);
		var group = device.createBindGroup(layouts, Tinted.PARAMS_GROUP, entries);

		var encoder = device.createCommandEncoder();
		var pass = encoder.beginRenderPass(target, 0, 0, 0, 0);
		pass.setPipeline(pipeline);
		pass.setBindGroup(Tinted.PARAMS_GROUP, group);
		pass.setVertexBuffer(0, positions);
		pass.draw(4, 1, 0, 0);
		pass.end();
		readTarget(device, encoder, target, [for (c in tint) Math.round(c * 255)], "render", () -> {
			for (o in ([pass, encoder, group, entries, uniforms, positions, target, pipeline, layout, layouts, module] : Array<{function close():Void;}>))
				o.close();
			next();
		});
	}

	/** A frame of the web page's canvas cleared to red, copied out before it is shown. **/
	static function canvas(instance:Instance, adapter:Adapter, device:Device, next:() -> Void) {
		var surface = instance.createCanvasSurface();
		var format = surface.preferredFormat(adapter);
		surface.configure(device, format, TextureUsage.RenderAttachment | TextureUsage.CopySrc, 64, 64);
		var frame = surface.currentTexture();
		var encoder = device.createCommandEncoder();
		var pass = encoder.beginRenderPass(frame, 1, 0, 0, 1);
		pass.end();
		var pitch = 256;
		var readback = device.createBuffer(pitch * 64, BufferUsage.MapRead | BufferUsage.CopyDst);
		encoder.copyTextureToBuffer(frame, readback, pitch);
		device.submit(encoder);
		// Red is the first channel of Rgba8 and the third of Bgra8.
		var expected = format == Format.Rgba8Unorm ? [255, 0, 0, 255] : [0, 0, 255, 255];
		readback.read(0, pitch * 64).then(pixels -> {
			for (c in 0...4)
				check(pixels.get(c) == expected[c], 'canvas: channel $c is ${pixels.get(c)}, not ${expected[c]}');
			Sys.println('canvas: a ${frame.width()}x${frame.height()} frame cleared to red');
			for (o in ([pass, encoder, readback, frame, surface] : Array<{function close():Void;}>))
				o.close();
			next();
		}, failed);
	}

	static function textured(device:Device, next:() -> Void) {
		var module = device.createShaderModule(Textured.WGSL);
		var layouts = WebGPUShaders.bindGroupLayouts(device, Textured.LAYOUT);
		var layout = device.createPipelineLayout(layouts);
		var pipeline = device.createRenderPipeline(module, "vertex_main", "fragment_main", layout, Format.Rgba8Unorm, Topology.TriangleStrip, 16, [
			{location: Textured.INPUT_position, format: VertexFormat.Float32x2, offset: 0},
			{location: Textured.INPUT_uv, format: VertexFormat.Float32x2, offset: 8}
		], []);

		// A 2x2 image of one color.
		var color = [40, 80, 120, 255];
		var image = device.createTexture(2, 2, Format.Rgba8Unorm, TextureUsage.TextureBinding | TextureUsage.CopyDst);
		var pixels = Bytes.alloc(2 * 2 * 4);
		for (i in 0...4)
			for (c in 0...4)
				pixels.set(i * 4 + c, color[c]);
		device.writeTexture(image, pixels, 2 * 4);
		var sampler = device.createSampler(Filter.Nearest, AddressMode.ClampToEdge);
		var entries = BindGroupEntries.create();
		entries.texture(Textured.TEXTURE_image, image);
		// The image's sampler has the next binding.
		entries.sampler(Textured.TEXTURE_image + 1, sampler);
		var group = device.createBindGroup(layouts, Textured.TEXTURE_image_GROUP, entries);

		var vertices = quad(device, [-1, -1, 0, 1, 1, -1, 1, 1, -1, 1, 0, 0, 1, 1, 1, 0]);
		var target = device.createTexture(4, 4, Format.Rgba8Unorm, TextureUsage.RenderAttachment | TextureUsage.CopySrc);
		var encoder = device.createCommandEncoder();
		var pass = encoder.beginRenderPass(target, 0, 0, 0, 0);
		pass.setPipeline(pipeline);
		pass.setBindGroup(Textured.TEXTURE_image_GROUP, group);
		pass.setVertexBuffer(0, vertices);
		pass.draw(4, 1, 0, 0);
		pass.end();
		readTarget(device, encoder, target, color, "textured", () -> {
			for (o in ([pass, encoder, group, entries, sampler, image, vertices, target, pipeline, layout, layouts, module] : Array<{function close():Void;}>))
				o.close();
			next();
		});
	}
}
