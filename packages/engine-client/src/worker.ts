import { safeParse } from "valibot";
import {
  type EngineInfo,
  engineInfoSchema,
  requestSchema,
  TRANSPORT_VERSION,
  type WorkerReply,
  withinRequestLimits,
} from "./transport";

export interface WorkerEngine {
  readonly info: EngineInfo;
  dispatch(
    control: string,
    bytes?: Uint8Array,
  ): { control: string; bytes?: Uint8Array };
}

type WorkerScope = Pick<
  DedicatedWorkerGlobalScope,
  "addEventListener" | "close" | "postMessage"
>;

/** Runs the application engine in the worker that receives its commands. */
export async function serveEngineWorker(
  scope: WorkerScope,
  load: () => Promise<WorkerEngine>,
): Promise<void> {
  const fail = (): void => {
    scope.postMessage({ kind: "failed" } satisfies WorkerReply);
    scope.close();
  };

  try {
    const engine = await load();
    const info = safeParse(engineInfoSchema, engine.info);
    if (!info.success) {
      fail();
      return;
    }

    let previousId = 0;
    scope.addEventListener("message", (event: MessageEvent<unknown>) => {
      const result = safeParse(requestSchema, event.data);
      if (
        !result.success ||
        result.output.id <= previousId ||
        !withinRequestLimits(result.output, info.output)
      ) {
        fail();
        return;
      }
      const { id, control, bytes } = result.output;
      previousId = id;
      try {
        const response = engine.dispatch(
          control,
          bytes === undefined ? undefined : new Uint8Array(bytes),
        );
        const output = response.bytes;
        const buffer = output?.buffer;
        const owned =
          output &&
          buffer instanceof ArrayBuffer &&
          output.byteLength === buffer.byteLength
            ? buffer
            : output?.slice().buffer;
        const reply: WorkerReply = {
          kind: "response",
          id,
          control: response.control,
          bytes: owned,
        };
        scope.postMessage(reply, owned ? [owned] : []);
      } catch {
        fail();
      }
    });
    scope.addEventListener("messageerror", fail);
    scope.postMessage({
      kind: "ready",
      version: TRANSPORT_VERSION,
      info: info.output,
    } satisfies WorkerReply);
  } catch {
    fail();
  }
}
