import { Html, Head, Main, NextScript } from "next/document";
// Runs before paint, so stored light mode never opens as a dark flash.
const bootstrap = `(function(){try{var m=localStorage.getItem('phaseforge.theme');if(m!=='light'&&m!=='dark')m=matchMedia('(prefers-color-scheme: light)').matches?'light':'dark';document.documentElement.dataset.theme=m;}catch(e){document.documentElement.dataset.theme='dark';}})();`;
export default function Document() {
  return <Html lang="en" suppressHydrationWarning><Head><script dangerouslySetInnerHTML={{ __html: bootstrap }} /></Head><body><Main /><NextScript /></body></Html>;
}
