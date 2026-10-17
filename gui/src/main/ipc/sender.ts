import { isAllowedNavigation } from "../origin";

/** The part of an IPC event's sender frame that is checked. */
export interface SenderFrame {
  url: string;
}

export interface SenderEvent {
  senderFrame: SenderFrame | null;
}

/**
 * Whether an IPC call came from the main window's top frame, showing a page on the app origin.
 * A subframe, another window or a navigated page is refused.
 */
export function isTrustedSender(
  event: SenderEvent,
  topFrame: SenderFrame | null,
  devServer: string | undefined,
): boolean {
  const frame = event.senderFrame;
  if (!frame || !topFrame || frame !== topFrame) return false;
  return isAllowedNavigation(frame.url, devServer);
}
