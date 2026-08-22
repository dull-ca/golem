// The only true oracle for the file format: load each generated file through the
// same restore() an Excalidraw editor runs on open, and fail if anything the
// generator wrote comes back changed, dropped, or flagged deleted.
//
//   cd docs/presentation/tools && bun install && bun run check
//
// Needs a one-off network install, so it is optional; test_scenes.py is the
// always-offline check. Import order matters — dom.mjs must install the browser
// stubs before the package is loaded.
import { readFileSync } from "node:fs";
import { basename } from "node:path";
import "./dom.mjs";

const { restore } = await import("@excalidraw/excalidraw");

const paths = process.argv.slice(2);
if (paths.length === 0) {
  console.error("usage: bun run restore_check.mjs <file.excalidraw> ...");
  process.exit(2);
}

let failures = 0;

const compared = ["type", "x", "y", "width", "height", "angle", "strokeColor", "backgroundColor", "fillStyle", "strokeWidth", "strokeStyle", "roughness", "opacity", "frameId", "containerId", "text", "originalText", "fontSize", "fontFamily", "textAlign", "verticalAlign", "name", "fileId", "status", "scale", "crop"];

for (const path of paths) {
  const label = basename(path);
  const source = JSON.parse(readFileSync(path, "utf8"));
  const restored = restore(source, null, null);
  const problems = [];

  const before = source.elements;
  const after = restored.elements;

  if (after.length !== before.length) problems.push(`restore() returned ${after.length} elements, input had ${before.length}`);

  const byId = new Map(after.map((el) => [el.id, el]));
  for (const original of before) {
    const survivor = byId.get(original.id);
    if (!survivor) { problems.push(`element ${original.id} (${original.type}) was dropped by restore()`); continue; }
    if (survivor.isDeleted) problems.push(`element ${original.id} (${original.type}) came back isDeleted`);
    for (const key of compared) {
      if (!(key in original)) continue;
      if (JSON.stringify(survivor[key]) !== JSON.stringify(original[key])) {
        problems.push(`element ${original.id} (${original.type}): restore() rewrote ${key} ${JSON.stringify(original[key])} -> ${JSON.stringify(survivor[key])}`);
      }
    }
    if (original.boundElements) {
      const kept = survivor.boundElements ?? [];
      for (const bound of original.boundElements) {
        if (!kept.some((b) => b.id === bound.id && b.type === bound.type)) {
          problems.push(`element ${original.id}: restore() dropped bound ${bound.type} ${bound.id}`);
        }
      }
    }
    if (original.type === "arrow" || original.type === "line") {
      if (JSON.stringify(survivor.points) !== JSON.stringify(original.points)) {
        problems.push(`element ${original.id} (${original.type}): restore() rewrote points`);
      }
    }
  }

  // An image element without a matching files entry loads as a blank rectangle, and
  // nothing above catches it: `fileId` round-trips whether or not the binary exists.
  // restore() hands `files` straight back, so comparing it to the input would only
  // prove the object was passed through — the reachability is what has to be checked.
  // Determinism of `created` is test_scenes.py's job, where there is a constant to
  // check it against.
  for (const original of before) {
    if (original.type !== "image") continue;
    const survivor = byId.get(original.id);
    if (survivor && !survivor.fileId) problems.push(`image ${original.id}: restore() left it with no fileId`);
    const entry = restored.files?.[original.fileId];
    if (!entry) { problems.push(`image ${original.id}: no files entry for ${original.fileId}`); continue; }
    if (typeof entry.dataURL !== "string" || !entry.dataURL.startsWith("data:")) problems.push(`image ${original.id}: file ${original.fileId} carries no data URL`);
  }

  // The generator omits `index` deliberately; this proves restore() supplies one and
  // that the array order it derives them from yields a strictly increasing z-order.
  for (const el of after) {
    if (!("index" in el) || typeof el.index !== "string") problems.push(`element ${el.id}: restore() did not assign a fractional index`);
  }
  const indices = after.map((el) => el.index);
  for (let i = 1; i < indices.length; i += 1) {
    if (!(indices[i - 1] < indices[i])) problems.push(`z-order broken: index ${indices[i - 1]} !< ${indices[i]} at position ${i}`);
  }

  if (problems.length === 0) {
    console.log(`PASS  ${label}  (${after.length} elements survived restore() unchanged, z-order strictly increasing)`);
  } else {
    failures += 1;
    console.log(`FAIL  ${label}`);
    for (const problem of problems.slice(0, 12)) console.log(`        ${problem}`);
    if (problems.length > 12) console.log(`        … and ${problems.length - 12} more`);
  }
}

process.exit(failures === 0 ? 0 : 1);
