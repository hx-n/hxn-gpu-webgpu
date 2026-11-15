/** Each value becomes three times itself plus one. **/
class Tripled implements gpu.webgpu.Shader {
	static var SRC = {
		@param var data : RWBuffer<Int>;

		function main() {
			setLayout(16);
			var i = computeVar.globalInvocation.x;
			data[i] = data[i] * 3 + 1;
		}
	};
}

/** Fills what it covers with `tint`. **/
class Tinted implements gpu.webgpu.Shader {
	static var SRC = {
		@input var input : { position : Vec2 };
		var output : { position : Vec4, color : Vec4 };
		@param var tint : Vec4;

		function vertex() {
			output.position = vec4(input.position, 0, 1);
		}

		function fragment() {
			output.color = tint;
		}
	};
}

/** Fills what it covers with `image`, sampled at the vertices' uv. **/
class Textured implements gpu.webgpu.Shader {
	static var SRC = {
		@input var input : { position : Vec2, uv : Vec2 };
		var output : { position : Vec4, color : Vec4 };
		@param var image : Sampler2D;
		var calculatedUV : Vec2;

		function vertex() {
			calculatedUV = input.uv;
			output.position = vec4(input.position, 0, 1);
		}

		function fragment() {
			output.color = image.get(calculatedUV);
		}
	};
}
