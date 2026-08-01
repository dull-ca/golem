import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const examplesDir = fileURLToPath(new URL("../../examples/", import.meta.url));

const regionMarker = /^\s*--\s*#(region|endregion)\b\s*(.*)$/;
const scrollHeader = /^ {2}scroll `([^`]+)`/;

export function readExample(file: string): string {
  try {
    return readFileSync(examplesDir + file, "utf8");
  } catch {
    throw new Error(
      `docs example \`${file}\` does not exist under sites/website/examples/ (ADR 0043)`,
    );
  }
}

export function sliceRegion(
  source: string,
  region: string,
  file: string,
): string {
  const kept: string[] = [];
  let inside = false;

  for (const line of source.split("\n")) {
    const marker = regionMarker.exec(line);
    if (marker) {
      if (marker[1] === "region" && marker[2].trim() === region) inside = true;
      else if (marker[1] === "endregion" && inside) inside = false;
      continue;
    }
    if (inside) kept.push(line);
  }

  if (kept.length === 0) {
    throw new Error(
      `docs example \`${file}\` has no region \`${region}\` — regions are delimited by ` +
        `\`-- #region ${region}\` / \`-- #endregion\` (ADR 0043)`,
    );
  }
  return kept.join("\n").replace(/^\n+/, "").trimEnd();
}

export function sliceScroll(
  rendered: string,
  scroll: string,
  file: string,
): string {
  const lines = rendered.split("\n");
  const start = lines.findIndex((line) => scrollHeader.exec(line)?.[1] === scroll);

  if (start === -1) {
    throw new Error(
      `golden \`${file}\` plans no scroll named \`${scroll}\` (ADR 0043)`,
    );
  }

  const rest = lines.slice(start + 1);
  const end = rest.findIndex((line) => scrollHeader.test(line));
  const stanza = end === -1 ? rest : rest.slice(0, end);

  return [...lines.slice(0, 2), lines[start], ...stanza].join("\n").trimEnd();
}
