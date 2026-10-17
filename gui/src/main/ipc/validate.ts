import { type ZodType, z } from "zod";
import { IpcError, type IpcResult } from "../../shared/ipc";
import { RpcError } from "../sidecar/client";
import { isTrustedSender, type SenderEvent, type SenderFrame } from "./sender";

export interface TrustContext {
  topFrame: () => SenderFrame | null;
  devServer: string | undefined;
  log: (message: string) => void;
}

/**
 * Wraps an invoke handler: the sender must be the window's top frame on the app origin, the
 * arguments must pass `schema`, and any failure leaves as `{ code, message }` without a stack.
 */
export function guarded<S extends ZodType, R>(
  trust: TrustContext,
  schema: S,
  handler: (args: z.output<S>) => Promise<R> | R,
): (event: SenderEvent, raw: unknown) => Promise<IpcResult<R>> {
  return async (event, raw) => {
    if (!isTrustedSender(event, trust.topFrame(), trust.devServer)) {
      trust.log("refused an IPC call from an untrusted sender");
      return { ok: false, error: { code: "untrusted_sender", message: "call refused" } };
    }
    try {
      const args = schema.safeParse(raw);
      if (!args.success) throw new IpcError("invalid_arguments", "the arguments are not valid");
      return { ok: true, value: await handler(args.data) };
    } catch (error) {
      return { ok: false, error: toWire(error) };
    }
  };
}

/** Same checks for a call that takes no arguments. */
export function guardedNoArgs<R>(
  trust: TrustContext,
  handler: () => Promise<R> | R,
): (event: SenderEvent) => Promise<IpcResult<R>> {
  const run = guarded(trust, z.undefined(), () => handler());
  return (event) => run(event, undefined);
}

function toWire(error: unknown): { code: string; message: string } {
  if (error instanceof IpcError || error instanceof RpcError) {
    return { code: error.code, message: error.message };
  }
  return { code: "internal", message: "the request failed" };
}
