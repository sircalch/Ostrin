import { createHash } from "node:crypto";

/**
 * Hash UTF-8 text independently of the checkout's newline convention.
 * Invalid UTF-8 and binary content return null so callers keep their raw hash.
 */
export function normalizedTextSha256(bytes) {
  const buffer = Buffer.isBuffer(bytes) ? bytes : Buffer.from(bytes);
  let text;
  try {
    text = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(buffer);
  } catch {
    return null;
  }
  if (text.includes("\0")) return null;
  const normalized = Buffer.from(text.replace(/\r\n?/g, "\n"), "utf8");
  return createHash("sha256").update(normalized).digest("hex");
}
