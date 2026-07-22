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
      expressiveCode: {
        shiki: {
          langs: [{ ...emetGrammar, aliases: ["emet"] }],
        },
      },
      social: {
        codeberg: "https://codeberg.org/dull/golem",
      },
      editLink: {
        baseUrl: "https://codeberg.org/dull/golem/_edit/main/docs/",
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
            { label: "A maintenance page", link: "/guides/maintenance-page/" },
            { label: "A tour of the lichess fleet", link: "/guides/litour/" },
          ],
        },
        {
          label: "Deploy a fleet",
          items: [
            { label: "Deploy to VMs", link: "/tutorials/the-vm-harness/" },
            { label: "A registry on the fleet", link: "/tutorials/registry-on-the-fleet/" },
            { label: "The website loop", link: "/tutorials/website-loop/" },
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
            { label: "Manifest format", link: "/reference/bundle-format/" },
            { label: "Architecture", link: "/reference/architecture/" },
            { label: "Reversible reconcile", link: "/reference/honest-convergence/" },
            { label: "Trust model", link: "/reference/trust/" },
            { label: "Status", link: "/reference/status/" },
          ],
        },
      ],
    }),
  ],
});
