import { SharedMemory } from "./shared_memory.ts";

let kernel: any;
let sharedMem: SharedMemory;

self.addEventListener("message", async (e: MessageEvent) => {
  const { type, wasmBytes, sharedBuffer } = e.data;

  if (type === "BOOT") {
    const wasmMemory = new WebAssembly.Memory({
      initial: 64,
      maximum: 256,
      shared: true,
      buffer: sharedBuffer,
    } as WebAssembly.MemoryDescriptor); // Cast to allow extra properties or missing strict types

    const { instance } = await WebAssembly.instantiate(wasmBytes, {
      env: {
        memory: wasmMemory, // Η Rust παίρνει αυτή τη μνήμη
        host_log: (ptr: number, len: number) => {
          const mem = new Uint8Array(wasmMemory.buffer, ptr, len);
          const text = new TextDecoder().decode(mem.slice());
          console.error("RUST KERNEL PANIC:", text);
        },
      },
    });

    console.log(
      "Memory Object Check:",
      instance.exports.memory || "Memory is Imported",
    );

    kernel = instance.exports;
    const inputPtr = kernel.get_input_buffer_ptr();
    const outputPtr = kernel.get_output_buffer_ptr();
    console.log("Rust Input Pointer Address:", inputPtr);

    // ΚΡΙΣΙΜΟ: Πρέπει η SharedMemory να ξέρει ποιο είναι το buffer
    sharedMem = new SharedMemory(
      kernel,
      wasmMemory.buffer as unknown as SharedArrayBuffer,
      inputPtr,
      outputPtr,
    );

    // Debug: Δες αν οι pointers είναι σωστοί
    console.log("Input Ptr:", inputPtr);

    postMessage({
      type: "READY",
      inputPtr,
      outputPtr,
      memoryBuffer: wasmMemory.buffer,
    });

    runKernelLoop();
  }

  // Removed KEYPRESS listener as it will be handled by Main Thread Directly
});

function runKernelLoop() {
  const inputPtr = kernel.get_input_buffer_ptr();
  const STATUS_INDEX = inputPtr / 4; 
  const memoryBuffer = sharedMem.memoryBuffer;
  const int32View = new Int32Array(memoryBuffer);

  // Initial run to start the Shell and print the first prompt
  if (kernel) {
    kernel.kernel_loop();
    if (sharedMem) {
      const output = sharedMem.readFromKernel();
      if (output && output.length > 0) {
        postMessage({ type: "SYSCALL_LOG", payload: output });
      }
    }
  }

  while (true) {
    // Wait up to 100ms for input, or wake up to process background tasks/output
    Atomics.wait(int32View, STATUS_INDEX, 0, 100);

    if (kernel) {
      kernel.kernel_loop();

      if (sharedMem) {
        const output = sharedMem.readFromKernel();
        if (output && output.length > 0) {
          postMessage({ type: "SYSCALL_LOG", payload: output });
        }
      }
    }
  }
}
