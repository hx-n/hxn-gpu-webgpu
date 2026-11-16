// The web page imports this when an Ash program's hxn-gpu-webgpu asks for its browser agent through
// `ash_host_agent("gpu_webgpu", mailbox)`. It starts the Worker that holds WebGPU and serves the
// program's mailbox, and hands it the page's canvas when the page has one.

let worker;

export function start({ memory, address, canvas, fail }) {
  if (worker) throw new Error("the WebGPU browser agent is already running");
  if (!(memory.buffer instanceof SharedArrayBuffer)) {
    throw new Error("hxn-gpu-webgpu needs the program's memory shared: a threaded Ash build");
  }
  let offscreen = null;
  if (canvas) {
    try {
      offscreen = canvas.transferControlToOffscreen();
      canvas.hidden = false;
    } catch {
      offscreen = null;
    }
  }
  worker = new Worker(new URL("./gpu_webgpu_worker.mjs", import.meta.url), { type: "module" });
  // A command that throws in the Worker, or the Worker failing, ends the web page with the
  // reason, where Ash passes `fail`; an older Ash only has the Worker's console.
  const end = (reason) => (typeof fail === "function" ? fail(reason) : console.error(reason));
  worker.onmessage = ({ data }) => {
    if (data && data.fail !== undefined) end(data.fail);
  };
  worker.onerror = (event) => end(event.message || "the WebGPU browser agent's Worker failed");
  worker.postMessage({ memory, address, canvas: offscreen }, offscreen ? [offscreen] : []);
}
