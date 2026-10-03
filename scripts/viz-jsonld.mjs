// Keeps the Viz gallery discoverable from the same source-backed catalogue that
// renders the page. The generated ItemList describes recorded SVG artifacts;
// it does not claim that search engines can execute the browser explorer.
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { GALLERY } from "./lab-data.mjs";

export const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const siteUrl = "https://sircalch.github.io/Ostrin/";
export const markerStart = "    <!-- viz-gallery-ld:start -->";
export const markerEnd = "    <!-- viz-gallery-ld:end -->";

function figureUrl(id) {
  return `${siteUrl}viz.html?figure=${encodeURIComponent(id)}#viz-${id}`;
}

export function vizGalleryStructuredData(gallery = GALLERY) {
  return {
    "@context": "https://schema.org",
    "@type": "ItemList",
    "@id": `${siteUrl}viz.html#gallery`,
    name: "Ostrin Viz gallery",
    description: "Source-backed scientific figures, tables, animations and 3D scenes rendered by std.viz.",
    numberOfItems: gallery.length,
    itemListElement: gallery.map((figure, index) => ({
      "@type": "ListItem",
      position: index + 1,
      item: {
        "@type": "ImageObject",
        "@id": `${siteUrl}viz.html#viz-${figure.id}-image`,
        name: figure.title,
        description: figure.blurb,
        url: figureUrl(figure.id),
        contentUrl: `${siteUrl}assets/viz/${figure.id}.svg`,
        encodingFormat: "image/svg+xml",
        isBasedOn: `https://github.com/sircalch/Ostrin/blob/main/${figure.file}`,
      },
    })),
  };
}

export function renderVizGalleryJsonLd(gallery = GALLERY) {
  const json = JSON.stringify(vizGalleryStructuredData(gallery), null, 2);
  return `${markerStart}\n    <script type="application/ld+json" data-generated="viz-gallery">\n${json.split("\n").map((line) => `      ${line}`).join("\n")}\n    </script>\n    ${markerEnd}`;
}

function replaceGeneratedBlock(html, block) {
  const start = html.indexOf(markerStart);
  const end = html.indexOf(markerEnd);
  if (start < 0 || end < start) throw new Error("website/viz.html: generated Viz JSON-LD markers are missing or out of order");
  return `${html.slice(0, start)}${block}${html.slice(end + markerEnd.length)}`;
}

export function renderVizPageWithStructuredData(html, gallery = GALLERY) {
  return replaceGeneratedBlock(html.replaceAll("\r\n", "\n"), renderVizGalleryJsonLd(gallery));
}

export function checkVizStructuredData(root = repositoryRoot, gallery = GALLERY) {
  const pagePath = path.join(root, "website", "viz.html");
  if (!existsSync(pagePath)) return ["website/viz.html: missing page"];
  const missing = gallery.flatMap((figure) => {
    const paths = [figure.file, `website/assets/viz/${figure.id}.svg`];
    return paths.filter((relativePath) => !existsSync(path.join(root, relativePath)))
      .map((relativePath) => `Viz JSON-LD item ${figure.id}: missing ${relativePath}`);
  });
  const actual = readFileSync(pagePath, "utf8").replaceAll("\r\n", "\n");
  let expected;
  try {
    expected = renderVizPageWithStructuredData(actual, gallery);
  } catch (error) {
    return [error.message];
  }
  return [
    ...missing,
    ...(actual === expected ? [] : ["website/viz.html: generated Viz JSON-LD is stale; run node scripts/viz-jsonld.mjs --write"]),
  ];
}

function main() {
  const pagePath = path.join(repositoryRoot, "website", "viz.html");
  if (process.argv.includes("--write")) {
    const current = readFileSync(pagePath, "utf8");
    writeFileSync(pagePath, renderVizPageWithStructuredData(current), "utf8");
    console.log("viz-jsonld: wrote website/viz.html");
    return;
  }
  const failures = checkVizStructuredData();
  if (failures.length) {
    console.error(failures.map((failure) => `viz-jsonld: ${failure}`).join("\n"));
    process.exitCode = 1;
  } else {
    console.log(`viz-jsonld: ok (${GALLERY.length} gallery items)`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) main();
