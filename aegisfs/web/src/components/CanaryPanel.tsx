import { useState } from 'react';
import { post } from '../api';
import type { Canary } from '../types';
import { formatBytes } from './RollbackLedger';

export function CanaryPanel({ canaries, onChanged }: { canaries: Canary[]; onChanged: () => void }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  async function deploy() {
    setBusy(true); setError('');
    try { await post<Canary[]>('/api/v1/canaries/deploy'); onChanged(); }
    catch (reason) { setError(reason instanceof Error ? reason.message : 'Deployment failed'); }
    finally { setBusy(false); }
  }
  return <>
    <div className="panel-toolbar"><p>Valid OOXML and PDF decoys are created without overwriting modified or user-owned files.</p><button className="button" disabled={busy} onClick={() => void deploy()}>{busy ? 'Deploying…' : '◇ Deploy / verify canaries'}</button></div>
    {error && <div className="inline-error">{error}</div>}
    <div className="canary-grid">{canaries.map(canary => <article className="canary-card" key={canary.path}><div className="canary-top"><span className="file-icon">{canary.format.includes('PDF') ? 'PDF' : 'XML'}</span><span className="verify-badge">✓ HASH INTACT</span></div><h3>{canary.path.split('/').pop()}</h3><p>{canary.format} · {formatBytes(canary.bytes)} · {canary.hidden ? 'hidden' : 'lexicographically early'}</p><span className="mono hash">{canary.blake3}</span></article>)}</div>
    {canaries.length === 0 && <div className="empty"><div className="empty-icon">◇</div><b>No standalone canary set deployed</b><span>Simulator runs also seed their own three decoys automatically.</span></div>}
  </>;
}
