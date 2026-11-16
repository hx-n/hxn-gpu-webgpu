// The Worker that holds WebGPU for an Ash program's hxn-gpu-webgpu, and runs the commands the
// program puts in its mailbox (gpu_webgpu_service.mjs, generated from WebGPU's IDL). The
// program names navigator.gpu handle 1, and the page's canvas context and element 2 and 3.

import { start } from "./gpu_webgpu_service.mjs";

// What WebGPU reports outside any error scope, and a device the browser takes back, are
// written to the console.
const requestDevice = GPUAdapter.prototype.requestDevice;
GPUAdapter.prototype.requestDevice = async function (...args) {
  const device = await requestDevice.apply(this, args);
  device.addEventListener("uncapturederror", (event) => {
    console.error(`WebGPU error: ${event.error.message}`);
  });
  device.lost.then((info) => {
    if (info.reason !== "destroyed") console.error(`WebGPU device lost: ${info.message}`);
  });
  return device;
};

start(async ({ canvas }) => {
  if (!navigator.gpu) throw new Error("this browser has no WebGPU");
  const roots = new Map([[1, navigator.gpu]]);
  if (canvas) {
    roots.set(2, canvas.getContext("webgpu"));
    roots.set(3, canvas);
  }
  return roots;
}, (wire) => {
  // A command that throws has no reply to carry the failure, so it goes to the shim,
  // which ends the web page with it.
  wire.error = (e, op) => {
    console.error(`${op}:`, e);
    self.postMessage({ fail: `${op}: ${e && e.stack !== undefined ? e.stack : e}` });
  };
});
