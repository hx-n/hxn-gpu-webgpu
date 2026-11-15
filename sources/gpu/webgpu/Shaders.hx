package gpu.webgpu;

/** Shaders from hxnsl, for WebGPU. **/
class Shaders {
	#if macro
	/**
		Registers a target for `gpu.webgpu.Shader` that adds nothing, so those
		shaders get only WGSL and the layout, not hxnsl's other languages. The
		package's extraParams.hxml runs it; without hxnsl it does nothing.
	**/
	public static function register():Void {
		#if hxnsl
		hxnsl.Targets.register(new hxnsl.Target("wgsl"), "gpu.webgpu.Shader");
		#end
	}
	#elseif hxnsl

	/** The bind group layouts of a shader's `LAYOUT`, group 0 first. **/
	public static function bindGroupLayouts(device:Device, layout:Array<Array<hxnsl.Layout.LayoutEntry>>):BindGroupLayouts {
		return device.createBindGroupLayouts([for (group in layout) {entries: [for (entry in group) layoutEntry(entry)]}]);
	}

	/** One hxnsl layout entry as WebGPU takes it. **/
	public static function layoutEntry(entry:hxnsl.Layout.LayoutEntry):LayoutEntry {
		return {
			binding: entry.binding,
			visibility: entry.visibility,
			kind: kind(entry.type),
			sampleType: sampleType(entry.sampleType),
			viewDimension: viewDimension(entry.viewDimension),
			multisampled: entry.multisampled == true,
			format: entry.format == null ? Format.Rgba8Unorm : format(entry.format),
			access: access(entry.access)
		};
	}

	static function kind(type:String):BindingKind {
		return switch (type) {
			case "uniform": BindingKind.Uniform;
			case "storage": BindingKind.Storage;
			case "read-only-storage": BindingKind.ReadOnlyStorage;
			case "sampler": BindingKind.Sampler;
			case "comparison-sampler": BindingKind.ComparisonSampler;
			case "texture": BindingKind.Texture;
			case "storage-texture": BindingKind.StorageTexture;
			case other: throw 'hxnsl binding type $other has no WebGPU binding';
		}
	}

	static function sampleType(type:Null<String>):SampleType {
		return switch (type) {
			case "sint": SampleType.Sint;
			case "uint": SampleType.Uint;
			case "depth": SampleType.Depth;
			case "unfilterable-float": SampleType.UnfilterableFloat;
			case _: SampleType.Float;
		}
	}

	static function viewDimension(dimension:Null<String>):ViewDimension {
		return switch (dimension) {
			case "1d": ViewDimension.D1;
			case "2d-array": ViewDimension.D2Array;
			case "cube": ViewDimension.Cube;
			case "cube-array": ViewDimension.CubeArray;
			case "3d": ViewDimension.D3;
			case _: ViewDimension.D2;
		}
	}

	static function access(access:Null<String>):StorageAccess {
		return switch (access) {
			case "read-only": StorageAccess.ReadOnly;
			case "read-write": StorageAccess.ReadWrite;
			case _: StorageAccess.WriteOnly;
		}
	}

	/** A storage texture's format by WGSL's name for it. **/
	static function format(name:String):Format {
		return switch (name) {
			case "rgba8unorm": Format.Rgba8Unorm;
			case "bgra8unorm": Format.Bgra8Unorm;
			case "r32float": Format.R32Float;
			case "r32uint": Format.R32Uint;
			case "rg32float": Format.Rg32Float;
			case "rgba16float": Format.Rgba16Float;
			case "rgba32float": Format.Rgba32Float;
			case other: throw 'storage texture format $other is not supported';
		}
	}
	#end
}
