// Host publication for a successful WASI build. The manifest is committed last.
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { randomUUID } from "node:crypto";
import { parseManifest } from "./manifest.js";

export function fingerprint(bytes) {
  let hash = 0xcbf29ce484222325n;
  for (const byte of bytes) hash = BigInt.asUintN(64, (hash ^ BigInt(byte)) * 0x100000001b3n);
  return hash.toString(16).padStart(16, "0");
}

export function publishArtifacts(manifestPath, manifest, files) {
  parseManifest(JSON.stringify(manifest));
  for (const artifact of manifest.artifacts) {
    const bytes = files.get(artifact.file);
    if (!bytes || fingerprint(bytes) !== artifact.hash) throw new Error(`Missing or inconsistent artifact: ${artifact.file}`);
  }
  if (files.size !== manifest.artifacts.length) throw new Error("Unexpected artifacts outside manifest");
  const previous = existsSync(manifestPath) ? parseManifest(readFileSync(manifestPath, "utf8")) : undefined;
  if (previous && (previous.input !== manifest.input || previous.output !== manifest.output)) {
    throw new Error("Manifest belongs to a different compilation");
  }
  const writes = new Map(files);
  writes.set(manifestPath, Buffer.from(JSON.stringify(manifest, null, 2) + "\n"));
  const stale = (previous?.artifacts ?? []).filter(({ file, hash }) =>
    !writes.has(file) && existsSync(file) && fingerprint(readFileSync(file)) === hash);
  const id = randomUUID();
  const staged = [];
  const changed = [];
  try {
    // Stage every byte before replacing any output. Unchanged files keep mtimes.
    for (const [path, data] of writes) {
      if (existsSync(path) && readFileSync(path).equals(Buffer.from(data))) continue;
      mkdirSync(dirname(path), { recursive: true });
      const stage = `${path}.${id}.stage`;
      staged.push(stage);
      writeFileSync(stage, data, { flag: "wx" });
      changed.push({ path, stage, backup: `${path}.${id}.backup`, saved: false, installed: false });
    }
    // Stale files are backed up too, so an ordinary I/O failure can roll back.
    const manifestWrite = changed.find(change => change.path === manifestPath);
    const commit = [...changed.filter(change => change !== manifestWrite),
      ...stale.map(({ file }) => ({ path: file, backup: `${file}.${id}.backup`, saved: false, installed: false })),
      ...(manifestWrite ? [manifestWrite] : [])];
    changed.splice(0, changed.length, ...commit);
    for (const change of changed) {
      if (existsSync(change.path)) {
        renameSync(change.path, change.backup);
        change.saved = true;
      }
      if (change.stage) {
        renameSync(change.stage, change.path);
        change.installed = true;
      }
    }
  } catch (error) {
    const recovery = [];
    for (const change of [...changed].reverse()) {
      try {
        if (change.installed) rmSync(change.path);
        if (change.saved) renameSync(change.backup, change.path);
      } catch (failure) { recovery.push(`${change.path}: ${failure}; backup: ${change.backup}`); }
    }
    throw new Error(`${error}${recovery.length ? `\nRecovery failures:\n${recovery.join("\n")}` : ""}`);
  } finally {
    for (const stage of staged) rmSync(stage, { force: true });
  }
  for (const change of changed) if (change.saved) rmSync(change.backup);
}
