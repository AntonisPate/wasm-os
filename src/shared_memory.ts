export class SharedMemory {
  kernel: any;
  memoryBuffer: SharedArrayBuffer;
  inputPtr: number;
  outputPtr: number;
  BUFFER_SIZE: number;
  STATUS: { EMPTY: number; READ: number; EDIT: number; READY: number };
  inputBus!: Uint8Array;
  outputBus!: Uint8Array;

  constructor(
    kernelInstance: any,
    memoryBuffer: SharedArrayBuffer,
    inputPtr?: number,
    outputPtr?: number,
  ) {
    this.kernel = kernelInstance;
    this.memoryBuffer = memoryBuffer;
    this.inputPtr =
      inputPtr !== undefined
        ? inputPtr
        : kernelInstance
          ? kernelInstance.get_input_buffer_ptr()
          : 0;
    this.outputPtr =
      outputPtr !== undefined
        ? outputPtr
        : kernelInstance
          ? kernelInstance.get_output_buffer_ptr()
          : 0;
    this.BUFFER_SIZE = 1024;

    // Status Codes (Πρέπει να ταυτίζονται με τη Rust)
    this.STATUS = {
      EMPTY: 0,
      READ: 1,
      EDIT: 2,
      READY: 3,
    };

    this.refreshViews();
  }

  refreshViews() {
    // Χρησιμοποιούμε απευθείας το buffer
    this.inputBus = new Uint8Array(
      this.memoryBuffer,
      this.inputPtr,
      this.BUFFER_SIZE,
    );
    this.outputBus = new Uint8Array(
      this.memoryBuffer,
      this.outputPtr,
      this.BUFFER_SIZE,
    );
  }

  /**
   * Στέλνει δεδομένα στον Kernel (Hardware -> Kernel)
   */
  writeToKernel(data: string | Uint8Array) {
    this.refreshViews();

    const encoded =
      typeof data === "string" ? new TextEncoder().encode(data) : data;

    if (this.inputBus[0] !== this.STATUS.EMPTY) {
      console.warn(`SharedMemory Busy: ${this.inputBus[0]}`);
      return false;
    }

    this.inputBus[0] = this.STATUS.EDIT;
    // Καθαρίζουμε τον buffer πριν γράψουμε νέα δεδομένα (για να μην έχουμε σκουπίδια)
    this.inputBus.fill(0, 1);

    const view = this.inputBus.subarray(4);
    view.set(encoded.slice(0, this.BUFFER_SIZE - 4));

    this.inputBus[0] = this.STATUS.READY;
    console.log("Memory Check:", this.inputBus[0]);

    // Χτύπα το κουδούνι αμέσως μετά το γράψιμο
    this.notifyKernel();
    return true;
  }

  readFromKernel(): string | null {
    if (this.outputBus[0] !== this.STATUS.READY) {
      return null;
    }

    const data = this.outputBus.subarray(4);
    // Ο TextDecoder δεν δέχεται SharedArrayBuffer views, οπότε κάνουμε copy με το .slice()
    const text = new TextDecoder().decode(data.slice()).replace(/\0/g, "");

    // Clear buffer and reset status
    this.outputBus.fill(0);
    this.outputBus[0] = this.STATUS.EMPTY;

    return text;
  }

  /**
   * Ελέγχει αν ο Kernel έχει "καθαρίσει" το input
   */
  isKernelIdle(): boolean {
    return this.inputBus[0] === this.STATUS.EMPTY;
  }

  notifyKernel() {
    // Το Atomics απαιτεί Int32Array aligned σε 4 bytes.
    // Ο pointer του status byte (index 0) πρέπει να είναι διαιρετός δια του 4.
    const STATUS_INDEX = this.inputPtr / 4;
    const int32View = new Int32Array(this.memoryBuffer);

    Atomics.notify(int32View, STATUS_INDEX, 1);
  }
}
