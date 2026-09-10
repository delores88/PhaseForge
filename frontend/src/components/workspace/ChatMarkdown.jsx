import { Fragment, useMemo, useState } from "react";
import { Check, Copy } from "lucide-react";

function inline(text) {
  const parts = String(text || "").split(/(`[^`]+`|\*\*[^*]+\*\*)/g);
  return parts.map((part, index) => {
    if (part.startsWith("`") && part.endsWith("`")) {
      return <code key={index}>{part.slice(1, -1)}</code>;
    }
    if (part.startsWith("**") && part.endsWith("**")) {
      return <strong key={index}>{part.slice(2, -2)}</strong>;
    }
    return <Fragment key={index}>{part}</Fragment>;
  });
}

function CodeBlock({ language, content }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(content);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      // Clipboard access is optional in browsers.
    }
  };
  return (
    <div className="chatCodeBlock">
      <header>
        <span>{language || "code"}</span>
        <button type="button" onClick={copy}>
          {copied ? <Check size={13} /> : <Copy size={13} />}
          {copied ? "Copied" : "Copy"}
        </button>
      </header>
      <pre><code>{content}</code></pre>
    </div>
  );
}

export default function ChatMarkdown({ children }) {
  const blocks = useMemo(() => {
    const lines = String(children || "").replace(/\r\n/g, "\n").split("\n");
    const output = [];
    let paragraph = [];
    let list = [];
    let code = null;

    const flushParagraph = () => {
      if (!paragraph.length) return;
      output.push({ type: "p", text: paragraph.join("\n") });
      paragraph = [];
    };
    const flushList = () => {
      if (!list.length) return;
      output.push({ type: "list", items: list });
      list = [];
    };

    for (const line of lines) {
      if (code) {
        if (line.startsWith("```")) {
          output.push({ type: "code", language: code.language, text: code.lines.join("\n") });
          code = null;
        } else {
          code.lines.push(line);
        }
        continue;
      }
      if (line.startsWith("```")) {
        flushParagraph();
        flushList();
        code = { language: line.slice(3).trim(), lines: [] };
        continue;
      }
      const heading = line.match(/^(#{1,3})\s+(.+)$/);
      if (heading) {
        flushParagraph();
        flushList();
        output.push({ type: `h${heading[1].length}`, text: heading[2] });
        continue;
      }
      const item = line.match(/^\s*[-*]\s+(.+)$/);
      if (item) {
        flushParagraph();
        list.push(item[1]);
        continue;
      }
      if (!line.trim()) {
        flushParagraph();
        flushList();
      } else {
        flushList();
        paragraph.push(line);
      }
    }
    flushParagraph();
    flushList();
    if (code) output.push({ type: "code", language: code.language, text: code.lines.join("\n") });
    return output;
  }, [children]);

  return (
    <div className="chatMarkdown">
      {blocks.map((block, index) => {
        if (block.type === "code") {
          return <CodeBlock key={index} language={block.language} content={block.text} />;
        }
        if (block.type === "list") {
          return (
            <ul key={index}>
              {block.items.map((item, itemIndex) => <li key={itemIndex}>{inline(item)}</li>)}
            </ul>
          );
        }
        if (block.type === "h1") return <h2 key={index}>{inline(block.text)}</h2>;
        if (block.type === "h2") return <h3 key={index}>{inline(block.text)}</h3>;
        if (block.type === "h3") return <h4 key={index}>{inline(block.text)}</h4>;
        return <p key={index}>{inline(block.text)}</p>;
      })}
    </div>
  );
}
