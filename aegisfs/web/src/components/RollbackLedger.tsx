import { post } from '../api';
import type { RunView, TxEntry } from '../types';
import { useState } from 'react';

export function RollbackLedger({ entries, latestRun, onChanged, limit }: { entries: TxEntry[]; latestRun?: RunView; onChanged: () => void; limit?: number }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const shown = limit ? entries.slice(0, limit) : entries;
  async function rollback() {
    if (!latestRun) return;
    setBusy(true); setError('');
    try { await post(`/api/v1/processes/${latestRun.ragid}/rollback`); onChanged(); }
    catch (reason) { setError(reason instanceof Error ? reason.message : 'Rollback failed'); }
    finally { setBusy(false); }
  }
  return <>
    {latestRun && <div className="recovery-summary"><div><span className={latestRun.recovered ? 'verify-badge' : 'verify-badge pending'}>{latestRun.recovered ? '✓ BLAKE3 TREE VERIFIED' : '○ RECOVERY PENDING'}</span><small>{latestRun.restored_operations} inverse operations · {formatBytes(latestRun.restored_bytes)} restored</small></div><button className="button small" disabled={busy || latestRun.recovered} onClick={() => void rollback()}>{busy ? 'Requesting…' : latestRun.recovered ? 'Recovery complete' : 'Rollback tree'}</button></div>}
    {error && <div className="inline-error">{error}</div>}
    <div className="table-wrap"><table><thead><tr><th>TX</th><th>INVERSE OPERATION</th><th>PATH</th><th>PRE-IMAGE BLAKE3</th><th>STATE</th></tr></thead><tbody>
      {shown.map(entry => <tr key={entry.id}><td className="mono dim">#{entry.id}</td><td><span className={`op op-${entry.operation.toLowerCase()}`}>{entry.operation}</span></td><td><span className="path-cell">{entry.path}</span>{entry.new_path && <small className="table-sub">→ {entry.new_path}</small>}</td><td><span className="mono hash">{entry.pre_hash ? `${entry.pre_hash.slice(0,12)}…` : '—'}</span></td><td>{entry.state === 'ROLLED_BACK' ? <span className="verified">✓ RESTORED</span> : <span className="dim">{entry.state}</span>}</td></tr>)}
    </tbody></table>{entries.length === 0 && <div className="table-empty">No CoW transactions recorded.</div>}</div>
  </>;
}

export function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1048576) return `${(bytes/1024).toFixed(1)} KiB`;
  return `${(bytes/1048576).toFixed(2)} MiB`;
}
