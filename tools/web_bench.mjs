// Measure the web build's frame intervals over the DevTools protocol (Chrome started with
// --remote-debugging-port): node tools/web_bench.mjs <port> <seconds> <out.txt> [quality]
// Waits for the Play button, presses it, then records every
// requestAnimationFrame interval (ms) for <seconds>.
const [port, secs, out] = process.argv.slice(2);
const fs = await import("node:fs");
const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = list.find(t => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise(r => ws.addEventListener("open", r));
let id = 0;
const call = (method, params) => new Promise(res => {
  const my = ++id;
  const on = ev => { const m = JSON.parse(ev.data); if (m.id === my) { ws.removeEventListener("message", on); res(m); } };
  ws.addEventListener("message", on);
  ws.send(JSON.stringify({ id: my, method, params }));
});
const expr = `(async () => {
  const play = document.getElementById("play");
  for (let i = 0; i < 600 && play.disabled; i++) await new Promise(r => setTimeout(r, 100));
  // the same texture quality as the native build's default
  const q = document.getElementById("quality"); q.value = "${process.argv[5] || "high"}"; q.dispatchEvent(new Event("change"));
  play.click();
  const t = []; let last = performance.now(); const end = last + ${Number(secs) * 1000};
  await new Promise(res => { function f(now) { t.push(now - last); last = now; if (now < end) requestAnimationFrame(f); else res(); } requestAnimationFrame(f); });
  const c = document.querySelector("canvas");
  return JSON.stringify({ t, canvas: c ? [c.width, c.height] : null, quality: document.getElementById("quality").value, dpr: devicePixelRatio, ua: navigator.userAgent, gpu: !!navigator.gpu });
})()`;
const r = await call("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true });
const v = JSON.parse(r.result.result.value);
// as the native benchmark: the first 30 frames (the level loading) left out
const t = v.t.slice(30);
fs.writeFileSync(out, t.map(x => x.toFixed(3)).join("\n") + "\n");
const sum = t.reduce((a, b) => a + b, 0);
console.log(JSON.stringify({ frames: t.length, secs: (sum / 1000).toFixed(3), fps: (t.length / (sum / 1000)).toFixed(3), canvas: v.canvas, quality: v.quality, dpr: v.dpr, webgpu: v.gpu, ua: v.ua.match(/Chrome\/[\d.]+/)?.[0] }));
const shot = await call("Page.captureScreenshot", { format: "png" });
fs.writeFileSync(out.replace(/\.txt$/, ".png"), Buffer.from(shot.result.data, "base64"));
ws.close();
