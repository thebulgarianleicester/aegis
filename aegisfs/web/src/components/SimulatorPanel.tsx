import { useState } from 'react';
import { post } from '../api';
import type { RunView } from '../types';

const workloads = [
  { id: 'compression', icon: '▰', name: 'Benign gzip', description: 'Real gzip -9 writes; validates that high entropy alone is insufficient.', tone: 'benign' },
  { id: 'full_encryption', icon: '◈', name: 'Full ChaCha20', description: 'Full-file cryptographic overwrite, rename and unlink in the isolated lab.', tone: 'attack' },
  { id: 'intermittent_encryption', icon: '▨', name: 'Intermittent encryption', description: 'Transforms one 4 KiB block in every four, LockBit-style striping.', tone: 'attack' },
  { id: 'multi_process', icon: '⑃', name: 'Multi-worker tree', description: 'Four synthetic workers aggregate evidence under one root ancestor.', tone: 'attack' },
  { id: 'canary', icon: '◇', name: 'Canary tripwire', description: 'Touches structured OOXML/PDF decoys for single-event containment.', tone: 'canary' },
];

export function SimulatorPanel({ activeRun, onChanged }: { activeRun?: string | null; onChanged: () => void }) {
  const [busy, setBusy] = useState('');
  const [error, setError] = useState('');
  async function start(workload: string) {
    setBusy(workload); setError('');
    try { await post<RunView>('/api/v1/simulator/run', { workload }); onChanged(); }
    catch (reason) { setError(reason instanceof Error ? reason.message : 'Unable to start workload'); }
    finally { setBusy(''); }
  }
  return <div>
    <div className="workload-grid">{workloads.map(workload => <button key={workload.id} className={`workload ${workload.tone}`} disabled={Boolean(activeRun) || Boolean(busy)} onClick={() => void start(workload.id)}>
      <span className="workload-icon">{workload.icon}</span><span><b>{busy === workload.id ? 'Starting…' : workload.name}</b><small>{workload.description}</small></span><span className="run-arrow">↗</span>
    </button>)}</div>
    {activeRun && <div className="running-note"><span className="spinner"/> Synthetic workload {activeRun} is running. New runs are locked until recovery finishes.</div>}
    {error && <div className="inline-error">{error}</div>}
  </div>;
}
