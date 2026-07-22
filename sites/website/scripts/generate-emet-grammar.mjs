import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..", "..", "..");
const grammarPath = resolve(repoRoot, "libs", "tree-sitter-emet", "grammar.js");
const outputPath = resolve(here, "..", "src", "grammars", "emet.tmLanguage.json");

const grammarSource = readFileSync(grammarPath, "utf8");

function extractBuiltins(source) {
  const match = source.match(/builtin:\s*\(\$\)\s*=>\s*choice\(([\s\S]*?)\)/);
  if (!match) throw new Error("could not locate `builtin` rule in grammar.js");
  return [...match[1].matchAll(/'([^']+)'/g)].map((m) => m[1]);
}

function extractBinaryOperators(source) {
  const match = source.match(/const table = \[([\s\S]*?)\];/);
  if (!match) throw new Error("could not locate binary operator table in grammar.js");
  return [...match[1].matchAll(/'([^']+)'/g)].map((m) => m[1]);
}

const preludeModules = ["List", "Maybe", "String"];

const declarationKeywords = ["let", "in"];
const conditionalKeywords = ["if", "then", "else", "case", "of"];
const structuralOperators = ["->", "=", ":", "|", "\\"];

const builtins = extractBuiltins(grammarSource);
const binaryOperators = extractBinaryOperators(grammarSource);

function escapeForRegex(literal) {
  return literal.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function alternation(literals) {
  return [...literals]
    .sort((a, b) => b.length - a.length)
    .map(escapeForRegex)
    .join("|");
}

const grammar = {
  $schema:
    "https://raw.githubusercontent.com/martinring/tmlanguage/master/tmlanguage.json",
  name: "Emet",
  scopeName: "source.emet",
  patterns: [
    { include: "#comment" },
    { include: "#string" },
    { include: "#number" },
    { include: "#type-signature" },
    { include: "#keyword" },
    { include: "#builtin" },
    { include: "#qualified" },
    { include: "#constructor" },
    { include: "#operator" },
  ],
  repository: {
    comment: {
      match: "--.*$",
      name: "comment.line.double-dash.emet",
    },
    number: {
      match: "\\b[0-9]+(\\.[0-9]+)?\\b",
      name: "constant.numeric.emet",
    },
    string: {
      name: "string.quoted.double.emet",
      begin: '"',
      beginCaptures: { 0: { name: "punctuation.definition.string.begin.emet" } },
      end: '"',
      endCaptures: { 0: { name: "punctuation.definition.string.end.emet" } },
      patterns: [
        { match: "\\\\.", name: "constant.character.escape.emet" },
        {
          name: "meta.embedded.line.emet",
          begin: "\\$\\{",
          beginCaptures: {
            0: { name: "punctuation.section.embedded.begin.emet" },
          },
          end: "\\}",
          endCaptures: { 0: { name: "punctuation.section.embedded.end.emet" } },
          patterns: [
            { include: "#string" },
            { include: "#number" },
            { include: "#qualified" },
            { include: "#constructor" },
            { include: "#operator" },
          ],
        },
      ],
    },
    keyword: {
      match: `\\b(${alternation([...declarationKeywords, ...conditionalKeywords])})\\b`,
      name: "keyword.control.emet",
    },
    builtin: {
      match: `\\b(${alternation(builtins)})\\b`,
      name: "support.function.builtin.emet",
    },
    "type-signature": {
      match: "^\\s*([a-z][a-zA-Z0-9_]*)\\s*(:)(?!:)",
      captures: {
        1: { name: "entity.name.function.emet" },
        2: { name: "keyword.operator.type.emet" },
      },
    },
    qualified: {
      match: `\\b(${alternation(preludeModules)})(\\.)([a-z][a-zA-Z0-9_]*)`,
      captures: {
        1: { name: "support.class.emet" },
        2: { name: "punctuation.accessor.emet" },
        3: { name: "support.function.prelude.emet" },
      },
    },
    constructor: {
      match: "\\b[A-Z][a-zA-Z0-9_]*\\b",
      name: "entity.name.type.emet",
    },
    operator: {
      match: `(${alternation([...binaryOperators, ...structuralOperators])})`,
      name: "keyword.operator.emet",
    },
  },
};

mkdirSync(dirname(outputPath), { recursive: true });
writeFileSync(outputPath, JSON.stringify(grammar, null, 2) + "\n");

process.stderr.write(
  `emet.tmLanguage.json generated from ${grammarPath}\n` +
    `  builtins: ${builtins.join(", ")}\n` +
    `  operators: ${binaryOperators.join(" ")}\n`,
);
