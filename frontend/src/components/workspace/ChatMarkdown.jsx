import {useEffect,useMemo,useRef,useState} from 'react';
import Markdown from 'react-markdown';
import {Check,Copy} from 'lucide-react';
import {markdownOptions,normalizeMathDelimiters} from '@/lib/markdown.mjs';
import 'katex/dist/katex.min.css';
import styles from './ChatMarkdown.module.css';
import WorkspaceErrorBoundary from './WorkspaceErrorBoundary';

const nodeText=node=>node?.type==='text'?node.value:(node?.children||[]).map(nodeText).join('');
function CodeBlock({children,node}) {
  const [copied,setCopied]=useState(false),timer=useRef(null);
  useEffect(()=>()=>clearTimeout(timer.current),[]);
  const language=node?.children?.[0]?.properties?.className?.find(value=>value.startsWith('language-'))?.slice(9)||'text';
  async function copy(){try{await navigator.clipboard.writeText(nodeText(node));setCopied(true);clearTimeout(timer.current);timer.current=setTimeout(()=>setCopied(false),1500);}catch{setCopied(false);}}
  return <div className={styles.codeBlock}><header><span>{language}</span><button type="button" onClick={copy} aria-label={copied?'Code copied':'Copy code'}>{copied?<Check size={14}/>:<Copy size={14}/>} {copied?'Copied':'Copy'}</button></header><pre>{children}</pre></div>;
}
const components={
  pre:CodeBlock,
  a:({node,href,children,...props})=>href?<a {...props} href={href} target={/^https?:/i.test(href)?'_blank':undefined} rel="noopener noreferrer">{children}</a>:<span>{children}</span>,
  img:({node,src,alt,...props})=>src?<img {...props} src={src} alt={alt||'Research image'} loading="lazy" decoding="async"/>:<span>{alt||'Unavailable image'}</span>,
  table:({node,children,...props})=><div className={styles.tableScroll} tabIndex={0} role="region" aria-label="Data table"><table {...props}>{children}</table></div>,
};
export default function ChatMarkdown({children}) {
  const source=useMemo(()=>normalizeMathDelimiters(children),[children]);
  return <div className={`chatMarkdown ${styles.markdown}`}><WorkspaceErrorBoundary scope="chat-markdown" resetKey={source} fallback={<div style={{whiteSpace:'pre-wrap',overflowWrap:'anywhere'}}><small>Formatting could not be displayed. The saved message is shown below.</small><p>{source}</p></div>}><Markdown {...markdownOptions} components={components}>{source}</Markdown></WorkspaceErrorBoundary></div>;
}
