'use strict';
const fs = require('node:fs');
const path = require('node:path');
const report = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const findings = Array.isArray(report.findings) ? report.findings : [];
const rows = findings.map(f => {
  const file = String(f.path || '');
  const location = `${file}:${f.start_line}-${f.end_line}`;
  let excerpt = location;
  try {
    const root = path.resolve(process.cwd());
    const absolute = path.resolve(root, file);
    if (absolute.startsWith(root + path.sep)) {
      const lines = fs.readFileSync(absolute, 'utf8').split(/\r?\n/);
      const selected = lines.slice(Math.max(0, f.start_line - 1), Math.min(lines.length, f.end_line)).join('\n').slice(0, 240);
      if (selected) excerpt = `${location} — ${selected}`;
    }
  } catch {}
  excerpt = excerpt.replace(/[\r\n|]/g, ' ').replace(/`/g, '\\`');
  const reason = String(f.message || '').replace(/[\r\n|]/g, ' ').slice(0, 500);
  return `| \`${excerpt}\` | \`${f.rule_id}\` (${f.track}/${f.severity}) | ${reason} |`;
}).join('\n');
const semantic = findings.filter(f => f.track === 'semantic' && f.disposition === 'pending_semantic');
const card = semantic.length ? `\n\n### 高危语义自查卡片\n\n请人工确认以下语义触发项是否适用，并在 PR 描述或评论中记录判断与理由。该卡片不构成静态违规判定。\n\n${semantic.map(f => `- [ ] **${f.rule_id}** — ${f.path}:${f.start_line}: ${String(f.message).replace(/[\r\n]/g, ' ').slice(0, 300)}`).join('\n')}` : '';
const body = `<!-- sker-compliance-matrix -->\n## Sker 合规矩阵\n\n| 代码位置 | 触发规约 | 判断理由 |\n|---|---|---|\n${rows || '| — | 无发现 | 当前 diff 未产生规约命中 |'}\n${card}\n\n_由 Sker CI 自动更新；语义提示不等同于确定性阻断。_`;
process.stdout.write(JSON.stringify({ body }));
