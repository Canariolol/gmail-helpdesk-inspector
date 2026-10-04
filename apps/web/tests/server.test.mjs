import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

test("malformed URLs return 400 without terminating the web server", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "mira-web-test-"));
  await mkdir(join(directory, "dist"));
  await writeFile(join(directory, "dist", "index.html"), "<!doctype html><title>Mira test</title>");
  const process = spawn(globalThis.process.execPath, [fileURLToPath(new URL("../server.mjs", import.meta.url))], {
    cwd: directory, env: { ...globalThis.process.env, PORT: "0", API_PROXY_TARGET: "" }, stdio: ["ignore", "pipe", "pipe"],
  });
  t.after(async () => {
    if (process.exitCode === null) { process.kill(); await once(process, "exit"); }
    await rm(directory, { recursive: true, force: true });
  });
  const started = await Promise.race([
    once(process.stdout, "data").then(([data]) => JSON.parse(data.toString())),
    once(process, "exit").then(([code]) => { throw new Error(`Web exited before listening: ${code}`); }),
  ]);
  const origin = `http://127.0.0.1:${started.port}`;
  for (const path of ["/%FF", "/%", "/%00"]) {
    const response = await fetch(`${origin}${path}`);
    assert.equal(response.status, 400);
    await response.text();
  }
  const response = await fetch(origin);
  assert.equal(response.status, 200);
  assert.match(await response.text(), /Mira test/);
  assert.equal(process.exitCode, null);
});
