import { createReadStream, statSync } from "node:fs";
import { createServer } from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";

const directory = path.dirname(fileURLToPath(import.meta.url));
const websiteRoot = path.resolve(directory, "../../website");
const prefix = "/Ostrin";
const port = Number(process.env.PORT ?? 4173);
const contentTypes = new Map([
  [".css", "text/css; charset=utf-8"],
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".json", "application/json; charset=utf-8"],
  [".png", "image/png"],
  [".svg", "image/svg+xml"],
  [".txt", "text/plain; charset=utf-8"],
  [".wasm", "application/wasm"],
  [".xml", "application/xml; charset=utf-8"],
]);

const server = createServer((request, response) => {
  if (request.method !== "GET" && request.method !== "HEAD") {
    response.writeHead(405, { Allow: "GET, HEAD" }).end();
    return;
  }

  let pathname;
  try {
    pathname = decodeURIComponent(new URL(request.url, "http://127.0.0.1").pathname);
  } catch {
    response.writeHead(400).end("Bad request");
    return;
  }

  if (pathname === prefix) {
    response.writeHead(308, { Location: `${prefix}/` }).end();
    return;
  }
  if (!pathname.startsWith(`${prefix}/`)) {
    response.writeHead(404).end("Not found");
    return;
  }

  const relativePath = pathname.slice(prefix.length) || "/";
  const filePath = path.resolve(websiteRoot, relativePath === "/" ? "index.html" : `.${relativePath}`);
  if (!filePath.startsWith(`${websiteRoot}${path.sep}`)) {
    response.writeHead(404).end("Not found");
    return;
  }

  let fileStat;
  try {
    fileStat = statSync(filePath);
  } catch {
    response.writeHead(404).end("Not found");
    return;
  }
  if (!fileStat.isFile()) {
    response.writeHead(404).end("Not found");
    return;
  }

  response.writeHead(200, {
    "Cache-Control": "no-store",
    "Content-Length": fileStat.size,
    "Content-Type": contentTypes.get(path.extname(filePath)) ?? "application/octet-stream",
    "X-Content-Type-Options": "nosniff",
  });
  if (request.method === "HEAD") response.end();
  else createReadStream(filePath).pipe(response);
});

server.listen(port, "127.0.0.1", () => {
  console.log(`Serving website at http://127.0.0.1:${port}${prefix}/`);
});

process.on("SIGINT", () => server.close(() => process.exit(0)));
process.on("SIGTERM", () => server.close(() => process.exit(0)));
