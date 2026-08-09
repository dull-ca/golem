import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

import emetGrammar from "./src/grammars/emet.tmLanguage.json" with { type: "json" };

// https://astro.build/config
export default defineConfig({
  site: "https://golem.example",
  integrations: [
    starlight({
      title: "Golem",
      description:
        "Small-fleet declarative orchestrator for bare-metal Debian boxes.",
      // This is already Starlight's default path; naming it keeps the icon from
      // looking like it works by accident, since the only other thing wiring it
      // up is the filename under `public/`.
      favicon: "/favicon.svg",
      // NOTE: the PNGs are opaque dark tiles, not the SVG's transparent
      // silhouette. The SVG flips fill on `prefers-color-scheme`; Safari and iOS
      // ignore that media query, so a transparent fallback would render a
      // near-black figure on Safari's dark tab bar. A tile carries its own
      // contrast and is legible against either.
      head: [
        {
          tag: "link",
          attrs: {
            rel: "icon",
            href: "/favicon-32.png",
            sizes: "32x32",
            type: "image/png",
          },
        },
        {
          tag: "link",
          attrs: {
            rel: "apple-touch-icon",
            href: "/apple-touch-icon.png",
            sizes: "180x180",
          },
        },
      ],
      expressiveCode: {
        shiki: {
          langs: [{ ...emetGrammar, aliases: ["emet"] }],
        },
      },
      social: {
        github: "https://github.com/dull-ca/golem",
      },
      // NOTE: Starlight appends the page's path from the Astro project root
      // (`src/content/docs/…`), so this must stop at the project directory —
      // spelling out `src/content/docs/` here doubles it.
      editLink: {
        baseUrl: "https://github.com/dull-ca/golem/edit/main/sites/website/",
      },
      sidebar: [
        {
          label: "Get started",
          items: [
            { label: "What golem is", link: "/" },
            { label: "Install", link: "/getting-started/install/" },
            { label: "The config", link: "/getting-started/the-config/" },
            { label: "Applying changes", link: "/getting-started/applying/" },
          ],
        },
        {
          label: "Guides",
          items: [
            { label: "A first glyph", link: "/guides/hello-agent/" },
            { label: "A service abstraction", link: "/guides/app-and-db/" },
            { label: "An app behind Traefik", link: "/guides/front-door/" },
            { label: "A maintenance page", link: "/guides/maintenance-page/" },
            { label: "A tour of the lichess fleet", link: "/guides/litour/" },
          ],
        },
        {
          label: "Tutorials",
          items: [
            { label: "Bring up the fleet", link: "/tutorials/the-vm-harness/" },
            { label: "A failing unit", link: "/tutorials/a-failing-unit/" },
            { label: "A registry on the fleet", link: "/tutorials/registry-on-the-fleet/" },
            { label: "The website loop", link: "/tutorials/website-loop/" },
          ],
        },
        {
          label: "Explanation",
          items: [
            { label: "Architecture", link: "/explanation/architecture/" },
            { label: "Reversible reconcile", link: "/explanation/reversible-reconcile/" },
            { label: "Trust model", link: "/explanation/trust/" },
            { label: "The fleet harness", link: "/explanation/the-fleet-harness/" },
          ],
        },
        {
          label: "Reference",
          items: [
            {
              label: "The Emet language",
              items: [
                { label: "Values & types", link: "/reference/language/values-and-types/" },
                { label: "Functions", link: "/reference/language/functions/" },
                { label: "Pattern matching", link: "/reference/language/pattern-matching/" },
                { label: "Modules", link: "/reference/language/modules/" },
                { label: "The prelude", link: "/reference/language/prelude/" },
              ],
            },
            { label: "CLI", link: "/reference/cli/" },
            { label: "The four glyphs", link: "/reference/primitives/" },
            { label: "The Quadlet library", link: "/reference/workloads/" },
            { label: "The Traefik library", link: "/reference/ingress/" },
            { label: "Manifest format", link: "/reference/bundle-format/" },
            { label: "Status", link: "/reference/status/" },
          ],
        },
      ],
    }),
  ],
});
