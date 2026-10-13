function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function inline(md: string): string {
  // Assumes input already escaped. Apply inline patterns.
  // Protect `code` spans so *, **, links etc. inside them are literal.
  const codeSpans: string[] = [];
  let out = md.replace(/`([^`\n]+)`/g, (_m, code: string) => {
    codeSpans.push(`<code>${code}</code>`);
    return `%%TRAYASKCODE${codeSpans.length - 1}%%`;
  });
  // Images first (must precede link handling).
  out = out.replace(
    /!\[([^\]]*)\]\(([^)\s]+)\)/g,
    '<img alt="$1" src="$2">',
  );
  // Markdown links. Allow any non-space URL, not just http(s).
  out = out.replace(
    /\[([^\]]+)\]\(([^)\s]+)(?:\s+&quot;[^)]*&quot;)?\)/g,
    '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>',
  );
  // Bare URLs (avoid matching inside href="..." or src="...").
  out = out.replace(
    /(?<!["'=])(https?:\/\/[^\s<]+)/g,
    '<a href="$1" target="_blank" rel="noopener noreferrer">$1</a>',
  );
  out = out.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  out = out.replace(/__([^_]+)__/g, "<strong>$1</strong>");
  out = out.replace(/(^|[\s(])\*([^*\n]+)\*/g, "$1<em>$2</em>");
  out = out.replace(/(^|[\s(])_([^_\n]+)_/g, "$1<em>$2</em>");
  out = out.replace(/~~([^~]+)~~/g, "<del>$1</del>");
  // Restore code spans.
  out = out.replace(/%%TRAYASKCODE(\d+)%%/g, (_m, i: string) => codeSpans[Number(i)]);
  return out;
}

function splitTableRow(row: string): string[] {
  let r = row.trim();
  if (r.startsWith("|")) r = r.slice(1);
  if (r.endsWith("|")) r = r.slice(0, -1);
  return r.split("|").map((c) => c.trim());
}

function isTableDelimiter(line: string): boolean {
  const cells = splitTableRow(line);
  if (cells.length === 0) return false;
  return cells.every((c) => /^:?-{1,}:?$/.test(c));
}

export function renderMarkdown(md: string): string {
  const esc = escapeHtml(md);
  const lines = esc.split("\n");
  let html = "";
  let inCode = false;
  let codeBuf: string[] = [];
  let inUl = false;
  let inOl = false;
  let quoteBuf: string[] = [];

  function closeUl(): void {
    if (inUl) {
      html += "</ul>";
      inUl = false;
    }
  }

  function closeOl(): void {
    if (inOl) {
      html += "</ol>";
      inOl = false;
    }
  }

  function closeLists(): void {
    closeUl();
    closeOl();
  }

  function flushQuote(): void {
    if (quoteBuf.length > 0) {
      html += `<blockquote>${quoteBuf.map((q) => `<p>${inline(q)}</p>`).join("")}</blockquote>`;
      quoteBuf = [];
    }
  }

  function closeBlocks(): void {
    closeLists();
    flushQuote();
  }

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (line.trim().startsWith("```")) {
      if (inCode) {
        html += "<pre><code>" + codeBuf.join("\n") + "</code></pre>";
        codeBuf = [];
        inCode = false;
      } else {
        closeBlocks();
        inCode = true;
      }
      continue;
    }
    if (inCode) {
      codeBuf.push(line);
      continue;
    }
    // GFM table: header row + delimiter row + body rows.
    if (
      line.includes("|") &&
      i + 1 < lines.length &&
      isTableDelimiter(lines[i + 1])
    ) {
      closeBlocks();
      const header = splitTableRow(line);
      i += 1; // skip delimiter
      html += "<table><thead><tr>";
      for (const h of header) html += `<th>${inline(h)}</th>`;
      html += "</tr></thead><tbody>";
      while (
        i + 1 < lines.length &&
        lines[i + 1].includes("|") &&
        lines[i + 1].trim() !== ""
      ) {
        i += 1;
        const cells = splitTableRow(lines[i]);
        html += "<tr>";
        for (let c = 0; c < header.length; c++)
          html += `<td>${inline(cells[c] ?? "")}</td>`;
        html += "</tr>";
      }
      html += "</tbody></table>";
      continue;
    }
    const heading = line.match(/^(#{1,6})\s+(.*)$/);
    if (heading) {
      closeBlocks();
      const level = heading[1].length;
      html += `<h${level}>${inline(heading[2])}</h${level}>`;
      continue;
    }
    if (/^\s*(---|\*\*\*|___)\s*$/.test(line)) {
      closeBlocks();
      html += "<hr>";
      continue;
    }
    const quote = line.match(/^\s*&gt;\s?(.*)$/);
    if (quote) {
      closeLists();
      quoteBuf.push(quote[1]);
      continue;
    }
    if (quoteBuf.length > 0) flushQuote();
    const ol = line.match(/^\s*\d+[.)]\s+(.*)$/);
    if (ol) {
      closeUl();
      if (!inOl) {
        html += "<ol>";
        inOl = true;
      }
      html += `<li>${inline(ol[1])}</li>`;
      continue;
    }
    const ul = line.match(/^\s*[-*+]\s+(.*)$/);
    if (ul) {
      closeOl();
      if (!inUl) {
        html += "<ul>";
        inUl = true;
      }
      html += `<li>${inline(ul[1])}</li>`;
      continue;
    }
    if (line.trim() === "") {
      closeBlocks();
      continue;
    }
    closeLists();
    html += `<p>${inline(line)}</p>`;
  }
  if (inCode) {
    html += "<pre><code>" + codeBuf.join("\n") + "</code></pre>";
  }
  closeBlocks();
  return html;
}
