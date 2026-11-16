// The Worker that holds WebGPU for an Ash program's hxn-gpu-webgpu, and runs the commands the
// program puts in its mailbox (gpu_webgpu_service.mjs, generated from WebGPU's IDL). The
// program names navigator.gpu handle 1, the page's canvas context and element 2 and 3 once
// claimed, and the agent's own calls 4.

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

let served = null;
// The page's answer to a claim: the canvas, or null when it has none or another agent holds it.
let claimed = null;
self.addEventListener("message", ({ data }) => {
  if (data && data.claimed !== undefined && claimed) {
    const settle = claimed;
    claimed = null;
    settle(data.claimed);
  }
});

const agent = {
  claimCanvas() {
    if (served.objects.has(3)) return Promise.resolve();
    return new Promise((resolve, reject) => {
      claimed = (canvas) => {
        if (!canvas) return reject(new Error("the web page has no canvas, or another hxn browser agent holds it"));
        served.set(2, canvas.getContext("webgpu"));
        served.set(3, canvas);
        resolve();
      };
      self.postMessage({ claim: true });
    });
  },
};

start(async () => {
  if (!navigator.gpu) throw new Error("this browser has no WebGPU");
  return new Map([
    [1, navigator.gpu],
    [4, agent],
  ]);
}, (wire) => {
  served = wire;
  // A command that throws has no reply to carry the failure, so it goes to the shim,
  // which ends the web page with it.
  wire.error = (e, op) => {
    console.error(`${op}:`, e);
    self.postMessage({ fail: `${op}: ${e && e.stack !== undefined ? e.stack : e}` });
  };
});
