/// <reference lib="webworker" />

import { serveEngineWorker } from "starbank-engine-client/worker";
import initialize, { WasmBankEngine } from "./generated/starbank_wasm.js";

declare const self: DedicatedWorkerGlobalScope;

void serveEngineWorker(self, async () => {
  await initialize();
  const engine = new WasmBankEngine();

  return {
    info: {
      schemaVersion: engine.schema_version,
      maxBankBytes: engine.max_bank_bytes,
      maxControlBytes: engine.max_control_bytes,
    },
    dispatch(control, bytes) {
      const reply = engine.dispatch(control, bytes);
      try {
        return { control: reply.take_control(), bytes: reply.take_bytes() };
      } finally {
        reply.free();
      }
    },
  };
});
