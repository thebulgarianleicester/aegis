import { useState } from 'react';
import { post } from '../api';
import type { ProcessView } from '../types';

export function ProcessTable({ processes, onChanged, expanded = false }: { processes: ProcessView[]; onChanged: () => void; expanded?: boolean }) {
  const [busy, setBusy] = useState('');
  const [error, setError] = useState('');
  async function contain(ragid: string) {
    setBusy(ragid); setError('');
    try { await post(`/api/v1/processes/${ragid}/contain`); onChanged(); }
    catch (reason) { setError(reason instanceof Error ? reason.message : 'Containment failed'); }
    finally { setBusy(''); }
  }
  return <>
    {error && <div className="inline-error">{error}</div>}
    <div className="table-wrap"><table><thead><tr><th>ROOT ANCESTOR GROUP</th><th>PROCESS / PID</th><th>WRITES</th><th>ΔH</th><th>SCORE</th><th>STATE</th>{expanded && <th>ACTION</th>}</tr></thead>
      <tbody>{processes.map(process => <tr key={process.ragid}>
        <td><span className="mono hash">{process.ragid}</span>{process.worker_pids.length > 1 && <small className="table-sub">{process.worker_pids.length} aggregated workers</small>}</td>
        <td><b>{process.executable}</b><small className="table-sub">root {process.root_pid}</small></td>
        <td className="mono">{process.writes}</td>
        <td className={process.delta_h > 1 ? 'danger-text mono' : 'mono'}>{process.delta_h >= 0 ? '+' : ''}{process.delta_h.toFixed(2)}</td>
        <td><div className="score-cell"><span className="mono">{process.score.toFixed(2)}</span><div className="score-track"><i style={{width:`${Math.min(100,process.score/9.210340371976184*100)}%`}}/></div></div></td>
        <td><Status status={process.status}/></td>
        {expanded && <td><button className="button small danger-outline" disabled={busy === process.ragid || !['ACTIVE','RUNNING'].includes(process.status)} onClick={() => void contain(process.ragid)}>{busy === process.ragid ? 'Freezing…' : 'Freeze tree'}</button></td>}
      </tr>)}</tbody></table>
      {processes.length === 0 && <div className="table-empty">No synthetic process groups yet. Launch a workload to begin.</div>}
    </div>
  </>;
}

export function Status({ status }: { status: string }) {
  const kind = status.includes('FROZEN') || status.includes('FAILED') ? 'critical' : status.includes('RECOVERED') || status.includes('EXITED') ? 'success' : 'active';
  return <span className={`status-pill ${kind}`}><i/>{status.replace(/_/g,' ')}</span>;
}
