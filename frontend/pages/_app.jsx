import { LiveRuntimeProvider } from "@/lib/liveRuntime";
import { ThemeProvider } from "@/lib/theme";
import { ModelSelectionProvider } from "@/lib/modelSelection";
import Head from "next/head";
import {useEffect} from 'react';
import {installClientDiagnostics} from '@/lib/clientDiagnostics.mjs';
import WorkspaceErrorBoundary from '@/components/workspace/WorkspaceErrorBoundary';
import "@/styles/globals.css";
import "@/styles/research.css";
import "@/styles/workspace.css";
import "@/styles/lab.css";
import "@/styles/discovery.css";
import "@/styles/verification.css";
import "@/styles/experiment.css";
import "@/styles/forge.css";

export default function PhaseForgeApp({ Component, pageProps, router }) {
  useEffect(()=>installClientDiagnostics(),[]);
  return (
    <>
      <Head>
        <title>PhaseForge · Scientific workbench</title>
        <link rel="icon" type="image/svg+xml" href="/phaseforge.svg?v=1.1" />
        <link rel="icon" type="image/x-icon" href="/favicon.ico?v=1.1" />
        <link rel="apple-touch-icon" href="/icon.png?v=1.1" />
        <meta
          name="description"
          content="Build scientific experiments, explore retained evidence and inspect detailed 3D models in your local scientific workbench."
        />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
      </Head>
      <ThemeProvider><ModelSelectionProvider><LiveRuntimeProvider><WorkspaceErrorBoundary scope="page" resetKey={router?.asPath} title="The workbench interface could not load."><Component {...pageProps} /></WorkspaceErrorBoundary></LiveRuntimeProvider></ModelSelectionProvider></ThemeProvider>
    </>
  );
}
