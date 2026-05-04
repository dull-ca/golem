import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

// https://astro.build/config
export default defineConfig({
  site: "https://golem.example",
  integrations: [
    starlight({
      title: "Golem",
      description:
        "Small-fleet declarative orchestrator for bare-metal Debian boxes.",
      social: {
        codeberg: "https://codeberg.org/dull/golem",
      },
      editLink: {
        baseUrl: "https://codeberg.org/dull/golem/_edit/main/docs/",
      },
      sidebar: [
        {
          label: "Start here",
          items: [
            { label: "What golem is", link: "/" },
            { label: "Install", link: "/getting-started/install/" },
            { label: "Your first bundle", link: "/getting-started/first-bundle/" },
          ],
        },
        {
          label: "Concepts",
          items: [
            { label: "The three layers", link: "/concepts/three-layers/" },
            { label: "Journal-before-mutate", link: "/concepts/journal/" },
            { label: "Trust model", link: "/concepts/trust/" },
          ],
        },
        {
          label: "Deployment guides",
          items: [
            { label: "Tier 1 — Hello, agent", link: "/guides/hello-agent/" },
            { label: "Tier 2 — One app, one DB", link: "/guides/app-and-db/" },
            { label: "Tier 3 — Litour on a box", link: "/guides/litour/" },
          ],
        },
        {
          label: "Reference",
          items: [
            { label: "Bundle format", link: "/reference/bundle-format/" },
            { label: "CLI", link: "/reference/cli/" },
            { label: "Status & milestones", link: "/reference/status/" },
          ],
        },
      ],
    }),
  ],
});
