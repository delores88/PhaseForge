import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import Markdown from 'react-markdown';
import {markdownOptions,normalizeMathDelimiters,safeMarkdownUrl} from '../src/lib/markdown.mjs';
const render=source=>renderToStaticMarkup(createElement(Markdown,{...markdownOptions,components:{img:({src,alt})=>src?createElement('img',{src,alt}):createElement('span',null,alt)}},normalizeMathDelimiters(source)));

test('scientific report renders nested ordered lists, tables, references, images and code rather than raw markup',()=>{
  const result=render('## Result\n\n1. First observation\n   - Nested measurement\n2. Control\n\n| Quantity | Value |\n| --- | ---: |\n| Energy | 2.4 |\n\n[Reference](https://example.org/paper)\n\n![Trajectory](/api/runs/example/frame.png)\n\n```python\nprint(2.4)\n```');
  for(const tag of ['<h2>','<ol>','<ul>','<table>','<thead>','<tbody>'])assert.ok(result.includes(tag),tag);
  assert.match(result,/href="https:\/\/example.org\/paper"/);
  assert.match(result,/src="\/api\/runs\/example\/frame.png"/);
  assert.match(result,/language-python/);assert.match(result,/hljs-/);
});
test('dollar and bracket mathematics render with accessible MathML; code keeps literal delimiters',()=>{
  const result=render('Inline $E=mc^2$ and \\(x^2\\).\n\n\\[\n\\frac{a}{b}\n\\]\n\n`\\(literal\\)`\n\n```text\n\\[not math\\]\n```');
  assert.match(result,/<math /);assert.match(result,/katex-display/);
  assert.match(result,/\\\(literal\\\)/);assert.match(result,/\\\[not math\\\]/);
  assert.equal(normalizeMathDelimiters('```text\n\\(literal\\)\n```'),'```text\n\\(literal\\)\n```');
});
test('untrusted HTML, javascript links, data images and trusted KaTeX commands cannot become active content',()=>{
  const result=render('<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\n[bad](javascript:alert%281%29)\n\n![bad](data:image/svg+xml;base64,abc)\n\n$\\href{javascript:alert(1)}{click}$');
  assert.doesNotMatch(result,/<script|onerror=|href="javascript:|src="data:/);
  for(const url of ['javascript:alert(1)','data:image/png;base64,abc','file:///C:/secret','java\nscript:alert(1)','\\\\host\\file'])assert.equal(safeMarkdownUrl(url,'src'),'');
  assert.equal(safeMarkdownUrl('mailto:research@example.org','href'),'mailto:research@example.org');
  assert.equal(safeMarkdownUrl('mailto:research@example.org','src'),'');
});
