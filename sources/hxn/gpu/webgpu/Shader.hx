package hxn.gpu.webgpu;

#if hxnsl
/**
	An hxnsl shader for WebGPU. It gets hxnsl's `WGSL`, which the browser
	compiles, and `LAYOUT`, whose bind groups `Shaders.bindGroupLayouts`
	makes; none of the other languages.
**/
interface Shader extends hxnsl.Shader {}
#end
