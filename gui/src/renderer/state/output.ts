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
    // Cut on a character boundary at or below the byte limit.
    let end = Math.min(text.length, MAX_OUTPUT_BYTES);
    while (end > 0 && encoder.encode(text.slice(0, end)).length > MAX_OUTPUT_BYTES) end--;
    text = text.slice(0, end);
    clipped = true;
  }
  return { text, clipped };
}
