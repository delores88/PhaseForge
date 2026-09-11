import { Html, Head, Main, NextScript } from "next/document";
// Dark is the default; an explicit saved preference applies before first paint.
const bootstrap = `(function(){try{var m=localStorage.getItem('phaseforge.theme');document.documentElement.dataset.theme=m==='light'?'light':'dark';}catch(e){document.documentElement.dataset.theme='dark';}})();`;
export default function Document() {
  return <Html lang="en" suppressHydrationWarning><Head><script dangerouslySetInnerHTML={{ __html: bootstrap }} /></Head><body><Main /><NextScript /></body></Html>;
}
