// The web page imports this when a threaded program's hxn-gpu-webgpu, under Ash or Emscripten, asks
// for its browser agent through `ash_host_agent("gpu_webgpu", mailbox)`. It starts the Worker that holds WebGPU and serves the
// program's mailbox, and hands it the page's canvas when the program first asks for it.

let worker;

export function start({ memory, address, canvas, fail }) {
  if (worker) throw new Error("the WebGPU browser agent is already running");
  if (!(memory.buffer instanceof SharedArrayBuffer)) {
    throw new Error("hxn-gpu-webgpu needs the program's memory shared: a threaded build");
  }
  worker = new Worker(new URL("./gpu_webgpu_worker.mjs", import.meta.url), { type: "module" });
  // A command that throws in the Worker, or the Worker failing, ends the web page with the
  // reason, where Ash passes `fail`, and stops the Worker so the commands after it add no
  // errors of their own. An older Ash only has the console, and the Worker keeps serving.
  let failed = false;
  const end = (reason) => {
    if (typeof fail !== "function") return console.error(reason);
    if (failed) return;
    failed = true;
    worker.terminate();
    fail(reason);
  };
  worker.onmessage = ({ data }) => {
    if (data && data.fail !== undefined) end(data.fail);
    else if (data && data.claim) claim(canvas);
  };
  worker.onerror = (event) => end(event.message || "the WebGPU browser agent's Worker failed");
  worker.postMessage({ memory, address });
}

// The agent's claim on the page's canvas, answered with the canvas moved to its Worker, or
// null when the page has none or another hxn browser agent holds it. Only one agent on a
// page can hold the canvas, so it goes to the first program part that asks.
function claim(canvas) {
  let offscreen = null;
  if (canvas && (globalThis.hxnCanvasHolder === undefined || globalThis.hxnCanvasHolder === "gpu_webgpu")) {
    try {
      offscreen = canvas.transferControlToOffscreen();
      canvas.hidden = false;
      globalThis.hxnCanvasHolder = "gpu_webgpu";
    } catch {
      offscreen = null;
    }
  }
  worker.postMessage({ claimed: offscreen }, offscreen ? [offscreen] : []);
}
