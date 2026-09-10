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
        <title>PhaseForge</title>
        <link rel="icon" type="image/svg+xml" href="/phaseforge.svg" />
        <meta
          name="description"
          content="An interactive computational discovery laboratory."
        />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
      </Head>
      <ThemeProvider><ModelSelectionProvider><LiveRuntimeProvider><Component {...pageProps} /></LiveRuntimeProvider></ModelSelectionProvider></ThemeProvider>
    </>
  );
}
