import { Fragment } from "react";

/** Renders `code` spans from backticks; a Markdown renderer arrives later. */
export function RichText({ text }: { text: string }) {
  // Keyed by where each part starts in the text, which is unique and stable.
  let offset = 0;
  const parts = text.split(/(`[^`]+`)/g).map((part) => {
    const key = offset;
    offset += part.length;
    return { key, part };
  });
  return (
    <>
      {parts.map(({ key, part }) =>
        part.startsWith("`") && part.length > 1 ? (
          <code
            key={key}
            className="rounded-[5px] bg-s3 px-1.5 py-0.5 font-mono text-[12.5px] text-fg"
          >
            {part.slice(1, -1)}
          </code>
        ) : (
          <Fragment key={key}>{part}</Fragment>
        ),
      )}
    </>
  );
}
