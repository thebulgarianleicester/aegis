import { useState } from 'react';
import { BaselineTable, type Confusion } from './BaselineTable';
import { ReplayPanel } from './ReplayPanel';

type Report = { schema_version: number; warning: string; pairs: number; blocks: number; bytes_compared: number; elapsed_seconds: number; confusion: Confusion; baselines?: Record<string, Confusion> };

export function ResearchPanel() {
  const [report, setReport] = useState<Report | null>(null);
  const [error, setError] = useState('');
  async function load(file?: File) {
    if (!file) return;
    try {
      const value = JSON.parse(await file.text()) as Report;
      if (value.schema_version !== 1 || !value.confusion) throw new Error('Unsupported benchmark report');
      setReport(value); setError('');
    } catch (reason) { setReport(null); setError(reason instanceof Error ? reason.message : 'Invalid report'); }
  }
  return <>
    <div className="panel-toolbar"><p>Load locally generated classifier and replay reports. Files never leave the browser.</p><label className="button upload">↑ Load benchmark JSON<input type="file" accept=".json,application/json" onChange={event => void load(event.target.files?.[0])}/></label></div>
    {error && <div className="inline-error">{error}</div>}
    {report && <div className="stats compact"><Stat label="FILE PAIRS" value={report.pairs.toLocaleString()}/><Stat label="BLOCKS" value={report.blocks.toLocaleString()}/><Stat label="DATA" value={`${(report.bytes_compared/1048576).toFixed(2)} MiB`}/><Stat label="TIME" value={`${report.elapsed_seconds.toFixed(2)} s`}/></div>}
    <BaselineTable baselines={report?.baselines}/>
    <ReplayPanel/>
  </>;
}
function Stat({label,value}:{label:string;value:string}) { return <div className="stat"><div>{label}</div><strong>{value}</strong></div>; }
