import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import rehypeHighlight from 'rehype-highlight';

export function safeMarkdownUrl(value, key) {
  const url=String(value||'').trim();
  if (!url || /[\u0000-\u001f\u007f\\]/.test(url)) return '';
  const protocol=url.match(/^([a-z][a-z\d+.-]*):/i)?.[1]?.toLowerCase();
  if (protocol && !['http','https',...(key==='href'?['mailto']:[])].includes(protocol)) return '';
  return url;
}

// Preserve fenced/inline code while accepting the LaTeX delimiters commonly
// emitted by scientific models. CommonMark parsing remains the parser's job.
export function normalizeMathDelimiters(value) {
  return String(value||'').replace(/(^[ \t]{0,3}(`{3,}|~{3,})[^\n]*\n[\s\S]*?^[ \t]{0,3}\2[ \t]*$|`+[^`\n]*`+)|\\\[([\s\S]*?)\\\]|\\\(([^\n]*?)\\\)/gm,
    (match,code,fence,display,inline)=>code|| (display!==undefined?`\n\n$$\n${display.trim()}\n$$\n\n`:`$${inline}$`));
}

export const markdownOptions={
  skipHtml:true,
  urlTransform:safeMarkdownUrl,
  remarkPlugins:[remarkGfm,remarkMath],
  rehypePlugins:[[rehypeKatex,{trust:false,strict:'ignore',throwOnError:false,maxExpand:1000}], [rehypeHighlight,{detect:false,ignoreMissing:true}]],
};
