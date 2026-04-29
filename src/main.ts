import { SharedMemory } from "./shared_memory.ts";

// @ts-ignore
import workerUrl from "./kernel_worker.ts?worker&url";

let hardwareMem: SharedMemory | undefined;

const outputEl = document.getElementById("output") as HTMLElement;
const terminalEl = document.getElementById("terminal") as HTMLElement;

let terminalBuffer = "";

function printTerminal(text: string) {
  // Handle ANSI Clear Screen: \x1b[2J
  if (text.includes("\x1b[2J")) {
    terminalBuffer = "";
    outputEl.textContent = "";
    // If it also includes \x1b[H (Home), we just clear and continue
    text = text.replace(/\x1b\[2J/g, "").replace(/\x1b\[H/g, "");
    if (text.length === 0) return;
  }

  for (let i = 0; i < text.length; i++) {
    const char = text[i];
    if (char === "\x08") {
      terminalBuffer = terminalBuffer.slice(0, -1);
    } else {
      terminalBuffer += char;
    }
  }
  outputEl.textContent = terminalBuffer;
  terminalEl.scrollTop = terminalEl.scrollHeight;
}

// System logs (not from kernel stdout)
function sysLog(msg: string) {
  const logMsg = `\r\n[SYS] ${msg}\r\n`;
  printTerminal(logMsg);
}

async function init() {
  try {
    sysLog("Booting Wasm OS...");
    
    const response = await fetch("/wasm/kernel.wasm");
    if (!response.ok) throw new Error("Failed to fetch WASM binary");

    const wasmBytes = await response.arrayBuffer();
    const sharedBuffer = new SharedArrayBuffer(64 * 65536);
    const kernelWorker = new Worker(workerUrl, { type: "module" });

    kernelWorker.postMessage({
      type: "BOOT",
      wasmBytes,
      sharedBuffer,
    });

    sysLog("Kernel Worker spawned.");

    kernelWorker.onmessage = (e: MessageEvent) => {
      const { type, payload, memoryBuffer, inputPtr, outputPtr } = e.data;
      if (type === "SYSCALL_LOG") {
        printTerminal(payload);
      } else if (type === "ERROR") {
        sysLog(`KERNEL_PANIC: ${payload}`);
      } else if (type === "READY") {
        hardwareMem = new SharedMemory(null, memoryBuffer, inputPtr, outputPtr);
        sysLog("Hardware Emulation Bound to Memory. Ready.");
      }
    };

    window.addEventListener("keydown", (e: KeyboardEvent) => {
      if (hardwareMem) {
        // Prevent default browser behavior for terminal keys (e.g. Backspace going back, Space scrolling)
        if (e.key.length === 1 || e.key === "Enter" || e.key === "Backspace") {
          e.preventDefault();
        }

        if (e.key.length === 1) {
          hardwareMem.writeToKernel(e.key);
        } else if (e.key === "Enter") {
          hardwareMem.writeToKernel("\r");
        } else if (e.key === "Backspace") {
          hardwareMem.writeToKernel("\x08");
        }
      }
    });
  } catch (err: any) {
    sysLog(`FATAL ERROR: ${err.message}`);
  }
}

init();
