// Local stand-in for the deployed site: serves web/ and runs relay/worker.js for /relay/*,
// so the web build can go online during development exactly as it will on Cloudflare.
//
//   node relay/dev.mjs [port]        (default 8765)
//
// web/gd (the player's own game files, a local link) is served too; the deployed site never has it.
import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { extname, join, normalize, resolve } from "node:path";
import worker from "./worker.js";

const root = resolve(new URL("../web", import.meta.url).pathname);
const port = Number(process.argv[2] || 8765);
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".json": "application/json", ".png": "image/png", ".plist": "text/xml", ".txt": "text/plain", ".mp3": "audio/mpeg", ".ogg": "audio/ogg", ".fnt": "text/plain" };

const assets = {
  async fetch(request) {
    const path = decodeURIComponent(new URL(request.url).pathname);
    let file = normalize(join(root, path));
    if (!file.startsWith(root)) return new Response("not found", { status: 404 });
    try {
      if ((await stat(file)).isDirectory()) file = join(file, "index.html");
      return new Response(await readFile(file), { headers: { "Content-Type": types[extname(file)] || "application/octet-stream", "Cache-Control": "no-store" } });
    } catch (_) {
      return new Response("not found", { status: 404 });
    }
  },
};

createServer(async (req, res) => {
  const chunks = [];
  for await (const c of req) chunks.push(c);
  const body = chunks.length ? Buffer.concat(chunks) : undefined;
  const request = new Request(`http://localhost:${port}${req.url}`, { method: req.method, headers: req.headers, body: req.method === "GET" || req.method === "HEAD" ? undefined : body });
  const resp = await worker.fetch(request, { ASSETS: assets });
  res.writeHead(resp.status, Object.fromEntries(resp.headers));
  res.end(Buffer.from(await resp.arrayBuffer()));
}).listen(port, () => process.stdout.write(`cadence dev site on http://localhost:${port}\n`));
