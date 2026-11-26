import { safeParse } from "valibot";
import { EDITOR_SCHEMA_VERSION } from "./protocol";
import {
  type EngineInfo,
  replySchema,
  type WorkerRequest,
  withinRequestLimits,
} from "./transport";

export type { EngineInfo } from "./transport";

const DEFAULT_TIMEOUT_MS = 30_000;

export interface EngineWorkerOptions {
  readonly startupTimeoutMs?: number;
  readonly requestTimeoutMs?: number;
}

export interface EngineResponse {
  readonly control: string;
  readonly bytes?: ArrayBuffer;
}

export type EngineTransportErrorCode =
  | "busy"
  | "closed"
  | "limit"
  | "protocol"
  | "startup"
  | "timeout"
  | "worker";

export class EngineTransportError extends Error {
  constructor(
    readonly code: EngineTransportErrorCode,
    message: string,
  ) {
    super(message);
    this.name = "EngineTransportError";
  }
}

interface Request {
  readonly id: number;
  readonly timeout: ReturnType<typeof setTimeout>;
  readonly resolve: (response: EngineResponse) => void;
  readonly reject: (error: EngineTransportError) => void;
}

type WorkerPort = Pick<
  Worker,
  "addEventListener" | "removeEventListener" | "postMessage" | "terminate"
>;

/** Owns one worker and its one active command. Close only after handling unsaved changes. */
export class EngineWorkerClient {
  readonly #worker: WorkerPort;
  readonly #ready: Promise<EngineInfo>;
  readonly #startupTimeout: ReturnType<typeof setTimeout>;
  readonly #requestTimeoutMs: number;
  #resolveReady!: (info: EngineInfo) => void;
  #rejectReady!: (error: EngineTransportError) => void;
  #active?: Request;
  #failure?: EngineTransportError;
  #nextId = 1;
  #started = false;
  #awaitingStartup = false;

  constructor(worker: WorkerPort, options: EngineWorkerOptions = {}) {
    this.#worker = worker;
    this.#requestTimeoutMs = options.requestTimeoutMs ?? DEFAULT_TIMEOUT_MS;
    this.#ready = new Promise((resolve, reject) => {
      this.#resolveReady = resolve;
      this.#rejectReady = reject;
    });
    // Startup can fail before the UI attaches its handlers.
    void this.#ready.catch(() => {});
    this.#startupTimeout = setTimeout(() => {
      this.#stop(
        new EngineTransportError("startup", "The bank engine did not start."),
      );
    }, options.startupTimeoutMs ?? DEFAULT_TIMEOUT_MS);
    worker.addEventListener("message", this.#receive);
    worker.addEventListener("error", this.#workerFailed);
    worker.addEventListener("messageerror", this.#workerFailed);
  }

  async ready(): Promise<EngineInfo> {
    const info = await this.#ready;
    if (this.#failure) {
      throw this.#failure;
    }
    return info;
  }

  /**
   * Sends a command for the current browser session. Ownership of bytes passes to the worker
   * when posting succeeds. A timed-out command ends the session because its result is uncertain.
   */
  async request(control: string, bytes?: ArrayBuffer): Promise<EngineResponse> {
    if (this.#active || this.#awaitingStartup) {
      throw new EngineTransportError(
        "busy",
        "The bank engine already has a command in progress.",
      );
    }
    this.#awaitingStartup = true;
    let info: EngineInfo;
    try {
      info = await this.ready();
    } finally {
      this.#awaitingStartup = false;
    }
    if (this.#failure) {
      throw this.#failure;
    }
    const id = this.#nextId;
    if (!Number.isSafeInteger(id)) {
      const error = new EngineTransportError(
        "closed",
        "The bank engine session has ended.",
      );
      this.#stop(error);
      throw error;
    }
    const request: WorkerRequest = { id, control, bytes };
    if (!withinRequestLimits(request, info)) {
      throw new EngineTransportError(
        "limit",
        "The bank or command exceeds the engine size limit.",
      );
    }

    this.#nextId += 1;
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.#stop(
          new EngineTransportError(
            "timeout",
            "The bank engine command timed out.",
          ),
        );
      }, this.#requestTimeoutMs);
      this.#active = { id, timeout, resolve, reject };

      try {
        this.#worker.postMessage(request, bytes ? [bytes] : []);
      } catch {
        this.#stop(
          new EngineTransportError(
            "worker",
            "The command could not reach the bank engine.",
          ),
        );
      }
    });
  }

  close(): void {
    this.#stop(
      new EngineTransportError("closed", "The bank engine session closed."),
    );
  }

  #receive = (event: MessageEvent<unknown>): void => {
    const parsed = safeParse(replySchema, event.data);
    if (!parsed.success) {
      this.#badReply();
      return;
    }

    const message = parsed.output;
    if (message.kind === "failed") {
      this.#workerFailed();
      return;
    }
    if (message.kind === "ready") {
      if (
        this.#started ||
        message.info.schemaVersion !== EDITOR_SCHEMA_VERSION
      ) {
        this.#badReply();
        return;
      }
      this.#started = true;
      clearTimeout(this.#startupTimeout);
      this.#resolveReady(message.info);
      return;
    }

    if (message.id >= this.#nextId || !this.#started) {
      this.#badReply();
      return;
    }
    const active = this.#active;
    if (active?.id === message.id) {
      clearTimeout(active.timeout);
      this.#active = undefined;
      active.resolve({ control: message.control, bytes: message.bytes });
    }
  };

  #workerFailed = (): void => {
    this.#stop(
      new EngineTransportError(
        "worker",
        "The bank engine stopped unexpectedly.",
      ),
    );
  };

  #badReply(): void {
    this.#stop(
      new EngineTransportError(
        "protocol",
        "The bank engine replied with an invalid message.",
      ),
    );
  }

  #stop(error: EngineTransportError): void {
    if (this.#failure) {
      return;
    }
    this.#failure = error;
    clearTimeout(this.#startupTimeout);
    if (!this.#started) {
      this.#rejectReady(error);
    }
    if (this.#active) {
      clearTimeout(this.#active.timeout);
      this.#active.reject(error);
      this.#active = undefined;
    }
    this.#worker.removeEventListener("message", this.#receive);
    this.#worker.removeEventListener("error", this.#workerFailed);
    this.#worker.removeEventListener("messageerror", this.#workerFailed);
    this.#worker.terminate();
  }
}
