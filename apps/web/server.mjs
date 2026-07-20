import { createReadStream, existsSync, statSync } from "node:fs";
import { Readable } from "node:stream";
import { extname, join, normalize } from "node:path";
import { createServer } from "node:http";

const port = Number(process.env.PORT ?? 5173);
const appEnv = process.env.APP_ENV ?? "development";
const root = join(process.cwd(), "dist");
const apiProxyTarget = process.env.API_PROXY_TARGET;
const apiPrefixes = [
  "/auth",
  "/gmail",
  "/mailbox",
  "/analysis-runs",
  "/threads",
  "/me",
  "/public",
  "/checkout",
  "/billing",
  "/health",
];

const securityHeaders = {
  "Content-Security-Policy": "base-uri 'self'; frame-ancestors 'none'; object-src 'none'",
  "Referrer-Policy": "strict-origin-when-cross-origin",
  "Permissions-Policy": "camera=(), geolocation=(), microphone=()",
  "Strict-Transport-Security": "max-age=31536000",
  "X-Content-Type-Options": "nosniff",
  "X-Frame-Options": "DENY",
};

const contentTypes = {
  ".css": "text/css; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".ico": "image/x-icon",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".webp": "image/webp",
};

createServer(async (request, response) => {
  for (const [name, value] of Object.entries(securityHeaders)) response.setHeader(name, value);

  const requestUrl = new URL(request.url ?? "/", "http://localhost");
  const rawPath = decodeURIComponent(requestUrl.pathname);
  if (apiProxyTarget && apiPrefixes.some((prefix) => rawPath === prefix || rawPath.startsWith(`${prefix}/`))) {
    await proxyApiRequest(request, response, requestUrl);
    return;
  }

  const safePath = normalize(rawPath).replace(/^(\.\.[/\\])+/, "");
  const requestedPath = join(root, safePath);
  const filePath = existsSync(requestedPath) && statSync(requestedPath).isFile()
    ? requestedPath
    : join(root, "index.html");

  response.setHeader("Content-Type", contentTypes[extname(filePath)] ?? "application/octet-stream");
  response.setHeader(
    "Cache-Control",
    filePath === requestedPath && rawPath.startsWith("/assets/") ? "public, max-age=31536000, immutable" : "no-cache",
  );
  createReadStream(filePath).pipe(response);
}).listen(port, "0.0.0.0", () => {
  console.log(JSON.stringify({
    severity: "INFO",
    message: "web listening",
    service: "web",
    environment: appEnv,
    operation: "startup",
    port,
  }));
});

async function proxyApiRequest(request, response, requestUrl) {
  const target = new URL(`${requestUrl.pathname}${requestUrl.search}`, apiProxyTarget);
  const headers = new Headers();
  for (const [name, value] of Object.entries(request.headers)) {
    if (value === undefined) continue;
    if (["connection", "content-length", "host"].includes(name.toLowerCase())) continue;
    if (Array.isArray(value)) {
      for (const item of value) headers.append(name, item);
    } else {
      headers.set(name, value);
    }
  }

  try {
    const upstream = await fetch(target, {
      method: request.method,
      headers,
      body: request.method === "GET" || request.method === "HEAD" ? undefined : request,
      redirect: "manual",
      duplex: "half",
    });

    response.statusCode = upstream.status;
    response.statusMessage = upstream.statusText;
    for (const [name, value] of upstream.headers.entries()) {
      if (name.toLowerCase() !== "set-cookie") {
        response.setHeader(name, value);
      }
    }
    const setCookies = upstream.headers.getSetCookie?.() ?? [];
    if (setCookies.length > 0) {
      response.setHeader("Set-Cookie", setCookies);
    } else {
      const setCookie = upstream.headers.get("set-cookie");
      if (setCookie) response.setHeader("Set-Cookie", setCookie);
    }
    response.setHeader("Cache-Control", "no-store");

    if (upstream.body) {
      Readable.fromWeb(upstream.body).pipe(response);
    } else {
      response.end();
    }
  } catch {
    console.error(JSON.stringify({
      severity: "ERROR",
      message: "API proxy failed",
      service: "web",
      environment: appEnv,
      operation: "api_proxy",
      status: 502,
      error_code: "api_proxy_failed",
    }));
    response.writeHead(502, { "Content-Type": "application/json; charset=utf-8" });
    response.end(JSON.stringify({ error: "No se pudo conectar con la API." }));
  }
}
