import { LiveRuntimeProvider } from "@/lib/liveRuntime";
import { ThemeProvider } from "@/lib/theme";
import { ModelSelectionProvider } from "@/lib/modelSelection";
import Head from "next/head";
import "@/styles/globals.css";
import "@/styles/research.css";
import "@/styles/workspace.css";
import "@/styles/lab.css";
import "@/styles/discovery.css";
import "@/styles/verification.css";
import "@/styles/experiment.css";
import "@/styles/forge.css";

export default function PhaseForgeApp({ Component, pageProps }) {
  return (
    <>
      <Head>
        <title>PhaseForge · Alpha research workbench</title>
        <link rel="icon" type="image/svg+xml" href="/phaseforge.svg?v=1.1" />
        <link rel="icon" type="image/x-icon" href="/favicon.ico?v=1.1" />
        <link rel="apple-touch-icon" href="/icon.png?v=1.1" />
        <meta
          name="description"
          content="An alpha research workbench for curious people. Build scientific experiments, explore evidence and inspect detailed 3D models. More features are coming."
        />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
      </Head>
      <ThemeProvider><ModelSelectionProvider><LiveRuntimeProvider><Component {...pageProps} /></LiveRuntimeProvider></ModelSelectionProvider></ThemeProvider>
    </>
  );
}
