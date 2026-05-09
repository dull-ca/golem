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
          label: "Get started",
          items: [
            { label: "What golem is", link: "/" },
            { label: "Install", link: "/getting-started/install/" },
            { label: "The config", link: "/getting-started/the-config/" },
            { label: "Applying changes", link: "/getting-started/applying/" },
          ],
        },
        {
          label: "Deployment guides",
          items: [
            { label: "Tier 1 — One service", link: "/guides/hello-agent/" },
            { label: "Tier 2 — Two hosts, one DB", link: "/guides/app-and-db/" },
            { label: "Tier 3 — Litour on a box", link: "/guides/litour/" },
          ],
        },
        {
          label: "Reference",
          items: [
            { label: "CLI", link: "/reference/cli/" },
            { label: "Bundle format", link: "/reference/bundle-format/" },
            { label: "Architecture", link: "/reference/architecture/" },
            { label: "Honest convergence", link: "/reference/honest-convergence/" },
            { label: "Trust model", link: "/reference/trust/" },
            { label: "Status & milestones", link: "/reference/status/" },
          ],
        },
      ],
    }),
  ],
});
