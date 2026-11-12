// Markdown for the text cells of the Ostrin Scientific Notebook: a small, explicit subset, parsed into a block tree.
//
// Supported: ATX headings (#..######), paragraphs, unordered and ordered lists, fenced code blocks, block quotes,
// horizontal rules, and inline code, **strong**, *emphasis*, and links [text](url) with http, https, mailto or
// fragment targets only.
// Not supported, and shown as the original text: raw HTML (always shown as text), images (shown as their alt text,
// never loaded), tables, and mathematical typesetting ($...$ is shown as text). Nothing is interpreted as markup
// beyond this subset.
//
// The renderer built on this tree creates DOM nodes with textContent and validated attributes only. It never sets
// innerHTML, so a document cannot inject markup or script through a text cell.

const SAFE_SCHEMES = ["http:", "https:", "mailto:"];

// The safe href of a link target, or null when the target is not allowed.
export function safeHref(target) {
  const text = String(target).trim();
  // No whitespace, no control characters: a target is one token.
  if (text === "" || /[\s\u0000-\u001f]/.test(text)) return null;
  // A fragment, or a plain relative path with no scheme and no leading "//": allowed as written.
  if (text.startsWith("#") || /^(\.{1,2}\/)?[A-Za-z0-9._~][A-Za-z0-9._~\/-]*$/.test(text)) return text;
  // Anything else must be an absolute URL with an allowed scheme. "javascript:", "data:" and "//host" are refused.
  try {
    const url = new URL(text);
    return SAFE_SCHEMES.includes(url.protocol) ? url.href : null;
  } catch (_) {
    return null;
  }
}

// Block tree: { type: "heading", level, inlines } | { type: "paragraph", inlines } | { type: "list", ordered, items: [inlines] }
// | { type: "code", text } | { type: "quote", blocks } | { type: "rule" }.
export function parseBlocks(source) {
  const lines = String(source).replace(/\r\n?/g, "\n").split("\n");
  return parseLines(lines);
}

function parseLines(lines) {
  const blocks = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (line.trim() === "") {
      i += 1;
      continue;
    }
    const fence = line.match(/^```/);
    if (fence) {
      const body = [];
      i += 1;
      while (i < lines.length && !lines[i].startsWith("```")) {
        body.push(lines[i]);
        i += 1;
      }
      i += 1; // the closing fence, or the end of the text
      blocks.push({ type: "code", text: body.join("\n") });
      continue;
    }
    const heading = line.match(/^(#{1,6})\s+(.*)$/);
    if (heading) {
      blocks.push({ type: "heading", level: heading[1].length, inlines: parseInlines(heading[2].replace(/\s+#+\s*$/, "")) });
      i += 1;
      continue;
    }
    if (/^\s*(-{3,}|\*{3,}|_{3,})\s*$/.test(line)) {
      blocks.push({ type: "rule" });
      i += 1;
      continue;
    }
    if (/^>\s?/.test(line)) {
      const inner = [];
      while (i < lines.length && /^>\s?/.test(lines[i])) {
        inner.push(lines[i].replace(/^>\s?/, ""));
        i += 1;
      }
      blocks.push({ type: "quote", blocks: parseLines(inner) });
      continue;
    }
    const bullet = /^\s*[-*+]\s+(.*)$/;
    const numbered = /^\s*\d+[.)]\s+(.*)$/;
    if (bullet.test(line) || numbered.test(line)) {
      const ordered = numbered.test(line);
      const pattern = ordered ? numbered : bullet;
      const items = [];
      while (i < lines.length && pattern.test(lines[i])) {
        items.push(parseInlines(lines[i].match(pattern)[1]));
        i += 1;
      }
      blocks.push({ type: "list", ordered, items });
      continue;
    }
    const paragraph = [];
    while (i < lines.length && lines[i].trim() !== "" && !/^(#{1,6}\s|```|>\s?|\s*[-*+]\s|\s*\d+[.)]\s|\s*(-{3,}|\*{3,}|_{3,})\s*$)/.test(lines[i])) {
      paragraph.push(lines[i].trim());
      i += 1;
    }
    if (paragraph.length === 0) {
      // A line that starts like a block but matched none: keep it as a paragraph so no text is lost.
      paragraph.push(lines[i].trim());
      i += 1;
    }
    blocks.push({ type: "paragraph", inlines: parseInlines(paragraph.join(" ")) });
  }
  return blocks;
}

// Inline nodes: { type: "text", text } | { type: "code", text } | { type: "strong", text } | { type: "em", text }
// | { type: "link", text, href } (href null when the target is not allowed; the text is kept).
export function parseInlines(source) {
  const nodes = [];
  let rest = String(source);
  const pattern = /`([^`]+)`|\*\*([^*]+)\*\*|\*([^*\s][^*]*)\*|!\[([^\]]*)\]\(([^)]*)\)|\[([^\]]+)\]\(([^)]*)\)/;
  while (rest.length > 0) {
    const match = rest.match(pattern);
    if (!match) {
      nodes.push({ type: "text", text: rest });
      break;
    }
    if (match.index > 0) nodes.push({ type: "text", text: rest.slice(0, match.index) });
    if (match[1] !== undefined) nodes.push({ type: "code", text: match[1] });
    else if (match[2] !== undefined) nodes.push({ type: "strong", text: match[2] });
    else if (match[3] !== undefined) nodes.push({ type: "em", text: match[3] });
    else if (match[4] !== undefined) nodes.push({ type: "text", text: match[4] || "image" });
    else if (match[6] !== undefined) nodes.push({ type: "link", text: match[6], href: safeHref(match[7]) });
    rest = rest.slice(match.index + match[0].length);
  }
  return nodes;
}
