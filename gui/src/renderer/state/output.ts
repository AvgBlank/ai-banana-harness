export const MAX_OUTPUT_LINES = 200;
export const MAX_OUTPUT_BYTES = 64 * 1024;

export interface ClippedOutput {
  text: string;
  clipped: boolean;
}

/** The part of a tool's output a card shows by default: at most 200 lines and 64 KiB. */
export function clipOutput(output: string): ClippedOutput {
  const lines = output.split("\n");
  let text = lines.length > MAX_OUTPUT_LINES ? lines.slice(0, MAX_OUTPUT_LINES).join("\n") : output;
  let clipped = lines.length > MAX_OUTPUT_LINES;
  const encoder = new TextEncoder();
  if (encoder.encode(text).length > MAX_OUTPUT_BYTES) {
    // Cut at the longest prefix within the byte limit. ponytail: an emoji can be split into a lone surrogate; cut on code points to fix.
    // Binary search: the encoded size only grows with the length, and a scan costs quadratic time.
    let low = 0;
    let high = Math.min(text.length, MAX_OUTPUT_BYTES);
    while (low < high) {
      const middle = Math.ceil((low + high) / 2);
      if (encoder.encode(text.slice(0, middle)).length <= MAX_OUTPUT_BYTES) low = middle;
      else high = middle - 1;
    }
    text = text.slice(0, low);
    clipped = true;
  }
  return { text, clipped };
}
