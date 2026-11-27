#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SKILL_DIR = path.dirname(fileURLToPath(import.meta.url));
const DISTRICTS_DIR = path.join(SKILL_DIR, "districts");
const WASM_PATH = path.join(SKILL_DIR, "sm.wasm");

function fail(message) {
  process.stderr.write(JSON.stringify({ ok: false, error: message }, null, 2) + "\n");
  process.exit(1);
}

function loadDistrictPackages() {
  if (!fs.existsSync(DISTRICTS_DIR)) {
    fail(`districts directory not found: ${DISTRICTS_DIR}`);
  }
  const files = fs
    .readdirSync(DISTRICTS_DIR)
    .filter((f) => f.endsWith(".json"))
    .sort();
  return files.map((file) =>
    JSON.parse(fs.readFileSync(path.join(DISTRICTS_DIR, file), "utf8")),
  );
}

async function invokeWasm(requestObj) {
  if (!fs.existsSync(WASM_PATH)) {
    fail(`embedded state machine binary not found: ${WASM_PATH}`);
  }
  const wasmBytes = fs.readFileSync(WASM_PATH);
  const { instance } = await WebAssembly.instantiate(wasmBytes, {});
  const { memory, sm_alloc, sm_dealloc, sm_invoke } = instance.exports;

  const encoder = new TextEncoder();
  const decoder = new TextDecoder("utf-8");
  const reqBytes = encoder.encode(JSON.stringify(requestObj));
  const reqPtr = sm_alloc(reqBytes.length);
  new Uint8Array(memory.buffer, reqPtr, reqBytes.length).set(reqBytes);

  const packed = sm_invoke(reqPtr, reqBytes.length);
  sm_dealloc(reqPtr, reqBytes.length);

  const packedBig = BigInt.asUintN(64, packed);
  const respPtr = Number(packedBig >> 32n);
  const respLen = Number(packedBig & 0xffffffffn);
  const respText = decoder.decode(new Uint8Array(memory.buffer, respPtr, respLen));
  sm_dealloc(respPtr, respLen);
  return JSON.parse(respText);
}

function parseFlags(argv) {
  const flags = {};
  const multi = new Set(["field", "field-file", "check", "artifact"]);
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
    if (!token.startsWith("--")) {
      fail(`unexpected positional argument: ${token}`);
    }
    const name = token.slice(2);
    if (name === "raw") {
      flags.raw = true;
      continue;
    }
    if (i + 1 >= argv.length) {
      fail(`missing value for --${name}`);
    }
    const val = argv[i + 1];
    i += 1;
    if (multi.has(name)) {
      flags[name] = flags[name] || [];
      flags[name].push(val);
    } else {
      flags[name] = val;
    }
  }
  return flags;
}

function splitKv(raw, flagName) {
  const idx = raw.indexOf("=");
  if (idx <= 0) {
    fail(`--${flagName} expects KEY=VALUE, got '${raw}'`);
  }
  return [raw.slice(0, idx).trim(), raw.slice(idx + 1)];
}

function parseArtifact(raw) {
  const first = raw.indexOf(":");
  const second = first >= 0 ? raw.indexOf(":", first + 1) : -1;
  if (first <= 0 || second <= first + 1) {
    fail(`--artifact expects KIND:LABEL:URI, got '${raw}'`);
  }
  return {
    kind: raw.slice(0, first).trim(),
    label: raw.slice(first + 1, second).trim(),
    uri: raw.slice(second + 1).trim(),
  };
}

function parseCommentsInput(rawText) {
  const trimmed = rawText.trim();
  if (!trimmed) return [];
  try {
    const parsed = JSON.parse(trimmed);
    if (Array.isArray(parsed)) {
      return parsed
        .map((item) => (typeof item === "string" ? item : item && typeof item.body === "string" ? item.body : ""))
        .filter(Boolean);
    }
    if (parsed && typeof parsed.body === "string") {
      return [parsed.body];
    }
  } catch {
    return [rawText];
  }
  return [rawText];
}

function buildRequest(cmd, flags) {
  const districts = loadDistrictPackages();
  if (cmd === "list") {
    return { districts, op: "list" };
  }
  if (cmd === "context") {
    if (!flags.district) fail("--district is required");
    return {
      districts,
      op: "context",
      district_id: flags.district,
      stage_id: flags.stage ?? null,
      sub_stage_id: flags["sub-stage"] ?? null,
    };
  }
  if (cmd === "enter") {
    if (!flags.district) fail("--district is required");
    return {
      districts,
      op: "enter",
      district_id: flags.district,
      runner_label: flags.runner ?? null,
      summary: flags.summary ?? null,
    };
  }
  if (cmd === "advance") {
    if (!flags.district || !flags.stage) {
      fail("--district and --stage are required");
    }
    const fields = {};
    if (flags["fields-json"]) {
      const obj = JSON.parse(flags["fields-json"]);
      for (const [k, v] of Object.entries(obj)) fields[k] = String(v);
    }
    for (const pair of flags.field || []) {
      const [k, v] = splitKv(pair, "field");
      fields[k] = v;
    }
    for (const pair of flags["field-file"] || []) {
      const [k, filePath] = splitKv(pair, "field-file");
      fields[k] = fs.readFileSync(filePath, "utf8").trim();
    }
    let gate = null;
    if (flags["gate-verdict"]) {
      const checks = (flags.check || []).map((raw) => {
        const idx = raw.lastIndexOf("=");
        if (idx <= 0) fail(`--check expects LABEL=true|false, got '${raw}'`);
        return [raw.slice(0, idx).trim(), raw.slice(idx + 1).trim().toLowerCase() === "true"];
      });
      gate = {
        verdict: flags["gate-verdict"],
        summary: (flags["gate-summary"] || flags.summary || "Gate verified").trim(),
        checks,
      };
    }
    const artifacts = (flags.artifact || []).map(parseArtifact);
    return {
      districts,
      op: "advance",
      district_id: flags.district,
      stage_id: flags.stage,
      sub_stage_id: flags["sub-stage"] ?? null,
      fields,
      gate,
      artifacts,
      runner_label: flags.runner ?? null,
      summary: flags.summary ?? null,
    };
  }
  if (cmd === "replay") {
    const rawText = flags.file
      ? fs.readFileSync(flags.file, "utf8")
      : fs.readFileSync(0, "utf8");
    return {
      districts,
      op: "replay",
      comments: parseCommentsInput(rawText),
    };
  }
  fail(`usage: sm.mjs <list|context|enter|advance|replay> [flags]`);
}

const [, , cmd, ...rest] = process.argv;
if (!cmd || cmd === "-h" || cmd === "--help") {
  fail("usage: sm.mjs <list|context|enter|advance|replay> [flags]");
}
const flags = parseFlags(rest);
const req = buildRequest(cmd, flags);
const resp = await invokeWasm(req);
if (!resp.ok) {
  process.stderr.write(JSON.stringify(resp, null, 2) + "\n");
  process.exit(1);
}
if (flags.raw && typeof resp.comment_markdown === "string") {
  process.stdout.write(resp.comment_markdown);
} else {
  process.stdout.write(JSON.stringify(resp, null, 2) + "\n");
}
