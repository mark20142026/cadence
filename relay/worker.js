// Cadence's relay: a Cloudflare Worker that serves the web build and forwards its online
// requests, since browsers can't call the game servers themselves (they send no CORS headers).
//
//   POST /relay/database/<endpoint>.php   → https://www.boomlings.com/database/<endpoint>.php
//   POST /relay/post?u=<url>              → an account server the game itself names (cloud saves)
//   GET  /relay/get?u=<url>               → news, sound effects, songs from the game's file hosts
//
// It is a pass-through: the body goes on byte for byte with the headers the game's own HTTP
// client sends, and the answer comes back as it is. Nothing is stored, and nothing is logged
// (no console calls; observability is off in wrangler.toml): login forms carry a password hash.
// Anything else under /relay is refused; everything outside /relay is the static site.

const SERVER = "https://www.boomlings.com/database/";
// The account servers `getAccountURL.php` hands out for cloud saves.
const POST_HOSTS = [/^www\.boomlings\.com$/, /^(www\.)?robtopgames\.(org|net|com)$/];
// The game's content CDNs and the hosts custom songs are downloaded from.
const GET_HOSTS = [/^geometrydashfiles\.b-cdn\.net$/, /^geometrydashcontent\.b-cdn\.net$/, /(^|\.)ngfiles\.com$/, /^(www\.)?newgrounds\.com$/];
const MAX_BODY = 16 << 20;

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname.startsWith("/relay/")) {
      try {
        return await relay(request, url);
      } catch (_) {
        return text(502, "relay: the server couldn't be reached");
      }
    }
    return env && env.ASSETS ? env.ASSETS.fetch(request) : text(404, "not found");
  },
};

async function relay(request, url) {
  const path = url.pathname.slice("/relay/".length);
  if (request.method === "POST" && path.startsWith("database/")) {
    const endpoint = path.slice("database/".length);
    if (!/^(accounts\/)?[A-Za-z0-9]+\.php$/.test(endpoint)) return text(400, "relay: unknown endpoint");
    return forward(SERVER + endpoint, request);
  }
  if (request.method === "POST" && path === "post") {
    const target = allowed(url.searchParams.get("u"), POST_HOSTS);
    if (!target || !/\.php$/.test(target.pathname)) return text(400, "relay: not a game server");
    return forward(target.href, request);
  }
  if (request.method === "GET" && path === "get") {
    const target = allowed(url.searchParams.get("u"), GET_HOSTS);
    if (!target) return text(400, "relay: not a game file host");
    const resp = await fetch(target.href, { headers: { "User-Agent": "" }, cf: { cacheTtl: 86400, cacheEverything: true } });
    return new Response(resp.body, {
      status: resp.status,
      headers: { "Content-Type": resp.headers.get("Content-Type") || "application/octet-stream", "Cache-Control": "public, max-age=86400" },
    });
  }
  return text(404, "relay: not found");
}

// The game's HTTP client (libcurl with only the POST fields set): no user agent, `Accept: */*`.
async function forward(target, request) {
  const body = await request.arrayBuffer();
  if (body.byteLength > MAX_BODY) return text(413, "relay: too large");
  const resp = await fetch(target, {
    method: "POST",
    body,
    headers: { "Content-Type": "application/x-www-form-urlencoded", Accept: "*/*", "User-Agent": "" },
  });
  return new Response(resp.body, { status: resp.status, headers: { "Content-Type": "text/plain; charset=utf-8", "Cache-Control": "no-store" } });
}

function allowed(raw, hosts) {
  let u;
  try {
    u = new URL(raw || "");
  } catch (_) {
    return null;
  }
  return (u.protocol === "https:" || u.protocol === "http:") && !u.username && !u.password && hosts.some((h) => h.test(u.hostname)) ? u : null;
}

function text(status, message) {
  return new Response(message, { status, headers: { "Content-Type": "text/plain; charset=utf-8" } });
}
