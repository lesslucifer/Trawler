import { useEffect, useState } from "react";
import { humanBytes } from "../body";

const CHECKERBOARD = {
  backgroundImage: "repeating-conic-gradient(#383330 0% 25%, #2b2622 0% 50%)",
  backgroundSize: "12px 12px",
};

/**
 * ImageRenderer — an image body as a letterboxed preview over a transparency checkerboard.
 * @tier T2 — owns the object-URL lifecycle and the metadata caption; the Blob MIME comes from the served content type
 * @anatomy letterboxed <img> (max-width 100%, hairline border) on a checkerboard background that reveals transparency · caption below in Inter mute — dimensions × MIME × human-readable size, metadata rather than payload, so it is UI text and not mono
 * @use a body dispatched to "image" — image/*
 * @avoid a cut marker: the element renders the full bytes, and save serves an oversized image · a copy action: an image has no clipboard text representation, which is why BodyViewer omits the copy button for this kind
 * @states rendered (once the object URL exists) · no cut, empty or error state — a body that is not an image never dispatches here
 * @madeof —
 */
export function ImageRenderer({
  bytes,
  contentType,
}: {
  bytes: Uint8Array;
  contentType: string | null;
}) {
  const [url, setUrl] = useState<string>();
  const [size, setSize] = useState<{ width: number; height: number }>();

  useEffect(() => {
    const objectUrl = URL.createObjectURL(new Blob([bytes], { type: contentType ?? "" }));
    setUrl(objectUrl);
    return () => URL.revokeObjectURL(objectUrl);
  }, [bytes, contentType]);

  if (!url) return null;

  const caption = [
    size && `${size.width} × ${size.height}`,
    contentType,
    humanBytes(bytes.length),
  ].filter(Boolean);

  return (
    <div className="flex flex-col gap-chrome-y">
      <img
        data-testid="body-image"
        src={url}
        alt="body"
        onLoad={(e) =>
          setSize({
            width: e.currentTarget.naturalWidth,
            height: e.currentTarget.naturalHeight,
          })
        }
        className="max-w-full border border-hairline"
        style={CHECKERBOARD}
      />
      <span className="font-ui text-body text-ink-mute">{caption.join(" · ")}</span>
    </div>
  );
}
